use super::*;
use tempfile::TempDir;

fn setup() -> (TempDir, Registry) {
    let tmp = TempDir::new().unwrap();
    let reg = Registry::new(tmp.path().join("home"));
    std::fs::create_dir_all(tmp.path().join("home")).unwrap();
    (tmp, reg)
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
}

fn names(reg: &Registry) -> Vec<String> {
    reg.scan().folders.into_iter().map(|f| f.name).collect()
}

#[test]
fn a_memory_naming_no_folder_goes_to_my_memory_and_an_old_current_file_is_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let reg = Registry::new(tmp.path());
    let work = reg.create("Work", "", None).unwrap();
    let first = reg.resolve(None, None, None).unwrap();
    assert_eq!(first, reg.home.join(DB_FILE));
    assert_eq!(reg.find(DEFAULT_NAME).unwrap().db_path, first);
    // A `current` file from before 5.0.8, naming a folder or "none", is not read and not touched.
    for old in [format!("{}\n", work.uuid), "none\n".to_string()] {
        std::fs::write(reg.home.join("current"), &old).unwrap();
        assert_eq!(reg.resolve(None, None, None).unwrap(), first);
        assert_eq!(std::fs::read_to_string(reg.home.join("current")).unwrap(), old);
    }
    // Naming a folder still works.
    assert_eq!(reg.resolve(Some("Work"), None, None).unwrap(), work.db_path);
}

#[test]
fn first_folder_appears_with_an_identity_and_no_memories() {
    let (_tmp, reg) = setup();
    let scan = reg.scan();
    assert_eq!(scan.folders.len(), 1);
    assert_eq!(scan.folders[0].name, DEFAULT_NAME);
    assert_eq!(scan.folders[0].stats().unwrap().0, 0);
}

#[test]
fn existing_memory_db_keeps_its_place_and_its_identity() {
    let (_tmp, reg) = setup();
    let db = reg.ensure_default().unwrap();
    let first = read_identity(&db).unwrap();
    reg.ensure_default().unwrap();
    assert_eq!(read_identity(&db).unwrap().uuid, first.uuid);
    assert_eq!(db.file_name().unwrap(), DB_FILE);
}

#[test]
fn an_old_station_db_elsewhere_is_named_not_hidden() {
    let (tmp, reg) = setup();
    reg.ensure_default().unwrap();
    let added = tmp.path().join("added");
    std::fs::create_dir_all(added.join("Old")).unwrap();
    Connection::open(added.join("Old").join(OLD_DB_FILE)).unwrap()
        .execute_batch("CREATE TABLE signals (signal_uuid TEXT);").unwrap();
    let err = reg.add(&added).unwrap_err();
    assert!(err.contains("station.db") && err.contains(crate::db::MIGRATION_SCRIPT), "{err}");
    reg.remember_added(&added).unwrap();
    let scan = reg.scan();
    let found: Vec<_> = scan.old_format.iter().map(|p| canonical(p)).collect();
    assert_eq!(found, [canonical(&added.join("Old").join(OLD_DB_FILE))]);
}

#[test]
fn an_old_station_db_first_folder_is_refused_not_hidden() {
    let (_tmp, reg) = setup();
    std::fs::create_dir_all(&reg.home).unwrap();
    Connection::open(reg.home.join("station.db")).unwrap()
        .execute_batch("CREATE TABLE signals (signal_uuid TEXT);").unwrap();
    let err = reg.ensure_default().unwrap_err();
    assert!(err.contains("before 4.0") && err.contains(crate::db::MIGRATION_SCRIPT), "{err}");
    assert!(!reg.home.join(DB_FILE).exists(), "no empty memory.db made beside it");
    let scan = reg.scan();
    assert!(scan.first_folder_problem.unwrap().contains("before 4.0"));
}

#[test]
fn a_file_from_before_4_0_is_refused_and_left_unchanged() {
    let tmp = TempDir::new().unwrap();
    for schema in [
        "CREATE TABLE signals (signal_uuid TEXT, payload TEXT, created_at TEXT);",
        "CREATE TABLE memories (signal_uuid TEXT, payload TEXT, created_at TEXT);",
    ] {
        let db = tmp.path().join("old.db");
        let _ = std::fs::remove_file(&db);
        Connection::open(&db).unwrap().execute_batch(schema).unwrap();
        let before = std::fs::read(&db).unwrap();
        let err = crate::db::DatabaseManager::new(&db.to_string_lossy()).err().unwrap();
        assert!(err.contains("before 4.0") && err.contains(crate::db::MIGRATION_SCRIPT), "{err}");
        assert_eq!(std::fs::read(&db).unwrap(), before, "not touched");
    }
}

#[test]
fn plain_path_drops_only_the_windows_drive_prefix() {
    assert_eq!(plain_path(PathBuf::from(r"\\?\C:\build\x")), PathBuf::from(r"C:\build\x"));
    assert_eq!(plain_path(PathBuf::from(r"\\?\UNC\host\share")), PathBuf::from(r"\\?\UNC\host\share"));
    assert_eq!(plain_path(PathBuf::from("/home/x")), PathBuf::from("/home/x"));
}

#[test]
fn create_goes_in_the_data_directory_by_default() {
    let (_tmp, reg) = setup();
    let f = reg.create("Work", "Client projects", None).unwrap();
    assert_eq!(f.db_path, reg.home.join(&f.uuid).join(DB_FILE));
    assert_eq!(f.description, "Client projects");
    assert!(reg.added().is_empty());
    assert_eq!(names(&reg), vec![DEFAULT_NAME, "Work"]);
}

