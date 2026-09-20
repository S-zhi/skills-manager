use crate::types::{
    AdoptIdeSkillRequest, BatchImportRequest, BatchImportResult, DeleteLocalSkillRequest,
    DiscoveredSkill, ExportSkillsRequest, IdeSkill, ImportRequest, InstallResult, LinkRequest,
    LocalScanRequest, LocalSkill, LocalSkillPreview, ManagerStorageInfo, Overview, ProjectIdeDir,
    ProjectScanRequest, ProjectScanResult, SkillDiscoveryRequest, SkillImportItemResult,
    UninstallRequest,
};
use crate::utils::download::copy_dir_recursive;
use crate::utils::path::{normalize_path, resolve_canonical, sanitize_skill_dir_name};
use crate::utils::security::{is_absolute_ide_path, is_valid_ide_path};
use crate::utils::skill_identity::{ensure_skill_uuid, normalize_skill_uuid, read_skill_uuid};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const MANAGED_COPY_MARKER: &str = ".skills-manager-source";
const MARKET_SKILL_METADATA: &str = ".skills-manager.json";
const MANAGER_ROOT_NAME: &str = "Skill Manager";
const MANAGER_SKILLS_NAME: &str = "Skills";
const MANAGER_PLUGINS_NAME: &str = "Plugins";
const MANAGER_METADATA_NAME: &str = ".metadata";
const MANAGER_STAGING_NAME: &str = ".staging";

const ISSUE_MISSING_FRONTMATTER: &str = "missing_frontmatter";
const ISSUE_MISSING_NAME: &str = "missing_name";
const ISSUE_INVALID_NAME: &str = "invalid_name";
const ISSUE_MISSING_DESCRIPTION: &str = "missing_description";
const ISSUE_DESCRIPTION_TOO_LONG: &str = "description_too_long";
const ISSUE_DIRECTORY_NAME_MISMATCH: &str = "directory_name_mismatch";

