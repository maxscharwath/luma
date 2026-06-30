//! HTTP LLM backend over `curl` (no heavy HTTP dependency same approach as the
//! TMDB client). The wire differences between vendors live behind one
//! [`Provider`] trait, so [`HttpLlm`] is provider-agnostic and adding a new
//! vendor is a single self-contained `impl` + one line in [`provider_for`].
//!
//! Shipped providers:
//!   * **OpenAI-compatible** `POST {base}/chat/completions` Ollama (base
//!     `http://host:11434/v1`), llama.cpp, LM Studio, vLLM, OpenRouter, OpenAI.
//!   * **Anthropic** `POST {base}/v1/messages` (`x-api-key` +
//!     `anthropic-version`) Claude.

use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::tools::{ToolBox, ToolCall, ToolDef, Turn};
use super::LlmClient;

const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Network budget for one completion (LLMs can be slow, especially local CPU).
const MAX_TIME_SECS: &str = "180";

/// A chat-completion wire protocol. Implement this + add a line to
/// [`provider_for`] to support a new vendor nothing else changes.
trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    /// Base URL used when the admin leaves it blank; `None` means a base URL is
    /// required (the OpenAI-compatible case there's no single default host).
    fn default_base(&self) -> Option<&'static str>;
    fn chat_url(&self, base: &str) -> String;
    fn models_url(&self, base: &str) -> String;
    fn headers(&self, api_key: &str) -> Vec<(&'static str, String)>;
    fn chat_body(&self, model: &str, system: &str, user: &str, max_tokens: u32, temperature: f32, reasoning: bool) -> Value;
    fn parse_reply(&self, v: &Value) -> Result<String>;
    /// Whether the `reasoning` flag actually changes the request (→ a
    /// reasoning-off retry is worth attempting when a model rejects it).
    fn reasoning_applies(&self) -> bool {
        false
    }

    // ----- function calling (default: unsupported) ----------------------------

    /// Whether this provider can do tool/function calling.
    fn supports_tools(&self) -> bool {
        false
    }
    /// Build a tool-enabled chat request from a running `messages` array (the
    /// conversation so far user/assistant/tool turns; `system` is applied by
    /// the provider as it sees fit). Mirrors the vendor request shape, hence the
    /// arg count.
    #[allow(clippy::too_many_arguments)]
    fn tools_request(
        &self,
        model: &str,
        system: &str,
        messages: &[Value],
        tools: &[ToolDef],
        max_tokens: u32,
        temperature: f32,
        reasoning: bool,
    ) -> Value {
        let _ = (model, system, messages, tools, max_tokens, temperature, reasoning);
        Value::Null
    }
    /// Parse one assistant turn: final text (if any), requested tool calls, and
    /// the raw assistant message to echo back into the next request.
    fn parse_turn(&self, v: &Value) -> Result<Turn> {
        let _ = v;
        bail!("provider does not support tool calling")
    }
    /// Build the conversation message(s) carrying tool results back to the model.
    /// OpenAI returns one `{role:tool}` message per call; Anthropic returns a
    /// single `{role:user}` message holding all `tool_result` blocks.
    fn tool_result_messages(&self, results: &[(ToolCall, String)]) -> Vec<Value> {
        let _ = results;
        Vec::new()
    }
}

/// OpenAI tool shape: `{type:"function", function:{name, description, parameters}}`.
fn openai_tool(t: &ToolDef) -> Value {
    json!({
        "type": "function",
        "function": { "name": t.name, "description": t.description, "parameters": t.schema },
    })
}

/// Anthropic tool shape: `{name, description, input_schema}`.
fn anthropic_tool(t: &ToolDef) -> Value {
    json!({ "name": t.name, "description": t.description, "input_schema": t.schema })
}

/// Resolve a provider id to its wire protocol. Unknown ids fall back to the
/// lenient OpenAI-compatible default.
fn provider_for(name: &str) -> Box<dyn Provider> {
    match name.trim().to_ascii_lowercase().as_str() {
        "anthropic" | "claude" => Box::new(Anthropic),
        "openrouter" => Box::new(OpenAi::openrouter()),
        _ => Box::new(OpenAi::openai()),
    }
}

// ----- OpenAI-compatible ------------------------------------------------------

