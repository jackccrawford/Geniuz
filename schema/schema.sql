-- Geniuz Memory Schema v1.0
CREATE TABLE IF NOT EXISTS _meta (key TEXT PRIMARY KEY, value TEXT);
INSERT OR IGNORE INTO _meta VALUES ('version', '1.0');
INSERT OR IGNORE INTO _meta VALUES ('product', 'geniuz');

CREATE TABLE IF NOT EXISTS memories (
    memory_uuid TEXT PRIMARY KEY NOT NULL CHECK(length(memory_uuid) = 36),
    payload TEXT NOT NULL CHECK(length(payload) >= 1),
    created_at TEXT NOT NULL DEFAULT(strftime('%Y-%m-%d %H:%M:%S', 'now')),
    parent_uuid TEXT CHECK(parent_uuid IS NULL OR length(parent_uuid) = 36)
);

CREATE INDEX IF NOT EXISTS idx_memories_chronological ON memories(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_memories_uuid_prefix ON memories(substr(memory_uuid, 1, 8));
CREATE INDEX IF NOT EXISTS idx_memories_parent ON memories(parent_uuid);

-- Convenience views for anyone reading a folder with SQL, in any language, without
-- parsing the JSON payload (relay had signal_full and signal_gist). A thread's root
-- is stored as its own parent; here it reads as NULL, plainly "no parent".
-- seq orders rows in the order memories were written. It is NOT a durable cursor:
-- seq is SQLite's hidden rowid, which another tool's VACUUM may renumber (order kept,
-- numbers changed). To remember a position, keep the memory's ID or address, as
-- `geniuz watch` does. A payload that isn't valid JSON reads as NULL gist and content
-- rather than failing the whole query.
CREATE VIEW IF NOT EXISTS memory_full AS
SELECT memory_uuid,
    CASE WHEN parent_uuid = memory_uuid THEN NULL ELSE parent_uuid END AS parent_uuid,
    created_at,
    CASE WHEN json_valid(payload) THEN json_extract(payload, '$.gist') END AS gist,
    CASE WHEN json_valid(payload) THEN json_extract(payload, '$.content') END AS content,
    rowid AS seq
FROM memories;

CREATE VIEW IF NOT EXISTS memory_gist AS
SELECT memory_uuid,
    CASE WHEN parent_uuid = memory_uuid THEN NULL ELSE parent_uuid END AS parent_uuid,
    created_at,
    CASE WHEN json_valid(payload) THEN json_extract(payload, '$.gist') END AS gist,
    rowid AS seq
FROM memories;

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

-- INSERT OR REPLACE resolves a PRIMARY KEY conflict by deleting the
-- conflicting row without firing DELETE triggers (unless
-- PRAGMA recursive_triggers is on), so prevent_memory_delete alone does not
-- stop it. This closes that gap independent of the pragma: any writer that
-- REPLACEs an existing memory_uuid gets a loud abort instead of silently
-- overwriting history. Side effect: INSERT OR IGNORE on an existing UUID
-- becomes a loud error too, since RAISE(ABORT) overrides OR IGNORE — nothing
-- in this project relies on that no-op today.
CREATE TRIGGER IF NOT EXISTS prevent_memory_replace
BEFORE INSERT ON memories
WHEN EXISTS(SELECT 1 FROM memories WHERE memory_uuid = NEW.memory_uuid)
BEGIN
    SELECT RAISE(ABORT, 'Memories are immutable, replacing an existing memory is not permitted');
END;

CREATE TABLE IF NOT EXISTS memory_embeddings (
    memory_uuid TEXT PRIMARY KEY,
    embedding   BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS embedding_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Images kept with memories (5.1). One row per distinct image, already downscaled
-- and stored as WebP, in the folder's own file: no loose files. `data` is last on
-- purpose: SQLite reads a row left to right, so listing sizes and dimensions never
-- touches the bytes. `sha256` is of the stored bytes; the same bytes kept twice
-- are stored once.
--
-- The folder's owner can take an image back (`geniuz image admin`): withdrawn, its
-- bytes, hash and size are erased and the row stays, so a memory that showed it
-- can say an image is unavailable, and no more than that; replaced, the bytes
-- change and the row says when. Done at the owner's word, never as an MCP tool.
CREATE TABLE IF NOT EXISTS images (
    image_uuid   TEXT PRIMARY KEY NOT NULL CHECK(length(image_uuid) = 36),
    sha256       TEXT UNIQUE CHECK(sha256 IS NULL OR length(sha256) = 64),
    media_type   TEXT NOT NULL,
    width        INTEGER NOT NULL,
    height       INTEGER NOT NULL,
    created_at   TEXT NOT NULL DEFAULT(strftime('%Y-%m-%d %H:%M:%S', 'now')),
    withdrawn_at TEXT,
    replaced_at  TEXT,
    data         BLOB,
    -- Bytes and hash together, and absent only once withdrawn.
    CHECK((data IS NULL) = (sha256 IS NULL)),
    CHECK((data IS NULL) = (withdrawn_at IS NOT NULL)),
    CHECK(data IS NULL OR length(data) >= 1),
    -- A withdrawn image keeps no size: that went with the bytes.
    CHECK((width > 0 AND height > 0) OR withdrawn_at IS NOT NULL)
);

-- Which memories show which images, and in what order. One image can serve many
-- memories; one memory can carry many images. (parent_uuid stays what it is: a
-- reply to a memory.)
CREATE TABLE IF NOT EXISTS memory_images (
    memory_uuid TEXT NOT NULL CHECK(length(memory_uuid) = 36),
    image_uuid  TEXT NOT NULL CHECK(length(image_uuid) = 36),
    position    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (memory_uuid, image_uuid)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_images_uuid_prefix ON images(substr(image_uuid, 1, 8));
CREATE INDEX IF NOT EXISTS idx_memory_images_image ON memory_images(image_uuid);

-- Kept means kept, for images as for memories. The one way through is the
-- owner's: `geniuz image admin` names a single image in `_meta` for the length of
-- its own transaction, changes it, and removes the name. Which image it is and
-- when it was kept never change.
CREATE TRIGGER IF NOT EXISTS prevent_image_update BEFORE UPDATE ON images
WHEN NEW.image_uuid IS NOT OLD.image_uuid
  OR NEW.created_at IS NOT OLD.created_at
  OR NOT EXISTS (SELECT 1 FROM _meta WHERE key = 'image_change_allowed' AND value = OLD.image_uuid)
BEGIN
    SELECT RAISE(ABORT, 'Images are immutable, update is not permitted');
END;

CREATE TRIGGER IF NOT EXISTS prevent_image_delete BEFORE DELETE ON images
BEGIN
    SELECT RAISE(ABORT, 'Images are immutable, delete is not permitted');
END;

-- As prevent_memory_replace: INSERT OR REPLACE deletes without firing DELETE triggers.
CREATE TRIGGER IF NOT EXISTS prevent_image_replace BEFORE INSERT ON images
WHEN EXISTS(SELECT 1 FROM images WHERE image_uuid = NEW.image_uuid OR sha256 = NEW.sha256)
BEGIN
    SELECT RAISE(ABORT, 'Images are immutable, replacing an existing image is not permitted');
END;

CREATE TRIGGER IF NOT EXISTS prevent_memory_image_update BEFORE UPDATE ON memory_images
BEGIN
    SELECT RAISE(ABORT, 'Images are immutable, update is not permitted');
END;

CREATE TRIGGER IF NOT EXISTS prevent_memory_image_delete BEFORE DELETE ON memory_images
BEGIN
    SELECT RAISE(ABORT, 'Images are immutable, delete is not permitted');
END;

-- A link joins a memory and an image that are both in this folder, once. Not an
-- image that was withdrawn: it has no bytes, so a new memory can't show it.
CREATE TRIGGER IF NOT EXISTS memory_image_must_exist BEFORE INSERT ON memory_images
WHEN NOT EXISTS (SELECT 1 FROM memories WHERE memory_uuid = NEW.memory_uuid)
  OR NOT EXISTS (SELECT 1 FROM images WHERE image_uuid = NEW.image_uuid AND withdrawn_at IS NULL)
  OR EXISTS (SELECT 1 FROM memory_images WHERE memory_uuid = NEW.memory_uuid AND image_uuid = NEW.image_uuid)
BEGIN
    SELECT RAISE(ABORT, 'That memory or image is not in this folder, or they are already joined');
END;

-- Everything about an image except its bytes. `tokens` is what a Claude model
-- spends to look at it: one per 28 x 28 pixel patch.
CREATE VIEW IF NOT EXISTS image_info AS
SELECT i.image_uuid, i.media_type, i.width, i.height,
    COALESCE(length(i.data), 0) AS bytes,
    ((i.width + 27) / 28) * ((i.height + 27) / 28) AS tokens,
    i.created_at,
    (SELECT count(*) FROM memory_images mi WHERE mi.image_uuid = i.image_uuid) AS used_by,
    i.withdrawn_at, i.replaced_at
FROM images i;

-- One row per memory-and-image pair, with no bytes. Order by position to show them.
CREATE VIEW IF NOT EXISTS memory_image AS
SELECT mi.memory_uuid, mi.position, i.image_uuid, i.media_type, i.width, i.height,
    COALESCE(length(i.data), 0) AS bytes, i.withdrawn_at, i.replaced_at
FROM memory_images mi JOIN images i ON i.image_uuid = mi.image_uuid;
