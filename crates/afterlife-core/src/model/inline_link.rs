use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineLink {
    pub range: Range<usize>,
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedInlineText {
    pub display_text: String,
    pub links: Vec<InlineLink>,
}

pub fn is_openable_url(url: &str) -> bool {
    let trimmed = url.trim();
    if trimmed.is_empty()
        || trimmed
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    for prefix in ["https://", "http://", "linear://", "slack://"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            return !rest.is_empty() && !rest.starts_with('/');
        }
    }
    false
}

fn find_matching_url_end(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            b'\n' | b'\r' => return None,
            _ => {}
        }
        i += 1;
    }
    None
}

pub fn parse_inline_links(input: &str) -> ParsedInlineText {
    let bytes = input.as_bytes();
    let mut display_text = String::with_capacity(input.len());
    let mut links = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'['
            && let Some(close_bracket_rel) = input[i + 1..].find(']')
        {
            let close_bracket = i + 1 + close_bracket_rel;
            let label = &input[i + 1..close_bracket];
            let valid_label = !label.trim().is_empty()
                && !label.contains('[')
                && !label.contains('\n')
                && !label.contains('\r');

            if valid_label
                && close_bracket + 1 < bytes.len()
                && bytes[close_bracket + 1] == b'('
                && let Some(close_paren) = find_matching_url_end(input, close_bracket + 2)
            {
                let raw_url = input[close_bracket + 2..close_paren].trim();
                if is_openable_url(raw_url) {
                    let start = display_text.len();
                    display_text.push_str(label);
                    let end = display_text.len();
                    links.push(InlineLink {
                        range: start..end,
                        label: label.to_string(),
                        url: raw_url.to_string(),
                    });
                    i = close_paren + 1;
                    continue;
                }
            }
        }

        let ch = input[i..].chars().next().unwrap();
        display_text.push(ch);
        i += ch.len_utf8();
    }

    ParsedInlineText {
        display_text,
        links,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linear_ticket_and_japanese_text_with_exact_utf8_byte_ranges() {
        let parsed = parse_inline_links(
            "[ENG-123](https://linear.app/example/issue/ENG-123) 日本語タスク [PR #42](https://github.com/org/repo/pull/42)",
        );
        assert_eq!(parsed.display_text, "ENG-123 日本語タスク PR #42");
        assert_eq!(parsed.links.len(), 2);
        assert_eq!(&parsed.display_text[parsed.links[0].range.clone()], "ENG-123");
        assert_eq!(parsed.links[0].url, "https://linear.app/example/issue/ENG-123");
        assert_eq!(&parsed.display_text[parsed.links[1].range.clone()], "PR #42");
        assert_eq!(parsed.links[1].url, "https://github.com/org/repo/pull/42");
    }

    #[test]
    fn rejects_unsafe_or_malformed_links_and_keeps_plain_text() {
        let unsafe_text =
            "[XSS](javascript:alert(1)) [Local](file:///etc/passwd) [WIP] [broken](https://a.com";
        let parsed = parse_inline_links(unsafe_text);
        assert_eq!(parsed.display_text, unsafe_text);
        assert!(parsed.links.is_empty());
    }

    #[test]
    fn supports_linear_and_slack_deep_link_schemes_and_balanced_parens_in_url() {
        let parsed = parse_inline_links(
            "[ENG-9](linear://example/issue/ENG-9) [#general](slack://channel?team=T123&id=C456) [Wiki](https://en.wikipedia.org/wiki/Rust_(programming_language))",
        );
        assert_eq!(parsed.display_text, "ENG-9 #general Wiki");
        assert_eq!(parsed.links.len(), 3);
        assert_eq!(parsed.links[0].url, "linear://example/issue/ENG-9");
        assert_eq!(parsed.links[1].url, "slack://channel?team=T123&id=C456");
        assert_eq!(
            parsed.links[2].url,
            "https://en.wikipedia.org/wiki/Rust_(programming_language)"
        );
        assert!(is_openable_url("slack://open?team=T123"));
        assert!(!is_openable_url("slack://"));
        assert!(!is_openable_url("slack:///invalid"));
    }
}
