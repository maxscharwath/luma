//! A focused interpreter for the Go `text/template` subset that Cardigann
//! definitions use. It is not a general Go-template engine: it supports exactly
//! the constructs that appear in real definitions -
//!
//! - interpolation: `{{ .Keywords }}`, `{{ .Config.sort }}`, `{{ .Result.x }}`
//! - conditionals: `{{ if COND }}…{{ else }}…{{ end }}`
//! - iteration: `{{ range .Categories }}…{{ . }}…{{ end }}`
//! - pipelines: `{{ .Keywords | re_replace "a" "b" }}`
//! - functions: `join`, `re_replace`, `replace`, `and`, `or`, `not`,
//!   `eq`, `ne`, `lt`, `le`, `gt`, `ge`, `printf`
//! - literals: `"double"`, `` `raw` ``, numbers, and the `.True`/`.False`
//!   constants
//! - whitespace trim markers `{{-` / `-}}`
//!
//! Anything unrecognized renders to empty rather than aborting the search, so a
//! definition using one exotic feature still returns results for the common
//! path.

use crate::context::{Context, Value};

/// Render a template string against a context. Parse errors degrade to a
/// best-effort literal rather than failing the whole search.
pub fn render(input: &str, ctx: &Context) -> String {
    match parse(input) {
        Ok(nodes) => {
            let mut out = String::new();
            eval_nodes(&nodes, ctx, &mut out);
            out
        }
        // A malformed template is far better surfaced as its literal source than
        // as a hard error that hides every release.
        Err(_) => input.to_string(),
    }
}

// ----- AST ------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Node {
    Text(String),
    Action(Pipeline),
    If { cond: Pipeline, then: Vec<Node>, els: Vec<Node> },
    Range { expr: Pipeline, body: Vec<Node> },
}

type Pipeline = Vec<Command>;

#[derive(Debug, Clone)]
struct Command {
    terms: Vec<Term>,
}

#[derive(Debug, Clone)]
enum Term {
    Field(Vec<String>),
    Str(String),
    Ident(String),
    Group(Pipeline),
}

// ----- lexing into text / action chunks -------------------------------------------

#[derive(Debug)]
enum Chunk {
    Text(String),
    /// The trimmed inner body of a `{{ … }}` action.
    Action(String),
}

fn lex(input: &str) -> Vec<Chunk> {
    let bytes = input.as_bytes();
    let mut chunks = Vec::new();
    let mut i = 0;
    let mut text = String::new();
    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            // Whitespace-trim marker `{{-` trims trailing text whitespace.
            let mut j = i + 2;
            let trim_left = bytes.get(j) == Some(&b'-');
            if trim_left {
                j += 1;
            }
            if trim_left {
                let trimmed = text.trim_end().len();
                text.truncate(trimmed);
            }
            if !text.is_empty() {
                chunks.push(Chunk::Text(std::mem::take(&mut text)));
            }
            // Find the closing `}}`.
            if let Some(rel) = input[j..].find("}}") {
                let mut body = &input[j..j + rel];
                let mut next = j + rel + 2;
                // `-}}` trims leading whitespace of the following text.
                let trim_right = body.ends_with('-');
                if trim_right {
                    body = &body[..body.len() - 1];
                }
                chunks.push(Chunk::Action(body.trim().to_string()));
                if trim_right {
                    while next < bytes.len() && bytes[next].is_ascii_whitespace() {
                        next += 1;
                    }
                }
                i = next;
                continue;
            }
            // No close: treat the rest as literal text.
            text.push_str(&input[i..]);
            break;
        }
        // Push one UTF-8 char.
        let ch_len = utf8_len(bytes[i]);
        text.push_str(&input[i..i + ch_len]);
        i += ch_len;
    }
    if !text.is_empty() {
        chunks.push(Chunk::Text(text));
    }
    chunks
}

fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

// ----- parsing chunks into a node tree --------------------------------------------

