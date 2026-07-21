// Expose the compile target triple to the binary (env!("KROMA_BUILD_TARGET")):
// the module store uses it at runtime to pick the matching per-target `.kmod`
// artifact from a registry catalog (a sidecar module carries a NATIVE binary,
// so its platform must match this server's).
fn main() {
    println!(
        "cargo:rustc-env=KROMA_BUILD_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
    // Short git commit hash for the admin "Version installée" row (env!(
    // "KROMA_GIT_HASH")). Best effort: a source tarball with no .git, or git
    // missing, yields "unknown". Re-run when HEAD moves so the hash stays fresh.
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=KROMA_GIT_HASH={commit}");

    // UTC build date for the admin "Version installée" row. `date -u` exists on
    // the Linux/macOS build hosts; anything else falls back to "unknown".
    let date = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%d %H:%M UTC"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=KROMA_BUILD_DATE={date}");
    println!("cargo:rerun-if-changed=../.git/HEAD");
}
