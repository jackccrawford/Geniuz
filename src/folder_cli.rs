//! `geniuz folder`: the command-line face of the folder registry.

use crate::cli::FolderCommand;
use geniuz::folders::{Folder, Registry, Scan};
use std::path::PathBuf;

const CLIPPED: usize = 200;

pub fn run(cmd: FolderCommand) -> Result<String, String> {
    let reg = Registry::from_env();
    match cmd {
        FolderCommand::List { archived: true, json } => list_archived(&reg, json),
        FolderCommand::List { archived: false, json } => list(&reg, json),
        FolderCommand::Show { name, json } => show(&reg, name.as_deref(), json),
        FolderCommand::Create { name, description, at } => {
            let f = reg.create(&name, &description, at.as_deref())?;
            Ok(format!("Created {} ({})\nKeep a memory in it with: geniuz keep -f {} -c \"...\" -g \"...\"", f.name, f.short_id(), f.short_id()))
        }
        FolderCommand::Add { path } => {
            let found = reg.add(&path)?;
            let mut lines = vec![format!("Added {}. Found {}:", path.display(), plural(found.len()))];
            lines.extend(found.iter().map(|f| format!("  {} ({})", f.name, f.short_id())));
            Ok(lines.join("\n"))
        }
        FolderCommand::Export { all: Some(dir), .. } => export_all(&reg, &dir),
        FolderCommand::Export { name, to, all: None } => {
            let name = name.ok_or("Name the folder to export, or use --all <DIRECTORY>.")?;
            let to = to.ok_or_else(|| format!("Say where to write it: geniuz folder export {} <FILE>", name))?;
            let f = reg.find(&name)?;
            let bytes = export_one(&f, &to)?;
            Ok(format!("Exported {} ({}) to {} ({}).\nIt is a backup to keep: every memory and image, in one file.",
                f.name, f.short_id(), to.display(), size(bytes)))
        }
        FolderCommand::Import { file, name } => {
            let got = reg.import(&file, name.as_deref())?;
            let f = &got.folder;
            let mut out = format!("Imported {} ({}): {}.", f.name, f.short_id(), memories(got.memories));
            if let Some(old) = &got.was_called {
                out.push_str(&format!(" It was called \"{}\"; its ID is the same.", old));
            }
            out.push_str(&format!(
                "\nIt holds what the folder held when it was exported. {} wasn't changed; keep it as a backup.\nRead it with: geniuz recall -f {}",
                file.display(), f.short_id()
            ));
            Ok(out)
        }
        FolderCommand::Remove { path } => {
            reg.remove(&path)?;
            Ok(format!("Geniuz no longer looks in {}. Its files weren't touched.", path.display()))
        }
        // 5.0.8 has no default folder. The command stays so whoever learned it is told
        // the new way; it changes nothing, and exits with an error so a script notices.
        FolderCommand::Use { .. } => Err(geniuz::folders::NO_DEFAULT_FOLDER.to_string()),
        FolderCommand::Rename { name, new_name } => {
            let f = reg.rename(&name, &new_name)?;
            Ok(format!("Renamed to {}. Its ID is still {}.", f.name, f.short_id()))
        }
        FolderCommand::Face { name, face } => {
            let f = reg.set_face(&name, face.as_deref().unwrap_or(""))?;
            Ok(if f.face.is_empty() {
                format!("{} has no face now. The window shows its mosaic.", f.name)
            } else {
                format!("{} now shows {} beside its name.", f.name, f.face)
            })
        }
        FolderCommand::Describe { name, description } => {
            let f = reg.describe(&name, &description)?;
            Ok(format!("Updated {}. The earlier wording is kept.", f.name))
        }
        FolderCommand::Move { name, to } => {
            let moved = reg.relocate(&name, &to)?;
            let mut out = format!("Moved {} to {}.", moved.folder.name, display_dir(&moved.folder));
            if let Some(old) = moved.left_behind {
                out.push_str(&format!(
                    "\nThe original is at {}. Geniuz won't read it again; delete it when you're satisfied.",
                    old.display()
                ));
            }
            Ok(out)
        }
        FolderCommand::Archive { name } => {
            let to = reg.archive(&name)?;
            Ok(format!(
                "Archived. Nothing was deleted; it's at {}.\nBring it back with: geniuz folder restore \"{}\"",
                to.display(),
                name
            ))
        }
        FolderCommand::Pin { name } => {
            let f = reg.set_pinned(&name, true)?;
            Ok(format!("Pinned {}.", f.name))
        }
        FolderCommand::Unpin { name } => {
            let f = reg.set_pinned(&name, false)?;
            Ok(format!("Unpinned {}.", f.name))
        }
        FolderCommand::Restore { name } => {
            let f = reg.restore(&name)?;
            Ok(format!("Restored {}.", f.name))
        }
    }
}