fn parse(input: &str) -> Result<Vec<Node>, String> {
    let chunks = lex(input);
    let mut pos = 0;
    let (nodes, stop) = parse_seq(&chunks, &mut pos)?;
    if stop.is_some() {
        return Err(format!("unexpected {:?}", stop));
    }
    Ok(nodes)
}

/// Parse a sequence of nodes, stopping (without consuming) at `else`/`end`.
/// Returns the control keyword that stopped it, if any.
fn parse_seq(chunks: &[Chunk], pos: &mut usize) -> Result<(Vec<Node>, Option<String>), String> {
    let mut nodes = Vec::new();
    while *pos < chunks.len() {
        match &chunks[*pos] {
            Chunk::Text(t) => {
                nodes.push(Node::Text(t.clone()));
                *pos += 1;
            }
            Chunk::Action(body) => {
                let (head, rest) = split_head(body);
                match head {
                    "end" | "else" => return Ok((nodes, Some(head.to_string()))),
                    "if" | "with" => {
                        *pos += 1;
                        let cond = parse_pipeline(rest)?;
                        let (then, stop) = parse_seq(chunks, pos)?;
                        let mut els = Vec::new();
                        if stop.as_deref() == Some("else") {
                            *pos += 1; // consume else
                            let (e, stop2) = parse_seq(chunks, pos)?;
                            if stop2.as_deref() != Some("end") {
                                return Err("if: missing end".into());
                            }
                            *pos += 1; // consume end
                            els = e;
                        } else if stop.as_deref() == Some("end") {
                            *pos += 1; // consume end
                        } else {
                            return Err("if: missing end".into());
                        }
                        nodes.push(Node::If { cond, then, els });
                    }
                    "range" => {
                        *pos += 1;
                        let expr = parse_pipeline(rest)?;
                        let (body, stop) = parse_seq(chunks, pos)?;
                        if stop.as_deref() != Some("end") {
                            return Err("range: missing end".into());
                        }
                        *pos += 1; // consume end
                        nodes.push(Node::Range { expr, body });
                    }
                    _ => {
                        *pos += 1;
                        nodes.push(Node::Action(parse_pipeline(body)?));
                    }
                }
            }
        }
    }
    Ok((nodes, None))
}

/// Split the first bareword off an action body (`if and (x) (y)` -> `("if", "and (x) (y)")`).
fn split_head(body: &str) -> (&str, &str) {
    let body = body.trim();
    match body.find(char::is_whitespace) {
        Some(i) => (&body[..i], body[i..].trim_start()),
        None => (body, ""),
    }
}

// ----- pipeline parsing -----------------------------------------------------------

/// Tokens inside an action body.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Pipe,
    LParen,
    RParen,
    Field(Vec<String>),
    Str(String),
    Ident(String),
}

fn tokenize_expr(s: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '|' {
            toks.push(Tok::Pipe);
            i += 1;
        } else if c == '(' {
            toks.push(Tok::LParen);
            i += 1;
        } else if c == ')' {
            toks.push(Tok::RParen);
            i += 1;
        } else if c == '"' || c == '`' {
            let quote = c;
            i += 1;
            let mut lit = String::new();
            while i < chars.len() && chars[i] != quote {
                if quote == '"' && chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    lit.push(match chars[i] {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        other => other,
                    });
                } else {
                    lit.push(chars[i]);
                }
                i += 1;
            }
            i += 1; // closing quote
            toks.push(Tok::Str(lit));
        } else if c == '.' {
            // A dotted field: `.A.B` or a bare `.`.
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            let raw: String = chars[start..i].iter().collect();
            let segs: Vec<String> =
                raw.trim_start_matches('.').split('.').filter(|s| !s.is_empty()).map(String::from).collect();
            toks.push(Tok::Field(segs));
        } else {
            // Identifier / number / function name.
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() && !matches!(chars[i], '|' | '(' | ')') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            toks.push(Tok::Ident(word));
        }
    }
    Ok(toks)
}

