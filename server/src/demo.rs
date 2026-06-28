//! Built-in demo content. Seeded when there are no media dirs configured or a
//! scan turns up nothing, so fresh clients have something to render. Demo items
//! have `rel_path == None` / `abs_path == None` and cannot be streamed.

use crate::model::{
    AudioStream, Kind, Library, LibraryKind, MediaFile, MediaItem, Show, SubtitleTrack, VideoStream,
};
use crate::scan::{now_iso8601, short_hash, ScanData};

/// Build a single synthetic (already-probed) file mirroring a demo item, so the
/// new multi-file schema is consistent for demo content too.
fn demo_file(item: &MediaItem) -> MediaFile {
    let fid = short_hash(&format!("{}|file", item.id));
    MediaFile {
        id: fid.clone(),
        rel_path: item.rel_path.clone(),
        container: item.container.clone(),
        duration_ms: item.duration_ms,
        video: item.video.clone(),
        audio: item.audio.clone(),
        audio_tracks: item.audio_tracks.clone(),
        subtitles: item.subtitles.clone(),
        size: None,
        edition: None,
        probed: true,
        // Demo files have no real path on disk; use a synthetic URI so the DB
        // `files` row (abs_path NOT NULL UNIQUE) is satisfied. It can't be
        // streamed, which matches demo behaviour.
        abs_path: Some(format!("demo://{fid}")),
    }
}

/// Attach a single representative file + defaultFileId to a demo item.
fn with_demo_file(mut item: MediaItem) -> MediaItem {
    let file = demo_file(&item);
    item.default_file_id = Some(file.id.clone());
    item.files = vec![file];
    item
}

/// Build the demo data: a Movies library and a Shows library (2 shows, 4 episodes).
pub fn demo_data() -> ScanData {
    let added = now_iso8601();
    let movies_lib = short_hash("demo://movies");
    let shows_lib = short_hash("demo://shows");

    let planet_earth = show_id(&shows_lib, "Planet Earth II");
    let the_office = show_id(&shows_lib, "The Office");

    let items = vec![
        movie("demo://blade-runner-2049", "Blade Runner 2049", Some(2017), Some(9_780_000), "mkv",
            video("hevc", 3840, 2160, true, 10),
            vec![audio("truehd", 8, Some("eng")), audio("eac3", 6, Some("fra")), audio("aac", 2, Some("eng"))],
            vec![sub(Some("eng"), "subrip"), sub(Some("fra"), "subrip")], &movies_lib, &added),
        movie("demo://dune-part-two", "Dune Part Two", Some(2024), Some(9_960_000), "mkv",
            video("hevc", 3840, 2160, true, 10),
            vec![audio("eac3", 6, Some("eng")), audio("ac3", 6, Some("fra"))],
            vec![sub(Some("eng"), "subrip")], &movies_lib, &added),
        movie("demo://the-matrix", "The Matrix", Some(1999), Some(8_160_000), "mp4",
            video("h264", 1920, 1080, false, 8),
            vec![audio("ac3", 6, Some("eng")), audio("aac", 2, Some("fra"))],
            vec![sub(Some("eng"), "mov_text")], &movies_lib, &added),
        movie("demo://spirited-away", "Spirited Away", Some(2001), Some(7_500_000), "mkv",
            video("h264", 1920, 1040, false, 8),
            vec![audio("flac", 2, Some("jpn")), audio("aac", 2, Some("eng"))],
            vec![sub(Some("eng"), "ass"), sub(Some("jpn"), "ass")], &movies_lib, &added),
        movie("demo://big-buck-bunny", "Big Buck Bunny", Some(2008), Some(596_000), "webm",
            video("vp9", 1280, 720, false, 8), vec![audio("opus", 2, None)], vec![], &movies_lib, &added),
        movie("demo://sintel-av1", "Sintel", Some(2010), Some(888_000), "mp4",
            video("av1", 4096, 1744, false, 10), vec![audio("aac", 2, Some("eng"))],
            vec![sub(Some("eng"), "subrip")], &movies_lib, &added),

        episode("demo://pe2-s01e01", &planet_earth, "Planet Earth II", 1, 1, "Islands",
            Some(2016), Some(2_940_000), "mkv", video("hevc", 3840, 2160, true, 10),
            vec![audio("eac3", 6, Some("eng")), audio("aac", 2, Some("fra"))], &shows_lib, &added),
        episode("demo://pe2-s01e02", &planet_earth, "Planet Earth II", 1, 2, "Mountains",
            Some(2016), Some(2_940_000), "mkv", video("hevc", 3840, 2160, true, 10),
            vec![audio("eac3", 6, Some("eng"))], &shows_lib, &added),
        episode("demo://office-s02e01", &the_office, "The Office", 2, 1, "The Dundies",
            Some(2005), Some(1_320_000), "mp4", video("h264", 1280, 720, false, 8),
            vec![audio("aac", 2, Some("eng"))], &shows_lib, &added),
        episode("demo://office-s02e02", &the_office, "The Office", 2, 2, "Sexual Harassment",
            Some(2005), Some(1_320_000), "mp4", video("h264", 1280, 720, false, 8),
            vec![audio("aac", 2, Some("eng"))], &shows_lib, &added),
    ];

    let movies_count = items.iter().filter(|i| i.library == movies_lib).count();
    let shows_count = items.iter().filter(|i| i.library == shows_lib).count();

    let shows = vec![
        Show { id: planet_earth, title: "Planet Earth II".into(), year: Some(2016), library: shows_lib.clone(),
            season_count: 1, episode_count: 2, video: None, added_at: added.clone(), metadata: None },
        Show { id: the_office, title: "The Office".into(), year: Some(2005), library: shows_lib.clone(),
            season_count: 1, episode_count: 2, video: None, added_at: added.clone(), metadata: None },
    ];

    let libraries = vec![
        Library { id: movies_lib, name: "Films (Démo)".into(), kind: LibraryKind::Movies,
            path: "<demo>".into(), item_count: movies_count },
        Library { id: shows_lib, name: "Séries (Démo)".into(), kind: LibraryKind::Shows,
            path: "<demo>".into(), item_count: shows_count },
    ];

    // Demo files are synthetic, so there are no real mtimes to carry.
    ScanData { libraries, shows, items, ..Default::default() }
}

