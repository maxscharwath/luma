//! EBU R128 loudness analysis per (file, audio track): the data behind the
//! "high dynamics / quiet dialogue" badge and the player's volume-boost
//! suggestion. One row per analyzed track; written by the `pipeline.loudness`
//! stage. Raw measured values are kept so remediation can reuse them.

use super::*;
use kroma_domain::{AudioAnalysis, AudioVerdict};

fn verdict_str(v: AudioVerdict) -> &'static str {
    match v {
        AudioVerdict::Ok => "ok",
        AudioVerdict::HighDynamics => "highDynamics",
        AudioVerdict::QuietDialog => "quietDialog",
    }
}

fn verdict_from_str(s: &str) -> Option<AudioVerdict> {
    match s {
        "ok" => Some(AudioVerdict::Ok),
        "highDynamics" => Some(AudioVerdict::HighDynamics),
        "quietDialog" => Some(AudioVerdict::QuietDialog),
        _ => None,
    }
}

fn row_to_analysis(r: &Row) -> rusqlite::Result<(String, Option<AudioAnalysis>)> {
    let file_id: String = r.get(0)?;
    let verdict: String = r.get(5)?;
    // An unknown verdict (from a future version) drops the row instead of
    // breaking older servers reading a newer DB.
    let analysis = verdict_from_str(&verdict).map(|v| AudioAnalysis {
        lufs_i: r.get(1).unwrap_or(0.0),
        lra: r.get(2).unwrap_or(0.0),
        true_peak: r.get(3).unwrap_or(0.0),
        dialog_lufs: r.get(4).ok().flatten(),
        verdict: v,
    });
    Ok((file_id, analysis))
}

/// Upsert the analysis of one audio track of one file.
pub fn set_audio_analysis(
    pool: &Pool,
    file_id: &str,
    track_index: u32,
    analysis: &AudioAnalysis,
) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO audio_analysis \
            (file_id, track_index, lufs_i, lra, true_peak, dialog_lufs, verdict, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now')) \
         ON CONFLICT(file_id, track_index) DO UPDATE SET \
           lufs_i = excluded.lufs_i, lra = excluded.lra, true_peak = excluded.true_peak, \
           dialog_lufs = excluded.dialog_lufs, verdict = excluded.verdict, \
           updated_at = excluded.updated_at",
        params![
            file_id,
            track_index,
            analysis.lufs_i,
            analysis.lra,
            analysis.true_peak,
            analysis.dialog_lufs,
            verdict_str(analysis.verdict),
        ],
    )?;
    Ok(())
}

/// The analysis of one file's analyzed track (only the default track is
/// analyzed today; lowest track index wins should several exist).
pub fn audio_analysis_for_file(
    conn: &Connection,
    file_id: &str,
) -> rusqlite::Result<Option<AudioAnalysis>> {
    let mut stmt = conn.prepare(
        "SELECT file_id, lufs_i, lra, true_peak, dialog_lufs, verdict \
         FROM audio_analysis WHERE file_id = ?1 ORDER BY track_index LIMIT 1",
    )?;
    let mut rows = stmt.query_map(params![file_id], row_to_analysis)?;
    match rows.next() {
        Some(r) => Ok(r?.1),
        None => Ok(None),
    }
}

/// [`audio_analysis_for_file`] over many files in one query per id-chunk,
/// keyed by file id (absent ids simply have no analysis yet).
pub(crate) fn audio_analysis_for_files(
    conn: &Connection,
    file_ids: &[&str],
) -> rusqlite::Result<std::collections::HashMap<String, AudioAnalysis>> {
    let mut out = std::collections::HashMap::new();
    for chunk in file_ids.chunks(super::IN_CHUNK) {
        let ph = vec!["?"; chunk.len()].join(",");
        let mut stmt = conn.prepare(&format!(
            "SELECT file_id, lufs_i, lra, true_peak, dialog_lufs, verdict \
             FROM audio_analysis WHERE file_id IN ({ph}) ORDER BY track_index",
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), row_to_analysis)?;
        for row in rows {
            let (file_id, analysis) = row?;
            if let Some(a) = analysis {
                // Rows arrive track-index-ascending; keep the first per file.
                out.entry(file_id).or_insert(a);
            }
        }
    }
    Ok(out)
}

/// `(id, "mtime:size")` of every **probed** file, for the `pipeline.loudness`
/// stage's enumeration: unprobed files stay out of scope (their track layout is
/// unknown) and flow in once the probe stage lands.
pub fn analyzable_file_sigs(pool: &Pool) -> Result<Vec<(String, String)>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, COALESCE(mtime,0) || ':' || COALESCE(size,0) FROM files WHERE probed = 1",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// `(abs_path, audio_tracks_json)` for one file the loudness stage's
/// per-subject lookup. `None` if the file row is gone.
pub fn loudness_target(pool: &Pool, file_id: &str) -> Result<Option<(String, String)>> {
    let conn = pool.get()?;
    let mut stmt =
        conn.prepare("SELECT abs_path, audio_tracks FROM files WHERE id = ?1 AND probed = 1")?;
    let mut rows =
        stmt.query_map(params![file_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}
