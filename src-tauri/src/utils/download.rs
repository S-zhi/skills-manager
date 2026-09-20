use crate::utils::path::{normalize_path, sanitize_skill_dir_name};
use crate::utils::security::is_within_directory;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;
use zip::ZipArchive;

const GITHUB_WEB_PREFIX: &str = "https://github.com/";
const USER_AGENT: &str = "skills-manager-gui/0.1";

#[derive(Debug, Clone, PartialEq, Eq)]
enum DownloadSource {
    GitHubRepo {
        owner: String,
        repo: String,
    },
    GitHubTree {
        owner: String,
        repo: String,
        git_ref: String,
        subpath: PathBuf,
    },
    ZipUrl {
        url: String,
    },
}

pub fn download_bytes(url: &str, headers: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    download_bytes_with_timeout(url, headers, 60)
}

fn download_bytes_with_timeout(
    url: &str,
    headers: &[(&str, &str)],
    timeout_secs: u64,
) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .redirects(5)
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build();
    let mut request = agent.get(url);
    for (key, value) in headers {
        request = request.set(key, value);
    }

    let response = request.call().map_err(|err| err.to_string())?;
    const MAX_DOWNLOAD_SIZE: u64 = 50 * 1024 * 1024;
    read_limited(response.into_reader(), MAX_DOWNLOAD_SIZE)
}

fn read_limited(reader: impl Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut buf)
        .map_err(|err| err.to_string())?;
    if buf.len() as u64 > limit {
        return Err("下载超过大小限制（仓库压缩包最大 50 MiB），请单独获取所需 Skill 后本地导入 / Download too large".into());
    }
    Ok(buf)
}

pub fn download_skill_to_dir(
    source_url: &str,
    skill_name: &str,
    install_base_dir: &Path,
    overwrite: bool,
    target_path: Option<&Path>,
) -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("无法获取用户目录")?;
    let allowed_bases = [
        normalize_path(&home.join("Skill Manager/Skills")),
        normalize_path(&home.join(".skills-manager/skills")),
    ];
    let requested_base = normalize_path(install_base_dir);
    if !allowed_bases.iter().any(|base| requested_base == *base) {
        return Err("安装目录不在允许范围内".to_string());
    }

    fs::create_dir_all(install_base_dir).map_err(|err| err.to_string())?;

    let safe_name = sanitize_skill_dir_name(skill_name, source_url);
    let target_dir = if let Some(target_path) = target_path {
        normalize_path(target_path)
    } else if overwrite {
        install_base_dir.join(&safe_name)
    } else {
        let first = install_base_dir.join(&safe_name);
        if !first.exists() {
            first
        } else {
            (2usize..)
                .map(|suffix| install_base_dir.join(format!("{safe_name}-{suffix}")))
                .find(|candidate| !candidate.exists())
                .expect("an available skill directory should exist")
        }
    };
    if target_dir.parent().map(normalize_path).as_ref() != Some(&requested_base) {
        return Err("更新目标不在安装目录内".to_string());
    }
    if target_dir.exists() {
        if !overwrite {
            return Err("目标目录已存在，请更换名称或先清理".to_string());
        }
    }

    let parsed_source = parse_download_source(source_url)?;
    let preferred_subpath = parsed_source.preferred_subpath();
    let zip_buf = download_archive_bytes(&parsed_source)?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_millis();
    let temp_dir = std::env::temp_dir().join(format!("skills-manager-{}", timestamp));
    let extract_dir = temp_dir.join("extract");
    fs::create_dir_all(&extract_dir).map_err(|err| err.to_string())?;

    let _temp_dir_guard = TempDirGuard::new(&temp_dir);

    extract_zip(&zip_buf, &extract_dir)?;
    let selected_root = find_skill_root(&extract_dir, &safe_name, preferred_subpath.as_deref())?;
    let prepared_dir = temp_dir.join("prepared");
    copy_dir_recursive(&selected_root, &prepared_dir)?;

    // Prepare on the destination volume before touching the current version.
    let staged = install_base_dir.join(format!(".update-{}", uuid::Uuid::new_v4()));
    copy_dir_recursive(&prepared_dir, &staged)?;
    let previous = install_base_dir.join(format!(".previous-{}", uuid::Uuid::new_v4()));
    let had_previous = target_dir.exists();
    if had_previous {
        fs::rename(&target_dir, &previous).map_err(|err| err.to_string())?;
    }
    if let Err(error) = fs::rename(&staged, &target_dir) {
        if had_previous {
            fs::rename(&previous, &target_dir).map_err(|recovery| format!("Update failed: {error}; recovery failed: {recovery}; previous files retained at {}", previous.display()))?;
        }
        return Err(format!("Update failed; original files preserved: {error}"));
    }
    if had_previous {
        fs::remove_dir_all(&previous)
            .map_err(|err| format!("Updated; previous staging cleanup failed: {err}"))?;
    }

    Ok(target_dir)
}

