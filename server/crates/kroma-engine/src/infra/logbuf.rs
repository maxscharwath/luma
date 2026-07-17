//! In-memory ring buffer of recent log lines, backing the admin "Journaux"
//! page. Two producers feed it: a tracing layer in `main` (the core's own
//! events, post-EnvFilter) and the module supervisor (each sidecar's piped
//! stdout/stderr, tagged with its module id). Process-global because the
//! tracing layer is installed before any state exists.

use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};

/// Lines kept in memory (oldest evicted first). At ~200 bytes a line this is
/// roughly 1 MiB, enough for hours of normal traffic or minutes of a crash
/// loop, which is exactly the window an admin needs to see.
const CAPACITY: usize = 5000;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// Arrival time, unix ms (module lines carry their own timestamp in the
    /// message; this one is close enough for ordering and display).
    pub ts: i64,
    /// `trace` | `debug` | `info` | `warn` | `error`.
    pub level: String,
    /// Rust tracing target for core lines (`kroma_engine::infra::watch`),
    /// empty for module lines (their target stays in the message).
    pub target: String,
    /// `core` or the module id (`tv.kroma.vpn`).
    pub source: String,
    pub message: String,
}

pub static LOG_BUFFER: LazyLock<LogBuffer> = LazyLock::new(LogBuffer::new);

pub struct LogBuffer {
    inner: Mutex<VecDeque<LogEntry>>,
}

impl LogBuffer {
    fn new() -> Self {
        Self { inner: Mutex::new(VecDeque::with_capacity(CAPACITY)) }
    }

    pub fn push(&self, entry: LogEntry) {
        let mut buf = self.inner.lock().unwrap();
        if buf.len() == CAPACITY {
            buf.pop_front();
        }
        buf.push_back(entry);
    }

    /// A core tracing event (already through the global EnvFilter).
    pub fn push_core(&self, level: &str, target: &str, message: String) {
        self.push(LogEntry {
            ts: now_ms(),
            level: level.to_lowercase(),
            target: target.to_string(),
            source: "core".to_string(),
            message,
        });
    }

    /// A raw line from a module sidecar's stdout/stderr. The sidecar already
    /// formatted it (`2026-07-16T07:37:53Z  INFO target: msg`, never ANSI on a
    /// pipe): parse the level for filtering and drop the leading timestamp
    /// (the entry carries its own).
    pub fn push_module_line(&self, module_id: &str, line: &str) {
        // Sidecars keep ANSI colour on (their fmt layer never checks the pipe),
        // so scrub escape sequences before parsing.
        let line = strip_ansi(line);
        let mut message = line.trim_end();
        let mut level = "info";
        if let Some(first) = message.split_whitespace().next() {
            if first.contains('T') && (first.ends_with('Z') || first.contains('+')) {
                message = message[first.len()..].trim_start();
            }
        }
        for candidate in ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"] {
            if let Some(rest) = message.strip_prefix(candidate) {
                level = match candidate {
                    "TRACE" => "trace",
                    "DEBUG" => "debug",
                    "WARN" => "warn",
                    "ERROR" => "error",
                    _ => "info",
                };
                message = rest.trim_start();
                break;
            }
        }
        self.push(LogEntry {
            ts: now_ms(),
            level: level.to_string(),
            target: String::new(),
            source: module_id.to_string(),
            message: message.to_string(),
        });
    }

    /// Newest-last snapshot matching the filters. `q` is a case-insensitive
    /// substring over message + target + source.
    pub fn snapshot(
        &self,
        limit: usize,
        level: Option<&str>,
        source: Option<&str>,
        q: Option<&str>,
    ) -> Vec<LogEntry> {
        let min_rank = level.map(level_rank);
        let q = q.map(str::to_lowercase).filter(|s| !s.is_empty());
        let buf = self.inner.lock().unwrap();
        let mut out: Vec<LogEntry> = buf
            .iter()
            .rev()
            .filter(|e| min_rank.is_none_or(|min| level_rank(&e.level) >= min))
            .filter(|e| source.is_none_or(|s| e.source == s))
            .filter(|e| {
                q.as_deref().is_none_or(|q| {
                    e.message.to_lowercase().contains(q)
                        || e.target.to_lowercase().contains(q)
                        || e.source.to_lowercase().contains(q)
                })
            })
            .take(limit)
            .cloned()
            .collect();
        out.reverse();
        out
    }

    /// The distinct sources currently present (for the page's source filter).
    pub fn sources(&self) -> Vec<String> {
        let buf = self.inner.lock().unwrap();
        let mut out: Vec<String> = buf.iter().map(|e| e.source.clone()).collect();
        out.sort();
        out.dedup();
        out
    }
}

/// `level` filter is a minimum severity, not an exact match: asking for warn
/// shows warn + error.
fn level_rank(level: &str) -> u8 {
    match level {
        "trace" => 0,
        "debug" => 1,
        "warn" => 3,
        "error" => 4,
        _ => 2, // info
    }
}

/// Drop ANSI escape sequences (`ESC [ ... <letter>`), keeping everything else.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                for esc in chars.by_ref() {
                    if esc.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_line_parses_level_and_strips_timestamp() {
        let buf = LogBuffer::new();
        buf.push_module_line("tv.kroma.vpn", "2026-07-16T07:39:03.457400Z  WARN kroma_vpn: wireguard bridge exited; restarting in 5s");
        let got = buf.snapshot(10, None, None, None);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].level, "warn");
        assert_eq!(got[0].source, "tv.kroma.vpn");
        assert_eq!(got[0].message, "kroma_vpn: wireguard bridge exited; restarting in 5s");
    }

    #[test]
    fn module_line_scrubs_ansi() {
        let buf = LogBuffer::new();
        buf.push_module_line(
            "tv.kroma.remote",
            "\x1b[2m2026-07-16T07:58:29.127560Z\x1b[0m \x1b[32m INFO\x1b[0m \x1b[2mkroma_module_runtime\x1b[0m\x1b[2m:\x1b[0m module process starting \x1b[3mmodule\x1b[0m\x1b[2m=\x1b[0mtv.kroma.remote",
        );
        let got = buf.snapshot(10, None, None, None);
        assert_eq!(got[0].level, "info");
        assert_eq!(
            got[0].message,
            "kroma_module_runtime: module process starting module=tv.kroma.remote"
        );
    }

    #[test]
    fn level_filter_is_a_minimum() {
        let buf = LogBuffer::new();
        buf.push_module_line("m", "INFO fine");
        buf.push_module_line("m", "WARN uh oh");
        buf.push_module_line("m", "ERROR broken");
        assert_eq!(buf.snapshot(10, Some("warn"), None, None).len(), 2);
        assert_eq!(buf.snapshot(10, Some("error"), None, None).len(), 1);
        assert_eq!(buf.snapshot(10, None, None, Some("BROKEN")).len(), 1);
    }

    #[test]
    fn capacity_evicts_oldest() {
        let buf = LogBuffer::new();
        for i in 0..(CAPACITY + 10) {
            buf.push_module_line("m", &format!("INFO line {i}"));
        }
        let got = buf.snapshot(usize::MAX, None, None, None);
        assert_eq!(got.len(), CAPACITY);
        assert!(got[0].message.ends_with("line 10"));
    }
}
