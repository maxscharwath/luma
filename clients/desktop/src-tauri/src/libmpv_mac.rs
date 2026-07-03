// In-process libmpv playback for macOS.
//
// Unlike Linux (where we drive the mpv BINARY over an IPC socket - see mpv.rs), macOS
// can't embed a separate-process window (NSView pointers are process-local), so here
// we link libmpv IN-PROCESS and (later) drive its OpenGL render API into an NSView
// behind the transparent WKWebView. This gives native VideoToolbox decode + direct
// play of MKV / AC3 5.1 with no server remux.
//
// Staging: [1 DONE] link+init; [2 THIS] open a stream + decode audio (no video window
// yet); [3] the OpenGL render layer; [4] the frontend bridge; [5] bundle libmpv.dylib.

use std::thread;
use std::time::Instant;

use libmpv2::{Mpv, MpvStr};

/// STAGE 2: open a LUMA stream in-process and decode its AUDIO (no video window yet,
/// `vo=null`). Runs on a background thread and logs the audio codec + an advancing
/// position, so we can confirm libmpv opened the stream and is decoding; it also
/// outputs sound. Triggered by the `LUMA_MPV_TEST_URL` env var (off for normal runs).
pub fn play_test(url: String) {
    thread::spawn(move || {
        let mpv = match Mpv::with_initializer(|init| {
            init.set_property("vo", "null")?; // audio only until the render layer (Stage 3)
            init.set_property("terminal", false)?;
            Ok(())
        }) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("LUMA libmpv: init failed: {e:?}");
                return;
            }
        };
        eprintln!("LUMA libmpv: loading {url}");
        if let Err(e) = mpv.command("loadfile", &[url.as_str()]) {
            eprintln!("LUMA libmpv: loadfile failed: {e:?}");
            return;
        }
        // CRUCIAL: drain mpv's event queue via wait_event, or the core stalls once
        // the queue fills (that is why a poll-only loop never started playback).
        let started = Instant::now();
        let mut last = u64::MAX;
        while started.elapsed().as_secs() < 15 {
            let _ = mpv.wait_event(0.5);
            let secs = started.elapsed().as_secs();
            if secs != last {
                last = secs;
                let pos = mpv.get_property::<f64>("time-pos").unwrap_or(-1.0);
                let dur = mpv.get_property::<f64>("duration").unwrap_or(-1.0);
                let acodec = mpv
                    .get_property::<MpvStr>("audio-codec-name")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| "-".into());
                let ach = mpv
                    .get_property::<i64>("audio-params/channel-count")
                    .unwrap_or(-1);
                eprintln!("LUMA libmpv: pos={pos:.1}s dur={dur:.0}s audio={acodec} ch={ach}");
            }
        }
        eprintln!("LUMA libmpv: Stage 2 test complete");
    });
}
