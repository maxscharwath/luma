//! On-demand HLS **audio-transcode** sessions.
//!
//! LUMA's streaming policy is direct-play: [`crate::infra::stream`] serves original
//! bytes and the server never re-encodes *video*. The one exception is audio.
//! HEVC files routinely carry AC3/EAC3/DTS/TrueHD tracks that browsers
//! (Chrome/Firefox) refuse to decode for licensing reasons, which yields
//! video-but-no-sound. For those clients we expose an HLS variant that *copies*
//! the video stream untouched and transcodes only the audio to stereo AAC
//! cheap (no video re-encode, runs many× realtime) and surgical.
//!
//! A session is one running `ffmpeg` writing fragmented-MP4 HLS segments into a
//! per-item directory under `<data>/transcode/`. The playlist is served as it
//! grows (`event` type); idle sessions are reaped after `IDLE_TIMEOUT`.
//!
//! Split into the [`ffmpeg`] command/arg construction and the running-process
//! [`session`] registry that spawns, serves, and reaps them.

mod ffmpeg;
mod session;

pub use ffmpeg::MasterTrack;
pub use session::Sessions;
