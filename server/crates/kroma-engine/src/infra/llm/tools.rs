//! Vendor-neutral function-calling types the "connector" foundation.
//!
//! These let an [`LlmClient`](super::LlmClient) run an agentic loop: the model is
//! handed a set of [`ToolDef`]s, asks to call them ([`ToolCall`]), and the loop
//! dispatches each call through a [`ToolBox`] and feeds the result back until the
//! model produces a final answer. The wire differences between OpenAI-style
//! `tool_calls` and Anthropic `tool_use` blocks are hidden behind the `Provider`
//! trait (`http.rs`); everything here is provider-agnostic, so a tool (e.g. the
//! catalog connector in `services/llm`) is written once and works on any backend.

use anyhow::Result;
use serde_json::Value;

/// A tool the model may call. `schema` is a JSON Schema **object** describing the
/// arguments (`{"type":"object","properties":{…},"required":[…]}`).
#[derive(Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

/// One tool invocation the model requested. `id` is the vendor's call id (echoed
/// back when returning the result so the model can match them); `args` is the
/// parsed argument object.
#[derive(Clone, Debug)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
}

/// A set of callable tools plus a dispatcher. One implementor = one connector
/// (e.g. `CatalogTools`); the loop calls [`defs`](ToolBox::defs) to advertise
/// them and [`call`](ToolBox::call) to run one. `call` returns the tool result as
/// a string (typically JSON) that is fed back to the model verbatim.
pub trait ToolBox: Send + Sync {
    fn defs(&self) -> Vec<ToolDef>;
    fn call(&self, name: &str, args: &Value) -> Result<String>;
}

/// One assistant turn parsed from a provider response: any final text, any
/// requested tool calls, and the raw assistant message to echo back into the next
/// request. The vendor shape of `assistant_msg` is preserved verbatim (including
/// Anthropic thinking blocks and OpenAI `tool_calls`) so the conversation
/// continues correctly.
pub struct Turn {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub assistant_msg: Value,
}