struct ManagerLayout {
    root: PathBuf,
    skills: PathBuf,
    plugins: PathBuf,
    metadata: PathBuf,
    staging: PathBuf,
    legacy_skills: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedSkillRecord {
    #[serde(default)]
    uuid: String,
    name: String,
    source_path: String,
    target_path: String,
    imported_at_unix: u64,
}

fn manager_layout(home: &Path) -> ManagerLayout {
    let root = home.join(MANAGER_ROOT_NAME);
    ManagerLayout {
        skills: root.join(MANAGER_SKILLS_NAME),
        plugins: root.join(MANAGER_PLUGINS_NAME),
        metadata: root.join(MANAGER_METADATA_NAME),
        staging: root.join(MANAGER_STAGING_NAME),
        legacy_skills: home.join(".skills-manager/skills"),
        root,
    }
}

fn ensure_manager_layout(home: &Path) -> Result<ManagerLayout, String> {
    let layout = manager_layout(home);
    for directory in [
        &layout.root,
        &layout.skills,
        &layout.plugins,
        &layout.metadata,
        &layout.staging,
    ] {
        fs::create_dir_all(directory).map_err(|err| {
            format!(
                "Failed to create Skills Manager directory {}: {}",
                directory.display(),
                err
            )
        })?;
    }
    normalize_import_record_names(&layout)?;
    Ok(layout)
}

fn normalize_import_record_names(layout: &ManagerLayout) -> Result<(), String> {
    let index_path = layout.metadata.join("skills.json");
    let raw = match fs::read_to_string(&index_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.to_string()),
    };
    let mut records: Vec<ManagedSkillRecord> = match serde_json::from_str(&raw) {
        Ok(records) => records,
        Err(_) => return Ok(()),
    };
    let mut changed = false;
    for record in &mut records {
        let normalized = unquote_yaml_scalar(&record.name);
        if normalized != record.name {
            record.name = normalized;
            changed = true;
        }
        let target = PathBuf::from(&record.target_path);
        if target.join("SKILL.md").is_file() {
            let preferred = (!record.uuid.trim().is_empty()).then_some(record.uuid.as_str());
            if let Ok(uuid) = ensure_skill_uuid(&target, preferred) {
                if record.uuid != uuid {
                    record.uuid = uuid;
                    changed = true;
                }
            }
        }
    }
    if changed {
        let content = serde_json::to_string_pretty(&records).map_err(|err| err.to_string())?;
        fs::write(index_path, content).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn manager_skill_roots(layout: &ManagerLayout) -> Vec<PathBuf> {
    [&layout.skills, &layout.legacy_skills]
        .iter()
        .map(|root| resolve_canonical(root).unwrap_or_else(|| normalize_path(root)))
        .collect()
}

fn path_is_within_any_root(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| path.starts_with(root))
}

fn next_available_target(skills_dir: &Path, base_name: &str) -> PathBuf {
    let first = skills_dir.join(base_name);
    if !first.exists() {
        return first;
    }
    for suffix in 2usize.. {
        let candidate = skills_dir.join(format!("{}-{}", base_name, suffix));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn compare_files(left: &Path, right: &Path) -> Result<bool, String> {
    let left_meta = fs::metadata(left).map_err(|err| err.to_string())?;
    let right_meta = fs::metadata(right).map_err(|err| err.to_string())?;
    if left_meta.len() != right_meta.len() {
        return Ok(false);
    }

    let mut left_file = File::open(left).map_err(|err| err.to_string())?;
    let mut right_file = File::open(right).map_err(|err| err.to_string())?;
    let mut left_buf = [0u8; 8192];
    let mut right_buf = [0u8; 8192];
    loop {
        let left_read = left_file
            .read(&mut left_buf)
            .map_err(|err| err.to_string())?;
        let right_read = right_file
            .read(&mut right_buf)
            .map_err(|err| err.to_string())?;
        if left_read != right_read || left_buf[..left_read] != right_buf[..right_read] {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
    }
}

fn directory_manifest(root: &Path) -> Result<Vec<(PathBuf, bool)>, String> {
    let mut entries = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|err| err.to_string())?;
        if entry.path() == root {
            continue;
        }
        if entry.file_type().is_symlink() {
            return Err(format!(
                "Symlinked content is not supported: {}",
                entry.path().display()
            ));
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|err| err.to_string())?
            .to_path_buf();
        entries.push((relative, entry.file_type().is_dir()));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(entries)
}

fn directories_equal(left: &Path, right: &Path) -> Result<bool, String> {
    let left_manifest = directory_manifest(left)?;
    let right_manifest = directory_manifest(right)?;
    if left_manifest != right_manifest {
        return Ok(false);
    }
    for (relative, is_dir) in left_manifest {
        if !is_dir {
            if relative == Path::new("SKILL.md") {
                let left_content =
                    fs::read_to_string(left.join(&relative)).map_err(|err| err.to_string())?;
                let right_content =
                    fs::read_to_string(right.join(&relative)).map_err(|err| err.to_string())?;
                if skill_content_without_uuid(&left_content)
                    != skill_content_without_uuid(&right_content)
                {
                    return Ok(false);
                }
            } else if !compare_files(&left.join(&relative), &right.join(&relative))? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn skill_content_without_uuid(content: &str) -> String {
    let normalized = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut in_frontmatter = false;
    let mut frontmatter_closed = false;
    normalized
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            if index == 0 && line.trim() == "---" {
                in_frontmatter = true;
                return Some(line);
            }
            if in_frontmatter && line.trim() == "---" {
                in_frontmatter = false;
                frontmatter_closed = true;
                return Some(line);
            }
            let is_uuid = in_frontmatter
                && !frontmatter_closed
                && !line.chars().next().is_some_and(char::is_whitespace)
                && line
                    .trim()
                    .split_once(':')
                    .is_some_and(|(key, _)| key.trim() == "uuid");
            (!is_uuid).then_some(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_import_records(layout: &ManagerLayout) -> Vec<ManagedSkillRecord> {
    fs::read_to_string(layout.metadata.join("skills.json"))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn append_import_record(
    layout: &ManagerLayout,
    uuid: &str,
    name: &str,
    source_path: &Path,
    target_path: &Path,
) -> Result<(), String> {
    let index_path = layout.metadata.join("skills.json");
    let mut records = read_import_records(layout);
    let imported_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_secs();
    records.retain(|record| {
        record.uuid != uuid && record.target_path != target_path.display().to_string()
    });
    records.push(ManagedSkillRecord {
        uuid: uuid.to_string(),
        name: name.to_string(),
        source_path: source_path.display().to_string(),
        target_path: target_path.display().to_string(),
        imported_at_unix,
    });
    let content = serde_json::to_string_pretty(&records).map_err(|err| err.to_string())?;
    fs::write(index_path, content).map_err(|err| err.to_string())
}

fn import_skill_to_layout(source_path: &Path, layout: &ManagerLayout) -> SkillImportItemResult {
    let fallback_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("skill")
        .to_string();
    let fail = |name: String, message: String| SkillImportItemResult {
        source_path: source_path.display().to_string(),
        name,
        target_path: None,
        status: "failed".to_string(),
        message,
    };

    let Some(source_canonical) = resolve_canonical(source_path) else {
        return fail(fallback_name, "Source path does not exist".to_string());
    };
    if !source_canonical.is_dir() {
        return fail(fallback_name, "Source path must be a directory".to_string());
    }
    if !source_canonical.join("SKILL.md").is_file() {
        return fail(
            fallback_name,
            "The selected directory does not contain SKILL.md".to_string(),
        );
    }
    let skills_root =
        resolve_canonical(&layout.skills).unwrap_or_else(|| normalize_path(&layout.skills));
    if source_canonical.starts_with(&skills_root) {
        let (name, _) = read_skill_metadata(&source_canonical);
        return SkillImportItemResult {
            source_path: source_canonical.display().to_string(),
            name,
            target_path: Some(source_canonical.display().to_string()),
            status: "skipped".to_string(),
            message: "Skill is already managed".to_string(),
        };
    }
    if let Err(err) = directory_manifest(&source_canonical) {
        return fail(fallback_name, err);
    }

    let (name, _) = read_skill_metadata(&source_canonical);
    let source_uuid = read_skill_uuid(&source_canonical);
    let source_path_string = source_canonical.display().to_string();

    for record in read_import_records(layout) {
        let target = PathBuf::from(&record.target_path);
        if record.source_path == source_path_string && target.join("SKILL.md").is_file() {
            let preferred = source_uuid
                .as_deref()
                .or_else(|| (!record.uuid.trim().is_empty()).then_some(record.uuid.as_str()));
            match ensure_skill_uuid(&target, preferred) {
                Ok(target_uuid) => {
                    let same_identity = source_uuid
                        .as_deref()
                        .is_some_and(|source_uuid| source_uuid == target_uuid);
                    let same_legacy_content = if source_uuid.is_none() {
                        match directories_equal(&source_canonical, &target) {
                            Ok(equal) => equal,
                            Err(err) => return fail(name, err),
                        }
                    } else {
                        false
                    };
                    if !same_identity && !same_legacy_content {
                        continue;
                    }
                    return SkillImportItemResult {
                        source_path: source_path_string,
                        name,
                        target_path: Some(target.display().to_string()),
                        status: "skipped".to_string(),
                        message: "Skill from this source is already managed".to_string(),
                    };
                }
                Err(err) => return fail(name, err),
            }
        }
    }

    if let Some(uuid) = source_uuid.as_deref() {
        if let Some(existing) = find_managed_skill_by_uuid(layout, uuid) {
            return SkillImportItemResult {
                source_path: source_path_string,
                name,
                target_path: Some(existing.display().to_string()),
                status: "skipped".to_string(),
                message: "A skill with the same UUID is already managed".to_string(),
            };
        }
    }

    let safe_name = sanitize_skill_dir_name(&name, &source_canonical.display().to_string());
    let first_target = layout.skills.join(&safe_name);
    if first_target.exists() {
        match directories_equal(&source_canonical, &first_target) {
            Ok(true) => {
                return SkillImportItemResult {
                    source_path: source_canonical.display().to_string(),
                    name,
                    target_path: Some(first_target.display().to_string()),
                    status: "skipped".to_string(),
                    message: "An identical skill is already managed".to_string(),
                }
            }
            Ok(false) => {}
            Err(err) => return fail(name, err),
        }
    }
    let target = next_available_target(&layout.skills, &safe_name);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let staging = layout.staging.join(format!("{}-{}", safe_name, timestamp));

    if let Err(err) = copy_dir_recursive(&source_canonical, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return fail(name, err);
    }
    if !staging.join("SKILL.md").is_file() {
        let _ = fs::remove_dir_all(&staging);
        return fail(name, "Copied skill is missing SKILL.md".to_string());
    }
    let uuid = match ensure_skill_uuid(&staging, source_uuid.as_deref()) {
        Ok(uuid) => uuid,
        Err(err) => {
            let _ = fs::remove_dir_all(&staging);
            return fail(name, format!("Failed to assign skill UUID: {err}"));
        }
    };
    if let Err(err) = fs::rename(&staging, &target) {
        let _ = fs::remove_dir_all(&staging);
        return fail(name, format!("Failed to finalize import: {}", err));
    }

    let metadata_message =
        match append_import_record(layout, &uuid, &name, &source_canonical, &target) {
            Ok(()) => "Imported into Skill Manager".to_string(),
            Err(err) => format!("Imported, but metadata could not be saved: {}", err),
        };
    SkillImportItemResult {
        source_path: source_canonical.display().to_string(),
        name,
        target_path: Some(target.display().to_string()),
        status: "imported".to_string(),
        message: metadata_message,
    }
}

#[derive(Default)]
struct SkillDocumentMetadata {
    uuid: Option<String>,
    name: Option<String>,
    description: Option<String>,
    has_frontmatter: bool,
}

fn unquote_yaml_scalar(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let first = trimmed.as_bytes()[0];
        let last = trimmed.as_bytes()[trimmed.len() - 1];
        if (first == b'\'' && last == b'\'') || (first == b'"' && last == b'"') {
            return trimmed[1..trimmed.len() - 1].trim().to_string();
        }
    }
    trimmed.to_string()
}

fn parse_skill_document(content: &str) -> SkillDocumentMetadata {
    let normalized = content.strip_prefix('\u{feff}').unwrap_or(content);
    let lines: Vec<&str> = normalized.lines().collect();
    if lines.first().map(|line| line.trim()) != Some("---") {
        return SkillDocumentMetadata::default();
    }

    let Some(end) = lines
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(idx, line)| (line.trim_end() == "---").then_some(idx))
    else {
        return SkillDocumentMetadata::default();
    };

    let mut metadata = SkillDocumentMetadata {
        has_frontmatter: true,
        ..SkillDocumentMetadata::default()
    };
    let frontmatter = &lines[1..end];
    let mut index = 0usize;
    while index < frontmatter.len() {
        let line = frontmatter[index];
        let trimmed = line.trim();
        if line.chars().next().is_some_and(char::is_whitespace)
            || trimmed.starts_with('#')
            || trimmed.is_empty()
        {
            index += 1;
            continue;
        }

        let Some((key, raw_value)) = trimmed.split_once(':') else {
            index += 1;
            continue;
        };
        let key = key.trim();
        let raw_value = raw_value.trim();
        if key == "uuid" {
            metadata.uuid = normalize_skill_uuid(raw_value);
        } else if key == "name" {
            metadata.name = Some(unquote_yaml_scalar(raw_value));
        } else if key == "description" {
            if raw_value == ">" || raw_value == "|" || raw_value == ">-" || raw_value == "|-" {
                let mut parts = Vec::new();
                index += 1;
                while index < frontmatter.len() {
                    let block_line = frontmatter[index];
                    if !block_line.chars().next().is_some_and(char::is_whitespace)
                        && !block_line.trim().is_empty()
                    {
                        index -= 1;
                        break;
                    }
                    let value = block_line.trim();
                    if !value.is_empty() {
                        parts.push(value);
                    }
                    index += 1;
                }
                metadata.description = Some(parts.join(" "));
            } else {
                metadata.description = Some(unquote_yaml_scalar(raw_value));
            }
        }
        index += 1;
    }

    metadata
}

fn skill_body_has_content(content: &str) -> bool {
    let normalized = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = normalized.lines();
    if lines.next().map(str::trim) != Some("---") {
        return false;
    }
    for line in &mut lines {
        if line.trim_end() == "---" {
            return lines.any(|body_line| !body_line.trim().is_empty());
        }
    }
    false
}

fn is_standard_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .bytes()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'-')
}

fn detect_skill_provider(skill_dir: &Path) -> String {
    let normalized = skill_dir
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let providers = [
        ("/.claude/", "Claude Code"),
        ("/.codex/", "Codex"),
        ("/.cursor/", "Cursor"),
        ("/.gemini/", "Gemini / Antigravity"),
        ("/.github/skills/", "VS Code / GitHub Copilot"),
        ("/.windsurf/", "Windsurf"),
        ("/.qoder/", "Qoder"),
        ("/.trae/", "Trae"),
        ("/.kiro/", "Kiro"),
        ("/.codebuddy/", "CodeBuddy"),
        ("/.openclaw/", "OpenClaw"),
        ("/.opencode/", "OpenCode"),
        ("/.agents/skills/", "Agent Skills"),
    ];

    providers
        .iter()
        .find_map(|(pattern, label)| normalized.contains(pattern).then_some((*label).to_string()))
        .unwrap_or_else(|| "Generic SKILL.md".to_string())
}

fn inspect_discovered_skill(skill_md_path: &Path) -> DiscoveredSkill {
    let skill_dir = skill_md_path.parent().unwrap_or(skill_md_path);
    let directory_name = skill_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("skill")
        .to_string();
    let raw = fs::read(skill_md_path).unwrap_or_default();
    let content = String::from_utf8_lossy(&raw);
    let metadata = parse_skill_document(&content);
    let name = metadata
        .name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| directory_name.clone());
    let description = metadata.description.clone().unwrap_or_default();
    let mut issues = Vec::new();

    if !metadata.has_frontmatter {
        issues.push(ISSUE_MISSING_FRONTMATTER.to_string());
    }
    match metadata.name.as_deref().map(str::trim) {
        None | Some("") => issues.push(ISSUE_MISSING_NAME.to_string()),
        Some(value) => {
            if !is_standard_skill_name(value) {
                issues.push(ISSUE_INVALID_NAME.to_string());
            }
            if directory_name != value {
                issues.push(ISSUE_DIRECTORY_NAME_MISMATCH.to_string());
            }
        }
    }
    match metadata.description.as_deref().map(str::trim) {
        None | Some("") => issues.push(ISSUE_MISSING_DESCRIPTION.to_string()),
        Some(value) if value.chars().count() > 1024 => {
            issues.push(ISSUE_DESCRIPTION_TOO_LONG.to_string())
        }
        Some(_) => {}
    }

    DiscoveredSkill {
        id: metadata
            .uuid
            .clone()
            .unwrap_or_else(|| skill_dir.display().to_string()),
        uuid: metadata.uuid,
        name,
        description,
        path: skill_dir.display().to_string(),
        skill_md_path: skill_md_path.display().to_string(),
        provider: detect_skill_provider(skill_dir),
        is_standard: issues.is_empty(),
        is_duplicate: false,
        issues,
    }
}

fn is_managed_duplicate(source_path: &Path, layout: &ManagerLayout) -> Result<bool, String> {
    let Some(source_canonical) = resolve_canonical(source_path) else {
        return Ok(false);
    };
    let skills_root =
        resolve_canonical(&layout.skills).unwrap_or_else(|| normalize_path(&layout.skills));
    if source_canonical.starts_with(&skills_root) {
        return Ok(true);
    }

    let source_uuid = read_skill_uuid(&source_canonical);
    let source_path_string = source_canonical.display().to_string();
    for record in read_import_records(layout) {
        let target = PathBuf::from(&record.target_path);
        if record.source_path != source_path_string || !target.join("SKILL.md").is_file() {
            continue;
        }
        if let Some(uuid) = source_uuid.as_deref() {
            if record.uuid == uuid || read_skill_uuid(&target).as_deref() == Some(uuid) {
                return Ok(true);
            }
        } else if directories_equal(&source_canonical, &target)? {
            return Ok(true);
        }
    }

    if let Some(uuid) = source_uuid.as_deref() {
        if find_managed_skill_by_uuid(layout, uuid).is_some() {
            return Ok(true);
        }
    }

    let (name, _) = read_skill_metadata(&source_canonical);
    let safe_name = sanitize_skill_dir_name(&name, &source_path_string);
    let target = layout.skills.join(safe_name);
    Ok(target.exists() && directories_equal(&source_canonical, &target)?)
}

fn read_skill_metadata(skill_dir: &Path) -> (String, String) {
    let directory_name = skill_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("skill")
        .to_string();

    let skill_file = skill_dir.join("SKILL.md");
    if !skill_file.exists() {
        return (directory_name, String::new());
    }

    let content = fs::read_to_string(&skill_file).unwrap_or_default();
    let metadata = parse_skill_document(&content);
    let name = metadata
        .name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(directory_name);
    (name, metadata.description.unwrap_or_default())
}

fn read_market_skill_source_url(skill_dir: &Path) -> Option<String> {
    let metadata_path = skill_dir.join(MARKET_SKILL_METADATA);
    let raw = fs::read_to_string(metadata_path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    parsed
        .get("source_url")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn managed_copy_marker_path(skill_dir: &Path) -> PathBuf {
    skill_dir.join(MANAGED_COPY_MARKER)
}

fn read_managed_copy_target(skill_dir: &Path) -> Option<PathBuf> {
    let marker_path = managed_copy_marker_path(skill_dir);
    let raw = fs::read_to_string(marker_path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    resolve_canonical(Path::new(trimmed)).or_else(|| Some(PathBuf::from(trimmed)))
}

#[cfg(target_family = "windows")]
fn write_managed_copy_marker(skill_dir: &Path, manager_skill_path: &Path) -> Result<(), String> {
    fs::write(
        managed_copy_marker_path(skill_dir),
        manager_skill_path.display().to_string(),
    )
    .map_err(|err| err.to_string())
}

fn find_managed_skill_by_uuid(layout: &ManagerLayout, expected_uuid: &str) -> Option<PathBuf> {
    [&layout.skills, &layout.legacy_skills]
        .into_iter()
        .filter_map(|base| fs::read_dir(base).ok())
        .flat_map(|entries| entries.filter_map(Result::ok))
        .find_map(|entry| {
            let path = entry.path();
            (path.is_dir() && read_skill_uuid(&path).as_deref() == Some(expected_uuid))
                .then_some(path)
        })
}

fn collect_skills_from_dir(
    base: &Path,
    source: &str,
    ide: Option<&str>,
) -> Result<Vec<LocalSkill>, String> {
    let mut skills = Vec::new();
    if !base.exists() {
        return Ok(skills);
    }

    let entries = match fs::read_dir(base) {
        Ok(entries) => entries,
        Err(err) => return Err(err.to_string()),
    };

    for entry in entries {
        let entry = match entry {
            Ok(item) => item,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_dir() || !path.join("SKILL.md").exists() {
            continue;
        }
        let (name, description) = read_skill_metadata(&path);
        let uuid = ensure_skill_uuid(&path, None)
            .map_err(|err| format!("Failed to assign UUID to {}: {}", path.display(), err))?;
        skills.push(LocalSkill {
            id: uuid.clone(),
            uuid,
            name,
            description,
            path: path.display().to_string(),
            source: source.to_string(),
            source_url: read_market_skill_source_url(&path),
            ide: ide.map(|value| value.to_string()),
            used_by: Vec::new(),
        });
    }

    Ok(skills)
}

fn collect_ide_skills(
    base: &Path,
    ide_label: &str,
    manager_map: &[(PathBuf, usize)],
    manager_skills: &mut [LocalSkill],
) -> Vec<IdeSkill> {
    let mut skills = Vec::new();
    if !base.exists() {
        return skills;
    }

    let entries = match fs::read_dir(base) {
        Ok(entries) => entries,
        Err(_) => return skills,
    };

    for entry in entries {
        let entry = match entry {
            Ok(item) => item,
            Err(_) => continue,
        };
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let link_target = fs::read_link(&path).ok();
        let managed_copy_target = read_managed_copy_target(&path);
        if !metadata.is_dir() && link_target.is_none() {
            continue;
        }

        let skill_dir = path.as_path();
        let has_skill_file = skill_dir.join("SKILL.md").exists();
        if !has_skill_file && link_target.is_none() && managed_copy_target.is_none() {
            continue;
        }

        let name = if has_skill_file {
            read_skill_metadata(skill_dir).0
        } else {
            skill_dir
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("skill")
                .to_string()
        };

        let path = skill_dir.to_path_buf();
        let mut managed = false;
        let source = if let Some(link_target) = link_target {
            let absolute_target = if link_target.is_relative() {
                if let Some(parent) = path.parent() {
                    parent.join(&link_target)
                } else {
                    link_target.clone()
                }
            } else {
                link_target
            };

            if let Some(target) = resolve_canonical(&absolute_target) {
                for (manager_path, idx) in manager_map {
                    if *manager_path == target {
                        managed = true;
                        if let Some(skill) = manager_skills.get_mut(*idx) {
                            if !skill.used_by.contains(&ide_label.to_string()) {
                                skill.used_by.push(ide_label.to_string());
                            }
                        }
                        break;
                    }
                }
            }
            "link"
        } else if let Some(copy_target) = managed_copy_target {
            for (manager_path, idx) in manager_map {
                if *manager_path == copy_target {
                    managed = true;
                    if let Some(skill) = manager_skills.get_mut(*idx) {
                        if !skill.used_by.contains(&ide_label.to_string()) {
                            skill.used_by.push(ide_label.to_string());
                        }
                    }
                    break;
                }
            }
            "link"
        } else {
            "local"
        };

        skills.push(IdeSkill {
            id: path.display().to_string(),
            name,
            path: path.display().to_string(),
            ide: ide_label.to_string(),
            source: source.to_string(),
            managed,
        });
    }

    skills
}

fn remove_path(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|err| err.to_string())?;
    if metadata.file_type().is_symlink() {
        // `path.is_dir()` follows symlinks and may report true for a symlink-to-dir.
        // Removing such a symlink with `remove_dir` triggers ENOTDIR on macOS.
        fs::remove_file(path)
            .or_else(|_| fs::remove_dir(path))
            .map_err(|err| err.to_string())
    } else if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|err| err.to_string())
    } else {
        fs::remove_file(path).map_err(|err| err.to_string())
    }
}

fn is_symlink_to(path: &Path, target: &Path) -> bool {
    match (resolve_canonical(path), resolve_canonical(target)) {
        (Some(link_target), Some(expected_target)) => link_target == expected_target,
        _ => false,
    }
}

fn create_symlink_dir(target: &Path, link: &Path) -> Result<(), String> {
    #[cfg(target_family = "unix")]
    {
        std::os::unix::fs::symlink(target, link).map_err(|err| err.to_string())
    }
    #[cfg(target_family = "windows")]
    {
        std::os::windows::fs::symlink_dir(target, link).map_err(|err| err.to_string())
    }
}

fn validate_manager_skill_path(
    target: &Path,
    manager_roots: &[PathBuf],
) -> Result<PathBuf, String> {
    let canonical =
        resolve_canonical(target).ok_or_else(|| "Target skill does not exist".to_string())?;
    if !path_is_within_any_root(&canonical, manager_roots) {
        return Err("Only Skills Manager local skills can be exported".to_string());
    }
    if manager_roots.iter().any(|root| canonical == *root) {
        return Err("Refusing to export the skills root directory".to_string());
    }
    if !canonical.join("SKILL.md").exists() {
        return Err("Refusing to export a directory without SKILL.md".to_string());
    }
    Ok(canonical)
}

fn ensure_export_path_is_safe(export_path: &Path, skill_paths: &[PathBuf]) -> Result<(), String> {
    let file_name = export_path
        .file_name()
        .ok_or_else(|| "Export path must include a file name".to_string())?;
    let export_parent = export_path
        .parent()
        .ok_or_else(|| "Export path must include a parent directory".to_string())?;
    let normalized_export_parent =
        resolve_canonical(export_parent).unwrap_or_else(|| normalize_path(export_parent));
    let normalized_export = normalized_export_parent.join(file_name);
    for skill_path in skill_paths {
        if normalized_export.starts_with(skill_path) {
            return Err("Export path cannot be inside a selected skill directory".to_string());
        }
    }
    Ok(())
}

fn zip_skill_directory(
    zip: &mut ZipWriter<File>,
    skill_path: &Path,
    root_name: &str,
) -> Result<(), String> {
    let dir_options = || {
        SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o755)
    };
    let file_options = || {
        SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644)
    };

    let root_dir = format!("{}/", root_name);
    zip.add_directory(&root_dir, dir_options())
        .map_err(|err| err.to_string())?;

    for entry in WalkDir::new(skill_path) {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let file_type = entry.file_type();

        if file_type.is_symlink() {
            return Err(format!(
                "Refusing to export symlinked content: {}",
                path.display()
            ));
        }
        if path == skill_path {
            continue;
        }

        let rel_path = path
            .strip_prefix(skill_path)
            .map_err(|err| err.to_string())?;
        let zip_path = format!(
            "{}/{}",
            root_name,
            rel_path.to_string_lossy().replace('\\', "/")
        );

        if file_type.is_dir() {
            zip.add_directory(format!("{}/", zip_path), dir_options())
                .map_err(|err| err.to_string())?;
            continue;
        }

        let mut file = File::open(path).map_err(|err| err.to_string())?;
        zip.start_file(zip_path, file_options())
            .map_err(|err| err.to_string())?;
        io::copy(&mut file, zip).map_err(|err| err.to_string())?;
    }

    Ok(())
}

#[cfg(target_family = "windows")]
fn create_junction_dir(target: &Path, link: &Path) -> Result<(), String> {
    use std::process::Command;

    fn to_cmd_path(path: &Path) -> String {
        path.to_string_lossy().replace('/', "\\")
    }

    fn validate_path(path: &str) -> Result<(), String> {
        let dangerous_chars = ['|', '^', '<', '>', '%', '!', '"', '&', '(', ')', ';'];
        for ch in dangerous_chars {
            if path.contains(ch) {
                return Err(format!("Path contains dangerous character: '{}'", ch));
            }
        }
        Ok(())
    }

    let target = to_cmd_path(target);
    let link = to_cmd_path(link);

    validate_path(&target)?;
    validate_path(&link)?;

    let output = Command::new("cmd")
        .args(["/C", "mklink", "/J", &link, &target])
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            "unknown error".to_string()
        };
        Err(format!("mklink /J failed: {}", detail))
    }
}

#[cfg(target_family = "windows")]
fn should_copy_for_target(target_dir: &Path) -> bool {
    let normalized = target_dir
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    normalized.ends_with("/.qoder/skills")
}

#[tauri::command]
pub fn link_local_skill(request: LinkRequest) -> Result<InstallResult, String> {
    let home = dirs::home_dir().ok_or("Unable to determine the home directory")?;
    let layout = ensure_manager_layout(&home)?;
    let normalized_home = normalize_path(&home);
    let mut allowed_roots = vec![normalized_home.clone()];
    if let Some(project_dir) = request.project_dir.as_ref() {
        let project_root = normalize_path(Path::new(project_dir));
        allowed_roots.push(project_root);
    }
    let manager_roots = manager_skill_roots(&layout);

    let skill_path = PathBuf::from(&request.skill_path);
    let skill_canon = resolve_canonical(&skill_path)
        .ok_or_else(|| "Local skill path does not exist".to_string())?;
    if !path_is_within_any_root(&skill_canon, &manager_roots) {
        return Err("Local skill path must stay inside Skills Manager storage".to_string());
    }
    let skill_path = skill_canon;

    let safe_name = sanitize_skill_dir_name(&request.skill_name, &request.skill_path);

    let mut linked = Vec::new();
    let mut skipped = Vec::new();

    for target in request.link_targets {
        let target_base = PathBuf::from(&target.path);
        let normalized_target = normalize_path(&target_base);
        if !allowed_roots
            .iter()
            .any(|root| normalized_target.starts_with(root))
        {
            return Err(format!(
                "Target directory is outside the allowed directories: {}",
                target.name
            ));
        }

        // Normalize resolved paths before comparison so Windows verbatim prefixes do not
        // trigger false-positive symlink attack errors.
        let target_canon =
            resolve_canonical(&target_base).unwrap_or_else(|| normalized_target.clone());
        if !allowed_roots
            .iter()
            .any(|root| target_canon.starts_with(root))
        {
            return Err(format!(
                "Target directory failed the path safety check: {}",
                target.name
            ));
        }

        fs::create_dir_all(&target_base).map_err(|err| err.to_string())?;
        let link_path = target_base.join(&safe_name);

        if fs::symlink_metadata(&link_path).is_ok() {
            if is_symlink_to(&link_path, &skill_path) {
                skipped.push(format!("{}: already linked", target.name));
                continue;
            }
            if read_managed_copy_target(&link_path)
                .is_some_and(|managed_target| managed_target == skill_path)
            {
                skipped.push(format!("{}: already synced", target.name));
                continue;
            }
            skipped.push(format!("{}: target already exists", target.name));
            continue;
        }

        let mut linked_done = false;
        let mut link_errors = Vec::new();

        #[cfg(target_family = "windows")]
        if should_copy_for_target(&target_base) {
            match copy_dir_recursive(&skill_path, &link_path) {
                Ok(()) => match write_managed_copy_marker(&link_path, &skill_path) {
                    Ok(()) => {
                        linked.push(format!("{}: synced {}", target.name, link_path.display()));
                        linked_done = true;
                    }
                    Err(err) => {
                        let _ = fs::remove_dir_all(&link_path);
                        link_errors.push(format!("copy marker: {}", err));
                    }
                },
                Err(err) => link_errors.push(format!("copy: {}", err)),
            }
        }

        if !linked_done {
            match create_symlink_dir(&skill_path, &link_path) {
                Ok(()) => {
                    linked.push(format!("{}: {}", target.name, link_path.display()));
                    linked_done = true;
                }
                Err(err) => link_errors.push(format!("symlink: {}", err)),
            }
        }

        #[cfg(target_family = "windows")]
        if !linked_done {
            match create_junction_dir(&skill_path, &link_path) {
                Ok(()) => {
                    linked.push(format!("{}: junction {}", target.name, link_path.display()));
                    linked_done = true;
                }
                Err(err) => link_errors.push(format!("junction: {}", err)),
            }
        }

        if !linked_done {
            let detail = if link_errors.is_empty() {
                "unknown error".to_string()
            } else {
                link_errors.join("; ")
            };
            return Err(format!(
                "Failed to create a link for {} in {}: {}",
                request.skill_name, target.name, detail
            ));
        }
    }

    Ok(InstallResult {
        installed_path: skill_path.display().to_string(),
        linked,
        skipped,
    })
}

#[tauri::command]
pub fn scan_overview(request: LocalScanRequest) -> Result<Overview, String> {
    let home = dirs::home_dir().ok_or("Unable to determine the home directory")?;
    let layout = ensure_manager_layout(&home)?;

    let mut manager_skills = collect_skills_from_dir(&layout.skills, "manager", None)?;
    manager_skills.extend(collect_skills_from_dir(
        &layout.legacy_skills,
        "legacy",
        None,
    )?);

    // Resolve IDE directories: absolute paths are used directly, relative paths are joined with home
    let ide_dirs: Vec<(String, PathBuf)> = if request.ide_dirs.is_empty() {
        vec![
            (
                "Antigravity".to_string(),
                home.join(".gemini/antigravity/skills"),
            ),
            ("Claude".to_string(), home.join(".claude/skills")),
            ("CodeBuddy".to_string(), home.join(".codebuddy/skills")),
            ("Codex".to_string(), home.join(".codex/skills")),
            ("Cursor".to_string(), home.join(".cursor/skills")),
            ("Kiro".to_string(), home.join(".kiro/skills")),
            ("Qoder".to_string(), home.join(".qoder/skills")),
            ("Trae".to_string(), home.join(".trae/skills")),
            ("VSCode".to_string(), home.join(".github/skills")),
            ("Windsurf".to_string(), home.join(".windsurf/skills")),
        ]
    } else {
        request
            .ide_dirs
            .iter()
            .map(|item| {
                if !is_valid_ide_path(&item.relative_dir) {
                    return Err(format!("Invalid IDE directory: {}", item.label));
                }
                // Absolute path: use directly
                if is_absolute_ide_path(&item.relative_dir) {
                    Ok((item.label.clone(), PathBuf::from(&item.relative_dir)))
                } else {
                    // Relative path: join with home directory
                    Ok((item.label.clone(), home.join(&item.relative_dir)))
                }
            })
            .collect::<Result<Vec<_>, String>>()?
    };

    let mut ide_skills: Vec<IdeSkill> = Vec::new();

    let mut manager_map: Vec<(PathBuf, usize)> = Vec::new();
    for (idx, skill) in manager_skills.iter().enumerate() {
        if let Some(path) = resolve_canonical(Path::new(&skill.path)) {
            manager_map.push((path, idx));
        }
    }

    for (label, dir) in &ide_dirs {
        ide_skills.extend(collect_ide_skills(
            dir,
            label,
            &manager_map,
            &mut manager_skills,
        ));
    }

    if let Some(project) = request.project_dir {
        let base = PathBuf::from(project);
        for (label, dir) in &ide_dirs {
            // For absolute paths, also check the same path under project
            // For relative paths, join with project directory
            let project_dir = if dir.is_absolute() {
                dir.clone()
            } else {
                base.join(dir)
            };
            ide_skills.extend(collect_ide_skills(
                &project_dir,
                label,
                &manager_map,
                &mut manager_skills,
            ));
        }
    }

    Ok(Overview {
        manager_skills,
        ide_skills,
    })
}

#[tauri::command]
pub fn uninstall_skill(request: UninstallRequest) -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Unable to determine the home directory")?;
    let layout = ensure_manager_layout(&home)?;
    let mut allowed_roots = vec![layout.skills, layout.legacy_skills];

