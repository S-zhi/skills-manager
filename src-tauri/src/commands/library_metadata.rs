use crate::utils::skill_identity::normalize_skill_uuid;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

static METADATA_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLibraryEntry {
    uuid: String,
    favorite: bool,
    tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLibraryStore {
    schema_version: u32,
    revision: u64,
    entries: Vec<SkillLibraryEntry>,
}

impl Default for SkillLibraryStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            entries: Vec::new(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSkillLibraryEntryRequest {
    uuid: String,
    favorite: bool,
    tags: Vec<String>,
    revision: u64,
}

fn path(home: &Path) -> PathBuf {
    home.join("Skill Manager/.metadata/skill-library.json")
}

fn read(home: &Path) -> Result<SkillLibraryStore, String> {
    let raw = match fs::read_to_string(path(home)) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SkillLibraryStore::default())
        }
        Err(error) => return Err(error.to_string()),
    };
    let store: SkillLibraryStore = serde_json::from_str(&raw)
        .map_err(|error| format!("Invalid skill-library.json: {error}"))?;
    if store.schema_version != 1 {
        return Err("Unsupported skill library metadata version".into());
    }
    let mut ids = HashSet::new();
    if store.entries.iter().any(|entry| {
        normalize_skill_uuid(&entry.uuid).as_deref() != Some(entry.uuid.as_str())
            || !ids.insert(entry.uuid.clone())
            || entry.tags.len() > 20
            || entry
                .tags
                .iter()
                .any(|tag| tag.is_empty() || tag.chars().count() > 32)
    }) {
        return Err("Invalid entry in skill-library.json".into());
    }
    Ok(store)
}

fn write(home: &Path, store: &SkillLibraryStore) -> Result<(), String> {
    let target = path(home);
    let parent = target.parent().ok_or("Invalid metadata path")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temp = parent.join(format!("skill-library-{}.tmp", uuid::Uuid::new_v4()));
    let previous = parent.join(format!("skill-library-{}.previous", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        if !target.exists() {
            return fs::rename(&temp, &target).map_err(|e| e.to_string());
        }
        fs::rename(&target, &previous).map_err(|e| e.to_string())?;
        if let Err(error) = fs::rename(&temp, &target) {
            let recovery = fs::rename(&previous, &target);
            return Err(format!(
                "Cannot save metadata: {error}; recovery: {recovery:?}"
            ));
        }
        fs::remove_file(&previous).map_err(|e| format!("Saved; old metadata cleanup failed: {e}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn save(home: &Path, request: SaveSkillLibraryEntryRequest) -> Result<SkillLibraryStore, String> {
    let mut store = read(home)?;
    if store.revision != request.revision {
        return Err(
            "收藏或标签已变化，请刷新后重试 / Favorites or tags changed; refresh and retry".into(),
        );
    }
    let uuid = normalize_skill_uuid(&request.uuid).ok_or("Invalid Skill UUID")?;
    let mut tags = Vec::new();
    let mut normalized = HashSet::new();
    for raw in request.tags {
        let tag = raw.trim();
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > 32 {
            return Err("每个标签最多 32 个字符 / Tags are limited to 32 characters".into());
        }
        let key = tag.to_lowercase();
        if normalized.insert(key) {
            tags.push(tag.to_string());
        }
    }
    if tags.len() > 20 {
        return Err("每个 Skill 最多 20 个标签 / Up to 20 tags per Skill".into());
    }
    store.entries.retain(|entry| entry.uuid != uuid);
    if request.favorite || !tags.is_empty() {
        store.entries.push(SkillLibraryEntry {
            uuid,
            favorite: request.favorite,
            tags,
        });
        store
            .entries
            .sort_by(|left, right| left.uuid.cmp(&right.uuid));
    }
    store.revision += 1;
    write(home, &store)?;
    Ok(store)
}

#[tauri::command]
pub fn get_skill_library_metadata() -> Result<SkillLibraryStore, String> {
    let _guard = METADATA_LOCK.lock().map_err(|_| "Metadata unavailable")?;
    read(&dirs::home_dir().ok_or("Cannot determine home directory")?)
}

#[tauri::command]
pub fn save_skill_library_entry(
    request: SaveSkillLibraryEntryRequest,
) -> Result<SkillLibraryStore, String> {
    let _guard = METADATA_LOCK.lock().map_err(|_| "Metadata unavailable")?;
    save(
        &dirs::home_dir().ok_or("Cannot determine home directory")?,
        request,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saves_normalized_favorites_and_tags_with_revision_checks() {
        let home = std::env::temp_dir().join(format!("library-metadata-{}", uuid::Uuid::new_v4()));
        let id = uuid::Uuid::new_v4().to_string();
        let store = save(
            &home,
            SaveSkillLibraryEntryRequest {
                uuid: id.clone(),
                favorite: true,
                tags: vec![" Work ".into(), "work".into(), "Windows".into()],
                revision: 0,
            },
        )
        .unwrap();
        assert_eq!(store.revision, 1);
        assert_eq!(store.entries[0].tags, vec!["Work", "Windows"]);
        assert!(save(
            &home,
            SaveSkillLibraryEntryRequest {
                uuid: id.clone(),
                favorite: false,
                tags: vec![],
                revision: 0
            }
        )
        .is_err());
        let empty = save(
            &home,
            SaveSkillLibraryEntryRequest {
                uuid: id,
                favorite: false,
                tags: vec![],
                revision: 1,
            },
        )
        .unwrap();
        assert!(empty.entries.is_empty());
        fs::remove_dir_all(home).unwrap();
    }
}
