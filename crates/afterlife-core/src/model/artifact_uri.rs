use crate::model::inline_link::is_openable_url;

pub const MAX_ARTIFACT_BLOB_BYTES: usize = 4 * 1024 * 1024;
const MAX_BLOB_FILENAME_LEN: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedBlobUri {
    pub owner_segment: String,
    pub sha256: String,
    pub filename: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedArtifactUri {
    External(String),
    Blob(ParsedBlobUri),
}

pub fn sanitize_owner_segment(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "owner".to_string()
    } else {
        cleaned
    }
}

pub fn sanitize_blob_filename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let mut cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .take(MAX_BLOB_FILENAME_LEN)
        .collect();
    while cleaned.starts_with('.') {
        cleaned.remove(0);
    }
    while cleaned.contains("..") {
        cleaned = cleaned.replace("..", "_");
    }
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "artifact.bin".to_string()
    } else {
        cleaned
    }
}

pub fn format_blob_uri(owner_id: &str, sha256: &str, filename: &str) -> String {
    format!(
        "blob://{}/{}/{}",
        sanitize_owner_segment(owner_id),
        sha256.to_ascii_lowercase(),
        sanitize_blob_filename(filename)
    )
}

pub fn is_valid_lower_sha256(sha: &str) -> bool {
    sha.len() == 64 && sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_valid_owner_segment(seg: &str) -> bool {
    !seg.is_empty()
        && seg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn is_valid_blob_filename(seg: &str) -> bool {
    !seg.is_empty()
        && seg.len() <= MAX_BLOB_FILENAME_LEN
        && seg != "."
        && seg != ".."
        && !seg.starts_with('.')
        && !seg.contains("..")
        && seg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

pub fn parse_blob_uri(uri: &str) -> Option<ParsedBlobUri> {
    let trimmed = uri.trim();
    if trimmed.is_empty()
        || trimmed
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return None;
    }
    let rest = trimmed
        .strip_prefix("blob://")
        .or_else(|| trimmed.strip_prefix("local://"))?;
    let mut parts = rest.split('/');
    let owner_segment = parts.next()?;
    let sha256 = parts.next()?;
    let filename = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if !is_valid_owner_segment(owner_segment)
        || !is_valid_lower_sha256(sha256)
        || !is_valid_blob_filename(filename)
    {
        return None;
    }
    Some(ParsedBlobUri {
        owner_segment: owner_segment.to_string(),
        sha256: sha256.to_string(),
        filename: filename.to_string(),
    })
}

pub fn parse_artifact_uri(uri: &str) -> Option<ParsedArtifactUri> {
    if let Some(blob) = parse_blob_uri(uri) {
        return Some(ParsedArtifactUri::Blob(blob));
    }
    if is_openable_url(uri) {
        return Some(ParsedArtifactUri::External(uri.trim().to_string()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn parses_external_and_blob_uris_and_rejects_fictional_schemes() {
        assert!(matches!(
            parse_artifact_uri("https://github.com/org/repo/pull/1"),
            Some(ParsedArtifactUri::External(_))
        ));
        assert!(matches!(
            parse_artifact_uri("linear://org/issue/ENG-1"),
            Some(ParsedArtifactUri::External(_))
        ));
        assert!(matches!(
            parse_artifact_uri("slack://channel?team=T1&id=C1"),
            Some(ParsedArtifactUri::External(_))
        ));

        let blob_uri = format_blob_uri("local-owner", SAMPLE_SHA, "before-baseline.json");
        assert_eq!(blob_uri, format!("blob://local-owner/{SAMPLE_SHA}/before-baseline.json"));
        let parsed = parse_blob_uri(&blob_uri).unwrap();
        assert_eq!(parsed.owner_segment, "local-owner");
        assert_eq!(parsed.sha256, SAMPLE_SHA);
        assert_eq!(parsed.filename, "before-baseline.json");

        let legacy_local = format!("local://local-owner/{SAMPLE_SHA}/before-baseline.json");
        assert_eq!(parse_blob_uri(&legacy_local), Some(parsed));

        for rejected in [
            "evidence://job-001/before-baseline.json",
            "file:///etc/passwd",
            "s3://bucket/key",
            "ftp://example.com/a",
            "javascript:alert(1)",
            "data:text/plain,abc",
            "vbscript:foo",
            "queue-drop.json",
            "blob://local-owner/not-a-sha/report.json",
            &format!("blob://local-owner/{SAMPLE_SHA}/../escape.json"),
            &format!("blob://local-owner/{SAMPLE_SHA}/nested/path.json"),
            &format!("blob://local-owner/{}/report.json", SAMPLE_SHA.to_ascii_uppercase()),
        ] {
            assert!(
                parse_artifact_uri(rejected).is_none(),
                "expected URI `{rejected}` to be rejected"
            );
        }
    }

    #[test]
    fn sanitizes_owner_segments_and_filenames_against_traversal() {
        assert_eq!(sanitize_owner_segment("alice@example.com"), "alice_example_com");
        assert_eq!(sanitize_owner_segment(""), "owner");
        assert_eq!(sanitize_blob_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_blob_filename("..."), "artifact.bin");
        assert_eq!(sanitize_blob_filename("a b/report (1).json"), "report__1_.json");
    }
}