#[test]
fn create_elsewhere_goes_on_the_list() {
    let (tmp, reg) = setup();
    let at = tmp.path().join("notes");
    let f = reg.create("Notes", "", Some(&at)).unwrap();
    assert!(f.db_path.starts_with(&at));
    assert_eq!(reg.added(), vec![canonical(&at)]);
    assert_eq!(reg.find("notes").unwrap().uuid, f.uuid);
}

#[test]
fn list_sorts_ignoring_case() {
    let (_tmp, reg) = setup();
    for n in ["zebra", "Apple", "mango"] {
        reg.create(n, "", None).unwrap();
    }
    assert_eq!(names(&reg), vec!["Apple", "mango", DEFAULT_NAME, "zebra"]);
}

#[test]
fn names_must_be_unique_ignoring_case() {
    let (_tmp, reg) = setup();
    reg.create("Work", "", None).unwrap();
    assert!(reg.create("WORK", "", None).is_err());
    reg.create("Home", "", None).unwrap();
    assert!(reg.rename("Home", "work").is_err());
    assert_eq!(reg.rename("Home", "HOME").unwrap().name, "HOME");
}

#[test]
fn bad_names_are_refused() {
    for bad in ["", "   ", "a/b", "a:b", "13625F6B", "two\nlines", &"x".repeat(61)] {
        assert!(validate_name(bad).is_err(), "{:?} should be refused", bad);
    }
    assert_eq!(validate_name("  Work  ").unwrap(), "Work");
}

#[test]
fn find_takes_a_name_a_uuid_or_a_short_id() {
    let (_tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    assert_eq!(reg.find("work").unwrap().uuid, f.uuid);
    assert_eq!(reg.find(&f.uuid.to_lowercase()).unwrap().uuid, f.uuid);
    assert_eq!(reg.find(&f.short_id()).unwrap().uuid, f.uuid);
    assert!(reg.find("nope").is_err());
}

#[test]
fn add_refuses_a_copy_and_changes_nothing() {
    let (tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    let backup = tmp.path().join("backup");
    copy_dir(f.db_path.parent().unwrap(), &backup.join("renamed-copy"));

    let err = reg.add(&backup).unwrap_err();
    assert!(err.contains("copy"), "{}", err);
    assert!(reg.added().is_empty());
    assert_eq!(reg.scan().folders.len(), 2);
}

#[test]
fn add_refuses_a_name_already_taken() {
    let (tmp, reg) = setup();
    reg.create("Work", "", None).unwrap();
    let other = Registry::new(tmp.path().join("other"));
    let there = tmp.path().join("other");
    std::fs::create_dir_all(&there).unwrap();
    other.create("work", "", None).unwrap();

    let err = reg.add(&there).unwrap_err();
    assert!(err.contains("name") || err.contains("copy"), "{}", err);
    assert!(reg.added().is_empty());
}

#[test]
fn add_refuses_a_duplicate_inside_the_directory_being_added() {
    let (tmp, reg) = setup();
    let other_home = tmp.path().join("other-home");
    let f = Registry::new(&other_home).create("Solo", "", None).unwrap();
    let there = tmp.path().join("there");
    copy_dir(f.db_path.parent().unwrap(), &there.join("a"));
    copy_dir(f.db_path.parent().unwrap(), &there.join("b"));

    assert!(reg.add(&there).is_err());
    assert!(reg.added().is_empty());
}

#[test]
fn add_finds_folders_and_a_second_add_is_refused() {
    let (tmp, reg) = setup();
    let other_home = tmp.path().join("other-home");
    let f = Registry::new(&other_home).create("Solo", "", None).unwrap();
    let there = tmp.path().join("there");
    copy_dir(f.db_path.parent().unwrap(), &there.join(&f.uuid));

    let added = reg.add(&there).unwrap();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].uuid, f.uuid);
    assert!(reg.add(&there).is_err());
}

#[test]
fn add_gives_an_older_store_an_identity() {
    let (tmp, reg) = setup();
    let old = tmp.path().join("old-machine");
    crate::db::DatabaseManager::new(&old.join(DB_FILE).to_string_lossy()).unwrap();

    let added = reg.add(&old).unwrap();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].name, "old-machine");
}

#[test]
fn add_never_gives_a_new_identity_to_a_copy_of_an_older_store() {
    let (tmp, reg) = setup();
    let insert = |db: &Path| {
        Connection::open(db)
            .unwrap()
            .execute(
                "INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')",
                ["11111111-2222-3333-4444-555555555555"],
            )
            .unwrap();
    };
    // Same memory in the first folder and in a store with no identity yet,
    // which is what copying a pre-5.0 memory.db leaves behind.
    insert(&reg.ensure_default().unwrap());
    let copy = tmp.path().join("restored");
    let copy_db = copy.join(DB_FILE);
    crate::db::DatabaseManager::new(&copy_db.to_string_lossy()).unwrap();
    insert(&copy_db);

    let err = reg.add(&copy).unwrap_err();
    assert!(err.contains("memories that are also in"), "{}", err);
    assert!(read_identity(&copy_db).is_none(), "the copy must not be stamped");
    assert!(reg.added().is_empty());
}

#[test]
fn a_relocated_folder_keeps_its_uuid() {
    let (tmp, reg) = setup();
    let from = tmp.path().join("from");
    let f = reg.create("Work", "", Some(&from)).unwrap();
    let to = tmp.path().join("to");
    std::fs::create_dir_all(&to).unwrap();
    std::fs::rename(f.db_path.parent().unwrap(), to.join(&f.uuid)).unwrap();
    reg.remove(&from).unwrap();

    reg.add(&to).unwrap();
    assert_eq!(reg.find("Work").unwrap().uuid, f.uuid);
}

