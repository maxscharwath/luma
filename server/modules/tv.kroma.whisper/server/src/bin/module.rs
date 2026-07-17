//! The Whisper module as a standalone process (its `.kmod` entrypoint).
//!
//! A port-provider-only sidecar: it serves transcription over the port bridge
//! (`/_port/whisper/transcribe`). The heavy candle inference (and its Metal/CUDA
//! deps + model downloads) live here, out of the core process. Progress + cancel
//! flow through a shared `whisper_jobs` DB row (see serve.rs).

use kroma_module_runtime::RemoteHost;
use kroma_module_sdk::host::HostCtx;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    kroma_module_runtime::serve(
        move |host| {
            // Make sure the coordination table exists before the core writes to it.
            kroma_whisper::ensure_jobs_table(host.db());
        },
        vec![],
        kroma_whisper::whisper_routes::<RemoteHost>(),
    )
    .await
}
