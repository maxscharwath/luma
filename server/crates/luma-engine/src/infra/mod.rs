//! Outbound adapters: OS/process/network/filesystem integrations.
//!
//! These modules shell out to external tools (`ffprobe`, `ffmpeg`, `curl`),
//! touch the filesystem, advertise over mDNS, sample system metrics, and bridge
//! live events the edges where LUMA talks to the world outside the process.

pub mod probe;
pub mod ffmpeg_gate;
pub mod hls;
pub mod metadata;
pub mod llm;
pub mod image;
pub mod storyboard;
pub mod subtitles;
pub mod theme;
pub mod stream;
pub mod watch;
pub mod metrics;
pub mod events;
pub mod logbuf;
