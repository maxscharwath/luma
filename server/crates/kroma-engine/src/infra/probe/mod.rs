//! Metadata extraction via the `ffprobe` CLI.
//!
//! We never transcode. ffprobe is invoked purely to read stream metadata. If it
//! is missing or fails on a given file we degrade gracefully: codec is inferred
//! from the container extension and unknown fields are left null.
//!
//! Split into the CLI invocation / orchestration ([`run`]) and the JSON-output
//! [`parse`]-into-model step.

mod markers;
mod parse;
mod run;

use crate::model::{AudioStream, SubtitleTrack, VideoStream};

pub use markers::markers_from_chapters;
pub use run::*;

/// Result of probing a file. All fields are best-effort.
#[derive(Debug, Default)]
pub struct ProbeResult {
    pub duration_ms: Option<u64>,
    pub video: Option<VideoStream>,
    /// Representative (first) audio track, for badges. `audio_tracks.first()`.
    pub audio: Option<AudioStream>,
    /// Every audio track, in container order (audio-relative index = position).
    pub audio_tracks: Vec<AudioStream>,
    pub subtitles: Vec<SubtitleTrack>,
    /// Embedded chapters (from `ffprobe -show_chapters`), in file order. Empty
    /// when the container has none. Used to derive intro/credits markers.
    pub chapters: Vec<Chapter>,
}

/// One embedded chapter: a titled time range (milliseconds).
#[derive(Debug, Clone)]
pub struct Chapter {
    pub start_ms: u64,
    pub end_ms: u64,
    pub title: Option<String>,
}
