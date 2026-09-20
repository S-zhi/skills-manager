use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::{BTreeMap, HashSet};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static OPERATION: Mutex<()> = Mutex::new(());
static STATUS: Mutex<Option<SyncStatus>> = Mutex::new(None);

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncConfig {
    repository: String,
    branch: String,
    automatic: bool,
    last_tree: String,
    last_success: Option<u64>,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    busy: bool,
    message: String,
    error: Option<String>,
    files: usize,
}

#[derive(Serialize)]
pub struct SyncView {
    config: SyncConfig,
    status: SyncStatus,
}

#[derive(Deserialize)]
pub struct BindRequest {
    repository: String,
    branch: String,
    automatic: bool,
}

fn root() -> Result<PathBuf, String> {
    Ok(dirs::home_dir()
        .ok_or("Cannot locate home directory")?
        .join("Skill Manager"))
}
fn config_path() -> Result<PathBuf, String> {
    Ok(root()?.join(".metadata/github-sync.json"))
}
fn read_config() -> Result<SyncConfig, String> {
    match fs::read(config_path()?) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "Invalid github-sync.json; automatic upload stopped".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SyncConfig::default()),
        Err(e) => Err(e.to_string()),
    }
}
fn write_config(config: &SyncConfig) -> Result<(), String> {
    let path = config_path()?;
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temp, &path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn validate_binding(repository: &str, branch: &str) -> Result<(), String> {
    let parts: Vec<_> = repository.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|p| {
            p.is_empty()
                || *p == "."
                || *p == ".."
                || p.starts_with('-')
                || !p
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
    {
        return Err("Repository must be owner/repository, for example alice/my-skills".into());
    }
    if branch.is_empty()
        || branch.len() > 200
        || branch.starts_with('-')
        || branch.contains("..")
        || branch
            .split('/')
            .any(|p| p.is_empty() || p.starts_with('.') || p.ends_with('.') || p.ends_with(".lock"))
        || !branch
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_./".contains(&c))
    {
        return Err("Invalid branch name".into());
    }
    Ok(())
}

fn excluded(path: &Path) -> bool {
    path.components().any(|part| {
        let name = part.as_os_str().to_string_lossy().to_lowercase();
        matches!(
            name.as_str(),
            ".git"
                | "node_modules"
                | "__pycache__"
                | ".venv"
                | "id_rsa"
                | "id_ed25519"
                | "credentials.json"
        ) || name.starts_with(".env")
            || [".pem", ".key", ".p12", ".pfx"]
                .iter()
                .any(|ext| name.ends_with(ext))
    })
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

// Fail closed on links/junctions and unreadable files, rather than uploading a partial snapshot.
fn snapshot(root: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    if !root.join("Skills").is_dir() {
        return Err("Managed Skills directory is missing; refusing to erase the backup".into());
    }
    let canonical = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    let mut total = 0usize;
    for relative in ["Skills", ".metadata/skill-packages.json"] {
        let source = root.join(relative);
        if !source.try_exists().map_err(|e| e.to_string())? {
            continue;
        }
        for entry in walkdir::WalkDir::new(&source)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !excluded(e.path().strip_prefix(root).unwrap_or(e.path())))
        {
            let entry = entry.map_err(|e| e.to_string())?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            if is_link(&metadata) {
                return Err(
                    "Sync stopped: symbolic links or junctions are not supported in managed Skills"
                        .into(),
                );
            }
            if !metadata.is_file() {
                continue;
            }
            if !fs::canonicalize(entry.path())
                .map_err(|e| e.to_string())?
                .starts_with(&canonical)
            {
                return Err("File is outside managed storage".into());
            }
            if metadata.len() > 5 * 1024 * 1024 {
                return Err("A file exceeds the 5 MiB sync limit".into());
            }
            let bytes = fs::read(entry.path()).map_err(|e| e.to_string())?;
            total += bytes.len();
            if total > 20 * 1024 * 1024 || files.len() >= 500 {
                return Err("Sync limit exceeded: 500 files / 20 MiB".into());
            }
            let relative = entry.path().strip_prefix(root).map_err(|e| e.to_string())?;
            let name = relative
                .to_str()
                .ok_or("Non-UTF8 filename cannot be uploaded")?
                .replace('\\', "/");
            files.push((name, bytes));
        }
    }
    if files.is_empty() {
        return Err("No managed files to upload; refusing to erase the remote backup".into());
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn token() -> Result<String, String> {
    let mut command = Command::new("gh");
    command
        .args(["auth", "token", "--hostname", "github.com"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|_| {
        "Install GitHub CLI, then run: gh auth login --hostname github.com".to_string()
    })?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("GitHub CLI authentication timed out".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(
            "Please run gh auth login --hostname github.com and grant repository write access"
                .into(),
        );
    }
    let token = String::from_utf8(output.stdout)
        .map_err(|_| "Invalid authentication response")?
        .trim()
        .to_string();
    if token.is_empty() {
        return Err("GitHub CLI has no stored login".into());
    }
    Ok(token)
}

fn blob_sha(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()));
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

struct Github {
    token: String,
    base: String,
    agent: ureq::Agent,
    deadline: Instant,
}
impl Github {
    fn new(config: &SyncConfig) -> Result<Self, String> {
        validate_binding(&config.repository, &config.branch)?;
        Ok(Self {
            token: token()?,
            base: format!("https://api.github.com/repos/{}", config.repository),
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(30))
                .redirects(0)
                .build(),
            deadline: Instant::now() + Duration::from_secs(600),
        })
    }
    fn call(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
        if Instant::now() > self.deadline {
            return Err("Sync exceeded its ten-minute limit; retry later".into());
        }
        let request = self
            .agent
            .request(method, &format!("{}{path}", self.base))
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28")
            .set("User-Agent", "Skill-Manager");
        let response = match body {
            Some(body) => request.send_json(body),
            None => request.call(),
        };
        match response {
            Ok(response) => response.into_json().map_err(|_| "Invalid GitHub response".into()),
            Err(ureq::Error::Status(code, _)) => Err(format!("GitHub HTTP {code}: check repository/branch, login and write permissions. 409/422 may indicate a concurrent edit or branch protection; no force push was used.")),
            Err(_) => Err("Cannot connect to GitHub (network error or timeout); retry later".into()),
        }
    }
}
fn field(value: &Value, name: &str) -> Result<String, String> {
    value[name]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("Missing GitHub field: {name}"))
}

fn check_remote(remote: Option<&str>, previous: &str, desired: &str) -> Result<(), String> {
    if remote == Some(desired) || remote.unwrap_or("") == previous {
        return Ok(());
    }
    Err("Remote SkillManager folder changed or belongs to another backup. Sync stopped to protect remote data; reconcile manually or bind a different repository/branch.".into())
}

fn upload(config: &mut SyncConfig) -> Result<usize, String> {
    let api = Github::new(config)?;
    let count = upload_snapshot(config, &root()?, &api)?;
    write_config(config)?;
    Ok(count)
}

fn upload_snapshot(config: &mut SyncConfig, root: &Path, api: &Github) -> Result<usize, String> {
    let files = snapshot(root)?;
    let reference_path = format!("/git/ref/heads/{}", config.branch);
    let head = api.call("GET", &reference_path, None)?;
    let parent = field(&head["object"], "sha")?;
    let commit = api.call("GET", &format!("/git/commits/{parent}"), None)?;
    let base_tree = field(&commit["tree"], "sha")?;
    let tree = api.call("GET", &format!("/git/trees/{base_tree}"), None)?;
    if tree["truncated"].as_bool() == Some(true) {
        return Err("Repository tree is too large".into());
    }
    let entries = tree["tree"].as_array().ok_or("Invalid GitHub tree")?;
    let remote = entries.iter().find(|entry| entry["path"] == "SkillManager");
    if remote.is_some_and(|entry| entry["type"] != "tree") {
        return Err("Remote SkillManager path is not a directory".into());
    }
    let remote_sha = remote.and_then(|entry| entry["sha"].as_str());
    let local_hashes: BTreeMap<_, _> = files
        .iter()
        .map(|(name, bytes)| (name.clone(), blob_sha(bytes)))
        .collect();
    let mut remote_hashes = BTreeMap::new();
    if let Some(sha) = remote_sha {
        let subtree = api.call("GET", &format!("/git/trees/{sha}?recursive=1"), None)?;
        if subtree["truncated"].as_bool() == Some(true) {
            return Err("Remote backup tree is too large".into());
        }
        for item in subtree["tree"].as_array().ok_or("Invalid backup tree")? {
            if item["type"] == "tree" {
                continue;
            }
            if item["type"] != "blob" || item["mode"] != "100644" {
                return Err(
                    "Remote backup contains unsupported file modes; manual review required".into(),
                );
            }
            remote_hashes.insert(field(item, "path")?, field(item, "sha")?);
        }
    }
    if local_hashes == remote_hashes {
        config.last_tree = remote_sha.unwrap_or("").to_string();
        config.last_success = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs(),
        );
        return Ok(files.len());
    }
    // Refuse unknown or remotely edited content BEFORE uploading any file data.
    check_remote(remote_sha, &config.last_tree, "")?;
    let known: HashSet<_> = remote_hashes.values().collect();
    let mut items = Vec::new();
    for (name, bytes) in &files {
        let sha = &local_hashes[name];
        if !known.contains(sha) {
            // Space content-generating requests to avoid GitHub secondary rate limits.
            std::thread::sleep(Duration::from_secs(1));
            let blob = api.call("POST", "/git/blobs", Some(json!({"content": base64::engine::general_purpose::STANDARD.encode(bytes), "encoding": "base64"})))?;
            if field(&blob, "sha")? != *sha {
                return Err("GitHub blob checksum mismatch".into());
            }
        }
        items.push(json!({"path": name, "mode": "100644", "type": "blob", "sha": sha}));
    }
    let desired = api.call("POST", "/git/trees", Some(json!({"tree": items})))?;
    let desired_sha = field(&desired, "sha")?;
    check_remote(remote_sha, &config.last_tree, &desired_sha)?;
    if snapshot(root)? != files {
        return Err("Local Skills changed during upload; no commit was published. Retry once changes finish.".into());
    }
    if remote_sha != Some(desired_sha.as_str()) {
        // Replacing this subtree also records local deletions, preserving all other root entries.
        let updated = api.call("POST", "/git/trees", Some(json!({"base_tree": base_tree, "tree": [{"path": "SkillManager", "mode": "040000", "type": "tree", "sha": desired_sha}]})))?;
        let new_commit = api.call("POST", "/git/commits", Some(json!({"message": "Sync managed Skills from Skill Manager", "tree": field(&updated, "sha")?, "parents": [parent]})))?;
        api.call(
            "PATCH",
            &format!("/git/refs/heads/{}", config.branch),
            Some(json!({"sha": field(&new_commit, "sha")?, "force": false})),
        )?;
    }
    config.last_tree = desired_sha;
    config.last_success = Some(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs(),
    );
    Ok(files.len())
}

#[tauri::command]
pub fn github_sync_status() -> Result<SyncView, String> {
    Ok(SyncView {
        config: read_config()?,
        status: STATUS
            .lock()
            .map_err(|_| "Sync state unavailable")?
            .clone()
            .unwrap_or_default(),
    })
}

#[tauri::command]
pub async fn test_github_sync(request: BindRequest) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = SyncConfig {
            repository: request
                .repository
                .trim()
                .trim_end_matches('/')
                .trim_start_matches("https://github.com/")
                .trim_end_matches(".git")
                .to_string(),
            branch: request.branch.trim().to_string(),
            ..Default::default()
        };
        let api = Github::new(&config)?;
        let repository = api.call("GET", "", None)?;
        if repository["permissions"]["push"].as_bool() != Some(true) {
            return Err("This GitHub account does not have repository write access".into());
        }
        api.call("GET", &format!("/git/ref/heads/{}", config.branch), None)?;
        Ok(json!({"private": repository["private"]}))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn save_github_sync(request: BindRequest) -> Result<SyncView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _lock = OPERATION
            .try_lock()
            .map_err(|_| "Synchronization is busy; try again when it finishes")?;
        let mut config = read_config()?;
        let repository = request
            .repository
            .trim()
            .trim_end_matches('/')
            .trim_start_matches("https://github.com/")
            .trim_end_matches(".git")
            .to_string();
        let branch = request.branch.trim().to_string();
        if repository.is_empty() {
            config = SyncConfig::default();
        } else {
            validate_binding(&repository, &branch)?;
            if config.repository != repository || config.branch != branch {
                config = SyncConfig {
                    repository,
                    branch,
                    ..Default::default()
                };
            }
            config.automatic = request.automatic;
        }
        write_config(&config)?;
        *STATUS.lock().map_err(|_| "Sync state unavailable")? = None;
        github_sync_status()
    })
    .await
    .map_err(|e| e.to_string())?
}