fn add_memory(db: &Path, uuid: &str) {
    Connection::open(db)
        .unwrap()
        .execute("INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')", [uuid])
        .unwrap();
}

const M1: &str = "aaaaaaaa-2222-3333-4444-555555555555";
const M2: &str = "bbbbbbbb-2222-3333-4444-555555555555";

#[test]
fn two_copies_of_an_older_store_get_the_same_identity() {
    let tmp = TempDir::new().unwrap();
    let (a, b) = (tmp.path().join("a").join(DB_FILE), tmp.path().join("b").join(DB_FILE));
    for db in [&a, &b] {
        crate::db::DatabaseManager::new(&db.to_string_lossy()).unwrap();
        add_memory(db, M1);
        stamp(db, "x").unwrap();
    }
    assert_eq!(read_identity(&a).unwrap().uuid, read_identity(&b).unwrap().uuid);
}

#[test]
fn empty_stores_get_different_identities() {
    let tmp = TempDir::new().unwrap();
    let (a, b) = (tmp.path().join("a").join(DB_FILE), tmp.path().join("b").join(DB_FILE));
    for db in [&a, &b] {
        crate::db::DatabaseManager::new(&db.to_string_lossy()).unwrap();
        stamp(db, "x").unwrap();
    }
    assert_ne!(read_identity(&a).unwrap().uuid, read_identity(&b).unwrap().uuid);
}

#[test]
fn relocate_keeps_every_uuid_and_memory() {
    let (tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    add_memory(&f.db_path, M1);
    let dest = tmp.path().join("elsewhere");

    let moved = reg.relocate("Work", &dest).unwrap();
    assert_eq!(moved.folder.uuid, f.uuid);
    assert!(moved.left_behind.is_none());
    assert!(!f.db_path.exists());
    assert_eq!(moved.folder.stats().unwrap().0, 1);
    assert_eq!(reg.added(), vec![canonical(&dest)]);
    assert!(reg.scan().blocked.is_empty());
    assert_eq!(canonical(&reg.resolve(Some(&f.uuid), None, None).unwrap()), canonical(&moved.folder.db_path));
}

#[test]
fn a_move_takes_an_emptied_directory_off_the_list_and_keeps_one_still_in_use() {
    let (tmp, reg) = setup();
    let projects = tmp.path().join("projects");
    let shared = tmp.path().join("shared");
    reg.create("Work", "", Some(&projects)).unwrap();
    reg.create("Home", "", Some(&shared)).unwrap();
    reg.create("Garden", "", Some(&shared)).unwrap();
    let dest = tmp.path().join("moved");

    // projects held only Work, so once Work leaves, projects leaves the list.
    reg.relocate("Work", &dest).unwrap();
    let listed: Vec<_> = reg.added().iter().map(|d| canonical(d)).collect();
    assert!(!listed.contains(&canonical(&projects)));
    assert!(listed.contains(&canonical(&dest)));

    // shared still holds Garden after Home moves, so it stays on the list.
    reg.relocate("Home", &dest).unwrap();
    let listed: Vec<_> = reg.added().iter().map(|d| canonical(d)).collect();
    assert!(listed.contains(&canonical(&shared)));
    assert!(reg.find("Garden").is_ok());
}

#[test]
fn relocate_refuses_the_first_folder_and_a_directory_of_unknown_stores() {
    let (tmp, reg) = setup();
    reg.ensure_default().unwrap();
    assert!(reg.relocate(DEFAULT_NAME, &tmp.path().join("x")).is_err());

    let f = reg.create("Work", "", None).unwrap();
    let other = tmp.path().join("other");
    Registry::new(&other).ensure_default().unwrap();
    assert!(reg.relocate("Work", &other).unwrap_err().contains("Add it first"));
    assert!(f.db_path.exists());
}

#[test]
fn crossing_drives_carries_writes_still_in_the_wal_and_retires_the_original() {
    let (tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    let writer = crate::db::DatabaseManager::new(&f.db_path.to_string_lossy()).unwrap();
    drop(writer);
    // Keep a connection open so the write stays in the -wal file, as it would
    // beside a running app. A plain copy of memory.db would lose it.
    let held = Connection::open(&f.db_path).unwrap();
    held.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;").unwrap();
    held.execute("INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')", [M1]).unwrap();
    held.execute("INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')", [M2]).unwrap();
    let wal = PathBuf::from(format!("{}-wal", f.db_path.display()));
    assert!(std::fs::metadata(&wal).unwrap().len() > 0, "the test needs a live WAL");

    let dest = tmp.path().join("drive");
    std::fs::create_dir_all(&dest).unwrap();
    let dir = f.db_path.parent().unwrap().to_path_buf();
    let lock = quiesce(&f.db_path).unwrap();
    let retired = copy_then_retire(&dir, &dest, &f, lock).unwrap();
    drop(held);

    let moved = read_identity(&dest.join(&f.uuid).join(DB_FILE)).unwrap();
    assert_eq!(moved.uuid, f.uuid);
    assert_eq!(moved.stats().unwrap().0, 2);
    assert!(!dir.exists());
    assert!(retired.join(DB_FILE).is_file(), "the original is retired, never deleted");
}

#[test]
fn a_folder_being_moved_is_not_visible_at_its_destination() {
    let (tmp, reg) = setup();
    let there = tmp.path().join("there");
    reg.create("Stays", "", Some(&there)).unwrap();
    let other_home = tmp.path().join("other-home");
    let f = Registry::new(&other_home).create("Mover", "", None).unwrap();
    copy_dir(f.db_path.parent().unwrap(), &there.join(".moving-x"));
    assert!(reg.find("Mover").is_err());
}

#[test]
fn another_products_store_is_refused_and_left_unchanged() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join(DB_FILE);
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT);
         INSERT INTO _meta VALUES ('product', 'elsewhere');
         CREATE TABLE signals (signal_uuid TEXT);",
    )
    .unwrap();
    drop(conn);
    let before = std::fs::read(&db).unwrap();

    let err = crate::db::DatabaseManager::new(&db.to_string_lossy()).err().unwrap();
    assert!(err.contains("another product"), "{}", err);
    assert_eq!(std::fs::read(&db).unwrap(), before);
}

