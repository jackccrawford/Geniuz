//! Database layer — signals, embedding cache, search

use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

pub struct DatabaseManager {
    pub db_path: String,
}

impl DatabaseManager {
    pub fn new(db_path: &str) -> Result<Self, String> {
        if let Some(parent) = Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        let conn = Connection::open(db_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| format!("Failed to enable WAL: {}", e))?;
        conn.busy_timeout(std::time::Duration::from_secs(30))
            .map_err(|e| format!("Failed to set timeout: {}", e))?;

        // Check for existing schema
        let has_memories: bool = conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='memories'",
            [], |_| Ok(true),
        ).unwrap_or(false);

        let has_legacy: bool = conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='signals'",
            [], |_| Ok(true),
        ).unwrap_or(false);

        if has_legacy && !has_memories {
            // Migrate old schema: signals → memories, signal_uuid → memory_uuid
            conn.execute_batch("
                ALTER TABLE signals RENAME TO memories;
                ALTER TABLE memories RENAME COLUMN signal_uuid TO memory_uuid;
                ALTER TABLE signal_embeddings RENAME TO memory_embeddings;
                ALTER TABLE memory_embeddings RENAME COLUMN signal_uuid TO memory_uuid;
                DROP VIEW IF EXISTS signal_chains;
                DROP TRIGGER IF EXISTS prevent_signal_delete;
                DROP TRIGGER IF EXISTS prevent_signal_update;
            ").map_err(|e| format!("Failed to migrate schema: {}", e))?;
            // Recreate views and triggers with new names
            let schema = include_str!("../schema/schema.sql");
            conn.execute_batch(schema)
                .map_err(|e| format!("Failed to apply schema after migration: {}", e))?;
            eprintln!("[geniuz] Migrated legacy schema (signals → memories)");
        } else if !has_memories {
            // Fresh database — create from scratch
            let schema = include_str!("../schema/schema.sql");
            conn.execute_batch(schema)
                .map_err(|e| format!("Failed to init schema: {}", e))?;
        }

        Ok(Self { db_path: db_path.to_string() })
    }

    fn conn(&self) -> Result<Connection, String> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;
        conn.busy_timeout(std::time::Duration::from_secs(30))
            .map_err(|e| format!("Failed to set timeout: {}", e))?;
        Ok(conn)
    }

    // =========================================================================
    // SIGNAL (write)
    // =========================================================================

    /// Insert a signal with optional gist and parent. Returns short UUID.
    /// Embeds content inline if no backend is provided (creates one per call).
    pub fn signal(
        &self, content: &str, gist: Option<&str>, parent: Option<&str>,
        created_at: Option<&str>,
    ) -> Result<String, String> {
        self.signal_with_backend(content, gist, parent, created_at, None)
    }

    /// Insert a signal, optionally reusing a pre-created embedding backend.
    pub fn signal_with_backend(
        &self, content: &str, gist: Option<&str>, parent: Option<&str>,
        created_at: Option<&str>,
        backend: Option<&dyn geniuz::embedding::EmbeddingBackend>,
    ) -> Result<String, String> {
        if content.trim().is_empty() {
            return Err("Content cannot be empty".to_string());
        }

        let uuid = uuid::Uuid::new_v4().to_string().to_uppercase();
        let auto_gist = gist.map(|g| g.to_string()).unwrap_or_else(|| {
            let trimmed = content.trim();
            if trimmed.len() <= 200 { trimmed.to_string() }
            else { format!("{}...", &trimmed[..197]) }
        });

        let payload = serde_json::json!({
            "content": content.trim(),
            "gist": auto_gist,
        });
        let payload_str = serde_json::to_string(&payload)
            .map_err(|e| format!("Failed to serialize: {}", e))?;

        let parent_uuid = if let Some(p) = parent {
            self.resolve_uuid(p)?.unwrap_or(uuid.clone())
        } else {
            uuid.clone()
        };

        // Embed content first (before touching the database)
        let embedding = if let Some(b) = backend {
            match b.embed(content) {
                Ok(emb) => Some(emb),
                Err(e) => { eprintln!("[geniuz] Embedding failed for {}: {}", &uuid[..8], e); None }
            }
        } else {
            match geniuz::embedding::embed_content(content) {
                Ok(emb) => Some(emb),
                Err(e) => { eprintln!("[geniuz] Embedding failed for {}: {}", &uuid[..8], e); None }
            }
        };

        // Insert signal + embedding in one transaction
        let conn = self.conn()?;
        let tx = conn.unchecked_transaction()
            .map_err(|e| format!("Failed to begin transaction: {}", e))?;
        if let Some(ts) = created_at {
            tx.execute(
                "INSERT INTO memories (memory_uuid, payload, created_at, parent_uuid) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![&uuid, &payload_str, ts, &parent_uuid],
            ).map_err(|e| format!("Failed to insert: {}", e))?;
        } else {
            tx.execute(
                "INSERT INTO memories (memory_uuid, payload, created_at, parent_uuid) VALUES (?1, ?2, datetime('now', 'utc'), ?3)",
                rusqlite::params![&uuid, &payload_str, &parent_uuid],
            ).map_err(|e| format!("Failed to insert: {}", e))?;
        }
        if let Some(ref emb) = embedding {
            let blob = geniuz::embedding::embedding_to_blob(emb);
            tx.execute(
                "INSERT OR REPLACE INTO memory_embeddings (memory_uuid, embedding) VALUES (?1, ?2)",
                rusqlite::params![&uuid, blob],
            ).map_err(|e| format!("Failed to cache embedding: {}", e))?;
        }
        tx.commit().map_err(|e| format!("Failed to commit: {}", e))?;

        Ok(uuid[..8].to_string())
    }

    // =========================================================================
    // TUNE (read)
    // =========================================================================

    pub fn recent(&self, limit: usize) -> Result<Vec<SignalEntry>, String> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT memory_uuid,
                    COALESCE(json_extract(payload, '$.gist'), substr(json_extract(payload, '$.content'), 1, 200)) as gist,
                    created_at, parent_uuid
             FROM memories ORDER BY created_at DESC LIMIT ?1"
        ).map_err(|e| format!("Query failed: {}", e))?;

        let rows = stmt.query_map(rusqlite::params![limit as i32], |row| {
            let uuid: String = row.get(0)?;
            let parent: Option<String> = row.get(3)?;
            let display_parent = parent.filter(|p| p != &uuid);
            Ok(SignalEntry {
                memory_uuid: uuid, gist: row.get(1)?, created_at: row.get(2)?,
                parent_uuid: display_parent, content: None, score: None,
            })
        }).map_err(|e| format!("Query failed: {}", e))?;

        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// Get signals created after a given timestamp, ordered oldest first.
    pub fn since(&self, timestamp: &str, limit: usize) -> Result<Vec<SignalEntry>, String> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT memory_uuid,
                    COALESCE(json_extract(payload, '$.gist'), substr(json_extract(payload, '$.content'), 1, 200)) as gist,
                    created_at, parent_uuid, json_extract(payload, '$.content')
             FROM memories WHERE created_at > ?1 ORDER BY created_at ASC LIMIT ?2"
        ).map_err(|e| format!("Query failed: {}", e))?;

        let rows = stmt.query_map(rusqlite::params![timestamp, limit as i32], |row| {
            let uuid: String = row.get(0)?;
            let parent: Option<String> = row.get(3)?;
            let display_parent = parent.filter(|p| p != &uuid);
            Ok(SignalEntry {
                memory_uuid: uuid, gist: row.get(1)?, created_at: row.get(2)?,
                parent_uuid: display_parent, content: row.get(4)?, score: None,
            })
        }).map_err(|e| format!("Query failed: {}", e))?;

        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// Get the created_at timestamp of a signal by UUID prefix.
    pub fn get_signal_timestamp(&self, uuid_prefix: &str) -> Result<Option<String>, String> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT created_at FROM memories WHERE memory_uuid LIKE ?1 LIMIT 1",
            rusqlite::params![format!("{}%", uuid_prefix.to_uppercase())],
            |row| row.get(0),
        ).optional().map_err(|e| format!("Query failed: {}", e))
    }

    pub fn keyword_search(&self, query: &str, limit: usize) -> Result<Vec<SignalEntry>, String> {
        let terms: Vec<&str> = query.split_whitespace().collect();
        if terms.is_empty() { return self.recent(limit); }

        // Build parameterized LIKE conditions: ?1, ?2, ... for terms, ?N+1 for limit
        let conditions: Vec<String> = (0..terms.len())
            .map(|i| format!("payload LIKE ?{}", i + 1))
            .collect();
        let where_clause = conditions.join(" OR ");
        let limit_param = terms.len() + 1;

        let sql = format!(
            "SELECT memory_uuid,
                    COALESCE(json_extract(payload, '$.gist'), substr(json_extract(payload, '$.content'), 1, 200)) as gist,
                    created_at, parent_uuid
             FROM memories WHERE {} ORDER BY created_at DESC LIMIT ?{}",
            where_clause, limit_param
        );

        let conn = self.conn()?;
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("Query failed: {}", e))?;

        // Bind search terms as %term% patterns, then limit
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = terms.iter()
            .map(|t| Box::new(format!("%{}%", t)) as Box<dyn rusqlite::types::ToSql>)
            .collect();
        params.push(Box::new(limit as i32));

        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            let uuid: String = row.get(0)?;
            let parent: Option<String> = row.get(3)?;
            let display_parent = parent.filter(|p| p != &uuid);
            Ok(SignalEntry {
                memory_uuid: uuid, gist: row.get(1)?, created_at: row.get(2)?,
                parent_uuid: display_parent, content: None, score: None,
            })
        }).map_err(|e| format!("Query failed: {}", e))?;

        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn semantic_search(&self, query: &str, limit: usize) -> Result<Vec<SignalEntry>, String> {
        let cached = self.get_cached_embeddings()?;
        if cached.is_empty() {
            eprintln!("[geniuz] No embedding cache. Run: geniuz backfill");
            return self.keyword_search(query, limit);
        }

        let results = geniuz::embedding::semantic_search_cached(query, cached, limit)?;
        Ok(results.into_iter().map(|r| SignalEntry {
            memory_uuid: r.memory_uuid, gist: r.gist, created_at: r.created_at,
            parent_uuid: None, content: None, score: Some(r.score),
        }).collect())
    }

    pub fn random(&self) -> Result<Option<SignalEntry>, String> {
        let conn = self.conn()?;
        let result = conn.query_row(
            "SELECT memory_uuid,
                    COALESCE(json_extract(payload, '$.gist'), substr(json_extract(payload, '$.content'), 1, 200)),
                    created_at, parent_uuid
             FROM memories ORDER BY RANDOM() LIMIT 1",
            [], |row| {
                let uuid: String = row.get(0)?;
                let parent: Option<String> = row.get(3)?;
                let display_parent = parent.filter(|p| p != &uuid);
                Ok(SignalEntry {
                    memory_uuid: uuid, gist: row.get(1)?, created_at: row.get(2)?,
                    parent_uuid: display_parent, content: None, score: None,
                })
            },
        ).optional().map_err(|e| format!("Query failed: {}", e))?;
        Ok(result)
    }

    pub fn get_full_content(&self, uuid: &str) -> Result<Option<String>, String> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT json_extract(payload, '$.content') FROM memories WHERE memory_uuid = ?1",
            rusqlite::params![uuid], |row| row.get(0),
        ).optional().map_err(|e| format!("Query failed: {}", e))
    }

    pub fn count(&self) -> Result<usize, String> {
        let conn = self.conn()?;
        let c: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
            .map_err(|e| format!("Count failed: {}", e))?;
        Ok(c as usize)
    }

    fn resolve_uuid(&self, partial: &str) -> Result<Option<String>, String> {
        if partial.len() == 36 { return Ok(Some(partial.to_uppercase())); }
        let conn = self.conn()?;
        conn.query_row(
            "SELECT memory_uuid FROM memories WHERE memory_uuid LIKE ?1 LIMIT 1",
            rusqlite::params![format!("{}%", partial.to_uppercase())],
            |row| row.get(0),
        ).optional().map_err(|e| format!("UUID resolve failed: {}", e))
    }

    // =========================================================================
    // EMBEDDING CACHE
    // =========================================================================

    pub fn cache_embedding(&self, uuid: &str, embedding: &[f32]) -> Result<(), String> {
        let conn = self.conn()?;
        let blob = geniuz::embedding::embedding_to_blob(embedding);
        conn.execute(
            "INSERT OR REPLACE INTO memory_embeddings (memory_uuid, embedding) VALUES (?1, ?2)",
            rusqlite::params![uuid, blob],
        ).map_err(|e| format!("Cache write failed: {}", e))?;
        Ok(())
    }

    pub fn get_cached_embeddings(&self) -> Result<Vec<geniuz::embedding::CachedEmbedding>, String> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT e.memory_uuid,
                    COALESCE(json_extract(s.payload, '$.gist'), substr(json_extract(s.payload, '$.content'), 1, 120)),
                    s.created_at, e.embedding
             FROM memory_embeddings e
             JOIN memories s ON s.memory_uuid = e.memory_uuid"
        ).map_err(|e| format!("Query failed: {}", e))?;

        let rows = stmt.query_map([], |row| {
            let blob: Vec<u8> = row.get(3)?;
            let uuid: String = row.get(0)?;
            match geniuz::embedding::blob_to_embedding(&blob) {
                Ok(embedding) => Ok(Some(geniuz::embedding::CachedEmbedding {
                    memory_uuid: uuid, gist: row.get(1)?,
                    created_at: row.get(2)?, embedding,
                })),
                Err(e) => {
                    eprintln!("[geniuz] Skipping corrupted embedding for {}: {}", &uuid[..8.min(uuid.len())], e);
                    Ok(None)
                }
            }
        }).map_err(|e| format!("Query failed: {}", e))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map(|v| v.into_iter().flatten().collect())
            .map_err(|e| e.to_string())
    }

    pub fn get_uncached_signals(&self) -> Result<Vec<(String, String)>, String> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT s.memory_uuid, json_extract(s.payload, '$.content')
             FROM memories s
             LEFT JOIN memory_embeddings e ON e.memory_uuid = s.memory_uuid
             WHERE json_extract(s.payload, '$.content') IS NOT NULL
               AND e.memory_uuid IS NULL"
        ).map_err(|e| format!("Query failed: {}", e))?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(|e| format!("Query failed: {}", e))?;

        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn embedding_count(&self) -> Result<usize, String> {
        let conn = self.conn()?;
        let c: i64 = conn.query_row("SELECT COUNT(*) FROM memory_embeddings", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        Ok(c as usize)
    }

    pub fn set_embedding_model(&self, model: &str) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO embedding_meta (key, value) VALUES ('model', ?1)",
            rusqlite::params![model],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub struct SignalEntry {
    pub memory_uuid: String,
    pub gist: String,
    pub created_at: String,
    pub parent_uuid: Option<String>,
    pub content: Option<String>,
    pub score: Option<f32>,
}