/// The OpenAI chat-completions wire protocol also serves every compatible
/// server (Ollama, llama.cpp, LM Studio, vLLM) and **OpenRouter**. OpenRouter is
/// the *same* wire format; only its default base URL and an optional `X-Title`
/// ranking header differ, so it's a config of this one `impl` rather than a
/// near-duplicate new trait methods can't silently regress on it.
struct OpenAi {
    id: &'static str,
    default_base: Option<&'static str>,
    /// An extra header sent on every request (OpenRouter's `X-Title`), or none.
    extra_header: Option<(&'static str, &'static str)>,
}

// `openai()` deliberately mirrors the variant name alongside `openrouter()`.
#[allow(clippy::self_named_constructors)]
impl OpenAi {
    /// Generic OpenAI-compatible endpoint: no universal host (base URL required).
    const fn openai() -> Self {
        Self { id: "openai", default_base: None, extra_header: None }
    }
    /// OpenRouter (<https://openrouter.ai>) an aggregator giving one key access
    /// to hundreds of models; identifies LUMA on its usage dashboard via `X-Title`.
    const fn openrouter() -> Self {
        Self { id: "openrouter", default_base: Some("https://openrouter.ai/api/v1"), extra_header: Some(("x-title", "LUMA")) }
    }
}

impl Provider for OpenAi {
    fn id(&self) -> &'static str {
        self.id
    }
    fn default_base(&self) -> Option<&'static str> {
        self.default_base
    }
    fn chat_url(&self, base: &str) -> String {
        format!("{base}/chat/completions")
    }
    fn models_url(&self, base: &str) -> String {
        format!("{base}/models")
    }
    fn headers(&self, api_key: &str) -> Vec<(&'static str, String)> {
        let mut h = vec![("content-type", "application/json".to_string())];
        if !api_key.is_empty() {
            h.push(("authorization", format!("Bearer {api_key}")));
        }
        if let Some((k, v)) = self.extra_header {
            h.push((k, v.to_string()));
        }
        h
    }
    fn chat_body(&self, model: &str, system: &str, user: &str, max_tokens: u32, temperature: f32, _reasoning: bool) -> Value {
        json!({
            "model": model,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": false,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
        })
    }
    fn parse_reply(&self, v: &Value) -> Result<String> {
        v.pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow!("OpenAI response missing choices[0].message.content"))
    }
    fn supports_tools(&self) -> bool {
        true
    }
    fn tools_request(
        &self,
        model: &str,
        system: &str,
        messages: &[Value],
        tools: &[ToolDef],
        max_tokens: u32,
        temperature: f32,
        _reasoning: bool,
    ) -> Value {
        // System is a leading message; the running turns follow.
        let mut msgs = Vec::with_capacity(messages.len() + 1);
        msgs.push(json!({ "role": "system", "content": system }));
        msgs.extend(messages.iter().cloned());
        json!({
            "model": model,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": false,
            "messages": msgs,
            "tools": tools.iter().map(openai_tool).collect::<Vec<_>>(),
        })
    }
    fn parse_turn(&self, v: &Value) -> Result<Turn> {
        let msg = v
            .pointer("/choices/0/message")
            .ok_or_else(|| anyhow!("OpenAI response missing choices[0].message"))?;
        let text = msg
            .get("content")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let mut calls = Vec::new();
        if let Some(tcs) = msg.get("tool_calls").and_then(Value::as_array) {
            for tc in tcs {
                let id = tc.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                let f = tc.get("function").ok_or_else(|| anyhow!("OpenAI tool_call missing function"))?;
                let name = f.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                // Spec says `arguments` is a JSON-encoded string, but several
                // OpenAI-compatible servers (Ollama, llama.cpp, LM Studio) hand
                // back an object instead accept both; empty/garbage → `{}`.
                let args = match f.get("arguments") {
                    Some(Value::String(s)) if !s.trim().is_empty() => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
                    Some(obj @ Value::Object(_)) => obj.clone(),
                    _ => json!({}),
                };
                calls.push(ToolCall { id, name, args });
            }
        }
        Ok(Turn { text, tool_calls: calls, assistant_msg: msg.clone() })
    }
    fn tool_result_messages(&self, results: &[(ToolCall, String)]) -> Vec<Value> {
        results
            .iter()
            .map(|(call, out)| json!({ "role": "tool", "tool_call_id": call.id, "content": out }))
            .collect()
    }
}

// ----- Anthropic --------------------------------------------------------------

struct Anthropic;