fn parse_pipeline(s: &str) -> Result<Pipeline, String> {
    if s.trim().is_empty() {
        return Ok(vec![]);
    }
    let toks = tokenize_expr(s)?;
    let mut pos = 0;
    let pipe = parse_pipeline_toks(&toks, &mut pos)?;
    Ok(pipe)
}

/// Parse commands separated by `|`, consuming until a top-level `)` or the end.
fn parse_pipeline_toks(toks: &[Tok], pos: &mut usize) -> Result<Pipeline, String> {
    let mut commands = Vec::new();
    commands.push(parse_command(toks, pos)?);
    while *pos < toks.len() && toks[*pos] == Tok::Pipe {
        *pos += 1;
        commands.push(parse_command(toks, pos)?);
    }
    Ok(commands)
}

fn parse_command(toks: &[Tok], pos: &mut usize) -> Result<Command, String> {
    let mut terms = Vec::new();
    while *pos < toks.len() {
        match &toks[*pos] {
            Tok::Pipe | Tok::RParen => break,
            Tok::LParen => {
                *pos += 1;
                let inner = parse_pipeline_toks(toks, pos)?;
                if *pos >= toks.len() || toks[*pos] != Tok::RParen {
                    return Err("missing )".into());
                }
                *pos += 1;
                terms.push(Term::Group(inner));
            }
            Tok::Field(segs) => {
                terms.push(Term::Field(segs.clone()));
                *pos += 1;
            }
            Tok::Str(s) => {
                terms.push(Term::Str(s.clone()));
                *pos += 1;
            }
            Tok::Ident(w) => {
                terms.push(Term::Ident(w.clone()));
                *pos += 1;
            }
        }
    }
    if terms.is_empty() {
        return Err("empty command".into());
    }
    Ok(Command { terms })
}

// ----- evaluation -----------------------------------------------------------------

fn eval_nodes(nodes: &[Node], ctx: &Context, out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(t),
            Node::Action(pipe) => out.push_str(&eval_pipeline(pipe, ctx).render()),
            Node::If { cond, then, els } => {
                if eval_pipeline(cond, ctx).truthy() {
                    eval_nodes(then, ctx, out);
                } else {
                    eval_nodes(els, ctx, out);
                }
            }
            Node::Range { expr, body } => {
                if let Value::List(items) = eval_pipeline(expr, ctx) {
                    for item in items {
                        let mut inner = ctx.clone();
                        inner.dot = Some(Value::Str(item));
                        eval_nodes(body, &inner, out);
                    }
                }
            }
        }
    }
}

fn eval_pipeline(pipe: &[Command], ctx: &Context) -> Value {
    let mut piped: Option<Value> = None;
    for cmd in pipe {
        piped = Some(eval_command(cmd, ctx, piped.take()));
    }
    piped.unwrap_or(Value::Nil)
}

fn eval_command(cmd: &Command, ctx: &Context, piped: Option<Value>) -> Value {
    // A single non-function term is just a value (a piped value, if any, is
    // ignored for a bare term - pipelines only feed function commands).
    let first = &cmd.terms[0];
    if let Term::Ident(name) = first {
        if is_function(name) {
            let mut args: Vec<Value> = cmd.terms[1..].iter().map(|t| eval_term(t, ctx)).collect();
            if let Some(p) = piped {
                args.push(p);
            }
            return call_function(name, &args);
        }
        // A bare identifier that isn't a function: treat as a string literal
        // (covers numbers and stray words).
        return Value::Str(name.clone());
    }
    if cmd.terms.len() == 1 {
        return eval_term(first, ctx);
    }
    // Multiple terms with a non-ident head: evaluate the head only.
    eval_term(first, ctx)
}

fn eval_term(term: &Term, ctx: &Context) -> Value {
    match term {
        Term::Field(segs) => {
            let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
            ctx.resolve(&refs)
        }
        Term::Str(s) => Value::Str(s.clone()),
        Term::Ident(w) => Value::Str(w.clone()),
        Term::Group(pipe) => eval_pipeline(pipe, ctx),
    }
}

