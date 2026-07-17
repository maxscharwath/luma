//! The Vector (embeddings) module as a standalone process (its `.kmod`
//! entrypoint).
//!
//! A port-provider-only sidecar: it hosts no admin routes and no `ServerModule`,
//! it just serves the embedder over the port bridge (`/_port/embedder/*`). The
//! core resolves an `EmbedderClient` proxy to it and uses `embed_batch` for the
//! catalog-wide reembed pass, so the heavy MiniLM/candle model (with `semantic`)
//! or the lexical backend runs out-of-process without slowing the core.

use kroma_module_runtime::RemoteHost;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Build the embedder ONCE (the semantic backend loads a ~25MB model), then
    // serve it. No modules, no consumed ports: purely a provider process.
    let embedder = kroma_vector::default_embedder();
    kroma_module_runtime::serve(
        move |_host| {},
        vec![],
        kroma_vector::embedder_routes::<RemoteHost>(embedder),
    )
    .await
}
