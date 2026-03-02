use std::collections::HashMap;
use std::process::Command;

fn main() {
    // ── Read version from build_constants.toml ──────────────────────────
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..");
    let constants_path = workspace_root.join("build_constants.toml");
    let constants_text = std::fs::read_to_string(&constants_path)
        .expect("failed to read build_constants.toml");
    let table: HashMap<String, toml::Value> =
        toml::from_str(&constants_text).expect("failed to parse build_constants.toml");
    let ver = table
        .get("version")
        .and_then(|v| v.as_table())
        .expect("build_constants.toml missing [version] table");
    let major = ver.get("major").and_then(|v| v.as_integer()).expect("missing version.major");
    let minor = ver.get("minor").and_then(|v| v.as_integer()).expect("missing version.minor");
    let patch = ver.get("patch").and_then(|v| v.as_integer()).expect("missing version.patch");
    let version = format!("{}.{}.{}", major, minor, patch);
    println!("cargo:rustc-env=RIDGEBACK_VERSION={}", version);

    // ── Sync workspace Cargo.toml version from build_constants.toml ─────
    // This keeps the workspace [workspace.package].version in lockstep
    // so that `cargo metadata` and Cargo.lock always reflect the true
    // version even if someone forgets to run the sync script.
    let workspace_cargo = workspace_root.join("Cargo.toml");
    if let Ok(cargo_text) = std::fs::read_to_string(&workspace_cargo) {
        let expected = format!("version = \"{}\"", version);
        // Only write if the version line actually differs (avoids
        // unnecessary rebuilds / Cargo re-resolves).
        let needle = "version = \"";
        if let Some(start) = cargo_text.find("[workspace.package]") {
            let section = &cargo_text[start..];
            if let Some(vstart) = section.find(needle) {
                let abs = start + vstart;
                let end = cargo_text[abs..].find('\n').map(|i| abs + i).unwrap_or(cargo_text.len());
                let current_line = cargo_text[abs..end].trim();
                if current_line != expected {
                    let mut new_text = String::with_capacity(cargo_text.len());
                    new_text.push_str(&cargo_text[..abs]);
                    new_text.push_str(&expected);
                    new_text.push_str(&cargo_text[end..]);
                    let _ = std::fs::write(&workspace_cargo, new_text);
                }
            }
        }
    }

    // Re-run when the constants file changes.
    println!("cargo:rerun-if-changed=../../build_constants.toml");

    // ── Git commit count for build code ─────────────────────────────────
    let commit_count = Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "0".to_string());

    println!("cargo:rustc-env=RIDGEBACK_COMMIT_COUNT={}", commit_count);

    // Re-run if the git HEAD changes (new commits).
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/");
}

