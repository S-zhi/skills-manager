use crate::utils::skill_identity::{read_skill_name, read_skill_uuid};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

static LOCK: Mutex<()> = Mutex::new(());
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashItem {
    id: String,
    name: String,
    uuid: Option<String>,
    original_path: String,
    deleted_at: u64,
}
fn home() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or("Cannot locate home directory".into())
}
fn plain_directory(path: &Path) -> Result<PathBuf, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("Refusing linked or non-directory path".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("Refusing junction/reparse directory".into());
        }
    }
    fs::canonicalize(path).map_err(|e| e.to_string())
}
fn trash_root(home: &Path) -> Result<PathBuf, String> {
    let manager = home.join("Skill Manager");
    fs::create_dir_all(&manager).map_err(|e| e.to_string())?;
    plain_directory(&manager)?;
    let root = manager.join(".trash");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    plain_directory(&root)
}
fn validate_source(home: &Path, path: &Path) -> Result<PathBuf, String> {
    let target = plain_directory(path)?;
    let mut allowed = false;
    for root in [
        home.join("Skill Manager/Skills"),
        home.join(".skills-manager/skills"),
    ] {
        if root.exists() && target.parent() == Some(plain_directory(&root)?.as_path()) {
            allowed = true;
        }
    }
    if !allowed || !target.join("SKILL.md").is_file() {
        return Err("Only direct managed Skill folders may be recycled".into());
    }
    Ok(target)
}
pub fn recycle(home: &Path, paths: Vec<String>) -> Result<String, String> {
    let _lock = LOCK.lock().map_err(|_| "Recycle bin unavailable")?;
    if paths.is_empty() {
        return Err("No Skills selected".into());
    }
    // Validate the complete batch before moving anything.
    let mut targets = Vec::new();
    for path in paths {
        let target = validate_source(home, Path::new(&path))?;
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    let root = trash_root(home)?;
    let mut count = 0;
    for target in targets {
        let id = uuid::Uuid::new_v4().to_string();
        let entry = root.join(&id);
        let item = TrashItem {
            id,
            name: read_skill_name(&target)
                .unwrap_or_else(|| target.file_name().unwrap().to_string_lossy().into()),
            uuid: read_skill_uuid(&target),
            original_path: target.to_string_lossy().into(),
            deleted_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs(),
        };
        fs::create_dir(&entry).map_err(|e| e.to_string())?;
        let write_record = (|| {
            use std::io::Write;
            let mut record = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(entry.join("record.json"))
                .map_err(|e| e.to_string())?;
            record
                .write_all(&serde_json::to_vec_pretty(&item).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            record.sync_all().map_err(|e| e.to_string())
        })();
        if let Err(error) = write_record {
            let _ = fs::remove_file(entry.join("record.json"));
            let _ = fs::remove_dir(&entry);
            return Err(format!(
                "Recycled {count}; cannot save recovery record: {error}"
            ));
        }
        // Rename only: a cross-volume failure leaves the source intact, never copy-then-delete.
        if let Err(error) = fs::rename(&target, entry.join("content")) {
            let _ = fs::remove_file(entry.join("record.json"));
            let _ = fs::remove_dir(&entry);
            return Err(format!(
                "已移入回收站 {count} 项；其余未完成 / Recycled {count}; move failed: {error}"
            ));
        }
        count += 1;
    }
    Ok(format!("已移入回收站 {count} 项 / Recycled {count} Skills"))
}
fn entry(home: &Path, id: &str) -> Result<(PathBuf, TrashItem), String> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| "Invalid recycle bin ID")?;
    if parsed.to_string() != id {
        return Err("Invalid recycle bin ID".into());
    }
    let root = trash_root(home)?;
    let path = plain_directory(&root.join(id))?;
    if path.parent() != Some(root.as_path()) {
        return Err("Invalid recycle bin entry".into());
    }
    let item: TrashItem =
        serde_json::from_slice(&fs::read(path.join("record.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if item.id != id {
        return Err("Recycle bin metadata mismatch".into());
    }
    plain_directory(&path.join("content"))?;
    Ok((path, item))
}
fn restore(home: &Path, id: &str) -> Result<String, String> {
    let (path, item) = entry(home, id)?;
    let target = PathBuf::from(&item.original_path);
    let parent = target.parent().ok_or("Invalid original path")?;
    let mut allowed = false;
    for root in [
        home.join("Skill Manager/Skills"),
        home.join(".skills-manager/skills"),
    ] {
        if root.exists() && plain_directory(parent)? == plain_directory(&root)? {
            allowed = true;
        }
    }
    if !allowed || target.file_name().is_none() {
        return Err("Original managed directory is unavailable".into());
    }
    if fs::symlink_metadata(&target).is_ok() {
        return Err(
            "原位置已存在文件，未覆盖。请先处理同名目录 / Original path is occupied".into(),
        );
    }
    if let Some(identity) = item.uuid.as_deref() {
        for root in [
            home.join("Skill Manager/Skills"),
            home.join(".skills-manager/skills"),
        ] {
            if !root.exists() {
                continue;
            }
            for current in fs::read_dir(root).map_err(|e| e.to_string())? {
                let current = current.map_err(|e| e.to_string())?;
                if read_skill_uuid(&current.path()).as_deref() == Some(identity) {
                    return Err(
                        "管理库已存在相同 UUID，请先处理身份冲突 / UUID already exists in library"
                            .into(),
                    );
                }
            }
        }
    }
    fs::rename(path.join("content"), &target).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(path.join("record.json"));
    let _ = fs::remove_dir(&path);
    Ok(target.to_string_lossy().into())
}
#[tauri::command]
pub fn list_trashed_skills() -> Result<Vec<TrashItem>, String> {
    let _lock = LOCK.lock().map_err(|_| "Recycle bin unavailable")?;
    let home = home()?;
    let mut items = Vec::new();
    for file in fs::read_dir(trash_root(&home)?).map_err(|e| e.to_string())? {
        let file = file.map_err(|e| e.to_string())?;
        let id = file.file_name().to_string_lossy().into_owned();
        let (_, item) =
            entry(&home, &id).map_err(|e| format!("回收站记录异常 {id}: {e}（未删除数据）"))?;
        items.push(item);
    }
    items.sort_by_key(|item| std::cmp::Reverse(item.deleted_at));
    Ok(items)
}
#[tauri::command]
pub fn restore_trashed_skill(id: String) -> Result<String, String> {
    let _lock = LOCK.lock().map_err(|_| "Recycle bin unavailable")?;
    restore(&home()?, &id)
}
#[tauri::command]
pub fn permanently_delete_trashed_skill(id: String, confirmation: String) -> Result<(), String> {
    let _lock = LOCK.lock().map_err(|_| "Recycle bin unavailable")?;
    if confirmation != "DELETE" {
        return Err("Type DELETE to confirm permanent deletion".into());
    }
    let (path, _) = entry(&home()?, &id)?;
    // Reject nested links/reparse points before recursive deletion.
    for file in walkdir::WalkDir::new(path.join("content")).follow_links(false) {
        let file = file.map_err(|e| e.to_string())?;
        if file.file_type().is_symlink() {
            return Err("Contains links; permanent deletion refused".into());
        }
        if file.file_type().is_dir() {
            plain_directory(file.path())?;
        }
    }
    fs::remove_dir_all(&path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recycling_preserves_content_and_restore_refuses_conflicts() {
        let home = std::env::temp_dir().join(format!("trash-test-{}", uuid::Uuid::new_v4()));
        let skill = home.join("Skill Manager/Skills/demo");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: demo\n---\ncontent").unwrap();
        assert!(recycle(
            &home,
            vec![home.join("Skill Manager/Skills").to_string_lossy().into()]
        )
        .is_err());
        recycle(&home, vec![skill.to_string_lossy().into()]).unwrap();
        assert!(!skill.exists());
        let id = fs::read_dir(trash_root(&home).unwrap())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .file_name()
            .to_string_lossy()
            .into_owned();
        assert!(entry(&home, "../bad").is_err());
        fs::create_dir(&skill).unwrap();
        assert!(restore(&home, &id).is_err());
        fs::remove_dir(&skill).unwrap();
        restore(&home, &id).unwrap();
        assert!(fs::read_to_string(skill.join("SKILL.md"))
            .unwrap()
            .contains("content"));
        fs::remove_dir_all(home).unwrap();
    }
}
