//! Element selection + text/attribute extraction over parsed HTML.
//!
//! Definitions target the DOM the way jQuery/AngleSharp do, which is a superset
//! of what the pure-CSS [`scraper`] engine parses. The two extensions that
//! actually appear in real definitions are handled here:
//!
//! - `:contains("text")` — not standard CSS; emulated by stripping it from the
//!   selector and post-filtering matched elements by their text content.
//! - `:has(sub)` — supported natively by recent `selectors`; left in the
//!   selector and only stripped if parsing fails.
//!
//! XPath selectors (a minority of definitions) are handled by the optional
//! [`crate::xpath`] module; without the `xpath` feature they select nothing and
//! the engine reports the definition as unsupported.

use scraper::{ElementRef, Html, Selector};

/// Is this selector string XPath (rather than CSS)? Cardigann uses a leading
/// slash / `./` / `//` to signal XPath.
pub fn is_xpath(sel: &str) -> bool {
    let s = sel.trim_start();
    s.starts_with('/') || s.starts_with("./") || s.starts_with("(/")
}

/// Select all elements matching `sel` within `scope` (descendants only), with
/// `:contains()` emulation.
pub fn select_all<'a>(scope: ElementRef<'a>, sel: &str) -> Vec<ElementRef<'a>> {
    let (clean, contains) = strip_contains(sel);
    let parsed = match Selector::parse(clean.trim()) {
        Ok(p) => p,
        // Retry once with `:has(...)` removed for the (rare) engine that can't
        // parse it.
        Err(_) => match Selector::parse(strip_has(&clean).trim()) {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        },
    };
    scope
        .select(&parsed)
        .filter(|el| contains.iter().all(|term| element_text(*el).contains(term.as_str())))
        .collect()
}

/// First element matching `sel` within `scope`.
pub fn select_first<'a>(scope: ElementRef<'a>, sel: &str) -> Option<ElementRef<'a>> {
    select_all(scope, sel).into_iter().next()
}

/// The normalized visible text of an element: all descendant text, trimmed with
/// internal whitespace runs collapsed to single spaces (matching how trackers
/// render titles).
pub fn element_text(el: ElementRef) -> String {
    normalize_ws(&el.text().collect::<String>())
}

/// Element text with a set of descendant sub-trees removed first (Cardigann's
/// `remove:`), e.g. dropping a nested "FREELEECH" badge from a title cell.
pub fn element_text_removing(el: ElementRef, remove: &str) -> String {
    // The node-id type comes from `ego_tree` (a transitive dep via scraper); let
    // inference name it so we needn't take the dependency directly.
    let mut excluded = std::collections::HashSet::new();
    for m in select_all(el, remove) {
        for d in m.descendants() {
            excluded.insert(d.id());
        }
    }
    let mut out = String::new();
    for node in el.descendants() {
        if excluded.contains(&node.id()) {
            continue;
        }
        if let Some(t) = node.value().as_text() {
            out.push_str(t);
        }
    }
    normalize_ws(&out)
}

/// Read an attribute off an element (normalized whitespace).
pub fn element_attr(el: ElementRef, attr: &str) -> Option<String> {
    el.value().attr(attr).map(|v| v.trim().to_string())
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Split `:contains("term")` (or `:contains(term)`) pseudo-classes out of a
/// selector, returning the cleaned selector and the required substrings.
fn strip_contains(sel: &str) -> (String, Vec<String>) {
    let mut clean = String::new();
    let mut terms = Vec::new();
    let bytes: Vec<char> = sel.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if sel[i..].starts_with(":contains(") {
            i += ":contains(".len();
            // Read the balanced parenthesized argument.
            let mut depth = 1;
            let mut arg = String::new();
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    '(' => {
                        depth += 1;
                        arg.push('(');
                    }
                    ')' => {
                        depth -= 1;
                        if depth > 0 {
                            arg.push(')');
                        }
                    }
                    c => arg.push(c),
                }
                i += 1;
            }
            let term = arg.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
            if !term.is_empty() {
                terms.push(term);
            }
        } else {
            clean.push(bytes[i]);
            i += 1;
        }
    }
    (clean, terms)
}

/// Remove `:has(...)` pseudo-classes (last-resort fallback for engines that
/// can't parse them). Best-effort: drops the whole `:has(...)` group.
fn strip_has(sel: &str) -> String {
    let mut out = String::new();
    let mut rest = sel;
    while let Some(pos) = rest.find(":has(") {
        out.push_str(&rest[..pos]);
        rest = &rest[pos + ":has(".len()..];
        let mut depth = 1;
        let mut j = 0;
        for (k, c) in rest.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        j = k + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &rest[j..];
    }
    out.push_str(rest);
    out
}

/// Parse an HTML body into a document.
pub fn parse_document(body: &str) -> Html {
    Html::parse_document(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"
      <table class="results">
        <tr class="torrent">
          <td class="name"><a href="/t/1">Cool Movie 2020 <span class="tag">FREELEECH</span></a></td>
          <td class="size">1.5 GB</td>
        </tr>
        <tr class="torrent">
          <td class="name"><a href="/t/2">Other Show</a></td>
          <td class="size">700 MB</td>
        </tr>
      </table>
    "#;

    #[test]
    fn selects_rows_and_text() {
        let doc = parse_document(DOC);
        let root = doc.root_element();
        let rows = select_all(root, "table.results tr.torrent");
        assert_eq!(rows.len(), 2);
        let name = select_first(rows[0], "td.name a").unwrap();
        assert_eq!(element_text(name), "Cool Movie 2020 FREELEECH");
        assert_eq!(element_attr(name, "href").as_deref(), Some("/t/1"));
    }

    #[test]
    fn remove_subtree_from_text() {
        let doc = parse_document(DOC);
        let root = doc.root_element();
        let rows = select_all(root, "tr.torrent");
        let name = select_first(rows[0], "td.name a").unwrap();
        assert_eq!(element_text_removing(name, "span.tag"), "Cool Movie 2020");
    }

    #[test]
    fn contains_emulation() {
        let doc = parse_document(DOC);
        let root = doc.root_element();
        // Only the row whose text contains "Cool" should match.
        let rows = select_all(root, "tr.torrent:contains(\"Cool\")");
        assert_eq!(rows.len(), 1);
        assert!(element_text(rows[0]).contains("Cool Movie"));
    }
}
