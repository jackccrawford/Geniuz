-- Bring a Geniuz memory file from before 4.0 forward, so Geniuz 4.0 and later can open it.
--
-- Files from before 4.0 keep memories in a `signals` table with a `signal_uuid` column,
-- and the first folder's file may be called station.db. Geniuz no longer converts them
-- when it opens them; this script does it, once, on purpose.
--
-- Steps (written so an AI agent can carry them out, and stop if one fails):
--   1. Quit everything that may have the file open (Claude, the Geniuz app).
--   2. Back it up first, as one consistent copy:
--        sqlite3 old.db ".backup 'old-backup.db'"
--   3. Check it is a file from before 4.0. This must print 1:
--        sqlite3 old.db "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='signals';"
--   4. Run this script:
--        sqlite3 old.db < schema/migrate-before-4.0.sql
--   5. If the file is called station.db, rename it memory.db (and its -wal and -shm
--      files, if any, before anything opens it).
--   6. Open it with Geniuz, which adds the current views and triggers:
--        GENIUZ_DB=/path/to/memory.db geniuz status
--      then `geniuz recall -l 3` to see the newest memories are there.
-- If any step fails, stop and restore the backup: nothing else is changed.

-- Stop at the first error, so nothing is half done: the transaction rolls back.
.bail on
BEGIN;
-- The old view and triggers name the old column, so they go first. Geniuz adds
-- the current ones when it next opens the file.
DROP VIEW IF EXISTS signal_chains;
DROP TRIGGER IF EXISTS prevent_signal_delete;
DROP TRIGGER IF EXISTS prevent_signal_update;
DROP TRIGGER IF EXISTS prevent_signal_replace;
ALTER TABLE signals RENAME TO memories;
ALTER TABLE memories RENAME COLUMN signal_uuid TO memory_uuid;
ALTER TABLE signal_embeddings RENAME TO memory_embeddings;
ALTER TABLE memory_embeddings RENAME COLUMN signal_uuid TO memory_uuid;
COMMIT;