    let ide_dirs: Vec<String> = if request.ide_dirs.is_empty() {
        vec![
            ".gemini/antigravity/skills".to_string(),
            ".claude/skills".to_string(),
            ".codebuddy/skills".to_string(),
            ".codex/skills".to_string(),
            ".cursor/skills".to_string(),
            ".kiro/skills".to_string(),
            ".qoder/skills".to_string(),
            ".trae/skills".to_string(),
            ".github/skills".to_string(),
            ".windsurf/skills".to_string(),
        ]
    } else {
        request
            .ide_dirs
            .iter()
            .map(|item| item.relative_dir.clone())
            .collect()
    };

    for dir in &ide_dirs {
        if !is_valid_ide_path(dir) {
            return Err("Invalid IDE directory".to_string());
        }
        // Absolute path: add directly to allowed roots
        if is_absolute_ide_path(dir) {
            allowed_roots.push(PathBuf::from(dir));
        } else {
            // Relative path: join with home directory
            allowed_roots.push(home.join(dir));
        }
    }
    if let Some(project) = request.project_dir {
        let base = PathBuf::from(project);
        allowed_roots.push(base.join(".codex/skills"));
        allowed_roots.push(base.join(".trae/skills"));
        allowed_roots.push(base.join(".opencode/skills"));
        allowed_roots.push(base.join(".skills-manager/skills"));
    }

