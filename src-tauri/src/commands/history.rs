use crate::utils::{download::copy_dir_recursive, skill_identity::read_skill_uuid};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub static HISTORY_LOCK: Mutex<()> = Mutex::new(());
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    id: String,
    skill_uuid: String,
    created_at: u64,
    reason: String,
}

fn home() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or("Cannot locate home directory".into())
}
fn safe_tree(path: &Path) -> Result<(), String> {
    let mut size = 0u64;
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        let meta = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            return Err("Snapshots cannot contain symbolic links".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("Snapshots cannot contain reparse points".into());
            }
        }
        size = size.saturating_add(meta.len());
        if size > 200 * 1024 * 1024 {
            return Err("Snapshot exceeds 200 MiB limit".into());
        }
    }
    Ok(())
}
fn target(home: &Path, path: &Path) -> Result<PathBuf, String> {
    let target = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let allowed = [
        home.join("Skill Manager/Skills"),
        home.join(".skills-manager/skills"),
    ]
    .iter()
    .any(|root| fs::canonicalize(root).ok().as_deref() == target.parent());
    if !allowed || !target.join("SKILL.md").is_file() {
        return Err("Not a managed Skill".into());
    }
    safe_tree(path)?;
    Ok(target)
}
fn root(home: &Path) -> Result<PathBuf, String> {
    let root = home.join("Skill Manager/.history");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    // Check the root itself without walking every historical snapshot.
    for path in [home.join("Skill Manager"), root.clone()] {
        let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            return Err("History root cannot be a link".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("History root cannot be a junction".into());
            }
        }
    }
    Ok(root)
}
fn snapshot(home: &Path, path: &Path, reason: &str) -> Result<String, String> {
    let path = target(home, path)?;
    let identity = read_skill_uuid(&path).ok_or("Skill has no UUID; refresh the library first")?;
    let id = uuid::Uuid::new_v4().to_string();
    let directory = root(home)?.join(&id);
    fs::create_dir(&directory).map_err(|e| e.to_string())?;
    // Incomplete snapshots have no record.json and are never offered for restore.
    copy_dir_recursive(&path, &directory.join("content"))?;
    let record = Snapshot {
        id: id.clone(),
        skill_uuid: identity,
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs(),
        reason: reason.into(),
    };
    use std::io::Write;
    let mut file = fs::File::create(directory.join("record.json")).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(id)
}
pub fn snapshot_before_update(path: &Path) -> Result<String, String> {
    snapshot(&home()?, path, "before-update")
}
pub(crate) fn snapshot_before_edit_at(home: &Path, path: &Path) -> Result<String, String> {
    snapshot(home, path, "before-edit")
}

fn restore(home: &Path, path: &Path, id: &str) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| "Invalid snapshot ID")?;
    if parsed.to_string() != id {
        return Err("Invalid snapshot ID".into());
    }
    let path = target(home, path)?;
    let directory = root(home)?.join(id);
    safe_tree(&directory)?;
    let record: Snapshot = serde_json::from_slice(
        &fs::read(directory.join("record.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if record.id != id
        || read_skill_uuid(&path).as_deref() != Some(&record.skill_uuid)
        || read_skill_uuid(&directory.join("content")).as_deref() != Some(&record.skill_uuid)
    {
        return Err("Snapshot UUID mismatch; restore refused".into());
    }
    // Keep the current version too, so a rollback can itself be reversed.
    snapshot(home, &path, "before-restore")?;
    let stage = path
        .parent()
        .unwrap()
        .join(format!(".restore-{}", uuid::Uuid::new_v4()));
    copy_dir_recursive(&directory.join("content"), &stage)?;
    let previous = path
        .parent()
        .unwrap()
        .join(format!(".previous-{}", uuid::Uuid::new_v4()));
    fs::rename(&path, &previous).map_err(|e| e.to_string())?;
    if let Err(error) = fs::rename(&stage, &path) {
        let recovery = fs::rename(&previous, &path);
        return Err(format!(
            "Restore failed: {error}; original recovery: {recovery:?}; preserved files: {}",
            previous.display()
        ));
    }
    // Previous content remains safely stored in a completed history snapshot.
    fs::remove_dir_all(&previous)
        .map_err(|e| format!("Restored, but previous staging folder needs cleanup: {e}"))?;
    Ok(())
}
#[tauri::command]
pub fn list_skill_history(skill_path: String) -> Result<Vec<Snapshot>, String> {
    let _guard = HISTORY_LOCK.lock().map_err(|_| "History unavailable")?;
    let home = home()?;
    let path = target(&home, Path::new(&skill_path))?;
    let identity = read_skill_uuid(&path).ok_or("Missing Skill UUID")?;
    let mut records = Vec::new();
    for entry in fs::read_dir(root(&home)?).map_err(|e| e.to_string())? {
        let directory = entry.map_err(|e| e.to_string())?.path();
        if !directory.join("record.json").exists() {
            continue;
        }
        safe_tree(&directory)?;
        let record: Snapshot = serde_json::from_slice(
            &fs::read(directory.join("record.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if record.skill_uuid == identity {
            records.push(record);
        }
    }
    records.sort_by_key(|record| std::cmp::Reverse(record.created_at));
    Ok(records)
}
#[tauri::command]
pub fn restore_skill_history(skill_path: String, id: String) -> Result<(), String> {
    let _guard = HISTORY_LOCK.lock().map_err(|_| "History unavailable")?;
    restore(&home()?, Path::new(&skill_path), &id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rollback_preserves_uuid_and_saves_current_version() {
        let home = std::env::temp_dir().join(format!("history-test-{}", uuid::Uuid::new_v4()));
        let skill = home.join("Skill Manager/Skills/demo");
        fs::create_dir_all(&skill).unwrap();
        let uuid = uuid::Uuid::new_v4().to_string();
        let content = format!("---\nname: demo\nuuid: {uuid}\n---\nold");
        fs::write(skill.join("SKILL.md"), &content).unwrap();
        fs::write(skill.join("asset.bin"), [0, 255]).unwrap();
        let id = snapshot(&home, &skill, "before-update").unwrap();
        fs::write(skill.join("SKILL.md"), content.replace("old", "new")).unwrap();
        assert!(restore(&home, &skill, "../invalid").is_err());
        restore(&home, &skill, &id).unwrap();
        assert_eq!(fs::read_to_string(skill.join("SKILL.md")).unwrap(), content);
        assert_eq!(fs::read(skill.join("asset.bin")).unwrap(), [0, 255]);
        assert_eq!(fs::read_dir(root(&home).unwrap()).unwrap().count(), 2);
        fs::remove_dir_all(home).unwrap();
    }
}
