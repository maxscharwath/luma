// Expose the compile target triple to the binary (env!("LUMA_BUILD_TARGET")):
// the module store uses it at runtime to pick the matching per-target `.lmod`
// artifact from a registry catalog (a sidecar module carries a NATIVE binary,
// so its platform must match this server's).
fn main() {
    println!(
        "cargo:rustc-env=LUMA_BUILD_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
}