    let target = PathBuf::from(&request.target_path);
    let parent = target.parent().unwrap_or(Path::new(&request.target_path));
    let parent_canon = resolve_canonical(parent).unwrap_or_else(|| normalize_path(parent));
    let allowed_roots_canon: Vec<PathBuf> = allowed_roots
        .iter()
        .map(|root| resolve_canonical(root).unwrap_or_else(|| normalize_path(root)))
        .collect();
    let allowed = allowed_roots_canon
        .iter()
        .any(|root| parent_canon.starts_with(root));
    if !allowed {
        return Err("Target path is outside the allowed directories".to_string());
    }

    let metadata = fs::symlink_metadata(&target).map_err(|err| err.to_string())?;
    if metadata.file_type().is_symlink() {
        // `target.is_dir()` follows symlinks and may report true for a symlink-to-dir.
        // Removing such a symlink with `remove_dir` triggers ENOTDIR/ENOTEMPTY on macOS.
        fs::remove_file(&target)
            .or_else(|_| fs::remove_dir(&target))
            .map_err(|err| err.to_string())?;
        return Ok("Link removed".to_string());
    }

    fs::remove_dir_all(&target).map_err(|err| err.to_string())?;
    Ok("Directory removed".to_string())
}

#[tauri::command]
pub fn import_local_skill(request: ImportRequest) -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Unable to determine the home directory")?;
    let layout = ensure_manager_layout(&home)?;
    let result = import_skill_to_layout(Path::new(&request.source_path), &layout);
    match result.status.as_str() {
        "imported" | "skipped" => Ok(result.message),
        _ => Err(result.message),
    }
}