impl Provider for Anthropic {
    fn id(&self) -> &'static str {
        "anthropic"
    }
    fn default_base(&self) -> Option<&'static str> {
        Some("https://api.anthropic.com")
    }
    fn chat_url(&self, base: &str) -> String {
        format!("{base}/v1/messages")
    }
    fn models_url(&self, base: &str) -> String {
        format!("{base}/v1/models")
    }
    fn headers(&self, api_key: &str) -> Vec<(&'static str, String)> {
        vec![
            ("content-type", "application/json".to_string()),
            ("x-api-key", api_key.to_string()),
            ("anthropic-version", ANTHROPIC_VERSION.to_string()),
        ]
    }
    fn chat_body(&self, model: &str, system: &str, user: &str, max_tokens: u32, _temperature: f32, reasoning: bool) -> Value {
        // `system` is top-level; temperature is omitted (modern Claude rejects it).
        let mut body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": [{"role": "user", "content": user}],
        });
        if reasoning {
            body["thinking"] = json!({ "type": "adaptive" });
        }
        body
    }
    fn parse_reply(&self, v: &Value) -> Result<String> {
        // Safety classifiers can decline with a 200 + stop_reason "refusal".
        if v.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
            bail!("Anthropic declined the request (stop_reason=refusal)");
        }
        v.get("content")
            .and_then(Value::as_array)
            .and_then(|blocks| {
                blocks
                    .iter()
                    .find(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                    .and_then(|b| b.get("text").and_then(Value::as_str))
            })
            .map(str::to_string)
            .ok_or_else(|| anyhow!("Anthropic response had no text block"))
    }
    fn reasoning_applies(&self) -> bool {
        true
    }
    fn supports_tools(&self) -> bool {
        true
    }
    fn tools_request(
        &self,
        model: &str,
        system: &str,
        messages: &[Value],
        tools: &[ToolDef],
        max_tokens: u32,
        _temperature: f32,
        reasoning: bool,
    ) -> Value {
        let mut body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": messages,
            "tools": tools.iter().map(anthropic_tool).collect::<Vec<_>>(),
        });
        if reasoning {
            body["thinking"] = json!({ "type": "adaptive" });
        }
        body
    }
    fn parse_turn(&self, v: &Value) -> Result<Turn> {
        if v.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
            bail!("Anthropic declined the request (stop_reason=refusal)");
        }
        // Echo the assistant content array back verbatim (preserves thinking
        // blocks + their signatures, which the API requires on the next turn).
        let content = v.get("content").and_then(Value::as_array).cloned().unwrap_or_default();
        let mut text = None;
        let mut calls = Vec::new();
        for block in &content {
            match block.get("type").and_then(Value::as_str) {
                Some("text") if text.is_none() => {
                    text = block.get("text").and_then(Value::as_str).map(str::to_string);
                }
                Some("tool_use") => {
                    let id = block.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                    let name = block.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                    let args = block.get("input").cloned().unwrap_or_else(|| json!({}));
                    calls.push(ToolCall { id, name, args });
                }
                _ => {}
            }
        }
        Ok(Turn { text, tool_calls: calls, assistant_msg: json!({ "role": "assistant", "content": content }) })
    }
    fn tool_result_messages(&self, results: &[(ToolCall, String)]) -> Vec<Value> {
        let blocks: Vec<Value> = results
            .iter()
            .map(|(call, out)| json!({ "type": "tool_result", "tool_use_id": call.id, "content": out }))
            .collect();
        vec![json!({ "role": "user", "content": blocks })]
    }
}

// ----- the client -------------------------------------------------------------

/// A configured HTTP LLM endpoint (provider + resolved base + model + params).
pub struct HttpLlm {
    provider: Box<dyn Provider>,
    base: String,
    model: String,
    api_key: String,
    temperature: f32,
    reasoning: bool,
}

impl HttpLlm {
    /// Build from settings; `None` when not enough config to be usable (no model,
    /// or an OpenAI-compatible provider with no base URL).
    pub fn from_config(
        provider: &str,
        base_url: &str,
        model: &str,
        api_key: &str,
        temperature: f32,
        reasoning: bool,
    ) -> Option<Self> {
        let model = model.trim();
        if model.is_empty() {
            return None;
        }
        let provider = provider_for(provider);
        let base = resolve_base(base_url, provider.default_base())?;
        Some(Self {
            provider,
            base,
            model: model.to_string(),
            api_key: api_key.trim().to_string(),
            temperature,
            reasoning,
        })
    }

