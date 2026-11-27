use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=AFTERLIFE_SM_WASM_PATH");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir.clone());
    let target_dir = env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_root.join("target"));

    let candidates = [
        env::var("AFTERLIFE_SM_WASM_PATH").ok().map(PathBuf::from),
        Some(target_dir.join("wasm32-unknown-unknown/wasm-release/afterlife_sm_wasm.wasm")),
        Some(target_dir.join("wasm32-unknown-unknown/release/afterlife_sm_wasm.wasm")),
    ];

    for c in candidates.iter().flatten() {
        println!("cargo:rerun-if-changed={}", c.display());
    }

    let found = candidates.into_iter().flatten().find(|p| p.is_file());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let generated = match found {
        Some(path) => format!(
            "pub const EMBEDDED_SM_WASM: Option<&[u8]> = Some(include_bytes!({:?}));\n",
            path.display().to_string()
        ),
        None => "pub const EMBEDDED_SM_WASM: Option<&[u8]> = None;\n".to_string(),
    };
    fs::write(out_dir.join("sm_wasm_embed.rs"), generated).unwrap();
}