#[tauri::command]
pub fn import_discovered_skills(request: BatchImportRequest) -> Result<BatchImportResult, String> {
    let home = dirs::home_dir().ok_or("Unable to determine the home directory")?;
    let layout = ensure_manager_layout(&home)?;
    if request.source_paths.is_empty() {
        return Err("No skills were selected for import".to_string());
    }

    let items: Vec<SkillImportItemResult> = request
        .source_paths
        .iter()
        .map(|source| import_skill_to_layout(Path::new(source), &layout))
        .collect();
    let imported = items
        .iter()
        .filter(|item| item.status == "imported")
        .count();
    let skipped = items.iter().filter(|item| item.status == "skipped").count();
    let failed = items.iter().filter(|item| item.status == "failed").count();
    Ok(BatchImportResult {
        items,
        imported,
        skipped,
        failed,
    })
}

#[tauri::command]
pub fn get_manager_storage_info() -> Result<ManagerStorageInfo, String> {
    let home = dirs::home_dir().ok_or("Unable to determine the home directory")?;
    let layout = ensure_manager_layout(&home)?;
    Ok(ManagerStorageInfo {
        root_path: layout.root.display().to_string(),
        skills_path: layout.skills.display().to_string(),
        plugins_path: layout.plugins.display().to_string(),
        legacy_skills_path: layout.legacy_skills.display().to_string(),
        legacy_exists: layout.legacy_skills.is_dir(),
    })
}