    /// Send one chat request and parse the reply.
    fn run(&self, system: &str, user: &str, max_tokens: u32, reasoning: bool) -> Result<String> {
        let body = self.provider.chat_body(&self.model, system, user, max_tokens, self.temperature, reasoning);
        let headers = self.provider.headers(&self.api_key);
        let v = curl_post(&self.provider.chat_url(&self.base), &headers, &body)?;
        check_error(&v)?;
        self.provider.parse_reply(&v)
    }

    /// One agentic tool-calling pass: call → parse → run any tool calls → feed
    /// results back → repeat, up to `max_steps`. A tool that errors is reported
    /// to the model as a JSON `{"error":…}` result (it can recover or pick
    /// another tool) rather than aborting the loop.
    #[allow(clippy::too_many_arguments)]
    fn run_tools_loop(
        &self,
        system: &str,
        user: &str,
        tools: &[ToolDef],
        toolbox: &dyn ToolBox,
        max_tokens: u32,
        max_steps: usize,
        reasoning: bool,
    ) -> Result<String> {
        let url = self.provider.chat_url(&self.base);
        let headers = self.provider.headers(&self.api_key);
        let mut messages: Vec<Value> = vec![json!({ "role": "user", "content": user })];
        let mut last_text = String::new();
        for step in 0..max_steps {
            let body =
                self.provider.tools_request(&self.model, system, &messages, tools, max_tokens, self.temperature, reasoning);
            let v = curl_post(&url, &headers, &body)?;
            check_error(&v)?;
            let turn = self.provider.parse_turn(&v)?;
            if let Some(t) = &turn.text {
                last_text = t.clone();
            }
            if turn.tool_calls.is_empty() {
                return Ok(last_text);
            }
            messages.push(turn.assistant_msg);
            let mut results = Vec::with_capacity(turn.tool_calls.len());
            for call in turn.tool_calls {
                let out = match toolbox.call(&call.name, &call.args) {
                    Ok(s) => {
                        tracing::debug!(step, tool = %call.name, args = %call.args, bytes = s.len(), "llm tool call");
                        s
                    }
                    Err(e) => {
                        tracing::debug!(step, tool = %call.name, args = %call.args, error = %e, "llm tool call failed");
                        json!({ "error": e.to_string() }).to_string()
                    }
                };
                results.push((call, out));
            }
            messages.extend(self.provider.tool_result_messages(&results));
        }
        bail!("LLM tool loop exhausted {max_steps} steps without a final answer")
    }
}

impl LlmClient for HttpLlm {
    fn available(&self) -> bool {
        true
    }

    fn complete(&self, system: &str, user: &str, max_tokens: u32) -> Result<String> {
        match self.run(system, user, max_tokens, self.reasoning) {
            Ok(text) => Ok(text),
            // Reasoning is unsupported on some models (e.g. Claude Haiku) and 400s
            // there retry once without it so enabling it degrades gracefully.
            Err(e) if self.reasoning && self.provider.reasoning_applies() => {
                tracing::warn!(error = %e, "LLM reasoning request failed; retrying without it");
                self.run(system, user, max_tokens, false)
            }
            Err(e) => Err(e),
        }
    }

    fn supports_tools(&self) -> bool {
        self.provider.supports_tools()
    }

    fn run_tools(
        &self,
        system: &str,
        user: &str,
        tools: &[ToolDef],
        toolbox: &dyn ToolBox,
        max_tokens: u32,
        max_steps: usize,
    ) -> Result<String> {
        match self.run_tools_loop(system, user, tools, toolbox, max_tokens, max_steps, self.reasoning) {
            Ok(s) => Ok(s),
            // Some models 400 on `thinking` (e.g. Claude Haiku) retry the whole
            // loop without it. Catalog tools are read-only, so a replay is safe.
            Err(e) if self.reasoning && self.provider.reasoning_applies() => {
                tracing::warn!(error = %e, "LLM tool run failed; retrying without reasoning");
                self.run_tools_loop(system, user, tools, toolbox, max_tokens, max_steps, false)
            }
            Err(e) => Err(e),
        }
    }

    fn describe(&self) -> String {
        format!("{} {} @ {}", self.provider.id(), self.model, self.provider.chat_url(&self.base))
    }
}

