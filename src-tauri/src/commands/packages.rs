use crate::utils::skill_identity::{normalize_skill_uuid, read_skill_uuid};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

static PACKAGE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPackage {
    id: String,
    name: String,
    description: String,
    skill_uuids: Vec<String>,
    created_at: u64,
    updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageStore {
    schema_version: u32,
    revision: u64,
    packages: Vec<SkillPackage>,
}

impl Default for PackageStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            packages: Vec::new(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePackageRequest {
    id: Option<String>,
    name: String,
    description: String,
    skill_uuids: Vec<String>,
    revision: u64,
}

fn config_path(home: &Path) -> PathBuf {
    home.join("Skill Manager/.metadata/skill-packages.json")
}

fn read_store(home: &Path) -> Result<PackageStore, String> {
    let raw = match fs::read_to_string(config_path(home)) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PackageStore::default())
        }
        Err(err) => return Err(err.to_string()),
    };
    let store: PackageStore =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid skill-packages.json: {e}"))?;
    if store.schema_version != 1 {
        return Err("Unsupported skill package configuration version".into());
    }
    let mut ids = HashSet::new();
    for package in &store.packages {
        if normalize_skill_uuid(&package.id).as_deref() != Some(&package.id)
            || !ids.insert(&package.id)
            || package
                .skill_uuids
                .iter()
                .any(|id| normalize_skill_uuid(id).as_deref() != Some(id))
        {
            return Err("Invalid or duplicate UUID in skill-packages.json".into());
        }
    }
    Ok(store)
}

fn write_store(home: &Path, store: &PackageStore) -> Result<(), String> {
    let path = config_path(home);
    let parent = path.parent().ok_or("Invalid package configuration path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = parent.join(format!("skill-packages-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?;
        file.write_all(&json).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temp, &path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn managed_ids(home: &Path) -> Result<HashSet<String>, String> {
    let mut ids = HashSet::new();
    for root in [
        home.join("Skill Manager/Skills"),
        home.join(".skills-manager/skills"),
    ] {
        let entries = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err.to_string()),
        };
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if let Some(id) = read_skill_uuid(&path) {
                ids.insert(id);
            }
        }
    }
    Ok(ids)
}

fn save(home: &Path, request: SavePackageRequest) -> Result<PackageStore, String> {
    let mut store = read_store(home)?;
    if request.revision != store.revision {
        return Err("Packages changed. Refresh and retry / 包配置已变化，请刷新后重试".into());
    }
    let name = request.name.trim();
    if name.is_empty() || name.chars().count() > 100 || request.description.chars().count() > 4000 {
        return Err(
            "包名称需为 1–100 字符，描述最多 4000 字符 / Invalid package name or description"
                .into(),
        );
    }
    let previous = match request.id.as_ref() {
        Some(id) => Some(
            store
                .packages
                .iter()
                .find(|p| &p.id == id)
                .ok_or("Package no longer exists")?,
        ),
        None => None,
    };
    let mut allowed = managed_ids(home)?;
    if let Some(old) = previous {
        allowed.extend(old.skill_uuids.iter().cloned());
    }
    let mut members = Vec::new();
    let mut seen = HashSet::new();
    for raw in request.skill_uuids {
        let id = normalize_skill_uuid(&raw).ok_or("Invalid member UUID")?;
        if !allowed.contains(&id) {
            return Err(format!("Skill is not managed / Skill 未托管: {id}"));
        }
        if seen.insert(id.clone()) {
            members.push(id);
        }
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let package = SkillPackage {
        id: previous
            .map(|p| p.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        created_at: previous.map(|p| p.created_at).unwrap_or(now),
        updated_at: now,
        name: name.to_string(),
        description: request.description.trim().to_string(),
        skill_uuids: members,
    };
    if let Some(index) = store.packages.iter().position(|p| p.id == package.id) {
        store.packages[index] = package;
    } else {
        store.packages.push(package);
    }
    store.revision += 1;
    write_store(home, &store)?;
    Ok(store)
}

fn delete(home: &Path, id: &str, revision: u64) -> Result<PackageStore, String> {
    let mut store = read_store(home)?;
    if store.revision != revision {
        return Err("Packages changed. Refresh and retry / 包配置已变化，请刷新后重试".into());
    }
    let index = store
        .packages
        .iter()
        .position(|p| p.id == id)
        .ok_or("Package no longer exists")?;
    store.packages.remove(index);
    store.revision += 1;
    write_store(home, &store)?;
    Ok(store)
}

#[tauri::command]
pub fn list_skill_packages() -> Result<PackageStore, String> {
    let _guard = PACKAGE_LOCK.lock().map_err(|e| e.to_string())?;
    read_store(&dirs::home_dir().ok_or("Cannot determine home directory")?)
}

#[tauri::command]
pub fn save_skill_package(request: SavePackageRequest) -> Result<PackageStore, String> {
    let _guard = PACKAGE_LOCK.lock().map_err(|e| e.to_string())?;
    save(
        &dirs::home_dir().ok_or("Cannot determine home directory")?,
        request,
    )
}

#[tauri::command]
pub fn delete_skill_package(id: String, revision: u64) -> Result<PackageStore, String> {
    let _guard = PACKAGE_LOCK.lock().map_err(|e| e.to_string())?;
    delete(
        &dirs::home_dir().ok_or("Cannot determine home directory")?,
        &id,
        revision,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::skill_identity::ensure_skill_uuid;

    #[test]
    fn package_lifecycle_preserves_skills_and_missing_references() {
        let home = std::env::temp_dir().join(format!("package-test-{}", Uuid::new_v4()));
        let skill = home.join("Skill Manager/Skills/example");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# Example").unwrap();
        let uuid = ensure_skill_uuid(&skill, None).unwrap();
        let original = fs::read(skill.join("SKILL.md")).unwrap();
        let request = |id, revision, members| SavePackageRequest {
            id,
            revision,
            name: " Test ".into(),
            description: "Description".into(),
            skill_uuids: members,
        };
        let created = save(&home, request(None, 0, vec![uuid.clone(), uuid.clone()])).unwrap();
        assert_eq!(created.packages[0].name, "Test");
        assert_eq!(created.packages[0].skill_uuids, vec![uuid.clone()]);
        let id = created.packages[0].id.clone();
        assert!(save(&home, request(Some(id.clone()), 0, vec![])).is_err());
        assert!(save(&home, request(None, 1, vec![Uuid::new_v4().to_string()])).is_err());
        assert_eq!(fs::read(skill.join("SKILL.md")).unwrap(), original);
        fs::rename(&skill, home.join("detached")).unwrap();
        let updated = save(&home, request(Some(id.clone()), 1, vec![uuid])).unwrap();
        assert_eq!(updated.packages[0].id, id);
        assert_eq!(read_store(&home).unwrap().revision, 2);
        assert!(delete(&home, &id, 1).is_err());
        assert!(delete(&home, &id, 2).unwrap().packages.is_empty());
        assert_eq!(fs::read(home.join("detached/SKILL.md")).unwrap(), original);
        fs::write(config_path(&home), "broken json").unwrap();
        assert!(save(&home, request(None, 0, vec![])).is_err());
        assert_eq!(
            fs::read_to_string(config_path(&home)).unwrap(),
            "broken json"
        );
        fs::remove_dir_all(home).unwrap();
    }
}
