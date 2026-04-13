-- Geniuz Station Schema v1.0
CREATE TABLE IF NOT EXISTS _meta (key TEXT PRIMARY KEY, value TEXT);
INSERT OR IGNORE INTO _meta VALUES ('version', '1.0');
INSERT OR IGNORE INTO _meta VALUES ('product', 'geniuz');

CREATE TABLE IF NOT EXISTS signals (
    signal_uuid TEXT PRIMARY KEY NOT NULL CHECK(length(signal_uuid) = 36),
    payload TEXT NOT NULL CHECK(length(payload) >= 1),
    created_at TEXT NOT NULL DEFAULT(datetime('now', 'utc')),
    parent_uuid TEXT CHECK(parent_uuid IS NULL OR length(parent_uuid) = 36)
);

CREATE INDEX IF NOT EXISTS idx_signals_chronological ON signals(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_signals_uuid_prefix ON signals(substr(signal_uuid, 1, 8));
CREATE INDEX IF NOT EXISTS idx_signals_parent ON signals(parent_uuid);

CREATE VIEW IF NOT EXISTS signal_chains AS
WITH RECURSIVE chain(signal_uuid, parent_uuid, gist, created_at, level, root_uuid) AS (
    SELECT signal_uuid,
        COALESCE(parent_uuid, signal_uuid) as parent_uuid,
        COALESCE(json_extract(payload, '$.gist'), substr(json_extract(payload, '$.content'), 1, 200)) as gist,
        created_at, 0 as level, signal_uuid as root_uuid
    FROM signals
    WHERE parent_uuid IS NULL OR signal_uuid = parent_uuid
    UNION ALL
    SELECT s.signal_uuid, s.parent_uuid,
        COALESCE(json_extract(s.payload, '$.gist'), substr(json_extract(s.payload, '$.content'), 1, 200)),
        s.created_at, c.level + 1, c.root_uuid
    FROM signals s
    JOIN chain c ON s.parent_uuid = c.signal_uuid
    WHERE s.parent_uuid IS NOT NULL AND s.signal_uuid <> s.parent_uuid
    AND c.level < 100
)
SELECT * FROM chain;

-- Signals are immutable — the log is the truth
CREATE TRIGGER IF NOT EXISTS prevent_signal_delete
BEFORE DELETE ON signals
BEGIN
    SELECT RAISE(ABORT, 'Signals are immutable — delete is not permitted');
END;

CREATE TRIGGER IF NOT EXISTS prevent_signal_update
BEFORE UPDATE ON signals
BEGIN
    SELECT RAISE(ABORT, 'Signals are immutable — update is not permitted');
END;

CREATE TABLE IF NOT EXISTS signal_embeddings (
    signal_uuid TEXT PRIMARY KEY,
    embedding   BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS embedding_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