/// List the models an endpoint advertises (`GET {models_url}`), powering the
/// admin "Load models" picker. Standalone (no model needed yet).
pub fn list_models(provider: &str, base_url: &str, api_key: &str) -> Result<Vec<String>> {
    let provider = provider_for(provider);
    let base = resolve_base(base_url, provider.default_base())
        .ok_or_else(|| anyhow!("a base URL is required to list models"))?;
    let v = curl_get(&provider.models_url(&base), &provider.headers(api_key.trim()))?;
    check_error(&v)?;
    let mut ids: Vec<String> = v
        .get("data")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// Apply the provider's default base when blank; trim a trailing slash. `None`
/// when blank and the provider has no default (base URL required).
fn resolve_base(base_url: &str, default: Option<&str>) -> Option<String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        default.map(str::to_string)
    } else {
        Some(base.to_string())
    }
}

// ----- curl transport ---------------------------------------------------------

fn curl_post(url: &str, headers: &[(&str, String)], body: &Value) -> Result<Value> {
    let body = serde_json::to_string(body)?;
    let mut cmd = Command::new("curl");
    cmd.args(["-s", "-S", "--max-time", MAX_TIME_SECS, "-X", "POST"]);
    for (k, v) in headers {
        cmd.arg("-H").arg(format!("{k}: {v}"));
    }
    cmd.arg("--data-binary").arg(&body).arg(url);
    run_curl(cmd, "LLM request")
}

fn curl_get(url: &str, headers: &[(&str, String)]) -> Result<Value> {
    let mut cmd = Command::new("curl");
    cmd.args(["-s", "-S", "--max-time", "20"]);
    for (k, v) in headers {
        cmd.arg("-H").arg(format!("{k}: {v}"));
    }
    cmd.arg(url);
    run_curl(cmd, "model list")
}