fn is_function(name: &str) -> bool {
    matches!(
        name,
        "join" | "re_replace" | "replace" | "and" | "or" | "not" | "eq" | "ne" | "lt" | "le"
            | "gt" | "ge" | "printf"
    )
}

fn call_function(name: &str, args: &[Value]) -> Value {
    let s = |i: usize| args.get(i).map(Value::render).unwrap_or_default();
    match name {
        "join" => {
            let sep = s(1);
            match args.first() {
                Some(Value::List(l)) => Value::Str(l.join(&sep)),
                Some(v) => Value::Str(v.render()),
                None => Value::Str(String::new()),
            }
        }
        "re_replace" => {
            let input = s(0);
            let pattern = s(1);
            let repl = s(2);
            match regex::Regex::new(&pattern) {
                Ok(re) => Value::Str(re.replace_all(&input, repl.as_str()).into_owned()),
                Err(_) => Value::Str(input),
            }
        }
        "replace" => Value::Str(s(0).replace(&s(1), &s(2))),
        "not" => Value::Bool(!args.first().map(Value::truthy).unwrap_or(false)),
        "and" => {
            // Go: first falsy arg, else the last arg.
            for a in args {
                if !a.truthy() {
                    return a.clone();
                }
            }
            args.last().cloned().unwrap_or(Value::Bool(true))
        }
        "or" => {
            for a in args {
                if a.truthy() {
                    return a.clone();
                }
            }
            args.last().cloned().unwrap_or(Value::Bool(false))
        }
        "eq" => Value::Bool(values_eq(args.first(), args.get(1))),
        "ne" => Value::Bool(!values_eq(args.first(), args.get(1))),
        "lt" | "le" | "gt" | "ge" => Value::Bool(compare(name, args.first(), args.get(1))),
        "printf" => {
            // Guard the slice: `{{ printf }}` with no args must not panic.
            let rest = if args.len() > 1 { &args[1..] } else { &[][..] };
            Value::Str(sprintf(&s(0), rest))
        }
        _ => Value::Nil,
    }
}

fn values_eq(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (Some(Value::Bool(x)), Some(Value::Bool(y))) => x == y,
        (Some(x), Some(y)) => x.render() == y.render(),
        _ => false,
    }
}

fn compare(op: &str, a: Option<&Value>, b: Option<&Value>) -> bool {
    let (a, b) = match (a, b) {
        (Some(a), Some(b)) => (a.render(), b.render()),
        _ => return false,
    };
    // Numeric when both parse; lexicographic otherwise.
    let ord = match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y),
        _ => Some(a.cmp(&b)),
    };
    match ord {
        Some(o) => match op {
            "lt" => o.is_lt(),
            "le" => o.is_le(),
            "gt" => o.is_gt(),
            "ge" => o.is_ge(),
            _ => false,
        },
        None => false,
    }
}

