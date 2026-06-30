//! The backup container: a real ZIP holding `backup.json` (the table dump,
//! deflate-compressed at max level it's repetitive text that shrinks a lot) and
//! `assets/<name>` files (user-uploaded avatars, stored as-is since WebP is
//! already compressed). Also reads the legacy v1 format (raw JSON with avatars
//! hex-embedded) so old backups still import.

use std::io::{Cursor, Read, Write};

use anyhow::{Context, Result};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::db::BackupDoc;

/// In-memory backup: the row document plus the asset files it references.
pub type Assets = Vec<(String, Vec<u8>)>;

const MANIFEST: &str = "backup.json";
const ASSET_DIR: &str = "assets/";

/// Serialize a backup to ZIP bytes: `backup.json` + one `assets/<name>` per file.
pub fn write_zip(doc: &BackupDoc, assets: &Assets) -> Result<Vec<u8>> {
    let mut zw = ZipWriter::new(Cursor::new(Vec::new()));
    // Max deflate (0–9 on the flate2 backend) for the JSON; avatars are already
    // compressed (WebP), so storing them avoids wasted CPU for no size win.
    let json_opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(9));
    let asset_opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    zw.start_file(MANIFEST, json_opts)?;
    zw.write_all(&serde_json::to_vec_pretty(doc)?)?;
    for (name, bytes) in assets {
        zw.start_file(format!("{ASSET_DIR}{name}"), asset_opts)?;
        zw.write_all(bytes)?;
    }
    Ok(zw.finish()?.into_inner())
}

/// Read a ZIP backup → the document + its asset files.
pub fn read_zip(bytes: &[u8]) -> Result<(BackupDoc, Assets)> {
    let mut za = ZipArchive::new(Cursor::new(bytes)).context("open backup zip")?;
    let mut doc: Option<BackupDoc> = None;
    let mut assets = Assets::new();
    for i in 0..za.len() {
        let mut entry = za.by_index(i)?;
        let name = entry.name().to_string();
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        if name == MANIFEST {
            doc = Some(serde_json::from_slice(&buf).context("parse backup.json")?);
        } else if let Some(asset) = name.strip_prefix(ASSET_DIR).filter(|a| !a.is_empty()) {
            assets.push((asset.to_string(), buf));
        }
    }
    Ok((doc.context("backup.json missing from archive")?, assets))
}

/// Read a legacy v1 backup (raw JSON with avatars hex-embedded in `doc.assets`).
pub fn read_legacy_json(bytes: &[u8]) -> Result<(BackupDoc, Assets)> {
    let doc: BackupDoc = serde_json::from_slice(bytes).context("parse legacy backup json")?;
    let assets = doc.assets.iter().filter_map(|(n, h)| Some((n.clone(), hex::decode(h).ok()?))).collect();
    Ok((doc, assets))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn doc() -> BackupDoc {
        let mut tables = BTreeMap::new();
        let mut row = serde_json::Map::new();
        row.insert("id".into(), serde_json::json!("u1"));
        tables.insert("users".to_string(), vec![row]);
        BackupDoc { version: 1, exported_at: "t".into(), tables, assets: BTreeMap::new() }
    }

    #[test]
    fn zip_round_trip_carries_doc_and_assets() {
        let assets = vec![("ab12.webp".to_string(), b"WEBP".to_vec())];
        let bytes = write_zip(&doc(), &assets).unwrap();
        assert_eq!(&bytes[..4], b"PK\x03\x04", "is a real zip");

        let (back, got) = read_zip(&bytes).unwrap();
        assert_eq!(back.tables["users"][0]["id"], serde_json::json!("u1"));
        assert_eq!(got, assets);
    }

    #[test]
    fn legacy_json_decodes_hex_assets() {
        let mut d = doc();
        d.assets.insert("ab12.webp".into(), hex::encode(b"WEBP"));
        let bytes = serde_json::to_vec(&d).unwrap();
        let (_, got) = read_legacy_json(&bytes).unwrap();
        assert_eq!(got, vec![("ab12.webp".to_string(), b"WEBP".to_vec())]);
    }
}
