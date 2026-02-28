use std::process::Command;

fn main() {
    // Get the git commit count for the build code.
    // Falls back to "0" if git is unavailable or not a git repo.
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