fn run_sync(automatic: bool) -> Result<SyncView, String> {
    let _lock = OPERATION
        .try_lock()
        .map_err(|_| "Synchronization is already running")?;
    let mut config = read_config()?;
    if automatic && !config.automatic {
        return github_sync_status();
    }
    if config.repository.is_empty() {
        return Err("Bind a GitHub repository first".into());
    }
    *STATUS.lock().map_err(|_| "Sync state unavailable")? = Some(SyncStatus {
        busy: true,
        message: "Uploading managed Skills…".into(),
        ..Default::default()
    });
    let result = upload(&mut config);
    *STATUS.lock().map_err(|_| "Sync state unavailable")? = Some(match result {
        Ok(files) => SyncStatus {
            message: "Backup is up to date".into(),
            files,
            ..Default::default()
        },
        Err(error) => SyncStatus {
            error: Some(error),
            ..Default::default()
        },
    });
    github_sync_status()
}

#[tauri::command]
pub async fn sync_github_now() -> Result<SyncView, String> {
    tauri::async_runtime::spawn_blocking(|| run_sync(false))
        .await
        .map_err(|e| e.to_string())?
}

pub fn start_worker() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(300));
        if let Err(error) = run_sync(true) {
            if let Ok(mut status) = STATUS.lock() {
                if !status.as_ref().is_some_and(|s| s.busy) {
                    *status = Some(SyncStatus {
                        error: Some(error),
                        ..Default::default()
                    });
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // Local HTTP fixtures exercise the actual GitHub request sequence without login or external writes.
    fn mock_api(
        responses: Vec<(u16, Value)>,
    ) -> (Github, std::thread::JoinHandle<Vec<(String, Value)>>) {
        use std::io::{BufRead, BufReader, Read};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (code, response) in responses {
                let deadline = Instant::now() + Duration::from_secs(10);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "Missing expected HTTP request");
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let first_line = line.trim().to_string();
                let mut length = 0;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                let body = if bytes.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_slice(&bytes).unwrap()
                };
                requests.push((first_line, body));
                let json = response.to_string();
                write!(socket, "HTTP/1.1 {code} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}", json.len()).unwrap();
            }
            requests
        });
        (
            Github {
                token: "test-only".into(),
                base,
                agent: ureq::AgentBuilder::new()
                    .timeout(Duration::from_secs(5))
                    .build(),
                deadline: Instant::now() + Duration::from_secs(30),
            },
            handle,
        )
    }

    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!("skill-sync-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("Skills/demo")).unwrap();
        fs::write(root.join("Skills/demo/SKILL.md"), "hello\n").unwrap();
        root
    }

    #[test]
    fn publishes_only_managed_subtree_and_never_force_pushes() {
        for status in [200, 422] {
            let root = fixture();
            let (api, server) = mock_api(vec![
                (200, json!({"object":{"sha":"parent"}})),
                (200, json!({"tree":{"sha":"base"}})),
                (
                    200,
                    json!({"tree":[{"path":"README.md","type":"blob","sha":"untouched"}]}),
                ),
                (201, json!({"sha":blob_sha(b"hello\n")})),
                (201, json!({"sha":"snapshot"})),
                (201, json!({"sha":"new-root"})),
                (201, json!({"sha":"new-commit"})),
                (status, json!({})),
            ]);
            let mut config = SyncConfig {
                branch: "main".into(),
                ..Default::default()
            };
            let result = upload_snapshot(&mut config, &root, &api);
            assert_eq!(result.is_ok(), status == 200);
            let requests = server.join().unwrap();
            assert!(requests[7].0.starts_with("PATCH /git/refs/heads/main "));
            assert_eq!(requests[7].1["force"], false);
            assert_eq!(requests[5].1["base_tree"], "base");
            assert_eq!(requests[5].1["tree"].as_array().unwrap().len(), 1);
            assert_eq!(requests[5].1["tree"][0]["path"], "SkillManager");
            assert_eq!(
                config.last_tree,
                if status == 200 { "snapshot" } else { "" }
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn unchanged_snapshot_only_reads_remote() {
        let root = fixture();
        let (api, server) = mock_api(vec![
            (200, json!({"object":{"sha":"parent"}})),
            (200, json!({"tree":{"sha":"base"}})),
            (
                200,
                json!({"tree":[{"path":"SkillManager","type":"tree","sha":"snapshot"}]}),
            ),
            (
                200,
                json!({"tree":[{"path":"Skills/demo/SKILL.md","type":"blob","mode":"100644","sha":blob_sha(b"hello\n") }]}),
            ),
        ]);
        let mut config = SyncConfig {
            branch: "main".into(),
            ..Default::default()
        };
        assert_eq!(upload_snapshot(&mut config, &root, &api).unwrap(), 1);
        assert!(server
            .join()
            .unwrap()
            .iter()
            .all(|(request, _)| request.starts_with("GET ")));
        assert_eq!(config.last_tree, "snapshot");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hashes_binary_files_as_git_blobs() {
        assert_eq!(
            blob_sha(b"hello\n"),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
    }

    #[test]
    fn snapshot_only_contains_managed_content() {
        let root = std::env::temp_dir().join(format!("skill-sync-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("Skills/demo")).unwrap();
        fs::create_dir_all(root.join(".metadata")).unwrap();
        fs::create_dir_all(root.join("Plugins")).unwrap();
        fs::write(root.join("Skills/demo/SKILL.md"), "test").unwrap();
        fs::write(root.join("Skills/demo/icon.png"), [0, 255, 1]).unwrap();
        fs::write(root.join("Skills/demo/.env"), "secret").unwrap();
        fs::write(root.join(".metadata/skill-packages.json"), "{}").unwrap();
        fs::write(root.join(".metadata/github-sync.json"), "private config").unwrap();
        fs::write(root.join("Plugins/test.txt"), "not a skill").unwrap();
        let files = snapshot(&root).unwrap();
        assert_eq!(files.len(), 3);
        assert!(files
            .iter()
            .any(|(name, data)| name == "Skills/demo/icon.png" && data == &[0, 255, 1]));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_skills_directory_does_not_erase_backup() {
        let root = std::env::temp_dir().join(format!("skill-sync-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join(".metadata")).unwrap();
        fs::write(root.join(".metadata/skill-packages.json"), "{}").unwrap();
        assert!(snapshot(&root).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn validates_github_repository_and_branch() {
        assert!(validate_binding("alice/skills", "main").is_ok());
        assert!(validate_binding("alice/skills", "backup/skills").is_ok());
        for repo in [
            "https://evil.test/a/b",
            "a/b/c",
            "a/..",
            "a/b?token=x",
            "a/",
        ] {
            assert!(validate_binding(repo, "main").is_err());
        }
        for branch in ["", "../main", "main?x=y", "-main", "main.lock", "a//b"] {
            assert!(validate_binding("alice/skills", branch).is_err());
        }
    }

    #[test]
    fn excludes_secrets_and_tool_state() {
        for path in [
            ".git/config",
            "one/.env",
            "one/.env.local",
            "one/token.pem",
            "one/node_modules/a.js",
            "one/id_rsa",
        ] {
            assert!(excluded(std::path::Path::new(path)), "{path}");
        }
        assert!(!excluded(std::path::Path::new("one/SKILL.md")));
        assert!(!excluded(std::path::Path::new("one/assets/icon.png")));
    }

    #[test]
    fn does_not_overwrite_unrecognized_remote_snapshot() {
        assert!(check_remote(None, "", "new").is_ok());
        assert!(check_remote(Some("old"), "old", "new").is_ok());
        assert!(check_remote(Some("same"), "", "same").is_ok());
        assert!(check_remote(Some("other"), "old", "new").is_err());
        assert!(check_remote(None, "old", "new").is_err());
    }
}