#[tauri::command]
pub fn discover_skills_in_directory(
    request: SkillDiscoveryRequest,
) -> Result<Vec<DiscoveredSkill>, String> {
    let root = PathBuf::from(&request.root_path);
    if !root.exists() {
        return Err("Discovery directory does not exist".to_string());
    }
    if !root.is_dir() {
        return Err("Discovery path must be a directory".to_string());
    }
    let home = dirs::home_dir().ok_or("Unable to determine the home directory")?;
    let layout = ensure_manager_layout(&home)?;

    let mut skills: Vec<DiscoveredSkill> = WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
        })
        .map(|entry| {
            let mut skill = inspect_discovered_skill(entry.path());
            skill.is_duplicate =
                is_managed_duplicate(Path::new(&skill.path), &layout).unwrap_or(false);
            skill
        })
        .collect();

    skills.sort_by(|left, right| {
        left.path
            .to_ascii_lowercase()
            .cmp(&right.path.to_ascii_lowercase())
    });
    Ok(skills)
}

#[tauri::command]
pub fn adopt_ide_skill(request: AdoptIdeSkillRequest) -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Unable to determine the home directory".to_string())?;
    let normalized_home = normalize_path(&home);
    let layout = ensure_manager_layout(&home)?;
    let manager_root = layout.skills;

    let target = PathBuf::from(&request.target_path);
    let normalized_target = normalize_path(&target);
    if !normalized_target.starts_with(&normalized_home) {
        return Err("IDE skill path must stay inside the home directory".to_string());
    }

    fs::symlink_metadata(&target).map_err(|_| "IDE skill path does not exist".to_string())?;
    let target_canon = resolve_canonical(&target);

    let (name, has_skill_file) = if let Some(path) = target_canon.as_ref() {
        (read_skill_metadata(path).0, path.join("SKILL.md").exists())
    } else {
        (
            target
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("skill")
                .to_string(),
            false,
        )
    };

    let fallback_key = target.to_str().unwrap_or(request.target_path.as_str());
    let safe_name = sanitize_skill_dir_name(&name, fallback_key);
    let manager_target = manager_root.join(&safe_name);

    if manager_target.exists() {
        let manager_canon = resolve_canonical(&manager_target)
            .ok_or_else(|| "Managed skill path does not exist".to_string())?;
        if target_canon
            .as_ref()
            .is_some_and(|target_path| *target_path == manager_canon)
        {
            return Ok(format!("{} is already managed", name));
        }
    } else {
        let source_dir = target_canon
            .as_ref()
            .ok_or_else(|| "IDE skill path does not exist".to_string())?;
        if !has_skill_file {
            return Err("Target directory does not contain SKILL.md".to_string());
        }
        copy_dir_recursive(source_dir, &manager_target)?;
    }

    remove_path(&target)?;

    let mut linked_done = false;
    let mut link_errors = Vec::new();

    match create_symlink_dir(&manager_target, &target) {
        Ok(()) => linked_done = true,
        Err(err) => link_errors.push(format!("symlink: {}", err)),
    }

    #[cfg(target_family = "windows")]
    if !linked_done {
        match create_junction_dir(&manager_target, &target) {
            Ok(()) => linked_done = true,
            Err(err) => link_errors.push(format!("junction: {}", err)),
        }
    }

    if !linked_done {
        copy_dir_recursive(&manager_target, &target)?;
        let detail = if link_errors.is_empty() {
            "unknown error".to_string()
        } else {
            link_errors.join("; ")
        };
        return Err(format!(
            "Managed {} in Skills Manager, but failed to create a link for {}. Restored a local copy instead. {}",
            name, request.ide_label, detail
        ));
    }

    Ok(format!(
        "Managed {} and re-linked it to {}",
        name, request.ide_label
    ))
}

#[tauri::command]
pub fn read_local_skill_preview(
    skill_path: String,
    target_language: Option<String>,
) -> Result<LocalSkillPreview, String> {
    let home = dirs::home_dir().ok_or("Unable to determine the home directory")?;
    let layout = ensure_manager_layout(&home)?;
    let manager_roots = manager_skill_roots(&layout);
    let canonical = validate_manager_skill_path(&PathBuf::from(skill_path), &manager_roots)?;
    let skill_md_path = canonical.join("SKILL.md");
    let original_content = fs::read_to_string(&skill_md_path).map_err(|err| err.to_string())?;
    let translated = match target_language
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        Some(language) => crate::commands::translation_settings::translate_skill_preview(
            &home,
            &original_content,
            language,
        )?,
        None => crate::commands::translation_settings::TranslationResult {
            content: original_content,
            status: "original",
        },
    };
    let display_description = parse_skill_document(&translated.content).description;

    Ok(LocalSkillPreview {
        skill_md_path: skill_md_path.display().to_string(),
        skill_md_content: translated.content,
        display_description,
        translation_status: translated.status.to_string(),
    })
}

fn save_local_skill_document(
    home: &Path,
    request: crate::types::SaveLocalSkillRequest,
) -> Result<LocalSkillPreview, String> {
    if request.content.len() > 1_048_576 {
        return Err("SKILL.md 不能超过 1 MiB / SKILL.md exceeds 1 MiB".into());
    }
    let layout = ensure_manager_layout(home)?;
    let roots = manager_skill_roots(&layout);
    let canonical = validate_manager_skill_path(&PathBuf::from(&request.skill_path), &roots)?;
    let file = canonical.join("SKILL.md");
    let current = fs::read_to_string(&file).map_err(|e| e.to_string())?;
    if current != request.expected_content {
        return Err(
            "检测到外部修改，请重新打开后再编辑 / File changed externally; reopen before saving"
                .into(),
        );
    }
    if request.content == current {
        let display_description = parse_skill_document(&current).description;
        return Ok(LocalSkillPreview {
            skill_md_path: file.display().to_string(),
            skill_md_content: current,
            display_description,
            translation_status: "original".into(),
        });
    }
    let old = parse_skill_document(&current);
    let edited = parse_skill_document(&request.content);
    if !edited.has_frontmatter {
        return Err("必须保留完整的 YAML frontmatter / YAML frontmatter is required".into());
    }
    let old_uuid = old
        .uuid
        .ok_or("当前 Skill 缺少有效 UUID，请先刷新管理库 / Existing Skill has no valid UUID")?;
    if edited.uuid.as_deref() != Some(old_uuid.as_str()) {
        return Err("不允许修改或删除 UUID / UUID cannot be changed or removed".into());
    }
    let old_name = old
        .name
        .as_deref()
        .map(str::trim)
        .ok_or("当前 Skill 缺少名称 / Existing Skill has no name")?;
    let name = edited
        .name
        .as_deref()
        .map(str::trim)
        .ok_or("名称不能为空 / Name is required")?;
    if name != old_name || !is_standard_skill_name(name) {
        return Err(
            "编辑器暂不允许重命名；名称须保持原值 / Renaming is not supported in the editor".into(),
        );
    }
    let description = edited
        .description
        .as_deref()
        .map(str::trim)
        .ok_or("描述不能为空 / Description is required")?;
    if description.is_empty() || description.chars().count() > 1024 {
        return Err("描述需为 1–1024 个字符 / Description must contain 1–1024 characters".into());
    }
    if !skill_body_has_content(&request.content) {
        return Err("正文不能为空 / Skill body cannot be empty".into());
    }
    let _history = crate::commands::history::HISTORY_LOCK
        .lock()
        .map_err(|_| "History unavailable")?;
    crate::commands::history::snapshot_before_edit_at(home, &canonical)?;
    let temp = canonical.join(format!(".SKILL-{}.tmp", uuid::Uuid::new_v4()));
    let previous = canonical.join(format!(".SKILL-{}.previous", uuid::Uuid::new_v4()));
    use std::io::Write;
    let result = (|| {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        output
            .write_all(request.content.as_bytes())
            .map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&file, &previous).map_err(|e| format!("Cannot stage current SKILL.md: {e}"))?;
        if let Err(error) = fs::rename(&temp, &file) {
            let recovery = fs::rename(&previous, &file);
            return Err(format!(
                "Cannot replace SKILL.md: {error}; original recovery: {recovery:?}"
            ));
        }
        fs::remove_file(&previous)
            .map_err(|e| format!("Saved, but previous temporary file needs cleanup: {e}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    Ok(LocalSkillPreview {
        skill_md_path: file.display().to_string(),
        skill_md_content: request.content,
        display_description: edited.description,
        translation_status: "original".into(),
    })
}

#[tauri::command]
pub fn save_local_skill(
    request: crate::types::SaveLocalSkillRequest,
) -> Result<LocalSkillPreview, String> {
    save_local_skill_document(
        &dirs::home_dir().ok_or("Unable to determine home directory")?,
        request,
    )
}

#[tauri::command]
pub fn delete_local_skills(request: DeleteLocalSkillRequest) -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Unable to determine the home directory")?;
    crate::commands::trash::recycle(&home, request.target_paths)
}