fn in_use() -> Option<PathBuf> {
    crate::resolve_db_path().ok().map(|p| std::fs::canonicalize(&p).unwrap_or(p))
}

fn is_in_use(f: &Folder, using: &Option<PathBuf>) -> bool {
    let path = std::fs::canonicalize(&f.db_path).unwrap_or_else(|_| f.db_path.clone());
    using.as_ref() == Some(&path)
}

fn list(reg: &Registry, json: bool) -> Result<String, String> {
    let scan = reg.scan();
    let using = in_use();
    if json {
        return list_json(reg, &scan, &using);
    }
    let pinned = reg.pinned();
    let mut lines = Vec::new();
    for f in &scan.folders {
        let mark = if is_in_use(f, &using) { "*" } else { " " };
        let (count, last) = f.stats().unwrap_or((0, None));
        let last = last.map(|t| crate::shorten_ts(&t)).unwrap_or_else(|| "never".to_string());
        let pin = if pinned.iter().any(|p| p.eq_ignore_ascii_case(&f.uuid)) { " · pinned" } else { "" };
        let mut line = format!("{} {} | {} | {} · {}{}", mark, f.short_id(), f.name, memories(count), last, pin);
        let summary = clip(&f.description);
        if !summary.is_empty() {
            line.push_str(&format!(" · {}", summary));
        }
        lines.push(line);
    }
    for b in &scan.blocked {
        lines.push(format!("! Blocked: {} {}: {} and {}. Move one of them.", b.name, b.reason, b.db_path.display(), b.other.display()));
    }
    for dir in &scan.pending_moves {
        lines.push(format!("? An unfinished move at {} is waiting for its original to be reachable.", dir.display()));
    }
    if let Some(why) = &scan.first_folder_problem {
        lines.push(format!("? {}", why));
    }
    for db in &scan.old_format {
        lines.push(format!(
            "? {} is from a Geniuz before 4.0 and isn't opened: back it up and run {} on it.",
            db.display(),
            geniuz::db::MIGRATION_SCRIPT
        ));
    }
    // The data directory comes first in `unavailable` when the first folder failed;
    // its reason is already said above.
    let skip = usize::from(scan.first_folder_problem.is_some());
    for dir in scan.unavailable.iter().skip(skip) {
        lines.push(format!("? Can't reach {} right now.", dir.display()));
    }
    Ok(lines.join("\n"))
}

fn list_archived(reg: &Registry, json: bool) -> Result<String, String> {
    let archived = reg.archived();
    if json {
        let rows: Vec<_> = archived.iter().map(|f| folder_json(f, false)).collect();
        return serde_json::to_string_pretty(&rows).map_err(|e| e.to_string());
    }
    if archived.is_empty() {
        return Ok("No archived folders.".to_string());
    }
    let lines: Vec<_> = archived
        .iter()
        .map(|f| {
            let (count, _) = f.stats().unwrap_or((0, None));
            format!("{} | {} | {} · {}", f.short_id(), f.name, memories(count), clip(&f.description))
        })
        .collect();
    Ok(lines.join("\n"))
}

