//! Library reads: the scanned library roots and their item counts.

use super::*;

use crate::model::{Library, LibraryKind};

pub fn list_libraries(pool: &Pool) -> Result<Vec<Library>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id,name,kind,path,(SELECT COUNT(*) FROM items i WHERE i.library=l.id) \
         FROM libraries l ORDER BY name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Library {
            id: r.get(0)?,
            name: r.get(1)?,
            kind: parse_library_kind(&r.get::<_, String>(2)?),
            path: r.get(3)?,
            item_count: r.get::<_, i64>(4)? as usize,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn parse_library_kind(s: &str) -> LibraryKind {
    match s {
        "shows" => LibraryKind::Shows,
        "mixed" => LibraryKind::Mixed,
        _ => LibraryKind::Movies,
    }
}
