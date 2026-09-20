use crate::types::{DownloadRequest, DownloadResult};
use crate::utils::download::download_skill_to_dir;
use crate::utils::path::sanitize_skill_dir_name;
use crate::utils::skill_identity::{
    ensure_skill_uuid, normalize_skill_uuid, read_skill_name, read_skill_uuid, write_skill_uuid,
};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

const MARKET_SKILL_METADATA: &str = ".skills-manager.json";

#[derive(Serialize)]
struct InstalledSkillMetadata<'a> {
    source_url: &'a str,
}

fn write_installed_skill_metadata(
    installed_dir: &std::path::Path,
    source_url: &str,
) -> Result<(), String> {
    let metadata = InstalledSkillMetadata { source_url };
    let raw = serde_json::to_string_pretty(&metadata).map_err(|err| err.to_string())?;
    fs::write(installed_dir.join(MARKET_SKILL_METADATA), raw).map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn download_marketplace_skill(
    request: DownloadRequest,
) -> Result<DownloadResult, String> {
    if request.install_base_dir.trim().is_empty() {
        return Err("安装目录不能为空".to_string());
    }

    let source_url = request.source_url.clone();
    let skill_name = request.skill_name.clone();
    let install_base_dir = PathBuf::from(&request.install_base_dir);

    let result = tauri::async_runtime::spawn_blocking(move || {
        let installed_dir =
            download_skill_to_dir(&source_url, &skill_name, &install_base_dir, false, None)?;
        ensure_skill_uuid(&installed_dir, None)?;
        write_installed_skill_metadata(&installed_dir, &source_url)?;
        Ok::<PathBuf, String>(installed_dir)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    Ok(DownloadResult {
        installed_path: result.display().to_string(),
    })
}

#[tauri::command]
pub async fn update_marketplace_skill(request: DownloadRequest) -> Result<DownloadResult, String> {
    if request.install_base_dir.trim().is_empty() {
        return Err("安装目录不能为空".to_string());
    }
    if request.source_url.trim().is_empty() {
        return Err("缺少有效的源码地址 (Source URL)，无法更新".to_string());
    }

    let source_url = request.source_url.clone();
    let skill_name = request.skill_name.clone();
    let install_base_dir = PathBuf::from(&request.install_base_dir);
    let requested_uuid = request.skill_uuid.as_deref().and_then(normalize_skill_uuid);
    let has_explicit_target = request.target_path.is_some();
    let target_dir = request
        .target_path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            install_base_dir.join(sanitize_skill_dir_name(&skill_name, &source_url))
        });

    let result = tauri::async_runtime::spawn_blocking(move || {
        if has_explicit_target && !target_dir.join("SKILL.md").is_file() {
            return Err("指定的 Skill 更新目标不存在".to_string());
        }
        if let Some(existing_name) = read_skill_name(&target_dir) {
            if existing_name.trim() != skill_name.trim() {
                return Err("Skill 名称不匹配，已拒绝更新错误的 Skill".to_string());
            }
        }
        let existing_uuid = read_skill_uuid(&target_dir);
        if let (Some(expected), Some(actual)) =
            (requested_uuid.as_deref(), existing_uuid.as_deref())
        {
            if expected != actual {
                return Err("Skill UUID 不匹配，已拒绝更新错误的 Skill".to_string());
            }
        }
        let identity = existing_uuid.or(requested_uuid);
        let _history_guard = crate::commands::history::HISTORY_LOCK
            .lock()
            .map_err(|_| "History unavailable")?;
        if target_dir.exists() {
            crate::commands::history::snapshot_before_update(&target_dir)?;
        }
        let installed_dir = download_skill_to_dir(
            &source_url,
            &skill_name,
            &install_base_dir,
            true,
            Some(&target_dir),
        )?;
        if let Some(uuid) = identity.as_deref() {
            write_skill_uuid(&installed_dir, uuid)?;
        } else {
            ensure_skill_uuid(&installed_dir, None)?;
        }
        write_installed_skill_metadata(&installed_dir, &source_url)?;
        Ok::<PathBuf, String>(installed_dir)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    Ok(DownloadResult {
        installed_path: result.display().to_string(),
    })
}