fn list_json(reg: &Registry, scan: &Scan, using: &Option<PathBuf>) -> Result<String, String> {
    let pinned = reg.pinned();
    let folders: Vec<_> = scan
        .folders
        .iter()
        .map(|f| {
            let mut row = folder_json(f, is_in_use(f, using));
            row["pinned"] = serde_json::json!(pinned.iter().any(|p| p.eq_ignore_ascii_case(&f.uuid)));
            row
        })
        .collect();
    let blocked: Vec<_> = scan
        .blocked
        .iter()
        .map(|b| serde_json::json!({ "name": b.name, "uuid": b.uuid, "path": b.db_path, "other": b.other, "reason": b.reason }))
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "folders": folders,
        "blocked": blocked,
        "unavailable": scan.unavailable,
        "first_folder_problem": scan.first_folder_problem,
        "old_format": scan.old_format,
    }))
    .map_err(|e| e.to_string())
}

fn folder_json(f: &Folder, using: bool) -> serde_json::Value {
    let (count, last) = f.stats().unwrap_or((0, None));
    serde_json::json!({
        "id": f.short_id(),
        "uuid": f.uuid,
        "name": f.name,
        "description": f.description,
        "face": f.face,
        "created": f.created,
        "memories": count,
        "last_written": last,
        "path": f.db_path,
        "in_use": using,
    })
}

fn show(reg: &Registry, name: Option<&str>, json: bool) -> Result<String, String> {
    // Resolved once: the path in use, or the reason there isn't one.
    let resolved = crate::resolve_db_path().map(|p| std::fs::canonicalize(&p).unwrap_or(p));
    let using = resolved.as_ref().ok().cloned();
    let folder = match name {
        Some(q) => reg.find(q)?,
        None => {
            // Say why there's no folder in use, as the command that resolves it would.
            if let Err(e) = &resolved {
                return Err(e.clone());
            }
            reg.scan()
                .folders
                .into_iter()
                .find(|f| is_in_use(f, &using))
                .ok_or_else(|| "The database in use isn't one of your folders: an explicit database file \
                                (GENIUZ_DB or GENIUZ_STATION) names a file outside them.".to_string())?
        }
    };
    if json {
        return serde_json::to_string_pretty(&folder_json(&folder, is_in_use(&folder, &using)))
            .map_err(|e| e.to_string());
    }
    let (count, last) = folder.stats()?;
    let mut lines = vec![
        folder.name.clone(),
        format!("ID: {}", folder.uuid),
        format!("Memories: {}", count),
        format!("Last written: {}", last.map(|t| crate::shorten_ts(&t)).unwrap_or_else(|| "never".to_string())),
        format!("Location: {}", display_dir(&folder)),
    ];
    if !folder.face.is_empty() {
        lines.insert(1, format!("Face: {}", folder.face));
    }
    if !folder.description.is_empty() {
        lines.push(String::new());
        lines.push(folder.description.clone());
    }
    Ok(lines.join("\n"))
}

fn display_dir(f: &Folder) -> String {
    f.db_path.parent().unwrap_or(&f.db_path).display().to_string()
}

fn memories(n: i64) -> String {
    if n == 1 { "1 memory".to_string() } else { format!("{} memories", n) }
}

fn plural(n: usize) -> String {
    if n == 1 { "1 folder".to_string() } else { format!("{} folders", n) }
}

/// A description on one line, short enough for a list. By length, not by line:
/// a rule on its second line is still a rule, and agents read this list too.
fn clip(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= CLIPPED {
        return flat;
    }
    let cut: String = flat.chars().take(CLIPPED).collect();
    format!("{}…", cut.trim_end())
}

/// A whole number of bytes as a person reads it.
fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{} KB", b >> 10),
        b => format!("{} bytes", b),
    }
}