/// Minimal `printf`: supports `%s`, `%d`, `%v`, and zero-padded `%0Nd`.
fn sprintf(format: &str, args: &[Value]) -> String {
    let mut out = String::new();
    let mut chars = format.chars().peekable();
    let mut arg_i = 0;
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut spec = String::from("%");
        while let Some(&n) = chars.peek() {
            spec.push(n);
            chars.next();
            if n.is_ascii_alphabetic() {
                break;
            }
        }
        let arg = args.get(arg_i).map(Value::render).unwrap_or_default();
        arg_i += 1;
        match spec.chars().last() {
            Some('s') | Some('v') => out.push_str(&arg),
            Some('d') => {
                let n: i64 = arg.parse().unwrap_or(0);
                // Flags/width between '%' and 'd', e.g. "%04d" -> flags_width "04".
                let flags_width = &spec[1..spec.len() - 1];
                // Zero-pad only with an explicit leading '0' FLAG ("%04d"), not
                // merely because the width digits contain a 0 ("%10d").
                let zero_pad = flags_width.starts_with('0');
                let width: usize = flags_width.trim_start_matches('0').parse().unwrap_or(0);
                if zero_pad && width > 0 {
                    out.push_str(&format!("{n:0width$}"));
                } else if width > 0 {
                    out.push_str(&format!("{n:width$}"));
                } else {
                    out.push_str(&n.to_string());
                }
            }
            _ => out.push_str(&spec),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Value;
    use std::collections::HashMap;

    fn ctx() -> Context {
        let mut config = HashMap::new();
        config.insert("username".into(), Value::Str("alice".into()));
        config.insert("freeleech".into(), Value::Bool(false));
        config.insert("disablesort".into(), Value::Bool(false));
        config.insert("sort".into(), Value::Str("seeders".into()));
        Context {
            keywords: "the matrix 1999".into(),
            categories: vec!["1".into(), "3".into()],
            config,
            ..Default::default()
        }
    }

    #[test]
    fn interpolation_and_config() {
        assert_eq!(render("q={{ .Keywords }}", &ctx()), "q=the matrix 1999");
        assert_eq!(render("u={{ .Config.username }}", &ctx()), "u=alice");
    }

    #[test]
    fn if_else_with_bool_config() {
        // freeleech is false -> else branch.
        assert_eq!(render("{{ if .Config.freeleech }}fl{{ else }}no{{ end }}", &ctx()), "no");
        // eq against the .False constant.
        let t = "{{ if eq .Config.disablesort .False }}sort{{ else }}x{{ end }}";
        assert_eq!(render(t, &ctx()), "sort");
    }

    #[test]
    fn and_or_grouping() {
        let t = "{{ if and (.Keywords) (eq .Config.disablesort .False) }}Y{{ else }}N{{ end }}";
        assert_eq!(render(t, &ctx()), "Y");
    }

    #[test]
    fn join_categories() {
        assert_eq!(render("cat={{ join .Categories \",\" }}", &ctx()), "cat=1,3");
    }

    #[test]
    fn range_categories() {
        assert_eq!(render("{{ range .Categories }}[{{ . }}]{{ end }}", &ctx()), "[1][3]");
    }

    #[test]
    fn re_replace_call() {
        // Cardigann funcs are input-first: `re_replace <input> <pat> <repl>`.
        let t = "{{ re_replace .Keywords \"[0-9]+\" \"\" }}";
        assert_eq!(render(t, &ctx()), "the matrix ");
    }

    #[test]
    fn result_reference() {
        let mut c = ctx();
        c.result.insert("_id".into(), "42".into());
        assert_eq!(render("/torrent/{{ .Result._id }}", &c), "/torrent/42");
    }

    #[test]
    fn printf_padding_and_empty_args() {
        // Explicit zero-pad flag vs a width that merely contains a 0 digit.
        assert_eq!(render("{{ printf \"%04d\" 5 }}", &Context::default()), "0005");
        assert_eq!(render("{{ printf \"%10d\" 5 }}", &Context::default()), "         5");
        assert_eq!(render("{{ printf \"%s-x\" \"a\" }}", &Context::default()), "a-x");
        // Must not panic with no args.
        assert_eq!(render("{{ printf }}", &Context::default()), "");
    }

    #[test]
    fn renders_templated_base_url() {
        let mut config = HashMap::new();
        config.insert("apiurl".to_string(), Value::Str("api.example.org".into()));
        let ctx = Context { config, ..Default::default() };
        assert_eq!(render("https://{{ .Config.apiurl }}", &ctx), "https://api.example.org");
        // Undefined config key renders empty, never the literal braces.
        assert_eq!(render("https://{{ .Config.missing }}", &Context::default()), "https://");
    }

    #[test]
    fn whitespace_trim_markers() {
        assert_eq!(render("a {{- \"b\" }}", &ctx()), "ab");
        assert_eq!(render("{{ \"a\" -}} b", &ctx()), "ab");
    }
}
