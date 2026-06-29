//! Outbound adapters: OS/process/network/filesystem integrations.
//!
//! These modules shell out to external tools (`ffprobe`, `ffmpeg`, `curl`),
//! touch the filesystem, advertise over mDNS, sample system metrics, and bridge
//! live events — the edges where LUMA talks to the world outside the process.

pub mod probe;
pub mod transcode;
pub mod metadata;
pub mod embed;
pub mod image;
pub mod stream;
pub mod discovery;
pub mod watch;
pub mod metrics;
pub mod events;