/// One folder to one file. Never over a file that is already there: a backup that
/// replaced an older backup by accident is worse than an error.
fn export_one(f: &Folder, to: &std::path::Path) -> Result<u64, String> {
    if to.is_dir() {
        return Err(format!("{} is a directory. Name the file to write, such as {}.",
            to.display(), to.join(export_name(f)).display()));
    }
    // Take the name first, in one step that fails if anything is there: looking and
    // then copying would let a file that appeared in between be written over.
    let exists = || format!("{} already exists, and an export never overwrites a file. Choose another name.", to.display());
    match std::fs::OpenOptions::new().write(true).create_new(true).open(to) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err(exists()),
        Err(e) => return Err(format!("Couldn't write {}: {}", to.display(), e)),
    }
    // Copy beside it, and put the whole file in place only when it is complete, so a
    // copy that fails leaves nothing behind to block the next try.
    let partial = to.with_file_name(format!(
        ".{}.partial-{}", to.file_name().and_then(|n| n.to_str()).unwrap_or("export"), std::process::id()
    ));
    let done = geniuz::folders::backup_to(&f.db_path, &partial)
        .and_then(|bytes| std::fs::rename(&partial, to).map(|_| bytes).map_err(|e| format!("Couldn't finish {}: {}", to.display(), e)));
    if done.is_err() {
        let _ = std::fs::remove_file(&partial);
        let _ = std::fs::remove_file(to);
    }
    done
}

/// `Work-13625F6B.db`: the name for whoever reads the directory, the ID because
/// two folders can share a name. Characters a file name can't hold become `-`.
fn export_name(f: &Folder) -> String {
    let name: String = f.name.chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' || c == '.' { c } else { '-' })
        .collect();
    format!("{}-{}.db", name.trim(), f.short_id())
}

fn export_all(reg: &Registry, dir: &std::path::Path) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't create {}: {}", dir.display(), e))?;
    let scan = reg.scan();
    let (mut lines, mut done, mut total) = (Vec::new(), 0, 0u64);
    // What Geniuz knows of but can't use right now is not exported, and must be said:
    // a backup that quietly leaves a folder out is the one found missing later.
    for dir in &scan.unavailable {
        lines.push(format!("  {}: can't be reached right now", dir.display()));
    }
    for b in &scan.blocked {
        lines.push(format!("  {} ({}): {}", b.name, b.uuid.chars().take(8).collect::<String>(), b.reason));
    }
    let folders = scan.folders;
    for f in &folders {
        match export_one(f, &dir.join(export_name(f))) {
            Ok(bytes) => { done += 1; total += bytes; }
            Err(e) => lines.push(format!("  {} ({}): {}", f.name, f.short_id(), e)),
        }
    }
    let mut out = format!("Exported {} of {} to {} ({}).", done, plural(folders.len()), dir.display(), size(total));
    if lines.is_empty() {
        Ok(out)
    } else {
        // Some were not written: say which, and fail, so a nightly script notices.
        out.push_str("\nNot exported:\n");
        out.push_str(&lines.join("\n"));
        Err(out)
    }
}

#[cfg(test)]
mod export_tests {
    use super::*;

    fn folder(name: &str, db: &std::path::Path) -> Folder {
        Folder {
            uuid: "13625F6B-0000-4000-8000-000000000000".into(), name: name.into(),
            description: String::new(), face: String::new(), created: String::new(),
            db_path: db.to_path_buf(),
        }
    }

    #[test]
    fn an_export_is_named_for_its_folder_and_never_replaces_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("memory.db");
        geniuz::db::DatabaseManager::new(&db.to_string_lossy()).unwrap()
            .keep_with_backend("kept", "kept", None, None).ok();
        let f = folder("Q3: plans/drafts", &db);
        // What a file name can't hold becomes a dash; the ID tells two "Work"s apart.
        assert_eq!(export_name(&f), "Q3- plans-drafts-13625F6B.db");

        let to = dir.path().join("copy.db");
        assert!(export_one(&f, &to).unwrap() > 0);
        let first = std::fs::read(&to).unwrap();
        assert!(export_one(&f, &to).unwrap_err().contains("already exists"));
        assert_eq!(std::fs::read(&to).unwrap(), first, "a refused export changes nothing");
        // A copy that fails leaves no file behind to block the next try.
        let gone = folder("gone", &dir.path().join("no-such-folder").join("memory.db"));
        let other = dir.path().join("other.db");
        assert!(export_one(&gone, &other).is_err());
        assert!(!other.exists());
        let left: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("partial")).collect();
        assert!(left.is_empty());
        assert!(export_one(&f, dir.path()).unwrap_err().contains("is a directory"));
        assert_eq!(size(512), "512 bytes");
        assert_eq!(size(217_088), "212 KB");
    }
}
