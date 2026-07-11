//! XPath selector support (a minority of definitions use it instead of CSS).
//!
//! `scraper` is CSS-only, so an XPath definition is parsed on a separate
//! `libxml2` path here rather than shoehorned into the CSS DOM. Definitions are
//! consistent within themselves (all-CSS or all-XPath), so the engine routes a
//! whole definition to one path (see [`crate::engine::uses_xpath`]).
//!
//! Gated behind the `xpath` cargo feature so the default (musl) build stays
//! pure-Rust; without it, an XPath definition surfaces a clear "rebuild with
//! the xpath feature" error instead of silently finding nothing.

#![cfg(feature = "xpath")]

use std::collections::HashMap;

use anyhow::{Context as _, Result};
use libxml::parser::Parser;
use libxml::tree::Node;
use libxml::xpath::Context as XpathCtx;

use crate::context::Context;
use crate::definition::{Definition, Field};
use crate::engine;
use crate::{filters, template, IndexerConfig, Release};

/// Parse an HTML body whose selectors are XPath into releases.
pub fn parse_html(def: &Definition, cfg: &IndexerConfig, body: &str) -> Result<Vec<Release>> {
    let parser = Parser::default_html();
    let doc = parser
        .parse_string(body)
        .map_err(|e| anyhow::anyhow!("libxml parse: {e:?}"))?;
    let mut xctx = XpathCtx::new(&doc).map_err(|_| anyhow::anyhow!("libxml xpath context"))?;
    let base_ctx = Context::with_config(def, cfg);

    let row_sel = def
        .search
        .rows
        .selector
        .as_deref()
        .context("definition has no rows selector")?;
    let row_sel = template::render(row_sel, &base_ctx);

    let mut releases = Vec::new();
    // `get_nodes_as_vec` returns owned nodes, so the context isn't borrowed
    // across the loop body's mutable calls.
    for row in eval_nodes(&mut xctx, &row_sel, None) {
        if let Some(result) = extract_row(def, &base_ctx, &mut xctx, &row) {
            releases.push(engine::to_release(def, cfg, &result));
        }
    }
    Ok(releases)
}

fn extract_row(
    def: &Definition,
    base_ctx: &Context,
    xctx: &mut XpathCtx,
    row: &Node,
) -> Option<HashMap<String, String>> {
    let mut result = HashMap::new();
    for (name, field) in &def.search.fields {
        let mut ctx = base_ctx.clone();
        ctx.result = result.clone();
        let value = resolve_field(field, xctx, row, &ctx)?;
        result.insert(name.clone(), value);
    }
    Some(result)
}

fn resolve_field(field: &Field, xctx: &mut XpathCtx, row: &Node, ctx: &Context) -> Option<String> {
    let raw: Option<String> = if let Some(text) = &field.text {
        Some(template::render(text, ctx))
    } else if !field.case.is_empty() {
        let mut default = None;
        let mut hit = None;
        for (sel, val) in &field.case {
            if sel == "*" {
                default = Some(val);
            } else if !eval_nodes(xctx, &template::render(sel, ctx), Some(row)).is_empty() {
                hit = Some(val);
                break;
            }
        }
        hit.or(default).map(|v| template::render(v, ctx))
    } else if let Some(sel) = &field.selector {
        let sel = template::render(sel, ctx);
        eval_nodes(xctx, &sel, Some(row)).first().map(|n| read_node(field, n))
    } else {
        Some(read_node(field, row))
    };

    let value = match raw {
        Some(v) => v,
        None => match &field.default {
            Some(d) => template::render(d, ctx),
            None if field.optional => String::new(),
            None => return None,
        },
    };
    Some(filters::apply(&value, &field.filters, ctx))
}

fn read_node(field: &Field, node: &Node) -> String {
    if let Some(attr) = &field.attribute {
        node.get_attribute(attr).unwrap_or_default().trim().to_string()
    } else {
        normalize_ws(&node.get_content())
    }
}

/// Evaluate an XPath expression, optionally relative to a context node.
fn eval_nodes(xctx: &mut XpathCtx, xpath: &str, node: Option<&Node>) -> Vec<Node> {
    if let Some(n) = node {
        // Relative evaluation: pin the context node first.
        if xctx.set_context_node(n).is_err() {
            return Vec::new();
        }
    }
    match xctx.evaluate(xpath) {
        Ok(obj) => obj.get_nodes_as_vec(),
        Err(_) => Vec::new(),
    }
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
