//! The Whisper module as a standalone process (its `.lmod` entrypoint).
//!
//! A port-provider-only sidecar: it serves transcription over the port bridge
//! (`/_port/whisper/transcribe`). The heavy candle inference (and its Metal/CUDA
//! deps + model downloads) live here, out of the core process. Progress + cancel
//! flow through a shared `whisper_jobs` DB row (see serve.rs).

use luma_module_runtime::RemoteHost;
use luma_module_sdk::host::HostCtx;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    luma_module_runtime::serve(
        move |host| {
            // Make sure the coordination table exists before the core writes to it.
            luma_whisper::ensure_jobs_table(host.db());
        },
        vec![],
        luma_whisper::whisper_routes::<RemoteHost>(),
    )
    .await
}
