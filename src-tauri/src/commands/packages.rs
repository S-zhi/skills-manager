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
const UNCATEGORIZED_GROUP_ID: &str = "__uncategorized__";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPackage {
    id: String,
    name: String,
    description: String,
    skill_uuids: Vec<String>,
    #[serde(default)]
    position: u32,
    created_at: u64,
    updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageStore {
    schema_version: u32,
    revision: u64,
    #[serde(default)]
    uncategorized_position: u32,
    packages: Vec<SkillPackage>,
}

impl Default for PackageStore {
    fn default() -> Self {
        Self {
            schema_version: 3,
            revision: 0,
            uncategorized_position: 0,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignPackageMembersRequest {
    package_id: Option<String>,
    skill_uuids: Vec<String>,
    revision: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderPackagesRequest {
    group_ids: Vec<String>,
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
    let mut store: PackageStore =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid skill-packages.json: {e}"))?;
    if !matches!(store.schema_version, 1..=3) {
        return Err("Unsupported skill package configuration version".into());
    }
    let source_schema_version = store.schema_version;
    let mut ids = HashSet::new();
    let mut assigned = HashSet::new();
    for (index, package) in store.packages.iter_mut().enumerate() {
        if normalize_skill_uuid(&package.id).as_deref() != Some(&package.id)
            || !ids.insert(&package.id)
            || package
                .skill_uuids
                .iter()
                .any(|id| normalize_skill_uuid(id).as_deref() != Some(id))
        {
            return Err("Invalid or duplicate UUID in skill-packages.json".into());
        }
        if source_schema_version < 3 {
            package.position = index as u32 + 1;
        }
        if source_schema_version == 1 {
            package.skill_uuids.retain(|id| assigned.insert(id.clone()));
        } else if package
            .skill_uuids
            .iter()
            .any(|id| !assigned.insert(id.clone()))
        {
            return Err("A Skill can belong to only one package".into());
        }
    }
    if source_schema_version == 3 {
        let mut positions = HashSet::new();
        if !positions.insert(store.uncategorized_position)
            || store
                .packages
                .iter()
                .any(|package| !positions.insert(package.position))
            || positions.len() != store.packages.len() + 1
            || positions
                .iter()
                .any(|position| *position as usize >= positions.len())
        {
            return Err("Invalid Skill group positions in skill-packages.json".into());
        }
        store.packages.sort_by_key(|package| package.position);
    } else {
        store.uncategorized_position = 0;
    }
    store.schema_version = 3;
    if source_schema_version < 3 {
        write_store(home, &store)?;
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
        position: previous.map(|p| p.position).unwrap_or_else(|| {
            store
                .packages
                .iter()
                .map(|package| package.position)
                .chain(std::iter::once(store.uncategorized_position))
                .max()
                .unwrap_or(0)
                + 1
        }),
    };
    let member_set: HashSet<String> = package.skill_uuids.iter().cloned().collect();
    for existing in &mut store.packages {
        if existing.id != package.id {
            existing.skill_uuids.retain(|id| !member_set.contains(id));
        }
    }
    if let Some(index) = store.packages.iter().position(|p| p.id == package.id) {
        store.packages[index] = package;
    } else {
        store.packages.push(package);
    }
    store.revision += 1;
    write_store(home, &store)?;
    Ok(store)
}

fn assign_members(
    home: &Path,
    request: AssignPackageMembersRequest,
) -> Result<PackageStore, String> {
    let mut store = read_store(home)?;
    if request.revision != store.revision {
        return Err("Packages changed. Refresh and retry / 包配置已变化，请刷新后重试".into());
    }
    if let Some(id) = request.package_id.as_ref() {
        if !store.packages.iter().any(|package| &package.id == id) {
            return Err("Package no longer exists".into());
        }
    }
    let allowed = managed_ids(home)?;
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
    for package in &mut store.packages {
        package.skill_uuids.retain(|id| !seen.contains(id));
    }
    if let Some(package_id) = request.package_id {
        let package = store
            .packages
            .iter_mut()
            .find(|package| package.id == package_id)
            .ok_or("Package no longer exists")?;
        package.skill_uuids.extend(members);
        package.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
    }
    store.revision += 1;
    write_store(home, &store)?;
    Ok(store)
}

fn reorder(home: &Path, request: ReorderPackagesRequest) -> Result<PackageStore, String> {
    let mut store = read_store(home)?;
    if request.revision != store.revision {
        return Err("Packages changed. Refresh and retry / 包配置已变化，请刷新后重试".into());
    }
    let requested: HashSet<&String> = request.group_ids.iter().collect();
    let mut existing: HashSet<String> = store
        .packages
        .iter()
        .map(|package| package.id.clone())
        .collect();
    existing.insert(UNCATEGORIZED_GROUP_ID.into());
    if requested.len() != request.group_ids.len()
        || request.group_ids.len() != existing.len()
        || requested.iter().any(|id| !existing.contains(id.as_str()))
    {
        return Err("Group order must include every group exactly once".into());
    }
    for (index, id) in request.group_ids.iter().enumerate() {
        if id == UNCATEGORIZED_GROUP_ID {
            store.uncategorized_position = index as u32;
        } else if let Some(package) = store.packages.iter_mut().find(|package| &package.id == id) {
            package.position = index as u32;
        }
    }
    store.packages.sort_by_key(|package| package.position);
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
    let mut positions = store
        .packages
        .iter()
        .map(|package| (package.id.clone(), package.position))
        .collect::<Vec<_>>();
    positions.push((UNCATEGORIZED_GROUP_ID.into(), store.uncategorized_position));
    positions.sort_by_key(|(_, position)| *position);
    for (position, (id, _)) in positions.iter().enumerate() {
        if id == UNCATEGORIZED_GROUP_ID {
            store.uncategorized_position = position as u32;
        } else if let Some(package) = store.packages.iter_mut().find(|package| &package.id == id) {
            package.position = position as u32;
        }
    }
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

#[tauri::command]
pub fn assign_skill_package_members(
    request: AssignPackageMembersRequest,
) -> Result<PackageStore, String> {
    let _guard = PACKAGE_LOCK.lock().map_err(|e| e.to_string())?;
    assign_members(
        &dirs::home_dir().ok_or("Cannot determine home directory")?,
        request,
    )
}

#[tauri::command]
pub fn reorder_skill_packages(request: ReorderPackagesRequest) -> Result<PackageStore, String> {
    let _guard = PACKAGE_LOCK.lock().map_err(|e| e.to_string())?;
    reorder(
        &dirs::home_dir().ok_or("Cannot determine home directory")?,
        request,
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

    #[test]
    fn assigning_members_moves_skills_between_packages_and_reorders_them() {
        let home = std::env::temp_dir().join(format!("package-assign-test-{}", Uuid::new_v4()));
        let first_skill = home.join("Skill Manager/Skills/first");
        let second_skill = home.join("Skill Manager/Skills/second");
        fs::create_dir_all(&first_skill).unwrap();
        fs::create_dir_all(&second_skill).unwrap();
        fs::write(first_skill.join("SKILL.md"), "# First").unwrap();
        fs::write(second_skill.join("SKILL.md"), "# Second").unwrap();
        let first_uuid = ensure_skill_uuid(&first_skill, None).unwrap();
        let second_uuid = ensure_skill_uuid(&second_skill, None).unwrap();

        let first = save(
            &home,
            SavePackageRequest {
                id: None,
                name: "First package".into(),
                description: String::new(),
                skill_uuids: vec![first_uuid.clone()],
                revision: 0,
            },
        )
        .unwrap();
        let first_id = first.packages[0].id.clone();
        let second = save(
            &home,
            SavePackageRequest {
                id: None,
                name: "Second package".into(),
                description: String::new(),
                skill_uuids: vec![second_uuid.clone()],
                revision: first.revision,
            },
        )
        .unwrap();
        let second_id = second.packages[1].id.clone();

        let assigned = assign_members(
            &home,
            AssignPackageMembersRequest {
                package_id: Some(second_id.clone()),
                skill_uuids: vec![first_uuid.clone()],
                revision: second.revision,
            },
        )
        .unwrap();
        assert!(assigned.packages[0].skill_uuids.is_empty());
        assert_eq!(
            assigned.packages[1].skill_uuids,
            vec![second_uuid, first_uuid.clone()]
        );

        let reordered = reorder(
            &home,
            ReorderPackagesRequest {
                group_ids: vec![
                    second_id.clone(),
                    UNCATEGORIZED_GROUP_ID.into(),
                    first_id.clone(),
                ],
                revision: assigned.revision,
            },
        )
        .unwrap();
        assert_eq!(reordered.packages[0].id, second_id);
        assert_eq!(reordered.packages[0].position, 0);
        assert_eq!(reordered.packages[1].id, first_id);
        assert_eq!(reordered.packages[1].position, 2);
        assert_eq!(reordered.uncategorized_position, 1);

        let uncategorized = assign_members(
            &home,
            AssignPackageMembersRequest {
                package_id: None,
                skill_uuids: vec![first_uuid],
                revision: reordered.revision,
            },
        )
        .unwrap();
        assert!(uncategorized
            .packages
            .iter()
            .all(|package| package.skill_uuids.len() <= 1));
        assert!(!uncategorized.packages[0]
            .skill_uuids
            .contains(&read_skill_uuid(&first_skill).expect("first skill should retain its UUID")));

        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn schema_one_migration_keeps_the_first_package_membership() {
        let home = std::env::temp_dir().join(format!("package-migration-test-{}", Uuid::new_v4()));
        let path = config_path(&home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let skill_id = Uuid::new_v4().to_string();
        let first_id = Uuid::new_v4().to_string();
        let second_id = Uuid::new_v4().to_string();
        let legacy = serde_json::json!({
            "schemaVersion": 1,
            "revision": 4,
            "packages": [
                { "id": first_id, "name": "One", "description": "", "skillUuids": [skill_id], "createdAt": 1, "updatedAt": 1 },
                { "id": second_id, "name": "Two", "description": "", "skillUuids": [skill_id], "createdAt": 2, "updatedAt": 2 }
            ]
        });
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

        let migrated = read_store(&home).unwrap();
        assert_eq!(migrated.schema_version, 3);
        assert_eq!(migrated.packages[0].skill_uuids.len(), 1);
        assert!(migrated.packages[1].skill_uuids.is_empty());
        assert_eq!(migrated.uncategorized_position, 0);
        assert_eq!(migrated.packages[0].position, 1);
        assert_eq!(migrated.packages[1].position, 2);
        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(persisted["schemaVersion"], 3);
        assert_eq!(persisted["uncategorizedPosition"], 0);
        assert_eq!(persisted["packages"][0]["position"], 1);

        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn reorder_requires_every_group_and_can_move_uncategorized() {
        let home = std::env::temp_dir().join(format!("package-order-test-{}", Uuid::new_v4()));
        let first = save(
            &home,
            SavePackageRequest {
                id: None,
                name: "First".into(),
                description: String::new(),
                skill_uuids: vec![],
                revision: 0,
            },
        )
        .unwrap();
        let first_id = first.packages[0].id.clone();
        let second = save(
            &home,
            SavePackageRequest {
                id: None,
                name: "Second".into(),
                description: String::new(),
                skill_uuids: vec![],
                revision: first.revision,
            },
        )
        .unwrap();
        let second_id = second.packages[1].id.clone();

        assert!(reorder(
            &home,
            ReorderPackagesRequest {
                group_ids: vec![first_id.clone(), second_id.clone()],
                revision: second.revision,
            }
        )
        .is_err());
        assert!(reorder(
            &home,
            ReorderPackagesRequest {
                group_ids: vec![
                    UNCATEGORIZED_GROUP_ID.into(),
                    first_id.clone(),
                    first_id.clone(),
                ],
                revision: second.revision,
            }
        )
        .is_err());

        let reordered = reorder(
            &home,
            ReorderPackagesRequest {
                group_ids: vec![
                    first_id.clone(),
                    second_id.clone(),
                    UNCATEGORIZED_GROUP_ID.into(),
                ],
                revision: second.revision,
            },
        )
        .unwrap();
        assert_eq!(reordered.uncategorized_position, 2);
        assert_eq!(reordered.packages[0].position, 0);
        assert_eq!(reordered.packages[1].position, 1);

        let after_delete = delete(&home, &first_id, reordered.revision).unwrap();
        assert_eq!(after_delete.packages[0].id, second_id);
        assert_eq!(after_delete.packages[0].position, 0);
        assert_eq!(after_delete.uncategorized_position, 1);

        fs::remove_dir_all(home).unwrap();
    }
}