fn run_curl(mut cmd: Command, what: &str) -> Result<Value> {
    let out = cmd.output().with_context(|| format!("spawn curl for {what}"))?;
    if !out.status.success() {
        bail!(
            "curl exit {}: {}",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    serde_json::from_slice(&out.stdout).with_context(|| {
        format!("parse {what} response: {}", String::from_utf8_lossy(&out.stdout).chars().take(200).collect::<String>())
    })
}

/// Surface an OpenAI (`{error:{message}}`) or Anthropic (`{type:"error",
/// error:{message}}`) error body as a Rust error. A present-but-`null` `error`
/// field (some OpenAI-compatible servers include it on success) is not an error.
fn check_error(v: &Value) -> Result<()> {
    if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_else(|| err.as_str().unwrap_or("unknown error"));
        bail!("LLM API error: {msg}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs() -> Vec<ToolDef> {
        vec![ToolDef {
            name: "find_titles".into(),
            description: "list titles".into(),
            schema: json!({ "type": "object", "properties": { "genre": { "type": "string" } } }),
        }]
    }

    /// A stand-in tool that echoes its name + args, for the round-trip test.
    struct EchoBox;
    impl ToolBox for EchoBox {
        fn defs(&self) -> Vec<ToolDef> {
            defs()
        }
        fn call(&self, name: &str, args: &Value) -> Result<String> {
            Ok(json!({ "echo": name, "args": args }).to_string())
        }
    }

    #[test]
    fn openai_request_prepends_system_and_maps_tools() {
        let messages = vec![json!({ "role": "user", "content": "hi" })];
        let body = OpenAi::openai().tools_request("m", "SYS", &messages, &defs(), 100, 0.5, false);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "SYS");
        assert_eq!(body["messages"][1]["content"], "hi");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "find_titles");
        assert!(body["tools"][0]["function"]["parameters"]["properties"]["genre"].is_object());
    }

    #[test]
    fn openai_parse_turn_reads_tool_calls_then_results() {
        let resp = json!({ "choices": [{ "message": {
            "role": "assistant", "content": null,
            "tool_calls": [{ "id": "call_1", "type": "function",
                "function": { "name": "find_titles", "arguments": "{\"genre\":\"Horror\"}" } }],
        } }] });
        let turn = OpenAi::openai().parse_turn(&resp).unwrap();
        assert!(turn.text.is_none());
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].id, "call_1");
        assert_eq!(turn.tool_calls[0].name, "find_titles");
        assert_eq!(turn.tool_calls[0].args["genre"], "Horror");
        // The raw assistant message is echoed back as the next turn's history.
        assert_eq!(turn.assistant_msg["tool_calls"][0]["id"], "call_1");

        let results = vec![(turn.tool_calls[0].clone(), "ok".to_string())];
        let msgs = OpenAi::openai().tool_result_messages(&results);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "tool");
        assert_eq!(msgs[0]["tool_call_id"], "call_1");
        assert_eq!(msgs[0]["content"], "ok");
    }

    #[test]
    fn openai_parse_turn_accepts_object_form_arguments() {
        // Ollama / llama.cpp / LM Studio hand back `arguments` as an object, not
        // a JSON string must still parse, not silently drop the args.
        let resp = json!({ "choices": [{ "message": {
            "role": "assistant",
            "tool_calls": [{ "id": "c1", "type": "function",
                "function": { "name": "find_titles", "arguments": { "genre": "Horror" } } }],
        } }] });
        let turn = OpenAi::openai().parse_turn(&resp).unwrap();
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].args["genre"], "Horror");
    }

    #[test]
    fn openai_parse_turn_final_text_stops_loop() {
        let resp = json!({ "choices": [{ "message": { "role": "assistant", "content": "done" } }] });
        let turn = OpenAi::openai().parse_turn(&resp).unwrap();
        assert!(turn.tool_calls.is_empty());
        assert_eq!(turn.text.as_deref(), Some("done"));
    }

    #[test]
    fn anthropic_tool_use_round_trip() {
        let body = Anthropic.tools_request("m", "SYS", &[json!({ "role": "user", "content": "hi" })], &defs(), 100, 0.0, false);
        assert_eq!(body["system"], "SYS");
        assert_eq!(body["tools"][0]["name"], "find_titles");
        assert!(body["tools"][0]["input_schema"]["properties"]["genre"].is_object());
        assert!(body.get("thinking").is_none());

        let resp = json!({ "stop_reason": "tool_use", "content": [
            { "type": "text", "text": "let me look" },
            { "type": "tool_use", "id": "toolu_1", "name": "find_titles", "input": { "genre": "Horror" } },
        ] });
        let turn = Anthropic.parse_turn(&resp).unwrap();
        assert_eq!(turn.text.as_deref(), Some("let me look"));
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].id, "toolu_1");
        assert_eq!(turn.tool_calls[0].args["genre"], "Horror");
        // Echoed assistant content preserves all blocks (text + tool_use).
        assert_eq!(turn.assistant_msg["role"], "assistant");
        assert_eq!(turn.assistant_msg["content"][1]["type"], "tool_use");

        let results = vec![(turn.tool_calls[0].clone(), "ok".to_string())];
        let msgs = Anthropic.tool_result_messages(&results);
        assert_eq!(msgs.len(), 1); // one user turn holding all tool_result blocks
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"][0]["type"], "tool_result");
        assert_eq!(msgs[0]["content"][0]["tool_use_id"], "toolu_1");
    }

    #[test]
    fn anthropic_refusal_errors_and_reasoning_adds_thinking() {
        assert!(Anthropic.parse_turn(&json!({ "stop_reason": "refusal", "content": [] })).is_err());
        let body = Anthropic.tools_request("m", "s", &[], &defs(), 10, 0.0, true);
        assert_eq!(body["thinking"]["type"], "adaptive");
    }

    /// One loop step end-to-end against a stand-in tool: parse a tool call, run it
    /// through the `ToolBox`, and shape the result messages the inner round-trip
    /// `run_tools_loop` performs (minus the HTTP call).
    #[test]
    fn simulated_round_trip_dispatches_through_toolbox() {
        let tb = EchoBox;
        let resp = json!({ "choices": [{ "message": { "role": "assistant",
            "tool_calls": [{ "id": "c1", "function": { "name": "find_titles", "arguments": "{\"genre\":\"Horror\"}" } }],
        } }] });
        let turn = OpenAi::openai().parse_turn(&resp).unwrap();
        let mut results = Vec::new();
        for call in turn.tool_calls {
            let out = tb.call(&call.name, &call.args).unwrap();
            results.push((call, out));
        }
        let msgs = OpenAi::openai().tool_result_messages(&results);
        let content = msgs[0]["content"].as_str().unwrap();
        assert!(content.contains("\"echo\":\"find_titles\""));
        assert!(content.contains("Horror"));
    }
}
