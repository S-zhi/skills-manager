use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

static SESSION_KEYS: LazyLock<Mutex<SessionKeys>> =
    LazyLock::new(|| Mutex::new(SessionKeys::default()));

#[derive(Default)]
struct SessionKeys {
    basic: Option<String>,
    advanced: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BasicTranslationConfig {
    pub provider: String,
    pub endpoint: String,
    pub region: String,
    pub source_language: String,
    pub target_language: String,
}

impl Default for BasicTranslationConfig {
    fn default() -> Self {
        Self {
            provider: "azure".into(),
            endpoint: "https://api.cognitive.microsofttranslator.com".into(),
            region: String::new(),
            source_language: "auto".into(),
            target_language: "zh-CN".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AdvancedTranslationConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub temperature: f64,
    pub preserve_structure: bool,
}

impl Default for AdvancedTranslationConfig {
    fn default() -> Self {
        Self {
            provider: "gemini".into(),
            base_url: "https://generativelanguage.googleapis.com".into(),
            model: "gemini-2.5-flash".into(),
            temperature: 0.2,
            preserve_structure: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TranslationSettingsStore {
    pub schema_version: u32,
    pub revision: u64,
    pub basic: BasicTranslationConfig,
    pub advanced: AdvancedTranslationConfig,
}

impl Default for TranslationSettingsStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            basic: BasicTranslationConfig::default(),
            advanced: AdvancedTranslationConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSettingsView {
    #[serde(flatten)]
    pub settings: TranslationSettingsStore,
    pub basic_api_key_configured: bool,
    pub advanced_api_key_configured: bool,
    pub config_path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTranslationSettingsRequest {
    pub revision: u64,
    pub basic: BasicTranslationConfig,
    pub advanced: AdvancedTranslationConfig,
    pub basic_api_key: Option<String>,
    pub advanced_api_key: Option<String>,
}

fn config_path() -> Result<PathBuf, String> {
    Ok(dirs::home_dir()
        .ok_or("Cannot locate home directory")?
        .join("Skill Manager/.metadata/translation-settings.json"))
}

fn read_store_from(path: &Path) -> Result<TranslationSettingsStore, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "Invalid translation-settings.json".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(TranslationSettingsStore::default())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn write_store_to(path: &Path, store: &TranslationSettingsStore) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("Invalid settings path")?)
        .map_err(|error| error.to_string())?;
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| error.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(store).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        if path.exists() {
            fs::remove_file(path).map_err(|error| error.to_string())?;
        }
        fs::rename(&temp, path).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn check_text(value: &str, field: &str, max: usize, required: bool) -> Result<(), String> {
    if required && value.trim().is_empty() {
        return Err(format!("{field} is required"));
    }
    if value.len() > max || value.contains(['\r', '\n', '\0']) {
        return Err(format!("Invalid {field}"));
    }
    Ok(())
}

fn validate_endpoint(value: &str, allow_local_http: bool) -> Result<(), String> {
    let endpoint = value.trim().trim_end_matches('/');
    if endpoint.is_empty() || endpoint.starts_with("https://") {
        return Ok(());
    }
    if allow_local_http
        && (endpoint.starts_with("http://localhost")
            || endpoint.starts_with("http://127.0.0.1")
            || endpoint.starts_with("http://[::1]"))
    {
        return Ok(());
    }
    Err("Endpoint must use HTTPS; only local LibreTranslate may use HTTP".into())
}

fn sanitize_request(
    request: SaveTranslationSettingsRequest,
    current_revision: u64,
) -> Result<(TranslationSettingsStore, SessionKeys), String> {
    if request.revision != current_revision {
        return Err("Translation settings changed elsewhere; reload and try again".into());
    }
    if !["azure", "deepl", "google", "mymemory", "libretranslate"]
        .contains(&request.basic.provider.as_str())
    {
        return Err("Unsupported basic translation provider".into());
    }
    if !["gemini", "openai-compatible"].contains(&request.advanced.provider.as_str()) {
        return Err("Unsupported advanced model provider".into());
    }
    validate_endpoint(
        &request.basic.endpoint,
        request.basic.provider == "libretranslate",
    )?;
    validate_endpoint(&request.advanced.base_url, false)?;
    check_text(&request.basic.region, "region", 128, false)?;
    check_text(&request.basic.source_language, "source language", 32, true)?;
    check_text(&request.basic.target_language, "target language", 32, true)?;
    check_text(&request.advanced.model, "model", 160, true)?;
    if !(0.0..=1.0).contains(&request.advanced.temperature) {
        return Err("Temperature must be between 0 and 1".into());
    }
    for key in [&request.basic_api_key, &request.advanced_api_key] {
        if key
            .as_ref()
            .is_some_and(|value| value.len() > 4096 || value.contains('\0'))
        {
            return Err("Invalid API key".into());
        }
    }
    let keys = SessionKeys {
        basic: request.basic_api_key.filter(|key| !key.trim().is_empty()),
        advanced: request
            .advanced_api_key
            .filter(|key| !key.trim().is_empty()),
    };
    Ok((
        TranslationSettingsStore {
            schema_version: 1,
            revision: current_revision + 1,
            basic: request.basic,
            advanced: request.advanced,
        },
        keys,
    ))
}

fn view(store: TranslationSettingsStore, path: &Path) -> Result<TranslationSettingsView, String> {
    let keys = SESSION_KEYS
        .lock()
        .map_err(|_| "Session key store is unavailable")?;
    Ok(TranslationSettingsView {
        settings: store,
        basic_api_key_configured: keys.basic.is_some(),
        advanced_api_key_configured: keys.advanced.is_some(),
        config_path: path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn get_translation_settings() -> Result<TranslationSettingsView, String> {
    let path = config_path()?;
    view(read_store_from(&path)?, &path)
}

#[tauri::command]
pub fn save_translation_settings(
    request: SaveTranslationSettingsRequest,
) -> Result<TranslationSettingsView, String> {
    let path = config_path()?;
    let current = read_store_from(&path)?;
    let update_basic = request.basic_api_key.is_some();
    let update_advanced = request.advanced_api_key.is_some();
    let (store, new_keys) = sanitize_request(request, current.revision)?;
    write_store_to(&path, &store)?;
    let mut keys = SESSION_KEYS
        .lock()
        .map_err(|_| "Session key store is unavailable")?;
    if update_basic {
        keys.basic = new_keys.basic;
    }
    if update_advanced {
        keys.advanced = new_keys.advanced;
    }
    drop(keys);
    view(store, &path)
}

#[tauri::command]
pub fn clear_translation_session_key(kind: String) -> Result<TranslationSettingsView, String> {
    let mut keys = SESSION_KEYS
        .lock()
        .map_err(|_| "Session key store is unavailable")?;
    match kind.as_str() {
        "basic" => keys.basic = None,
        "advanced" => keys.advanced = None,
        _ => return Err("Unknown translation key kind".into()),
    }
    drop(keys);
    get_translation_settings()
}

#[cfg(test)]
impl SaveTranslationSettingsRequest {
    fn example_with_keys() -> Self {
        Self {
            revision: 0,
            basic: BasicTranslationConfig::default(),
            advanced: AdvancedTranslationConfig::default(),
            basic_api_key: Some("basic-secret".into()),
            advanced_api_key: Some("advanced-secret".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_settings_are_valid() {
        let settings = TranslationSettingsStore::default();
        assert_eq!(settings.schema_version, 1);
        assert_eq!(settings.basic.provider, "azure");
        assert_eq!(settings.advanced.provider, "gemini");
    }
    #[test]
    fn remote_http_endpoint_is_rejected() {
        assert!(validate_endpoint("http://api.example.com", false).is_err());
    }
    #[test]
    fn localhost_http_endpoint_is_allowed_for_libretranslate() {
        assert!(validate_endpoint("http://127.0.0.1:5000", true).is_ok());
        assert!(validate_endpoint("http://localhost:5000", true).is_ok());
    }
    #[test]
    fn api_keys_are_never_serialized() {
        let (stored, _) = sanitize_request(SaveTranslationSettingsRequest::example_with_keys(), 0)
            .expect("valid");
        let json = serde_json::to_string(&stored).expect("serialize");
        assert!(!json.contains("secret"));
        assert!(!json.to_lowercase().contains("apikey"));
    }
    #[test]
    fn stale_revision_is_rejected() {
        assert!(sanitize_request(SaveTranslationSettingsRequest::example_with_keys(), 2).is_err());
    }
    #[test]
    fn advanced_temperature_must_be_bounded() {
        let mut request = SaveTranslationSettingsRequest::example_with_keys();
        request.advanced.temperature = 1.5;
        assert!(sanitize_request(request, 0).is_err());
    }
    #[test]
    fn writing_settings_never_writes_keys() {
        let directory = std::env::temp_dir().join(format!(
            "skill-manager-translation-test-{}",
            uuid::Uuid::new_v4()
        ));
        let path = directory.join("translation-settings.json");
        let (stored, _) = sanitize_request(SaveTranslationSettingsRequest::example_with_keys(), 0)
            .expect("valid");
        write_store_to(&path, &stored).expect("write");
        assert!(!fs::read_to_string(path).expect("read").contains("secret"));
        fs::remove_dir_all(directory).expect("cleanup");
    }
}