fn show_id(lib: &str, title: &str) -> String {
    short_hash(&format!("{lib}|show|{}", title.to_lowercase()))
}

#[allow(clippy::too_many_arguments)]
fn movie(
    seed: &str, title: &str, year: Option<u32>, duration_ms: Option<u64>, container: &str,
    video: Option<VideoStream>, audio_tracks: Vec<AudioStream>, subtitles: Vec<SubtitleTrack>,
    library: &str, added_at: &str,
) -> MediaItem {
    let audio_tracks = tracks(audio_tracks);
    with_demo_file(MediaItem {
        id: short_hash(seed), title: title.into(), kind: Kind::Movie, year, duration_ms,
        container: container.into(), video, audio: audio_tracks.first().cloned(), audio_tracks,
        subtitles, library: library.into(),
        show_id: None, show_title: None, season: None, episode: None, episode_end: None,
        episode_title: None, rel_path: None, added_at: added_at.into(), metadata: None, abs_path: None,
        files: Vec::new(), default_file_id: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn episode(
    seed: &str, show: &str, show_title: &str, season: u32, episode: u32, episode_title: &str,
    year: Option<u32>, duration_ms: Option<u64>, container: &str, video: Option<VideoStream>,
    audio_tracks: Vec<AudioStream>, library: &str, added_at: &str,
) -> MediaItem {
    let audio_tracks = tracks(audio_tracks);
    with_demo_file(MediaItem {
        id: short_hash(seed), title: episode_title.into(), kind: Kind::Episode, year, duration_ms,
        container: container.into(), video, audio: audio_tracks.first().cloned(), audio_tracks,
        subtitles: vec![sub(Some("eng"), "subrip")],
        library: library.into(), show_id: Some(show.into()), show_title: Some(show_title.into()),
        season: Some(season), episode: Some(episode), episode_end: None,
        episode_title: Some(episode_title.into()), rel_path: None, added_at: added_at.into(),
        metadata: None, abs_path: None, files: Vec::new(), default_file_id: None,
    })
}

fn video(codec: &str, width: u32, height: u32, hdr: bool, bit_depth: u32) -> Option<VideoStream> {
    Some(VideoStream { codec: codec.into(), width: Some(width), height: Some(height), hdr, bit_depth: Some(bit_depth) })
}

fn audio(codec: &str, channels: u32, language: Option<&str>) -> AudioStream {
    AudioStream {
        index: 0,
        codec: codec.into(),
        channels: Some(channels),
        language: language.map(Into::into),
        title: None,
        default: false,
    }
}

/// Assign sequential audio-relative indices and mark the first track default.
fn tracks(list: Vec<AudioStream>) -> Vec<AudioStream> {
    list.into_iter()
        .enumerate()
        .map(|(i, mut a)| {
            a.index = i as u32;
            a.default = i == 0;
            a
        })
        .collect()
}

fn sub(language: Option<&str>, codec: &str) -> SubtitleTrack {
    SubtitleTrack { language: language.map(Into::into), codec: codec.into() }
}