#[tauri::command]
pub fn export_local_skills(request: ExportSkillsRequest) -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Unable to determine the home directory")?;
    let layout = ensure_manager_layout(&home)?;
    let manager_roots = manager_skill_roots(&layout);

    if request.target_paths.is_empty() {
        return Err("No skills were provided for export".to_string());
    }
    if request.export_path.trim().is_empty() {
        return Err("Export path is required".to_string());
    }

    let export_path = PathBuf::from(&request.export_path);
    let export_parent = export_path
        .parent()
        .ok_or_else(|| "Export path must include a parent directory".to_string())?;
    fs::create_dir_all(export_parent).map_err(|err| err.to_string())?;

    let mut skill_paths = Vec::new();
    for raw_path in request.target_paths {
        let canonical = validate_manager_skill_path(&PathBuf::from(raw_path), &manager_roots)?;
        skill_paths.push(canonical);
    }

    ensure_export_path_is_safe(&export_path, &skill_paths)?;

    let file = File::create(&export_path).map_err(|err| err.to_string())?;
    let mut zip = ZipWriter::new(file);

    for skill_path in &skill_paths {
        let root_name = skill_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("skill");
        if let Err(err) = zip_skill_directory(&mut zip, skill_path, root_name) {
            let _ = zip.finish();
            let _ = fs::remove_file(&export_path);
            return Err(err);
        }
    }

    zip.finish().map_err(|err| err.to_string())?;
    Ok(export_path.display().to_string())
}

