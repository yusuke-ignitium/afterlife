use std::sync::Arc;

use afterlife_core::{Artifact, ArtifactId, ArtifactKind, OwnerId, parse_blob_uri};
use afterlife_daemon::{
    AppDaemonState,
    artifacts::{ArtifactBlobError, read_owned_blob},
};

const MAX_PREVIEW_TEXT_BYTES: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactPreviewBody {
    Text { text: String, truncated: bool },
    Binary { byte_len: usize },
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactPreview {
    pub artifact_id: ArtifactId,
    pub label: String,
    pub kind_label: &'static str,
    pub uri: String,
    pub sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub body: ArtifactPreviewBody,
}

fn artifact_kind_label(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::RawNote => "Raw Note",
        ArtifactKind::Issue => "Issue",
        ArtifactKind::Spec => "Spec",
        ArtifactKind::SourceCode => "Source Code",
        ArtifactKind::PullRequest => "Pull Request",
        ArtifactKind::CiReport => "CI Report",
        ArtifactKind::Snapshot => "Snapshot",
        ArtifactKind::VerificationReport => "Verification Report",
    }
}

fn format_preview_text(raw: String) -> (String, bool) {
    let formatted = serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or(raw);
    if formatted.len() <= MAX_PREVIEW_TEXT_BYTES {
        return (formatted, false);
    }
    let mut end = MAX_PREVIEW_TEXT_BYTES;
    while end > 0 && !formatted.is_char_boundary(end) {
        end -= 1;
    }
    (formatted[..end].to_string(), true)
}

pub fn load_artifact_preview(
    owner_id: &OwnerId,
    daemon: Option<&Arc<AppDaemonState>>,
    art: &Artifact,
) -> ArtifactPreview {
    let kind_label = artifact_kind_label(art.kind);
    let parsed = parse_blob_uri(&art.uri);
    let fallback_sha = parsed
        .as_ref()
        .map(|b| b.sha256.clone())
        .or_else(|| art.sha256.clone());

    let Some(daemon) = daemon else {
        let msg = if parsed.is_some() {
            "Local blob store is unavailable in ephemeral mode."
        } else {
            "This artifact is not stored in the Afterlife blob store."
        };
        return ArtifactPreview {
            artifact_id: art.id.clone(),
            label: art.label.clone(),
            kind_label,
            uri: art.uri.clone(),
            sha256: fallback_sha,
            size_bytes: None,
            body: ArtifactPreviewBody::Unavailable(msg.into()),
        };
    };

    match read_owned_blob(daemon.ports.blobs.as_ref(), owner_id, &art.uri) {
        Ok(read) => {
            let len = read.bytes.len();
            let body = match String::from_utf8(read.bytes) {
                Ok(text) => {
                    let (formatted, truncated) = format_preview_text(text);
                    ArtifactPreviewBody::Text {
                        text: formatted,
                        truncated,
                    }
                }
                Err(_) => ArtifactPreviewBody::Binary { byte_len: len },
            };
            ArtifactPreview {
                artifact_id: art.id.clone(),
                label: art.label.clone(),
                kind_label,
                uri: art.uri.clone(),
                sha256: Some(read.sha256),
                size_bytes: Some(len as u64),
                body,
            }
        }
        Err(err) => {
            let msg = match err {
                ArtifactBlobError::NotBlobUri => {
                    "This artifact is not stored in the Afterlife blob store.".into()
                }
                ArtifactBlobError::OwnerMismatch { .. } => {
                    "Blob belongs to a different owner and cannot be opened.".into()
                }
                ArtifactBlobError::NotFound(_) => {
                    "Stored blob payload was not found for this artifact.".into()
                }
                ArtifactBlobError::Storage(e) => format!("Failed to read artifact blob: {e}"),
                ArtifactBlobError::InvalidSha256 | ArtifactBlobError::DigestMismatch => {
                    "Artifact blob digest is invalid.".into()
                }
            };
            ArtifactPreview {
                artifact_id: art.id.clone(),
                label: art.label.clone(),
                kind_label,
                uri: art.uri.clone(),
                sha256: fallback_sha,
                size_bytes: None,
                body: ArtifactPreviewBody::Unavailable(msg),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use afterlife_protocol::AfterlifeConfig;

    use crate::app::state::AppState;

    use super::*;

    #[test]
    fn artifact_preview_loads_owner_blobs_pretty_prints_json_and_blocks_foreign_owner() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("afterlife_ui_preview_{nanos}"));
        let cfg = AfterlifeConfig::desktop_embedded(dir.to_str().unwrap());
        let daemon = AppDaemonState::from_config(cfg).unwrap();

        let mut st = AppState::new_ephemeral();
        st.daemon = Some(daemon.clone());

        let json_blob = daemon
            .ports
            .blobs
            .put_blob(&st.owner_id, "report.json", br#"{"status":"pass","n":3}"#)
            .unwrap();
        let json_art = Artifact {
            id: ArtifactId::from("art-00001"),
            kind: ArtifactKind::VerificationReport,
            label: "report.json".into(),
            uri: json_blob.uri,
            sha256: Some(json_blob.sha256.clone()),
            produced_at_stage: None,
            produced_at_sub_stage: None,
            revision_at_capture: 2,
        };
        st.open_artifact_preview(&json_art);
        let preview = st.preview_artifact.as_ref().unwrap();
        assert_eq!(preview.size_bytes, Some(23));
        assert_eq!(preview.sha256.as_deref(), Some(json_blob.sha256.as_str()));
        match &preview.body {
            ArtifactPreviewBody::Text { text, truncated } => {
                assert!(!truncated);
                assert!(text.contains("\"status\": \"pass\""));
                assert!(text.contains('\n'));
            }
            other => panic!("expected Text preview, got {other:?}"),
        }

        let bin_blob = daemon
            .ports
            .blobs
            .put_blob(&st.owner_id, "img.bin", &[0xff, 0xfe, 0x00, 0x01])
            .unwrap();
        let bin_art = Artifact {
            id: ArtifactId::from("art-00002"),
            kind: ArtifactKind::Snapshot,
            label: "img.bin".into(),
            uri: bin_blob.uri,
            sha256: Some(bin_blob.sha256),
            produced_at_stage: None,
            produced_at_sub_stage: None,
            revision_at_capture: 3,
        };
        st.open_artifact_preview(&bin_art);
        assert_eq!(
            st.preview_artifact.as_ref().unwrap().body,
            ArtifactPreviewBody::Binary { byte_len: 4 }
        );

        let foreign_art = Artifact {
            id: ArtifactId::from("art-00003"),
            kind: ArtifactKind::Snapshot,
            label: "foreign.json".into(),
            uri: format!(
                "blob://other-owner/{}/foreign.json",
                json_blob.sha256
            ),
            sha256: Some(json_blob.sha256),
            produced_at_stage: None,
            produced_at_sub_stage: None,
            revision_at_capture: 4,
        };
        st.open_artifact_preview(&foreign_art);
        assert!(matches!(
            st.preview_artifact.as_ref().unwrap().body,
            ArtifactPreviewBody::Unavailable(_)
        ));

        let legacy_art = Artifact::simple("JOB-001-drop", "queue-drop.json", ArtifactKind::RawNote);
        st.open_artifact_preview(&legacy_art);
        assert!(matches!(
            st.preview_artifact.as_ref().unwrap().body,
            ArtifactPreviewBody::Unavailable(_)
        ));

        st.close_artifact_preview();
        assert!(st.preview_artifact.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
