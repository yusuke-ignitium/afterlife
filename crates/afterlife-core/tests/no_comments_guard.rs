use std::{
    fs,
    path::{Path, PathBuf},
};

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|x| x.path())).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn try_skip_raw_string(bytes: &[u8], i: usize) -> Option<(usize, usize)> {
    let len = bytes.len();
    let mut j = i;
    if j < len && (bytes[j] == b'b' || bytes[j] == b'c') {
        j += 1;
    }
    if j >= len || bytes[j] != b'r' {
        return None;
    }
    if i > 0 && is_ident_continue(bytes[i - 1]) {
        return None;
    }
    j += 1;
    let mut hashes = 0usize;
    while j < len && bytes[j] == b'#' {
        hashes += 1;
        j += 1;
    }
    if j >= len || bytes[j] != b'"' {
        return None;
    }
    j += 1;
    let mut newlines = 0usize;
    while j < len {
        if bytes[j] == b'\n' {
            newlines += 1;
        }
        if bytes[j] == b'"' {
            let mut k = j + 1;
            let mut matched = 0usize;
            while matched < hashes && k < len && bytes[k] == b'#' {
                matched += 1;
                k += 1;
            }
            if matched == hashes {
                return Some((k, newlines));
            }
        }
        j += 1;
    }
    Some((len, newlines))
}

fn try_skip_char_literal(bytes: &[u8], i: usize) -> Option<usize> {
    let len = bytes.len();
    if bytes[i] != b'\'' {
        return None;
    }
    if i + 2 < len && bytes[i + 1] == b'\\' {
        let mut j = i + 2;
        while j < len && bytes[j] != b'\'' && bytes[j] != b'\n' {
            j += 1;
        }
        if j < len && bytes[j] == b'\'' {
            return Some(j + 1);
        }
        return None;
    }
    let mut j = i + 1;
    if j < len && bytes[j] != b'\'' && bytes[j] != b'\n' {
        while j < len && (bytes[j] & 0b1100_0000) == 0b1000_0000 {
            j += 1;
        }
        j += 1;
        while j < len && (bytes[j] & 0b1100_0000) == 0b1000_0000 {
            j += 1;
        }
        if j < len && bytes[j] == b'\'' {
            return Some(j + 1);
        }
    }
    None
}

fn find_comment_lines(src: &str) -> Vec<(usize, &'static str)> {
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut hits = Vec::new();

    while i < len {
        if let Some((next_i, nl)) = try_skip_raw_string(bytes, i) {
            line += nl;
            i = next_i;
            continue;
        }

        match bytes[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'"' => {
                i += 1;
                while i < len {
                    match bytes[i] {
                        b'\\' => i = (i + 2).min(len),
                        b'"' => {
                            i += 1;
                            break;
                        }
                        b'\n' => {
                            line += 1;
                            i += 1;
                        }
                        _ => i += 1,
                    }
                }
            }
            b'\'' => {
                if let Some(next_i) = try_skip_char_literal(bytes, i) {
                    i = next_i;
                } else {
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                hits.push((line, "//"));
                i += 2;
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                hits.push((line, "/*"));
                i += 2;
                let mut depth = 1usize;
                while i < len && depth > 0 {
                    if bytes[i] == b'\n' {
                        line += 1;
                        i += 1;
                    } else if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        depth += 1;
                        i += 2;
                    } else if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            _ => i += 1,
        }
    }

    hits
}

#[test]
fn first_party_rust_sources_contain_zero_comments() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("resolve workspace root");

    let mut files = Vec::new();
    collect_rust_files(&workspace_root.join("crates"), &mut files);
    collect_rust_files(&workspace_root.join("tools"), &mut files);
    assert!(!files.is_empty());

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed reading {}: {e}", path.display()));
        let hits = find_comment_lines(&src);
        if !hits.is_empty() {
            let rel = path.strip_prefix(workspace_root).unwrap_or(&path);
            for (ln, kind) in hits {
                violations.push(format!("{}:{ln} ({kind})", rel.display()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Code comments are strictly forbidden in first-party Rust sources (found {}):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn lexer_ignores_comment_tokens_inside_literals_and_catches_real_comments() {
    let clean = concat!(
        "fn demo<'a>() -> &'a str {\n",
        "    let url = \"http://127.0.0.1:4319/mcp /* not a comment */\";\n",
        "    let raw = r#\"// raw string /* also ignored */\"#;\n",
        "    let slash = '/';\n",
        "    let quote = '\\'';\n",
        "    let _ = (url, raw, slash, quote);\n",
        "    \"ok\"\n",
        "}\n"
    );
    assert!(find_comment_lines(clean).is_empty());

    let dirty = concat!(
        "fn a() {\n",
        "    // line comment\n",
        "    /// doc comment\n",
        "    /* block /* nested */ comment\n",
        "       spanning lines */\n",
        "    let x = 1; // trailing\n",
        "}\n"
    );
    assert_eq!(
        find_comment_lines(dirty),
        vec![(2, "//"), (3, "//"), (4, "/*"), (6, "//")]
    );
}