fn download_archive_bytes(source: &DownloadSource) -> Result<Vec<u8>, String> {
    match source {
        DownloadSource::GitHubRepo { owner, repo } => {
            let archive_url = format!("https://api.github.com/repos/{owner}/{repo}/zipball/HEAD");
            download_bytes(
                &archive_url,
                &[
                    ("Accept", "application/vnd.github+json"),
                    ("X-GitHub-Api-Version", "2022-11-28"),
                    ("User-Agent", USER_AGENT),
                ],
            )
        }
        DownloadSource::GitHubTree {
            owner,
            repo,
            git_ref,
            ..
        } => {
            let archive_url = format!(
                "https://api.github.com/repos/{owner}/{repo}/zipball/{}",
                urlencoding::encode(git_ref)
            );
            download_bytes(
                &archive_url,
                &[
                    ("Accept", "application/vnd.github+json"),
                    ("X-GitHub-Api-Version", "2022-11-28"),
                    ("User-Agent", USER_AGENT),
                ],
            )
        }
        DownloadSource::ZipUrl { url } => download_bytes(url, &[("User-Agent", USER_AGENT)]),
    }
}

fn parse_download_source(source_url: &str) -> Result<DownloadSource, String> {
    let trimmed = source_url.trim();
    if trimmed.is_empty() {
        return Err("缺少有效的源码地址 (Source URL)".to_string());
    }

    if let Some(github) = parse_github_source(trimmed)? {
        return Ok(github);
    }

    if is_supported_zip_url(trimmed) {
        return Ok(DownloadSource::ZipUrl {
            url: trimmed.to_string(),
        });
    }

    Err("仅支持 GitHub 仓库链接、GitHub 子目录链接或 ZIP 下载链接".to_string())
}

fn parse_github_source(source_url: &str) -> Result<Option<DownloadSource>, String> {
    let Some(stripped) = source_url.strip_prefix(GITHUB_WEB_PREFIX) else {
        return Ok(None);
    };

    let path_without_query = stripped
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_matches('/');
    let parts: Vec<&str> = path_without_query
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();

    if parts.len() < 2 {
        return Err("GitHub 链接格式无效，至少需要 owner/repo".to_string());
    }

    let owner = parts[0].to_string();
    let repo = parts[1]
        .strip_suffix(".git")
        .unwrap_or(parts[1])
        .to_string();
    if owner.is_empty() || repo.is_empty() {
        return Err("GitHub 链接格式无效，缺少 owner 或 repo".to_string());
    }

    if parts.len() == 2 {
        return Ok(Some(DownloadSource::GitHubRepo { owner, repo }));
    }

    match parts[2] {
        "tree" => {
            if parts.len() < 5 {
                return Err("GitHub 子目录链接格式无效，缺少分支或路径".to_string());
            }
            let git_ref = parts[3].to_string();
            let subpath = sanitize_relative_subpath(&parts[4..].join("/"))?;
            Ok(Some(DownloadSource::GitHubTree {
                owner,
                repo,
                git_ref,
                subpath,
            }))
        }
        "blob" => Err("暂不支持 GitHub 文件链接，请改用仓库、目录或 ZIP 链接".to_string()),
        _ => Ok(Some(DownloadSource::GitHubRepo { owner, repo })),
    }
}

