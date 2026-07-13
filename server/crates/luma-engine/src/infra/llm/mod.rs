//! Pluggable LLM client the small text model behind LUMA's "smart" features
//! (personalized, auto-named home sections; an evolving per-user taste profile).
//!
//! LUMA is open source and self-hosted, so the user chooses *which* model and
//! *where* it runs. Backends sit behind one [`LlmClient`] trait, mirroring the
//! embedder-port design (see [`crate::ports::Embedder`]):
//!   * [`http`] any **OpenAI-compatible** server (Ollama, llama.cpp, LM Studio,
//!     vLLM, OpenRouter, …) **or** the **Anthropic** Messages API (Claude). Shells
//!     out to `curl`, exactly like the TMDB client no heavy HTTP dep.
//!
//! The task ("name this cluster of titles" / "summarize this taste") is tiny, so
//! a small model is enough Qwen2.5-0.5B/1.5B on Ollama, or Claude Haiku. It runs
//! in the nightly `sections.personalize` job, so even slow CPU inference is fine.

use std::sync::Arc;

use crate::services::settings::Settings;

mod http;
mod tools;

pub use http::list_models;
pub use tools::{ToolBox, ToolDef};

/// Build a one-off HTTP client from explicit config (the admin Test / Load-models
/// endpoints, which probe values the admin is still editing before they're
/// saved). `None` when the config can't form a usable client.
pub fn build_http(
    provider: &str,
    base_url: &str,
    model: &str,
    api_key: &str,
    temperature: f32,
    reasoning: bool,
) -> Option<Arc<dyn LlmClient>> {
    http::HttpLlm::from_config(provider, base_url, model, api_key, temperature, reasoning)
        .map(|c| Arc::new(c) as Arc<dyn LlmClient>)
}

/// A text-generation backend. Implementations are cheap to clone via `Arc`.
pub trait LlmClient: Send + Sync {
    /// Whether the client is configured and usable (a real endpoint/model).
    fn available(&self) -> bool;

    /// Run a single completion: a system instruction + a user message in, the
    /// assistant's text out. `max_tokens` caps the reply. Blocking (network /
    /// CPU) call from a blocking context (the job runs on `spawn_blocking`).
    fn complete(&self, system: &str, user: &str, max_tokens: u32) -> anyhow::Result<String>;

    /// Whether this client can run the agentic [`run_tools`](LlmClient::run_tools)
    /// loop (function calling). `false` clients only do [`complete`]; tool-driven
    /// features should check this and fall back to a prompt path.
    fn supports_tools(&self) -> bool {
        false
    }

    /// Agentic tool loop: hand the model `tools`, dispatch each requested call
    /// through `toolbox`, feed results back, and repeat up to `max_steps` until
    /// the model produces a final answer (returned as text). Errors including
    /// "unsupported" so callers can fall back to a non-tool path. Blocking.
    fn run_tools(
        &self,
        system: &str,
        user: &str,
        tools: &[ToolDef],
        toolbox: &dyn ToolBox,
        max_tokens: u32,
        max_steps: usize,
    ) -> anyhow::Result<String> {
        let _ = (system, user, tools, toolbox, max_tokens, max_steps);
        anyhow::bail!("this LLM client does not support tool calling")
    }

    /// Short human description for logs (`"openai qwen2.5:1.5b @ …"`).
    fn describe(&self) -> String;
}

/// Build the configured client from settings. Returns a [`Disabled`] client when
/// the feature is off or unconfigured, so callers can always call `complete` and
/// just check `available()` first. With more than one configured provider it
/// returns a [`Failover`] that tries the default first, then the rest so a
/// primary that's out of credits / rate-limited / down degrades to a secondary
/// (e.g. cloud Claude → local Ollama) transparently.
pub fn from_settings(settings: &Settings) -> Arc<dyn LlmClient> {
    if !settings.get_bool("llmEnabled", false) {
        return Arc::new(Disabled);
    }
    let clients: Vec<Arc<dyn LlmClient>> = crate::services::settings::ordered_providers(settings)
        .iter()
        .filter_map(|p| {
            http::HttpLlm::from_config(&p.provider, p.base_url.trim(), p.model.trim(), p.api_key.trim(), p.temperature, p.reasoning)
                .map(|c| Arc::new(c) as Arc<dyn LlmClient>)
        })
        .collect();
    match clients.len() {
        0 => Arc::new(Disabled),
        1 => clients.into_iter().next().expect("one client"),
        _ => Arc::new(Failover { clients }),
    }
}

/// Tries each configured provider in order (default first), falling through to
/// the next on error resilience against a primary that's out of credits,
/// rate-limited, or down. Per-provider failures are logged (server tracing); the
/// caller only sees the first success or, if all fail, the last error.
struct Failover {
    clients: Vec<Arc<dyn LlmClient>>,
}

impl LlmClient for Failover {
    fn available(&self) -> bool {
        self.clients.iter().any(|c| c.available())
    }

    fn supports_tools(&self) -> bool {
        self.clients.iter().any(|c| c.supports_tools())
    }

    fn complete(&self, system: &str, user: &str, max_tokens: u32) -> anyhow::Result<String> {
        let mut last = None;
        for c in &self.clients {
            match c.complete(system, user, max_tokens) {
                Ok(s) => return Ok(s),
                Err(e) => {
                    tracing::warn!(provider = %c.describe(), error = %e, "LLM provider failed; trying next");
                    last = Some(e);
                }
            }
        }
        Err(last.unwrap_or_else(|| anyhow::anyhow!("no LLM provider available")))
    }

    fn run_tools(
        &self,
        system: &str,
        user: &str,
        tools: &[ToolDef],
        toolbox: &dyn ToolBox,
        max_tokens: u32,
        max_steps: usize,
    ) -> anyhow::Result<String> {
        let mut last = None;
        // Only tool-capable providers; the caller falls back to `complete` if
        // every tool attempt fails.
        for c in self.clients.iter().filter(|c| c.supports_tools()) {
            match c.run_tools(system, user, tools, toolbox, max_tokens, max_steps) {
                Ok(s) => return Ok(s),
                Err(e) => {
                    tracing::warn!(provider = %c.describe(), error = %e, "LLM tool run failed; trying next");
                    last = Some(e);
                }
            }
        }
        Err(last.unwrap_or_else(|| anyhow::anyhow!("no tool-capable LLM provider available")))
    }

    fn describe(&self) -> String {
        let chain: Vec<String> = self.clients.iter().map(|c| c.describe()).collect();
        format!("failover[{}]", chain.join(" → "))
    }
}

/// The no-op client used when no LLM is configured. `available()` is false and
/// `complete()` errors, so dependent features degrade gracefully (the home falls
/// back to the static themed-row bank).
pub struct Disabled;

impl LlmClient for Disabled {
    fn available(&self) -> bool {
        false
    }
    fn complete(&self, _system: &str, _user: &str, _max_tokens: u32) -> anyhow::Result<String> {
        anyhow::bail!("no LLM configured (enable one under Admin → settings)")
    }
    fn describe(&self) -> String {
        "disabled".to_string()
    }
}
