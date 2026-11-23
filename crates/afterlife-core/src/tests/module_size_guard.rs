use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const DEFAULT_MAX_LINES: usize = 650;

const LEGACY_RATCHET_LIMITS: &[(&str, usize)] = &[
    ("crates/afterlife-core/src/engine/mod.rs", 1685),
    ("crates/afterlife-core/src/fixtures.rs", 1960),
    ("crates/afterlife-daemon/src/auth.rs", 955),
    ("crates/afterlife-daemon/src/lib.rs", 920),
    ("crates/afterlife-daemon/tests/e2e_human_mcp.rs", 1535),
    ("crates/afterlife-protocol/src/backdrop.rs", 900),
    ("crates/afterlife-ui/src/platform/apple_music.rs", 1015),
    ("tools/demos/src/bin/detail_overlap_demo.rs", 715),
    ("tools/demos/src/bin/palette_demo.rs", 665),
    ("tools/demos/src/bin/step_history_demo.rs", 1135),
];

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

fn check_line_budgets(
    counts: &BTreeMap<String, usize>,
    default_max: usize,
    ratchets: &[(&str, usize)],
) -> Vec<String> {
    let mut ratchet_map = BTreeMap::new();
    let mut violations = Vec::new();
    for &(rel, cap) in ratchets {
        if ratchet_map.insert(rel, cap).is_some() {
            violations.push(format!("duplicate ratchet entry for {rel}"));
        }
        if cap <= default_max {
            violations.push(format!(
                "ratchet entry {rel} ({cap}) must exceed default max ({default_max})"
            ));
        }
        match counts.get(rel) {
            None => violations.push(format!(
                "stale ratchet entry {rel}: file no longer exists; remove it from LEGACY_RATCHET_LIMITS"
            )),
            Some(&actual) if actual <= default_max => violations.push(format!(
                "stale ratchet entry {rel}: actual {actual} <= default max {default_max}; remove it to tighten the ratchet"
            )),
            Some(_) => {}
        }
    }

    for (rel, &actual) in counts {
        let limit = ratchet_map.get(rel.as_str()).copied().unwrap_or(default_max);
        if actual > limit {
            violations.push(format!(
                "{rel}: {actual} lines exceeds limit {limit} (split responsibilities into focused modules instead of appending)"
            ));
        }
    }
    violations
}

#[test]
fn first_party_rust_modules_respect_size_budget_and_ratchet() {
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

    let violations = check_line_budgets(&counts, DEFAULT_MAX_LINES, LEGACY_RATCHET_LIMITS);
    assert!(
        violations.is_empty(),
        "Module size budget violation(s) detected:\n{}",
        violations.join("\n")
    );
}

#[test]
fn size_guard_catches_oversized_files_regrowth_and_stale_ratchets() {
    let mut counts = BTreeMap::new();
    counts.insert("crates/foo/src/normal.rs".to_string(), 700);
    counts.insert("crates/foo/src/legacy_regrown.rs".to_string(), 901);
    counts.insert("crates/foo/src/legacy_shrunk.rs".to_string(), 400);

    let ratchets = &[
        ("crates/foo/src/legacy_regrown.rs", 900),
        ("crates/foo/src/legacy_shrunk.rs", 800),
        ("crates/foo/src/deleted.rs", 850),
    ];

    let violations = check_line_budgets(&counts, 650, ratchets);
    assert_eq!(violations.len(), 4, "unexpected violations: {violations:?}");
    assert!(violations.iter().any(|v| v.contains("normal.rs")));
    assert!(violations.iter().any(|v| v.contains("legacy_regrown.rs")));
    assert!(violations.iter().any(|v| v.contains("legacy_shrunk.rs")));
    assert!(violations.iter().any(|v| v.contains("deleted.rs")));
}