#[test]
fn stores_from_earlier_names_can_be_added() {
    let (tmp, reg) = setup();
    let old = tmp.path().join("old");
    let db = old.join(DB_FILE);
    crate::db::DatabaseManager::new(&db.to_string_lossy()).unwrap();
    Connection::open(&db)
        .unwrap()
        .execute("UPDATE _meta SET value = 'clawmark' WHERE key = 'product'", [])
        .unwrap();
    assert_eq!(reg.add(&old).unwrap().len(), 1);
}

#[test]
fn a_backup_holds_writes_still_in_the_wal() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join(DB_FILE);
    crate::db::DatabaseManager::new(&db.to_string_lossy()).unwrap();
    let held = Connection::open(&db).unwrap();
    held.execute_batch("PRAGMA wal_autocheckpoint=0;").unwrap();
    held.execute("INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')", [M1]).unwrap();

    let target = tmp.path().join("backup.db");
    backup_to(&db, &target).unwrap();
    drop(held);
    let count: i64 = Connection::open(&target)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

// ---- From the outside review of v5 (Codex Astra, 2 Nov). ----

#[test]
fn review1_a_waiting_writer_cannot_write_to_a_moved_original() {
    let (tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    let waiting = Connection::open(&f.db_path).unwrap();
    waiting.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get::<_, i64>(0)).unwrap();
    let dest = tmp.path().join("drive");
    std::fs::create_dir_all(&dest).unwrap();
    let dir = f.db_path.parent().unwrap().to_path_buf();
    copy_then_retire(&dir, &dest, &f, quiesce(&f.db_path).unwrap()).unwrap();

    let wrote = waiting.execute("INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')", [M1]);
    assert!(wrote.is_err(), "a write to the retired original must fail, not succeed where nobody reads");
}

