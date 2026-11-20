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

#[test]
fn overflow_fade_calls_use_fade_px_tokens() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rust_files(&src_dir, &mut files);
    assert!(!files.is_empty());

    let mut typed_calls = 0usize;
    let mut raw_calls = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed reading {}: {e}", path.display()));
        for (idx, line) in src.lines().enumerate() {
            if line.contains(".overflow_fade_b(") {
                raw_calls.push(format!("{}:{}: {}", path.display(), idx + 1, line.trim()));
            }
            if line.contains(".overflow_fade_bottom(") {
                typed_calls += 1;
            }
        }
    }

    assert!(typed_calls >= 3);
    assert!(
        raw_calls.is_empty(),
        "use typed ScrollFadeExt::overflow_fade_bottom(FadePx) instead of raw overflow_fade_b:\n{}",
        raw_calls.join("\n")
    );
}
