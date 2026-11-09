//! A folder's identity: the `_meta` table in its `memory.db`, read without changing
//! the file, and stamped once on a store that has none.

use super::*;

pub(super) fn open_read_only(db: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|e| format!("Couldn't open {}: {}", db.display(), e))
}

pub(super) fn open_read_write(db: &Path) -> Result<Connection, String> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|e| format!("Couldn't open {}: {}", db.display(), e))?;
    conn.busy_timeout(std::time::Duration::from_secs(30))
        .map_err(|e| format!("Couldn't open {}: {}", db.display(), e))?;
    Ok(conn)
}

pub(super) fn get_meta(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM _meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
}

pub(super) fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute("INSERT OR REPLACE INTO _meta (key, value) VALUES (?1, ?2)", [key, value])
        .map(|_| ())
        .map_err(|e| format!("Couldn't save {}: {}", key, e))
}

/// Change a value and keep the old one beside it, dated, so a description's
/// history survives every edit.
pub(super) fn replace_meta(db: &Path, key: &str, value: &str) -> Result<(), String> {
    let mut conn = open_read_write(db)?;
    let tx = conn.transaction().map_err(|e| format!("Couldn't save {}: {}", key, e))?;
    if let Some(old) = get_meta(&tx, key) {
        // The earlier wording is kept under key@time. Two changes in the same second
        // would share that key, so a later one is numbered (#2, #3) rather than lost.
        let stamp = format!("{}@{}", key, now());
        let mut n = 1;
        loop {
            let name = if n == 1 { stamp.clone() } else { format!("{}#{}", stamp, n) };
            let added = tx
                .execute("INSERT OR IGNORE INTO _meta (key, value) VALUES (?1, ?2)", [name, old.clone()])
                .map_err(|e| format!("Couldn't save {}: {}", key, e))?;
            if added == 1 {
                break;
            }
            n += 1;
        }
    }
    set_meta(&tx, key, value)?;
    tx.commit().map_err(|e| format!("Couldn't save {}: {}", key, e))
}

/// The folder whose database is at `db`, read without changing anything on disk.
pub fn identity_at(db: &Path) -> Option<Folder> {
    read_identity(db)
}

/// Read a file's identity. `None` means it isn't a folder, or can't be read.
pub(super) fn read_identity(db: &Path) -> Option<Folder> {
    identity_from(&open_read_only(db).ok()?, db)
}

pub(super) fn identity_from(conn: &Connection, db: &Path) -> Option<Folder> {
    let uuid = get_meta(conn, KEY_UUID)?;
    Some(Folder {
        name: get_meta(conn, KEY_NAME).unwrap_or_else(|| uuid.chars().take(SHORT_ID).collect()),
        description: get_meta(conn, KEY_DESCRIPTION).unwrap_or_default(),
        created: get_meta(conn, KEY_CREATED).unwrap_or_default(),
        uuid,
        db_path: db.to_path_buf(),
    })
}

/// A store written by Geniuz that has no folder identity yet.
pub(super) fn is_geniuz_store(db: &Path) -> bool {
    open_read_only(db)
        .ok()
        .and_then(|c| get_meta(&c, "product"))
        .map(|p| crate::db::OWN_PRODUCTS.contains(&p.as_str()))
        .unwrap_or(false)
}

/// The table holding a store's memories and its UUID column. Files from before
/// Geniuz 4.0 used other names; they are brought forward by a script, not read here.
pub(super) fn memory_table(conn: &Connection) -> Result<(&'static str, &'static str), String> {
    if conn.prepare("SELECT memory_uuid FROM memories LIMIT 1").is_ok() {
        return Ok(("memories", "memory_uuid"));
    }
    Err("It has no memory table from Geniuz 4.0 or later.".to_string())
}

/// Every memory ID in a store, normalised for comparing.
pub(super) fn memory_ids(db: &Path) -> Vec<String> {
    let read = || -> Option<Vec<String>> {
        let conn = open_read_only(db).ok()?;
        let (table, column) = memory_table(&conn).ok()?;
        let mut stmt = conn.prepare(&format!("SELECT {} FROM {}", column, table)).ok()?;
        let ids = stmt.query_map([], |r| r.get::<_, String>(0)).ok()?.flatten();
        Some(ids.map(|id| id.to_lowercase()).collect())
    };
    read().unwrap_or_default()
}

/// The UUID for a store getting its first identity. It comes from the store's
/// oldest memory, so if a 4.x file was copied to another machine and both copies
/// upgrade, they get the same UUID rather than giving one set of memories two
/// identities. Only an empty store, with nothing to duplicate, gets a random one.
///
/// This is best effort across machines: importing a memory dated earlier into one
/// copy before upgrading changes which is oldest. On one machine, `add` compares
/// the memories themselves.
pub(super) fn identity_for(conn: &Connection) -> Result<String, String> {
    let (table, column) = memory_table(conn)?;
    let sql = format!("SELECT {c} FROM {t} ORDER BY created_at ASC, {c} ASC LIMIT 1", c = column, t = table);
    let oldest: Option<String> = conn
        .query_row(&sql, [], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Couldn't read its memories: {}", e))?;
    let id = match oldest {
        Some(memory) => {
            let name = format!("geniuz.folder:{}", memory.to_lowercase());
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, name.as_bytes())
        }
        None => uuid::Uuid::new_v4(),
    };
    Ok(id.to_string().to_uppercase())
}

/// Give a store an identity if it has none. Safe to repeat, and safe beside a
/// running older version, which ignores these rows.
pub(super) fn stamp(db: &Path, name: &str) -> Result<(), String> {
    if read_identity(db).is_some() {
        return Ok(());
    }
    let conn = open_read_write(db)?;
    let uuid = identity_for(&conn).map_err(|e| format!("Couldn't identify {}: {}", db.display(), e))?;
    for (key, value) in [(KEY_UUID, uuid.as_str()), (KEY_NAME, name), (KEY_CREATED, now().as_str())] {
        conn.execute("INSERT OR IGNORE INTO _meta (key, value) VALUES (?1, ?2)", [key, value])
            .map_err(|e| format!("Couldn't update {}: {}", db.display(), e))?;
    }
    Ok(())
}