fn sanitize_relative_subpath(raw: &str) -> Result<PathBuf, String> {
    let mut output = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(value) => output.push(value),
            Component::CurDir => {}
            _ => return Err("GitHub 子目录路径无效".to_string()),
        }
    }

    if output.as_os_str().is_empty() {
        return Err("GitHub 子目录路径不能为空".to_string());
    }

    Ok(output)
}

fn is_supported_zip_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && url
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .ends_with(".zip")
}

impl DownloadSource {
    fn preferred_subpath(&self) -> Option<PathBuf> {
        match self {
            DownloadSource::GitHubTree { subpath, .. } => Some(subpath.clone()),
            _ => None,
        }
    }
}

struct TempDirGuard<'a> {
    path: &'a Path,
    armed: bool,
}

impl<'a> TempDirGuard<'a> {
    fn new(path: &'a Path) -> Self {
        Self { path, armed: true }
    }

    #[allow(dead_code)]
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl<'a> Drop for TempDirGuard<'a> {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(self.path);
        }
    }
}

pub fn extract_zip(buf: &[u8], extract_dir: &Path) -> Result<(), String> {
    let cursor = Cursor::new(buf);
    let mut zip = ZipArchive::new(cursor).map_err(|err| err.to_string())?;

    let canonical_extract = extract_dir
        .canonicalize()
        .unwrap_or_else(|_| extract_dir.to_path_buf());

    for i in 0..zip.len() {
        let file = zip.by_index(i).map_err(|err| err.to_string())?;
        let Some(enclosed) = file.enclosed_name() else {
            continue;
        };
        let out_path = canonical_extract.join(&enclosed);

        if !is_within_directory(&canonical_extract, &out_path) {
            return Err(format!(
                "Zip Slip attack detected: {} attempts to write outside of {}",
                enclosed.display(),
                extract_dir.display()
            ));
        }

        if file.is_dir() {
            fs::create_dir_all(&out_path).map_err(|err| err.to_string())?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let mut outfile = fs::File::create(&out_path).map_err(|err| err.to_string())?;

        const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;
        std::io::copy(&mut file.take(MAX_FILE_SIZE), &mut outfile)
            .map_err(|err| err.to_string())?;
    }

    Ok(())
}

pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    for entry in WalkDir::new(src) {
        let entry = entry.map_err(|err| err.to_string())?;
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            return Err(format!(
                "检测到符号链接，已拒绝复制: {}",
                entry.path().display()
            ));
        }
        let rel_path = entry
            .path()
            .strip_prefix(src)
            .map_err(|err| err.to_string())?;
        let target = dst.join(rel_path);
        if file_type.is_dir() {
            fs::create_dir_all(&target).map_err(|err| err.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            fs::copy(entry.path(), &target).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn find_skill_root(
    extract_dir: &Path,
    expected: &str,
    preferred_subpath: Option<&Path>,
) -> Result<PathBuf, String> {
    if let Some(preferred) = preferred_subpath {
        if let Some(found) = find_preferred_root(extract_dir, preferred)? {
            if found.join("SKILL.md").is_file() {
                return Ok(found);
            }
            return Err(
                "指定目录中没有 SKILL.md，已停止导入 / Linked directory has no SKILL.md".into(),
            );
        }
        return Err(
            "源码中的指定 Skill 目录不存在，已停止导入 / Linked Skill directory no longer exists"
                .into(),
        );
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(extract_dir).max_depth(5) {
        let entry = entry.map_err(|err| err.to_string())?;
        if entry.file_type().is_file() && entry.file_name() == "SKILL.md" {
            if let Some(parent) = entry.path().parent() {
                candidates.push(parent.to_path_buf());
            }
        }
    }

    if candidates.is_empty() {
        return Err("下载内容中未找到 SKILL.md / No SKILL.md found in download".into());
    }

    let expected_lower = expected.to_ascii_lowercase();
    if let Some(best) = candidates.iter().find(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_ascii_lowercase() == expected_lower)
            .unwrap_or(false)
    }) {
        return Ok(best.clone());
    }

    Ok(candidates[0].clone())
}

fn find_preferred_root(
    extract_dir: &Path,
    preferred_subpath: &Path,
) -> Result<Option<PathBuf>, String> {
    let direct = extract_dir.join(preferred_subpath);
    if direct.exists() && direct.is_dir() {
        return Ok(Some(direct));
    }

    let entries = fs::read_dir(extract_dir).map_err(|err| err.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let candidate = path.join(preferred_subpath);
        if candidate.exists() && candidate.is_dir() {
            return Ok(Some(candidate));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{find_skill_root, parse_download_source, DownloadSource};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_github_repo_url() {
        let parsed = parse_download_source("https://github.com/owner/repo").unwrap();
        assert_eq!(
            parsed,
            DownloadSource::GitHubRepo {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
            }
        );
    }

    #[test]
    fn oversized_download_is_rejected_instead_of_silently_truncated() {
        assert_eq!(
            super::read_limited(std::io::Cursor::new(b"1234"), 4).unwrap(),
            b"1234"
        );
        assert!(super::read_limited(std::io::Cursor::new(b"12345"), 4).is_err());
    }

    #[test]
    fn missing_linked_skill_cannot_import_another_directory() {
        let root = std::env::temp_dir().join(format!("skill-source-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("repo/other")).unwrap();
        fs::write(root.join("repo/other/SKILL.md"), "other").unwrap();
        assert!(find_skill_root(&root, "other", Some(std::path::Path::new("missing"))).is_err());
        fs::create_dir_all(root.join("repo/missing")).unwrap();
        assert!(find_skill_root(&root, "other", Some(std::path::Path::new("missing"))).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_github_tree_url() {
        let parsed =
            parse_download_source("https://github.com/anthropics/skills/tree/main/skills/docx")
                .unwrap();
        assert_eq!(
            parsed,
            DownloadSource::GitHubTree {
                owner: "anthropics".to_string(),
                repo: "skills".to_string(),
                git_ref: "main".to_string(),
                subpath: PathBuf::from("skills/docx"),
            }
        );
    }

    #[test]
    fn parses_zip_url() {
        let parsed =
            parse_download_source("https://example.com/files/skill-pack.zip?download=1").unwrap();
        assert_eq!(
            parsed,
            DownloadSource::ZipUrl {
                url: "https://example.com/files/skill-pack.zip?download=1".to_string(),
            }
        );
    }

    #[test]
    fn rejects_unsupported_url() {
        let error = parse_download_source("https://example.com/skill-page").unwrap_err();
        assert!(error.contains("仅支持"));
    }

    #[test]
    fn prioritizes_preferred_subpath() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("skills-manager-test-{timestamp}"));
        let extract_dir = temp_root.join("extract");
        let repo_root = extract_dir.join("repo-main");
        let preferred = repo_root.join("skills/docx");
        let fallback = repo_root.join("skills/other-skill");

        fs::create_dir_all(&preferred).unwrap();
        fs::create_dir_all(&fallback).unwrap();
        fs::write(preferred.join("SKILL.md"), "# docx").unwrap();
        fs::write(fallback.join("SKILL.md"), "# other").unwrap();

        let selected = find_skill_root(
            &extract_dir,
            "other-skill",
            Some(PathBuf::from("skills/docx").as_path()),
        )
        .unwrap();
        assert_eq!(selected, preferred);

        let _ = fs::remove_dir_all(temp_root);
    }
}