#[test]
fn review3_a_failure_to_retire_never_publishes_a_second_copy() {
    let (tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    std::fs::write(reg.home.join(ARCHIVE_DIR), "in the way").unwrap();
    let dest = tmp.path().join("drive");
    std::fs::create_dir_all(&dest).unwrap();
    let dir = f.db_path.parent().unwrap().to_path_buf();
    let result = copy_then_retire(&dir, &dest, &f, quiesce(&f.db_path).unwrap());

    assert!(result.is_err());
    assert!(f.db_path.exists());
    assert!(!dest.join(&f.uuid).exists(), "nothing may be published when the original can't be retired");
}

#[test]
fn review1_a_waiting_writer_follows_a_move_on_the_same_drive() {
    let (tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    let waiting = Connection::open(&f.db_path).unwrap();
    waiting.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get::<_, i64>(0)).unwrap();

    let moved = reg.relocate("Work", &tmp.path().join("elsewhere")).unwrap();
    waiting.execute("INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')", [M1]).unwrap();
    drop(waiting);

    assert_eq!(moved.folder.stats().unwrap().0, 1, "the write must land in the moved folder");
    assert!(!f.db_path.exists());
}

#[test]
fn review2_opening_a_moved_folder_fails_and_creates_nothing() {
    let (tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    let old = f.db_path.clone();
    reg.relocate("Work", &tmp.path().join("elsewhere")).unwrap();

    assert!(crate::db::DatabaseManager::open_existing(&old.to_string_lossy()).is_err());
    assert!(!old.exists(), "no empty store may appear at the old path");
    assert!(!old.parent().unwrap().exists());
}

#[test]
fn review3_a_move_interrupted_after_the_fence_is_finished() {
    let (tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    add_memory(&f.db_path, M1);
    let dest = tmp.path().join("drive");
    std::fs::create_dir_all(&dest).unwrap();
    reg.remember_added(&dest).unwrap();
    // The state after copying and fencing, before retiring.
    let staging = dest.join(format!("{}{}", MOVING_PREFIX, f.uuid));
    copy_dir(f.db_path.parent().unwrap(), &staging);
    Connection::open(&f.db_path)
        .unwrap()
        .execute_batch(&format!(
            "CREATE TRIGGER {}_memories_insert BEFORE INSERT ON memories BEGIN SELECT RAISE(ABORT, 'moved'); END;",
            FENCE_TRIGGER
        ))
        .unwrap();

    let found = reg.find("Work").unwrap();
    assert!(canonical(&found.db_path).starts_with(canonical(&dest)));
    assert_eq!(found.stats().unwrap().0, 1);
    assert!(!f.db_path.exists());
    assert!(reg.scan().blocked.is_empty());
}

#[test]
fn review3_a_copy_left_while_the_original_is_live_is_set_aside() {
    let (tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    let dest = tmp.path().join("drive");
    std::fs::create_dir_all(&dest).unwrap();
    reg.remember_added(&dest).unwrap();
    let staging = dest.join(format!("{}{}", MOVING_PREFIX, f.uuid));
    copy_dir(f.db_path.parent().unwrap(), &staging);

    assert_eq!(canonical(&reg.find("Work").unwrap().db_path), canonical(&f.db_path));
    assert!(!staging.exists());
    assert!(reg.archived().is_empty(), "an abandoned copy must not be offered for restore");
}

#[test]
fn review3_a_registry_failure_leaves_the_folder_findable() {
    let (tmp, reg) = setup();
    reg.create("Work", "", None).unwrap();
    std::fs::create_dir_all(reg.home.join("added-folders.tmp")).unwrap();
    let _ = reg.relocate("Work", &tmp.path().join("elsewhere"));
    assert!(reg.find("Work").is_ok(), "the folder must stay findable whatever the registry did");
}

#[test]
fn review4_a_new_copy_never_takes_over_from_the_original() {
    let (_tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    copy_dir(f.db_path.parent().unwrap(), &reg.home.join("000-copy"));
    add_memory(&f.db_path, M1);

    match reg.resolve(Some(&f.uuid), None, None) {
        Ok(path) => assert_eq!(canonical(&path), canonical(&f.db_path), "the older copy was selected"),
        Err(e) => assert!(e.contains("copy") || e.contains("copies"), "{}", e),
    }
}

#[test]
fn review6_two_unstamped_copies_in_one_add_are_refused() {
    let (tmp, reg) = setup();
    reg.ensure_default().unwrap();
    let there = tmp.path().join("there");
    for name in ["a", "b"] {
        let db = there.join(name).join(DB_FILE);
        crate::db::DatabaseManager::new(&db.to_string_lossy()).unwrap();
        add_memory(&db, M1);
    }
    assert!(reg.add(&there).is_err());
    assert!(read_identity(&there.join("a").join(DB_FILE)).is_none(), "nothing may be stamped");
}

#[test]
fn review6_identical_unstamped_stores_get_one_identity() {
    let tmp = TempDir::new().unwrap();
    let (a, b) = (tmp.path().join("a.db"), tmp.path().join("b.db"));
    for db in [&a, &b] {
        Connection::open(db).unwrap().execute_batch(&format!(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT);
             INSERT INTO _meta VALUES ('product','geniuz');
             CREATE TABLE memories (memory_uuid TEXT, payload TEXT, created_at TEXT);
             INSERT INTO memories VALUES ('{}','{{}}','2026-01-01 00:00:00');", M1
        )).unwrap();
        stamp(db, "x").unwrap();
    }
    assert_eq!(read_identity(&a).unwrap().uuid, read_identity(&b).unwrap().uuid);
}

#[test]
fn review7_simultaneous_additions_are_all_kept() {
    let (tmp, reg) = setup();
    reg.ensure_default().unwrap();
    let home = reg.home.clone();
    let mut threads = Vec::new();
    for i in 0..24 {
        let (home, at) = (home.clone(), tmp.path().join(format!("d{}", i)));
        threads.push(std::thread::spawn(move || {
            std::fs::create_dir_all(&at).unwrap();
            Registry::new(home).remember_added(&at)
        }));
    }
    let ok = threads.into_iter().map(|t| t.join().unwrap()).filter(Result::is_ok).count();
    assert_eq!(reg.added().len(), ok, "every addition reported as done must be on the list");
    assert_eq!(ok, 24);
}

#[test]
fn review8_a_name_cannot_shadow_a_uuid() {
    let (_tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    let _ = reg.create(&f.uuid, "", None);
    assert_eq!(canonical(&reg.resolve(Some(&f.uuid), None, None).unwrap()), canonical(&f.db_path));
    assert!(validate_name(&f.uuid).is_err());
}

#[cfg(unix)]
#[test]
fn review9_a_symlinked_folder_is_refused_before_anything_moves() {
    let (tmp, reg) = setup();
    let elsewhere = Registry::new(tmp.path().join("other"));
    let f = elsewhere.create("Linked", "", None).unwrap();
    std::os::unix::fs::symlink(f.db_path.parent().unwrap(), reg.home.join("link")).unwrap();
    assert_eq!(reg.find("Linked").unwrap().uuid, f.uuid);

    assert!(reg.relocate("Linked", &tmp.path().join("deep").join("er")).is_err());
    assert!(reg.find("Linked").is_ok(), "the folder must still be there");
    assert!(reg.archive("Linked").is_err());
}

#[test]
fn review10_restore_compares_names_like_everything_else() {
    let (_tmp, reg) = setup();
    reg.create("École", "", None).unwrap();
    reg.archive("École").unwrap();
    reg.create("école", "", None).unwrap();
    assert!(reg.restore("École").is_err());
}

// ---- From the second review (Codex Astra, 3 Nov). ----

fn staged(reg: &Registry, f: &Folder, dest: &Path) -> PathBuf {
    std::fs::create_dir_all(dest).unwrap();
    reg.remember_added(dest).unwrap();
    let staging = dest.join(format!("{}{}", MOVING_PREFIX, f.uuid));
    copy_dir(f.db_path.parent().unwrap(), &staging);
    staging
}

#[test]
fn pass2_a_scan_never_touches_a_move_still_running() {
    let (tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    let staging = staged(&reg, &f, &tmp.path().join("drive"));
    let running = std::fs::File::create(staging.join(MOVE_OWNER)).unwrap();
    running.lock().unwrap();

    reg.scan();
    assert!(staging.exists(), "the running mover still needs this");
    drop(running);
}

#[test]
fn pass2_a_copy_with_no_proof_waits_when_its_original_is_out_of_reach() {
    let (tmp, reg) = setup();
    let source = tmp.path().join("removable");
    let f = reg.create("Work", "", Some(&source)).unwrap();
    let staging = staged(&reg, &f, &tmp.path().join("drive"));
    add_memory(&f.db_path, M1); // newer than the copy
    std::fs::rename(&source, tmp.path().join("unplugged")).unwrap();

    let scan = reg.scan();
    assert_eq!(scan.pending_moves.len(), 1);
    assert!(staging.exists());
    assert!(reg.find("Work").is_err(), "a stale copy must not become the folder");
}

#[test]
fn pass2_a_committed_copy_is_finished_even_with_its_original_out_of_reach() {
    let (tmp, reg) = setup();
    let source = tmp.path().join("removable");
    let f = reg.create("Work", "", Some(&source)).unwrap();
    let dest = tmp.path().join("drive");
    let staging = staged(&reg, &f, &dest);
    std::fs::File::create(staging.join(MOVE_COMMITTED)).unwrap();
    std::fs::rename(&source, tmp.path().join("unplugged")).unwrap();

    let found = reg.find("Work").unwrap();
    assert!(canonical(&found.db_path).starts_with(canonical(&dest)));
    assert!(!found.db_path.parent().unwrap().join(MOVE_COMMITTED).exists());
}

#[test]
fn pass2_the_fence_refuses_every_kind_of_write() {
    let (tmp, reg) = setup();
    let f = reg.create("Work", "note", None).unwrap();
    let waiting = Connection::open(&f.db_path).unwrap();
    waiting.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get::<_, i64>(0)).unwrap();
    let dest = tmp.path().join("drive");
    std::fs::create_dir_all(&dest).unwrap();
    let dir = f.db_path.parent().unwrap().to_path_buf();
    copy_then_retire(&dir, &dest, &f, quiesce(&f.db_path).unwrap()).unwrap();

    for sql in [
        "UPDATE _meta SET value = 'changed' WHERE key = 'folder_description'",
        "INSERT OR REPLACE INTO _meta (key, value) VALUES ('folder_description', 'changed')",
        "INSERT OR REPLACE INTO embedding_meta (key, value) VALUES ('model', 'other')",
        "INSERT INTO memory_embeddings (memory_uuid, embedding) VALUES ('x', x'00')",
    ] {
        assert!(waiting.execute(sql, []).is_err(), "{}", sql);
    }
    assert!(!dest.join(&f.uuid).join(MOVE_COMMITTED).exists());
    assert!(!dest.join(&f.uuid).join(MOVE_OWNER).exists());
}

#[test]
fn pass2_simultaneous_creates_of_one_name_leave_one_folder() {
    let (_tmp, reg) = setup();
    reg.ensure_default().unwrap();
    let home = reg.home.clone();
    let threads: Vec<_> = (0..16)
        .map(|_| {
            let home = home.clone();
            std::thread::spawn(move || Registry::new(home).create("Work", "", None).is_ok())
        })
        .collect();
    let ok = threads.into_iter().map(|t| t.join().unwrap()).filter(|x| *x).count();
    assert_eq!(ok, 1);
    assert_eq!(names(&reg), vec![DEFAULT_NAME, "Work"]);
}

#[test]
fn pass2_a_name_clash_made_by_hand_is_refused_by_name_and_mended_by_id() {
    let (tmp, reg) = setup();
    let first = reg.create("Work", "", None).unwrap();
    let other = tmp.path().join("other");
    let second = Registry::new(&other).create("work", "", None).unwrap();
    reg.remember_added(&other).unwrap();

    assert!(reg.find("Work").unwrap_err().contains("More than one"));
    assert_eq!(reg.find(&first.uuid).unwrap().uuid, first.uuid);
    reg.rename(&second.uuid, "Other").unwrap();
    assert_eq!(reg.find("Work").unwrap().uuid, first.uuid);
}

#[test]
fn pass2_a_stamped_and_an_unstamped_copy_are_refused_together() {
    let (tmp, reg) = setup();
    reg.ensure_default().unwrap();
    let there = tmp.path().join("there");
    let stamped = there.join("a").join(DB_FILE);
    crate::db::DatabaseManager::new(&stamped.to_string_lossy()).unwrap();
    add_memory(&stamped, M1);
    stamp(&stamped, "A").unwrap();
    let plain = there.join("b").join(DB_FILE);
    crate::db::DatabaseManager::new(&plain.to_string_lossy()).unwrap();
    add_memory(&plain, M1);

    assert!(reg.add(&there).is_err());
    assert!(read_identity(&plain).is_none());
}

#[test]
fn pass2_a_shared_memory_outside_the_oldest_rows_is_still_found() {
    let (tmp, reg) = setup();
    let first = reg.ensure_default().unwrap();
    add_memory(&first, M1);
    let copy = tmp.path().join("copy");
    let db = copy.join(DB_FILE);
    crate::db::DatabaseManager::new(&db.to_string_lossy()).unwrap();
    add_memory(&db, M1);
    let conn = Connection::open(&db).unwrap();
    for i in 0..25 {
        conn.execute(
            "INSERT INTO memories (memory_uuid, payload, created_at) VALUES (?1, '{}', '2001-01-01 00:00:00')",
            [format!("dddddddd-2222-3333-4444-{:012}", i)],
        )
        .unwrap();
    }
    drop(conn);
    assert!(reg.add(&copy).is_err());
}

#[cfg(unix)]
#[test]
fn pass2_a_hard_link_is_named_for_what_it_is() {
    let (_tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    let linked = reg.home.join("linked");
    std::fs::create_dir_all(&linked).unwrap();
    std::fs::hard_link(&f.db_path, linked.join(DB_FILE)).unwrap();
    let err = reg.find("Work").unwrap_err();
    assert!(err.contains("two names for one file"), "{}", err);
}

#[test]
fn pinned_folders_come_first_and_stay_pinned_through_a_rename() {
    let (_tmp, reg) = setup();
    reg.create("Apple", "", None).unwrap();
    reg.create("Zebra", "", None).unwrap();
    reg.set_pinned("Zebra", true).unwrap();
    assert_eq!(names(&reg), vec!["Zebra", "Apple", DEFAULT_NAME]);

    reg.rename("Zebra", "Yak").unwrap();
    assert_eq!(names(&reg)[0], "Yak");

    reg.set_pinned("Yak", false).unwrap();
    assert_eq!(names(&reg), vec!["Apple", DEFAULT_NAME, "Yak"]);
}

#[test]
fn addresses_are_two_eight_character_ids() {
    assert_eq!(
        parse_address("13625f6b:3487B9B0"),
        Some(("13625F6B".to_string(), "3487B9B0".to_string()))
    );
    for bad in ["13625F6B", "13625F6B:", "note: 3487B9B0", "13625F6B:3487B9B", "ZZZZZZZZ:3487B9B0"] {
        assert!(parse_address(bad).is_none(), "{}", bad);
    }
}

#[test]
fn searching_across_labels_each_hit_and_merges_newest_first() {
    let (_tmp, reg) = setup();
    let a = reg.create("Alpha", "", None).unwrap();
    let b = reg.create("Beta", "", None).unwrap();
    let open = |f: &Folder| crate::db::DatabaseManager::new(&f.db_path.to_string_lossy()).unwrap();
    open(&a).keep_at_for_tests("the older note", "older", None, Some("2026-01-01 00:00:00"), None).unwrap();
    open(&b).keep_at_for_tests("the newer note", "newer", None, Some("2026-02-01 00:00:00"), None).unwrap();
    open(&b).keep_at_for_tests("unrelated", "other", None, Some("2026-03-01 00:00:00"), None).unwrap();

    let window = Window::parse(None, None).unwrap();
    let found = search_across(&reg, &Search::Keyword("note"), &window, 10);
    assert_eq!(found.searched, 3);
    assert_eq!(found.memories, 3);
    let names: Vec<_> = found.hits.iter().map(|h| h.folder.name.as_str()).collect();
    assert_eq!(names, vec!["Beta", "Alpha"]);
    assert!(found.hits[0].address().starts_with(&b.short_id()));

    let recent = search_across(&reg, &Search::Recent, &window, 1);
    assert_eq!(recent.hits[0].entry.gist, "other");
}

#[test]
fn add_leaves_a_database_it_does_not_own_untouched() {
    let (tmp, reg) = setup();
    let foreign = tmp.path().join("foreign");
    std::fs::create_dir_all(foreign.join("ABC")).unwrap();
    let db = foreign.join("ABC").join(DB_FILE);
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TABLE other (x);").unwrap();
    drop(conn);
    let before = std::fs::read(&db).unwrap();

    assert!(reg.add(&foreign).unwrap_err().contains("No Geniuz folders"));
    assert_eq!(std::fs::read(&db).unwrap(), before);
    assert!(reg.added().is_empty());
}

#[test]
fn a_copy_appearing_later_blocks_every_copy_until_one_is_moved() {
    let (tmp, reg) = setup();
    let first = tmp.path().join("first");
    let f = reg.create("Work", "", Some(&first)).unwrap();
    let second = tmp.path().join("second");
    reg.create("Other", "", Some(&second)).unwrap();
    copy_dir(f.db_path.parent().unwrap(), &second.join("dropped-in"));

    // Nothing can tell which is the original, so neither may be used.
    let scan = reg.scan();
    assert_eq!(scan.blocked.len(), 2);
    assert_eq!(scan.folders.iter().filter(|x| x.uuid == f.uuid).count(), 0);
    assert!(reg.find("Work").unwrap_err().contains("two copies"));
    std::fs::rename(second.join("dropped-in"), tmp.path().join("set-aside")).unwrap();
    assert_eq!(canonical(&reg.find("Work").unwrap().db_path), canonical(&f.db_path));
}

#[test]
fn an_unreachable_added_directory_is_reported_and_nothing_else_breaks() {
    let (tmp, reg) = setup();
    let gone = tmp.path().join("drive");
    reg.create("OnDrive", "", Some(&gone)).unwrap();
    std::fs::rename(&gone, tmp.path().join("unplugged")).unwrap();

    let scan = reg.scan();
    assert_eq!(scan.unavailable.len(), 1);
    assert_eq!(names(&reg), vec![DEFAULT_NAME]);
    assert_eq!(reg.added().len(), 1);
}

#[test]
fn resolve_order_is_flag_env_dotfile_then_my_memory() {
    let (tmp, reg) = setup();
    let work = reg.create("Work", "", None).unwrap();
    let home = reg.create("Home", "", None).unwrap();
    let project = tmp.path().join("project").join("deep");
    std::fs::create_dir_all(&project).unwrap();

    let first = reg.resolve(None, None, None).unwrap();
    assert_eq!(first.file_name().unwrap(), DB_FILE);
    assert_eq!(first.parent().unwrap(), reg.home);

    assert_eq!(reg.resolve(None, None, Some(&project)).unwrap(), first, "no dotfile: My memory");

    std::fs::write(tmp.path().join("project").join(DOTFILE), "Home\n").unwrap();
    assert_eq!(reg.resolve(None, None, Some(&project)).unwrap(), home.db_path);

    assert_eq!(
        reg.resolve(None, Some("/elsewhere/x.db"), Some(&project)).unwrap(),
        PathBuf::from("/elsewhere/x.db")
    );
    assert_eq!(reg.resolve(Some("Work"), Some("/elsewhere/x.db"), Some(&project)).unwrap(), work.db_path);
}

#[test]
fn a_wrong_name_errors_and_never_falls_back() {
    let (tmp, reg) = setup();
    assert!(reg.resolve(Some("typo"), None, None).is_err());

    let dir = tmp.path().join("p");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(DOTFILE), "typo").unwrap();
    assert!(reg.resolve(None, None, Some(&dir)).unwrap_err().contains(DOTFILE));
}

#[test]
fn edits_keep_the_old_value() {
    let (_tmp, reg) = setup();
    let f = reg.create("Work", "first", None).unwrap();
    assert_eq!(reg.describe("Work", "second").unwrap().description, "second");

    let conn = open_read_only(&f.db_path).unwrap();
    let old: String = conn
        .query_row(
            "SELECT value FROM _meta WHERE key LIKE 'folder_description@%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(old, "first");
}

#[test]
fn archive_moves_without_deleting_and_the_folder_stays_hidden() {
    let (_tmp, reg) = setup();
    let f = reg.create("Scratch", "", None).unwrap();

    let moved = reg.archive("Scratch").unwrap();
    assert!(moved.join(DB_FILE).is_file());
    assert!(!f.db_path.exists());
    assert!(reg.find("Scratch").is_err());
}

#[test]
fn an_archived_folder_can_be_seen_and_restored_unchanged() {
    let (_tmp, reg) = setup();
    let f = reg.create("Scratch", "note", None).unwrap();
    assert!(reg.archived().is_empty());

    reg.archive("Scratch").unwrap();
    let archived = reg.archived();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].uuid, f.uuid);
    assert!(reg.find("Scratch").is_err());

    let back = reg.restore("scratch").unwrap();
    assert_eq!(back.uuid, f.uuid);
    assert_eq!(back.db_path, f.db_path);
    assert_eq!(back.description, "note");
    assert!(reg.archived().is_empty());
}

#[test]
fn restoring_is_refused_if_the_name_has_been_taken() {
    let (_tmp, reg) = setup();
    reg.create("Scratch", "", None).unwrap();
    reg.archive("Scratch").unwrap();
    reg.create("scratch", "", None).unwrap();

    assert!(reg.restore("Scratch").unwrap_err().contains("already a folder"));
    assert_eq!(reg.archived().len(), 1);
}

#[test]
fn restoring_is_refused_if_a_copy_is_in_use() {
    let (tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    let elsewhere = tmp.path().join("elsewhere");
    copy_dir(f.db_path.parent().unwrap(), &elsewhere.join("copy"));
    reg.archive("Work").unwrap();
    reg.add(&elsewhere).unwrap();

    assert!(reg.restore(&f.uuid).unwrap_err().contains("in use"));
    assert_eq!(reg.archived().len(), 1);
}

#[test]
fn the_original_left_by_a_move_across_drives_is_not_offered_for_restore() {
    let (tmp, reg) = setup();
    let f = reg.create("Work", "", None).unwrap();
    let dest = tmp.path().join("drive");
    std::fs::create_dir_all(&dest).unwrap();
    let dir = f.db_path.parent().unwrap().to_path_buf();
    copy_then_retire(&dir, &dest, &f, quiesce(&f.db_path).unwrap()).unwrap();
    assert!(reg.archived().is_empty());
}

#[test]
fn every_earlier_description_is_kept_even_within_one_second() {
    let (_tmp, reg) = setup();
    let f = reg.create("Work", "one", None).unwrap();
    reg.describe("Work", "two").unwrap();
    reg.describe("Work", "three").unwrap();
    reg.describe("Work", "four").unwrap();
    let conn = open_read_only(&f.db_path).unwrap();
    let mut kept: Vec<String> = conn
        .prepare("SELECT value FROM _meta WHERE key LIKE 'folder_description@%'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    kept.sort();
    assert_eq!(kept, ["one", "three", "two"]);
    assert_eq!(reg.find("Work").unwrap().description, "four");
}

#[test]
fn the_first_folder_cannot_be_archived() {
    let (_tmp, reg) = setup();
    reg.ensure_default().unwrap();
    assert!(reg.archive(DEFAULT_NAME).is_err());
}

#[test]
fn identity_is_invisible_to_memory_counts_and_old_versions() {
    let (_tmp, reg) = setup();
    let f = reg.create("Work", "note", None).unwrap();
    let db = crate::db::DatabaseManager::new(&f.db_path.to_string_lossy()).unwrap();
    drop(db);
    assert_eq!(f.stats().unwrap().0, 0);
    let conn = open_read_only(&f.db_path).unwrap();
    let product: String = conn
        .query_row("SELECT value FROM _meta WHERE key='product'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(product, "geniuz");
}
