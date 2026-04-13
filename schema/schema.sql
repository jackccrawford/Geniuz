-- Geniuz Memory Schema v1.0
CREATE TABLE IF NOT EXISTS _meta (key TEXT PRIMARY KEY, value TEXT);
INSERT OR IGNORE INTO _meta VALUES ('version', '1.0');
INSERT OR IGNORE INTO _meta VALUES ('product', 'geniuz');

CREATE TABLE IF NOT EXISTS memories (
    memory_uuid TEXT PRIMARY KEY NOT NULL CHECK(length(memory_uuid) = 36),
    payload TEXT NOT NULL CHECK(length(payload) >= 1),
    created_at TEXT NOT NULL DEFAULT(datetime('now', 'utc')),
    parent_uuid TEXT CHECK(parent_uuid IS NULL OR length(parent_uuid) = 36)
);

CREATE INDEX IF NOT EXISTS idx_memories_chronological ON memories(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_memories_uuid_prefix ON memories(substr(memory_uuid, 1, 8));
CREATE INDEX IF NOT EXISTS idx_memories_parent ON memories(parent_uuid);

CREATE VIEW IF NOT EXISTS memory_chains AS
WITH RECURSIVE chain(memory_uuid, parent_uuid, gist, created_at, level, root_uuid) AS (
    SELECT memory_uuid,
        COALESCE(parent_uuid, memory_uuid) as parent_uuid,
        COALESCE(json_extract(payload, '$.gist'), substr(json_extract(payload, '$.content'), 1, 200)) as gist,
        created_at, 0 as level, memory_uuid as root_uuid
    FROM memories
    WHERE parent_uuid IS NULL OR memory_uuid = parent_uuid
    UNION ALL
    SELECT m.memory_uuid, m.parent_uuid,
        COALESCE(json_extract(m.payload, '$.gist'), substr(json_extract(m.payload, '$.content'), 1, 200)),
        m.created_at, c.level + 1, c.root_uuid
    FROM memories m
    JOIN chain c ON m.parent_uuid = c.memory_uuid
    WHERE m.parent_uuid IS NOT NULL AND m.memory_uuid <> m.parent_uuid
    AND c.level < 100
)
SELECT * FROM chain;

-- Memories are immutable — the log is the truth
CREATE TRIGGER IF NOT EXISTS prevent_memory_delete
BEFORE DELETE ON memories
BEGIN
    SELECT RAISE(ABORT, 'Memories are immutable — delete is not permitted');
END;

CREATE TRIGGER IF NOT EXISTS prevent_memory_update
BEFORE UPDATE ON memories
BEGIN
    SELECT RAISE(ABORT, 'Memories are immutable — update is not permitted');
END;

CREATE TABLE IF NOT EXISTS memory_embeddings (
    memory_uuid TEXT PRIMARY KEY,
    embedding   BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS embedding_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