#[tauri::command]
pub fn scan_project_ide_dirs(request: ProjectScanRequest) -> Result<ProjectScanResult, String> {
    let project_dir = PathBuf::from(&request.project_dir);

    if !project_dir.exists() {
        return Err("Project directory does not exist".to_string());
    }

    let ide_dir_patterns = [
        (".gemini/antigravity/skills", "Antigravity"),
        (".claude/skills", "Claude Code"),
        (".codebuddy/skills", "CodeBuddy"),
        (".codex/skills", "Codex"),
        (".cursor/skills", "Cursor"),
        (".kiro/skills", "Kiro"),
        (".openclaw/skills", "OpenClaw"),
        (".opencode/skills", "OpenCode"),
        (".qoder/skills", "Qoder"),
        (".trae/skills", "Trae"),
        (".github/skills", "VSCode"),
        (".windsurf/skills", "Windsurf"),
    ];

    let mut detected_ide_dirs = Vec::new();

    for (relative_path, label) in ide_dir_patterns.iter() {
        let ide_path = project_dir.join(relative_path);
        if ide_path.exists() && ide_path.is_dir() {
            detected_ide_dirs.push(ProjectIdeDir {
                label: label.to_string(),
                relative_dir: relative_path.to_string(),
                absolute_path: ide_path.display().to_string(),
            });
        }
    }

    Ok(ProjectScanResult {
        project_dir: request.project_dir,
        detected_ide_dirs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("skills-manager-{label}-{unique}"))
    }

    fn managed_skill_fixture(label: &str) -> (PathBuf, PathBuf, String, String) {
        let root = test_dir(label);
        let home = root.join("home");
        let skill = home.join("Skill Manager/Skills/sample-skill");
        fs::create_dir_all(&skill).expect("create managed skill");
        let uuid = uuid::Uuid::new_v4().to_string();
        let content = format!(
            "---\nname: sample-skill\nuuid: {uuid}\ndescription: Original description.\n---\n\n# Original body\n"
        );
        fs::write(skill.join("SKILL.md"), &content).expect("write managed skill");
        (root, home, uuid, content)
    }

    fn edit_request(
        skill: &Path,
        expected: &str,
        content: String,
    ) -> crate::types::SaveLocalSkillRequest {
        crate::types::SaveLocalSkillRequest {
            skill_path: skill.display().to_string(),
            expected_content: expected.to_string(),
            content,
        }
    }

    fn history_record_count(home: &Path) -> usize {
        let root = home.join("Skill Manager/.history");
        fs::read_dir(root)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| entry.path().join("record.json").is_file())
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn edits_managed_skill_and_creates_snapshot() {
        let (root, home, uuid, original) = managed_skill_fixture("edit-success");
        let skill = home.join("Skill Manager/Skills/sample-skill");
        let edited = format!(
            "---\nname: sample-skill\nuuid: {uuid}\ndescription: Edited description.\n---\n\n# Edited body\n"
        );

        let result =
            save_local_skill_document(&home, edit_request(&skill, &original, edited.clone()))
                .expect("edit should succeed");

        assert_eq!(result.skill_md_content, edited);
        assert_eq!(fs::read_to_string(skill.join("SKILL.md")).unwrap(), edited);
        assert_eq!(history_record_count(&home), 1);
        let history_root = home.join("Skill Manager/.history");
        let snapshot = fs::read_dir(history_root)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let record: serde_json::Value = serde_json::from_slice(
            &fs::read(snapshot.join("record.json")).expect("read snapshot record"),
        )
        .expect("parse snapshot record");
        assert_eq!(record["reason"], "before-edit");
        assert_eq!(
            fs::read_to_string(snapshot.join("content/SKILL.md")).unwrap(),
            original
        );
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn edit_rejects_external_changes_and_identity_changes() {
        let (root, home, uuid, original) = managed_skill_fixture("edit-conflicts");
        let skill = home.join("Skill Manager/Skills/sample-skill");
        let external = original.replace("Original body", "Externally changed");
        fs::write(skill.join("SKILL.md"), &external).unwrap();
        let error = save_local_skill_document(
            &home,
            edit_request(&skill, &original, external.replace("Externally", "Editor")),
        )
        .unwrap_err();
        assert!(error.contains("外部修改"), "unexpected error: {error}");

        let changed_uuid = external.replace(&uuid, &uuid::Uuid::new_v4().to_string());
        let error = save_local_skill_document(&home, edit_request(&skill, &external, changed_uuid))
            .unwrap_err();
        assert!(error.contains("UUID"));

        let renamed = external.replace("name: sample-skill", "name: renamed-skill");
        let error =
            save_local_skill_document(&home, edit_request(&skill, &external, renamed)).unwrap_err();
        assert!(error.contains("重命名"));
        assert_eq!(history_record_count(&home), 0);
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn edit_validates_description_and_body_without_snapshotting() {
        let (root, home, _uuid, original) = managed_skill_fixture("edit-validation");
        let skill = home.join("Skill Manager/Skills/sample-skill");
        let empty_description =
            original.replace("description: Original description.", "description:");
        let error =
            save_local_skill_document(&home, edit_request(&skill, &original, empty_description))
                .unwrap_err();
        assert!(error.contains("描述"));

        let empty_body = original.replace("\n# Original body\n", "\n   \n");
        let error = save_local_skill_document(&home, edit_request(&skill, &original, empty_body))
            .unwrap_err();
        assert!(error.contains("正文"));
        assert_eq!(history_record_count(&home), 0);
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn unchanged_edit_does_not_create_snapshot() {
        let (root, home, _uuid, original) = managed_skill_fixture("edit-unchanged");
        let skill = home.join("Skill Manager/Skills/sample-skill");
        save_local_skill_document(&home, edit_request(&skill, &original, original.clone()))
            .expect("unchanged save should succeed");
        assert_eq!(history_record_count(&home), 0);
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn validates_agent_skills_frontmatter() {
        let multiline = parse_skill_document(
            "---\nname: sample\ndescription: |-\n  First line\n  ---\n  Last line\n---\n# Body",
        );
        assert_eq!(
            multiline.description.as_deref(),
            Some("First line --- Last line")
        );
        let metadata = parse_skill_document(
            "---\nname: sample-skill\nuuid: 550e8400-e29b-41d4-a716-446655440000\ndescription: A useful sample skill.\n---\n# Sample\n",
        );
        assert!(metadata.has_frontmatter);
        assert_eq!(metadata.name.as_deref(), Some("sample-skill"));
        assert_eq!(
            metadata.description.as_deref(),
            Some("A useful sample skill.")
        );
        assert_eq!(
            metadata.uuid.as_deref(),
            Some("550e8400-e29b-41d4-a716-446655440000")
        );
        assert!(is_standard_skill_name("sample-skill"));
        assert!(!is_standard_skill_name("Sample Skill"));
    }

    #[test]
    fn recursively_discovers_standard_and_compatible_skills() {
        let root = test_dir("discovery");
        let standard_dir = root.join(".codex/skills/standard-skill");
        let compatible_dir = root.join("tools/custom-skill");
        fs::create_dir_all(&standard_dir).expect("create standard directory");
        fs::create_dir_all(&compatible_dir).expect("create compatible directory");
        fs::write(
            standard_dir.join("SKILL.md"),
            "---\nname: standard-skill\ndescription: Standard metadata.\n---\n",
        )
        .expect("write standard skill");
        fs::write(compatible_dir.join("SKILL.md"), "# Custom CLI skill\n")
            .expect("write compatible skill");

        let result = discover_skills_in_directory(SkillDiscoveryRequest {
            root_path: root.display().to_string(),
        })
        .expect("discovery should succeed");

        assert_eq!(result.len(), 2);
        let standard = result
            .iter()
            .find(|skill| skill.name == "standard-skill")
            .expect("standard skill should be found");
        assert!(standard.is_standard);
        assert_eq!(standard.provider, "Codex");

        let compatible = result
            .iter()
            .find(|skill| skill.name == "custom-skill")
            .expect("compatible skill should be found");
        assert!(!compatible.is_standard);
        assert!(compatible
            .issues
            .contains(&ISSUE_MISSING_FRONTMATTER.to_string()));

        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn imports_skills_and_skips_an_identical_copy() {
        let root = test_dir("import-identical");
        let home = root.join("home");
        let source = root.join("source/sample-skill");
        fs::create_dir_all(&source).expect("create source directory");
        fs::write(
            source.join("SKILL.md"),
            "---\nname: \"sample-skill\"\ndescription: Import test.\n---\n",
        )
        .expect("write skill");
        let layout = ensure_manager_layout(&home).expect("create manager layout");

        let imported = import_skill_to_layout(&source, &layout);
        assert_eq!(imported.status, "imported");
        assert_eq!(imported.name, "sample-skill");
        assert!(layout.skills.join("sample-skill/SKILL.md").is_file());
        assert!(read_skill_uuid(&layout.skills.join("sample-skill")).is_some());
        assert!(
            read_skill_uuid(&source).is_none(),
            "discovery source is read-only"
        );
        let records: Vec<ManagedSkillRecord> = serde_json::from_str(
            &fs::read_to_string(layout.metadata.join("skills.json")).expect("read metadata"),
        )
        .expect("parse metadata");
        assert_eq!(records[0].name, "sample-skill");
        assert_eq!(
            records[0].uuid,
            read_skill_uuid(&layout.skills.join("sample-skill")).unwrap()
        );

        let repeated = import_skill_to_layout(&source, &layout);
        assert_eq!(repeated.status, "skipped");
        assert_eq!(repeated.target_path, imported.target_path);
        assert!(
            is_managed_duplicate(&source, &layout).expect("duplicate check should succeed"),
            "an already imported source should be disabled during discovery"
        );

        let first_scan =
            collect_skills_from_dir(&layout.skills, "manager", None).expect("first scan");
        let second_scan =
            collect_skills_from_dir(&layout.skills, "manager", None).expect("second scan");
        assert_eq!(first_scan[0].uuid, second_scan[0].uuid);
        assert_eq!(first_scan[0].id, first_scan[0].uuid);

        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn normalizes_existing_quoted_names_in_import_metadata() {
        let root = test_dir("metadata-normalization");
        let home = root.join("home");
        let layout = ensure_manager_layout(&home).expect("create manager layout");
        let records = vec![ManagedSkillRecord {
            uuid: String::new(),
            name: "\"sample-skill\"".to_string(),
            source_path: root.join("source").display().to_string(),
            target_path: layout.skills.join("sample-skill").display().to_string(),
            imported_at_unix: 1,
        }];
        fs::write(
            layout.metadata.join("skills.json"),
            serde_json::to_string_pretty(&records).expect("serialize metadata"),
        )
        .expect("write metadata");

        let target = layout.skills.join("sample-skill");
        fs::create_dir_all(&target).expect("create managed skill");
        fs::write(
            target.join("SKILL.md"),
            "---\nname: sample-skill\ndescription: Existing skill.\n---\n",
        )
        .expect("write managed skill");

        ensure_manager_layout(&home).expect("normalize manager layout");
        let normalized: Vec<ManagedSkillRecord> = serde_json::from_str(
            &fs::read_to_string(layout.metadata.join("skills.json")).expect("read metadata"),
        )
        .expect("parse metadata");
        assert_eq!(normalized[0].name, "sample-skill");
        assert!(normalize_skill_uuid(&normalized[0].uuid).is_some());
        assert_eq!(
            read_skill_uuid(&target).as_deref(),
            Some(normalized[0].uuid.as_str())
        );

        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn retains_different_skills_with_the_same_name() {
        let root = test_dir("import-conflict");
        let home = root.join("home");
        let first = root.join("source-a/sample-skill");
        let second = root.join("source-b/sample-skill");
        fs::create_dir_all(&first).expect("create first source");
        fs::create_dir_all(&second).expect("create second source");
        fs::write(
            first.join("SKILL.md"),
            "---\nname: sample-skill\ndescription: First version.\n---\n",
        )
        .expect("write first skill");
        fs::write(
            second.join("SKILL.md"),
            "---\nname: sample-skill\ndescription: Second version.\n---\n",
        )
        .expect("write second skill");
        let layout = ensure_manager_layout(&home).expect("create manager layout");

        assert_eq!(import_skill_to_layout(&first, &layout).status, "imported");
        let second_result = import_skill_to_layout(&second, &layout);
        assert_eq!(second_result.status, "imported");
        assert!(layout.skills.join("sample-skill-2/SKILL.md").is_file());
        let first_uuid = read_skill_uuid(&layout.skills.join("sample-skill")).unwrap();
        let second_uuid = read_skill_uuid(&layout.skills.join("sample-skill-2")).unwrap();
        assert_ne!(first_uuid, second_uuid);

        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn preserves_a_source_uuid_when_importing() {
        let root = test_dir("import-existing-uuid");
        let home = root.join("home");
        let source = root.join("source/sample-skill");
        let expected = "550e8400-e29b-41d4-a716-446655440000";
        fs::create_dir_all(&source).expect("create source directory");
        fs::write(
            source.join("SKILL.md"),
            format!(
                "---\nname: sample-skill\nuuid: {expected}\ndescription: Existing identity.\n---\n"
            ),
        )
        .expect("write skill");
        let layout = ensure_manager_layout(&home).expect("create manager layout");

        assert_eq!(import_skill_to_layout(&source, &layout).status, "imported");
        assert_eq!(
            read_skill_uuid(&layout.skills.join("sample-skill")).as_deref(),
            Some(expected)
        );

        fs::remove_dir_all(root).expect("remove test directory");
    }
}
