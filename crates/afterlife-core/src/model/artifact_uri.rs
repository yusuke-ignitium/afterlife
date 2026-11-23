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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestRef {
    pub repo: String,
    pub number: u64,
}

impl PullRequestRef {
    pub fn display_label(&self) -> String {
        format!("{} #{}", self.repo, self.number)
    }
}

fn split_http_url_host_and_segments(uri: &str) -> Option<(String, String, Vec<&str>)> {
    let trimmed = uri.trim();
    if !is_openable_url(trimmed) {
        return None;
    }
    let (scheme, rest) = trimmed.split_once("://")?;
    let scheme_lower = scheme.to_ascii_lowercase();
    if scheme_lower != "http" && scheme_lower != "https" {
        return None;
    }
    let no_frag = rest.split('#').next().unwrap_or(rest);
    let no_query = no_frag.split('?').next().unwrap_or(no_frag);
    let (host, path) = no_query.split_once('/')?;
    let host_lower = host.trim().to_ascii_lowercase();
    if host_lower.is_empty() {
        return None;
    }
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    Some((scheme_lower, host_lower, segments))
}

fn parse_positive_pr_number(raw: &str) -> Option<u64> {
    let num = raw.parse::<u64>().ok()?;
    (num > 0).then_some(num)
}

fn extract_pr_repo_and_number<'a>(segments: &'a [&'a str]) -> Option<(&'a str, u64, usize)> {
    for (idx, seg) in segments.iter().enumerate() {
        if (*seg == "pull" || *seg == "pulls" || *seg == "pull-requests")
            && idx >= 2
            && let Some(num) = segments.get(idx + 1).and_then(|s| parse_positive_pr_number(s))
        {
            let repo = segments[idx - 1];
            if !repo.is_empty() {
                return Some((repo, num, idx + 2));
            }
        }
        if *seg == "merge_requests"
            && idx >= 2
            && let Some(num) = segments.get(idx + 1).and_then(|s| parse_positive_pr_number(s))
        {
            let repo_idx = if segments[idx - 1] == "-" {
                idx.checked_sub(2)?
            } else {
                idx - 1
            };
            let repo = segments[repo_idx];
            if !repo.is_empty() {
                return Some((repo, num, idx + 2));
            }
        }
        if *seg == "pullrequest"
            && idx >= 2
            && segments[idx - 2] == "_git"
            && let Some(num) = segments.get(idx + 1).and_then(|s| parse_positive_pr_number(s))
        {
            let repo = segments[idx - 1];
            if !repo.is_empty() {
                return Some((repo, num, idx + 2));
            }
        }
    }
    None
}

pub fn parse_pull_request_ref(uri: &str) -> Option<PullRequestRef> {
    let (_, _, segments) = split_http_url_host_and_segments(uri)?;
    let (repo, number, _) = extract_pr_repo_and_number(&segments)?;
    Some(PullRequestRef {
        repo: repo.to_string(),
        number,
    })
}

pub fn canonical_pull_request_key(uri: &str) -> String {
    if let Some((scheme, host, segments)) = split_http_url_host_and_segments(uri) {
        let kept = match extract_pr_repo_and_number(&segments) {
            Some((_, _, base_len)) => &segments[..base_len],
            None => &segments[..],
        };
        return format!("{scheme}://{host}/{}", kept.join("/"));
    }
    uri.trim().trim_end_matches('/').to_string()
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

    #[test]
    fn parses_pull_request_refs_and_canonicalizes_duplicates_across_forges() {
        let gh = parse_pull_request_ref("https://github.com/org/afterlife/pull/14").unwrap();
        assert_eq!(gh.display_label(), "afterlife #14");

        let gh_files =
            parse_pull_request_ref("https://GITHUB.COM/org/afterlife/pull/14/files?w=1#diff-0")
                .unwrap();
        assert_eq!(gh_files, gh);
        assert_eq!(
            canonical_pull_request_key("https://github.com/org/afterlife/pull/14"),
            canonical_pull_request_key("https://GITHUB.COM/org/afterlife/pull/14/files?w=1#diff-0")
        );

        let gitea = parse_pull_request_ref("https://code.example.com/team/sdk/pulls/7/").unwrap();
        assert_eq!(gitea.display_label(), "sdk #7");

        let gitlab = parse_pull_request_ref(
            "https://gitlab.com/org/sub/afterlife-cloud/-/merge_requests/42",
        )
        .unwrap();
        assert_eq!(gitlab.display_label(), "afterlife-cloud #42");

        let bitbucket = parse_pull_request_ref(
            "https://bitbucket.org/workspace/design-tokens/pull-requests/18/diff",
        )
        .unwrap();
        assert_eq!(bitbucket.display_label(), "design-tokens #18");

        let ado = parse_pull_request_ref(
            "https://dev.azure.com/org/proj/_git/control-plane/pullrequest/309",
        )
        .unwrap();
        assert_eq!(ado.display_label(), "control-plane #309");

        for invalid in [
            "https://github.com/org/afterlife/pull/0",
            "https://github.com/org/afterlife/issues/14",
            "https://github.com/org/afterlife/pull/not-a-number",
            "javascript:alert(1)",
            "file:///org/afterlife/pull/1",
            "blob://local-owner/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/pr.md",
            "linear://org/issue/ENG-1",
        ] {
            assert!(
                parse_pull_request_ref(invalid).is_none(),
                "expected PR URI `{invalid}` not to parse"
            );
        }
    }
}
