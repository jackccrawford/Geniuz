//! Addresses and search across every folder.

use super::*;

/// Split an address like `13625F6B:3487B9B0` into folder and memory.
pub fn parse_address(text: &str) -> Option<(String, String)> {
    let (folder, memory) = text.trim().split_once(':')?;
    let hex8 = |s: &str| s.len() == SHORT_ID && s.chars().all(|c| c.is_ascii_hexdigit());
    (hex8(folder) && hex8(memory)).then(|| (folder.to_uppercase(), memory.to_uppercase()))
}

/// What to look for across every folder.
pub enum Search<'a> {
    Recent,
    Keyword(&'a str),
    Semantic(&'a str, Option<&'a dyn EmbeddingBackend>),
}

/// One result, with the folder it came from.
pub struct Hit {
    pub folder: Folder,
    pub entry: MemoryEntry,
}

impl Hit {
    /// The address that opens this memory directly.
    pub fn address(&self) -> String {
        let memory: String = self.entry.memory_uuid.chars().take(SHORT_ID).collect();
        format!("{}:{}", self.folder.short_id(), memory.to_uppercase())
    }
}

/// The result of searching every folder.
pub struct Across {
    pub hits: Vec<Hit>,
    pub searched: usize,
    pub memories: usize,
    /// Folders that couldn't be read, reported rather than quietly left out.
    pub skipped: Vec<String>,
}

/// Search every visible folder and merge: best score first, then newest.
pub fn search_across(reg: &Registry, search: &Search, window: &Window, limit: usize) -> Across {
    let mut out = Across { hits: Vec::new(), searched: 0, memories: 0, skipped: Vec::new() };
    for folder in reg.scan().folders {
        let found = crate::db::DatabaseManager::open_existing(&folder.db_path.to_string_lossy()).and_then(|db| {
            let entries = match search {
                Search::Recent => db.recent_in(window, limit),
                Search::Keyword(q) => db.keyword_search_in(q, window, limit),
                // As in one folder: failing to search by meaning falls back to words.
                Search::Semantic(q, backend) => db
                    .semantic_search_with(q, window, limit, *backend)
                    .or_else(|_| db.keyword_search_in(q, window, limit)),
            }?;
            Ok((entries, db.count_in(window)?))
        });
        match found {
            Ok((entries, count)) => {
                out.searched += 1;
                out.memories += count;
                out.hits.extend(entries.into_iter().map(|entry| Hit { folder: folder.clone(), entry }));
            }
            Err(_) => out.skipped.push(folder.name),
        }
    }
    out.hits.sort_by(|a, b| {
        b.entry
            .score
            .unwrap_or(f32::MIN)
            .total_cmp(&a.entry.score.unwrap_or(f32::MIN))
            .then_with(|| b.entry.created_at.cmp(&a.entry.created_at))
    });
    out.hits.truncate(limit);
    out
}
