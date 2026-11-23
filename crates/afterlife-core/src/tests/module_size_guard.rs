use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const DEFAULT_MAX_LINES: usize = 650;

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

fn check_line_budgets(counts: &BTreeMap<String, usize>, max_lines: usize) -> Vec<String> {
    let mut violations = Vec::new();
    for (rel, &actual) in counts {
        if actual > max_lines {
            violations.push(format!(
                "{rel}: {actual} lines exceeds limit {max_lines} (split responsibilities into focused modules instead of appending)"
            ));
        }
    }
    violations
}

#[test]
fn first_party_rust_modules_respect_size_budget_without_legacy_exceptions() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("resolve workspace root");

    let mut files = Vec::new();
    collect_rust_files(&workspace_root.join("crates"), &mut files);
    collect_rust_files(&workspace_root.join("tools"), &mut files);
    assert!(!files.is_empty());

    let mut counts = BTreeMap::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed reading {}: {e}", path.display()));
        let rel = path
            .strip_prefix(workspace_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        counts.insert(rel, src.lines().count());
    }

    let violations = check_line_budgets(&counts, DEFAULT_MAX_LINES);
    assert!(
        violations.is_empty(),
        "Module size budget violation(s) detected:\n{}",
        violations.join("\n")
    );
}

#[test]
fn size_guard_catches_oversized_files() {
    let mut counts = BTreeMap::new();
    counts.insert("crates/foo/src/ok.rs".to_string(), 650);
    counts.insert("crates/foo/src/oversized.rs".to_string(), 651);

    let violations = check_line_budgets(&counts, DEFAULT_MAX_LINES);
    assert_eq!(violations.len(), 1, "unexpected violations: {violations:?}");
    assert!(violations[0].contains("oversized.rs"));
}
