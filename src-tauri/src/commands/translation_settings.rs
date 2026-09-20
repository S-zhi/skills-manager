use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::Duration,
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
    pub enabled: bool,
    pub provider: String,
    pub endpoint: String,
    pub region: String,
    pub source_language: String,
    pub target_language: String,
}

impl Default for BasicTranslationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
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
    pub enabled: bool,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub temperature: f64,
    pub preserve_structure: bool,
}

impl Default for AdvancedTranslationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
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
            schema_version: 2,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslationCacheEntry {
    schema_version: u32,
    source_hash: String,
    target_language: String,
    engine: String,
    content: String,
}

pub(crate) struct TranslationResult {
    pub content: String,
    pub status: &'static str,
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
    if endpoint.starts_with("https://") {
        return Ok(());
    }
    if allow_local_http
        && (endpoint.starts_with("http://localhost")
            || endpoint.starts_with("http://127.0.0.1")
            || endpoint.starts_with("http://[::1]"))
    {
        return Ok(());
    }
    Err("Endpoint is required and must use HTTPS; only local LibreTranslate may use HTTP".into())
}

fn sanitize_request(
    request: SaveTranslationSettingsRequest,
    current_revision: u64,
) -> Result<(TranslationSettingsStore, SessionKeys), String> {
    if request.revision != current_revision {
        return Err("Translation settings changed elsewhere; reload and try again".into());
    }
    if request.basic.enabled && request.advanced.enabled {
        return Err("Only one translation engine can be enabled at a time".into());
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
            schema_version: 2,
            revision: current_revision + 1,
            basic: request.basic,
            advanced: request.advanced,
        },
        keys,
    ))
}

fn session_key(kind: &str) -> Result<Option<String>, String> {
    let keys = SESSION_KEYS
        .lock()
        .map_err(|_| "Session key store is unavailable")?;
    Ok(match kind {
        "advanced" => keys.advanced.clone(),
        "basic" => keys.basic.clone(),
        _ => None,
    })
}

fn cache_root(home: &Path) -> PathBuf {
    home.join("Skill Manager/.metadata/translations")
}

fn source_hash(content: &str, target_language: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(content.as_bytes());
    hasher.update([0]);
    hasher.update(target_language.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn read_cached_translation(
    home: &Path,
    content: &str,
    target_language: &str,
) -> Result<Option<String>, String> {
    let hash = source_hash(content, target_language);
    let path = cache_root(home).join(format!("{hash}.json"));
    match fs::read(path) {
        Ok(bytes) => {
            let Ok(entry) = serde_json::from_slice::<TranslationCacheEntry>(&bytes) else {
                return Ok(None);
            };
            if entry.source_hash == hash && entry.target_language == target_language {
                Ok(Some(entry.content))
            } else {
                Ok(None)
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn write_cached_translation(
    home: &Path,
    source: &str,
    target_language: &str,
    engine: &str,
    content: &str,
) -> Result<(), String> {
    let hash = source_hash(source, target_language);
    let root = cache_root(home);
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let path = root.join(format!("{hash}.json"));
    let entry = TranslationCacheEntry {
        schema_version: 1,
        source_hash: hash,
        target_language: target_language.to_string(),
        engine: engine.to_string(),
        content: content.to_string(),
    };
    write_json_atomically(&path, &entry)
}

fn write_json_atomically<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| error.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?)
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

fn response_json(response: ureq::Response) -> Result<Value, String> {
    response
        .into_json::<Value>()
        .map_err(|error| format!("Invalid translation response: {error}"))
}

fn send_json(request: ureq::Request, body: Value) -> Result<Value, String> {
    response_json(request.send_json(body).map_err(|error| match error {
        ureq::Error::Status(code, _) => format!("Translation service returned HTTP {code}"),
        _ => "Translation request failed; check the network and service address".to_string(),
    })?)
}

fn json_text<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| "Translation service returned no translated text".to_string())
}

fn translation_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(90))
        .timeout_write(Duration::from_secs(30))
        .build()
}

fn translate_with_basic(
    config: &BasicTranslationConfig,
    api_key: Option<&str>,
    content: &str,
    target_language: &str,
) -> Result<String, String> {
    let agent = translation_agent();
    let endpoint = config.endpoint.trim_end_matches('/');
    match config.provider.as_str() {
        "azure" => {
            let key = api_key.ok_or("The enabled translation API requires an API key")?;
            let mut url = format!(
                "{endpoint}/translate?api-version=3.0&to={}",
                urlencoding::encode(target_language)
            );
            if config.source_language != "auto" {
                url.push_str("&from=");
                url.push_str(&urlencoding::encode(&config.source_language));
            }
            let mut request = agent
                .post(&url)
                .set("Ocp-Apim-Subscription-Key", key)
                .set("Content-Type", "application/json");
            if !config.region.trim().is_empty() {
                request = request.set("Ocp-Apim-Subscription-Region", config.region.trim());
            }
            let value = send_json(request, json!([{ "Text": content }]))?;
            Ok(json_text(&value, "/0/translations/0/text")?.to_string())
        }
        "deepl" => {
            let key = api_key.ok_or("The enabled translation API requires an API key")?;
            let value = response_json(
                agent
                    .post(&format!("{endpoint}/v2/translate"))
                    .set("Authorization", &format!("DeepL-Auth-Key {key}"))
                    .send_form(&[("text", content), ("target_lang", target_language)])
                    .map_err(|error| format!("Translation request failed: {error}"))?,
            )?;
            Ok(json_text(&value, "/translations/0/text")?.to_string())
        }
        "google" => {
            let key = api_key.ok_or("The enabled translation API requires an API key")?;
            let url = format!("{endpoint}?key={}", urlencoding::encode(key));
            let value = send_json(
                agent.post(&url),
                json!({ "q": content, "target": target_language, "format": "text" }),
            )?;
            Ok(json_text(&value, "/data/translations/0/translatedText")?.to_string())
        }
        "mymemory" => {
            let source = if config.source_language == "auto" {
                "en"
            } else {
                config.source_language.as_str()
            };
            let url = format!(
                "{endpoint}/get?q={}&langpair={}%7C{}",
                urlencoding::encode(content),
                urlencoding::encode(source),
                urlencoding::encode(target_language)
            );
            let value = response_json(
                agent
                    .get(&url)
                    .call()
                    .map_err(|error| format!("Translation request failed: {error}"))?,
            )?;
            Ok(json_text(&value, "/responseData/translatedText")?.to_string())
        }
        "libretranslate" => {
            let mut body = json!({
                "q": content,
                "source": config.source_language,
                "target": target_language,
                "format": "text"
            });
            if let Some(key) = api_key {
                body["api_key"] = Value::String(key.to_string());
            }
            let value = send_json(agent.post(&format!("{endpoint}/translate")), body)?;
            Ok(json_text(&value, "/translatedText")?.to_string())
        }
        _ => Err("Unsupported basic translation provider".into()),
    }
}

fn clean_llm_translation(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with("```markdown") && trimmed.ends_with("```") {
        return trimmed[11..trimmed.len() - 3].trim().to_string();
    }
    if trimmed.starts_with("```") && trimmed.ends_with("```") {
        return trimmed[3..trimmed.len() - 3]
            .trim_start_matches(|character: char| character.is_ascii_alphabetic())
            .trim()
            .to_string();
    }
    trimmed.to_string()
}

fn translate_with_advanced(
    config: &AdvancedTranslationConfig,
    api_key: &str,
    content: &str,
    target_language: &str,
) -> Result<String, String> {
    let instruction = format!(
        "Translate the following Skill document into {target_language}. Return only the complete translated document. Preserve YAML keys, name, uuid, Markdown syntax, code blocks, commands, paths, URLs, placeholders, and technical identifiers exactly. Translate only human-readable prose."
    );
    let agent = translation_agent();
    let endpoint = config.base_url.trim_end_matches('/');
    let value = match config.provider.as_str() {
        "gemini" => {
            let url = format!(
                "{endpoint}/v1beta/models/{}:generateContent?key={}",
                urlencoding::encode(&config.model),
                urlencoding::encode(api_key)
            );
            send_json(
                agent.post(&url),
                json!({
                    "contents": [{ "parts": [{ "text": format!("{instruction}\n\n{content}") }] }],
                    "generationConfig": { "temperature": config.temperature }
                }),
            )?
        }
        "openai-compatible" => send_json(
            agent
                .post(&format!("{endpoint}/v1/chat/completions"))
                .set("Authorization", &format!("Bearer {api_key}")),
            json!({
                "model": config.model,
                "temperature": config.temperature,
                "messages": [
                    { "role": "system", "content": instruction },
                    { "role": "user", "content": content }
                ]
            }),
        )?,
        _ => return Err("Unsupported advanced model provider".into()),
    };
    let pointer = if config.provider == "gemini" {
        "/candidates/0/content/parts/0/text"
    } else {
        "/choices/0/message/content"
    };
    Ok(clean_llm_translation(json_text(&value, pointer)?))
}

pub(crate) fn translate_skill_preview(
    home: &Path,
    source: &str,
    target_language: &str,
) -> Result<TranslationResult, String> {
    check_text(target_language, "target language", 32, true)?;
    let normalized_language = target_language.to_ascii_lowercase();
    if !normalized_language.starts_with("zh") {
        return Ok(TranslationResult {
            content: source.to_string(),
            status: if normalized_language.starts_with("en") {
                "original"
            } else {
                "unavailable"
            },
        });
    }
    if let Some(content) = read_cached_translation(home, source, target_language)? {
        return Ok(TranslationResult {
            content,
            status: "cached",
        });
    }

    let settings = read_store_from(&config_path()?)?;
    // LLM remains the explicit first choice for compatibility with older/corrupt stores.
    let (engine, translated) = if settings.advanced.enabled {
        let key = session_key("advanced")?
            .ok_or("LLM translation is enabled, but its session API key is missing")?;
        (
            format!("llm:{}", settings.advanced.provider),
            translate_with_advanced(&settings.advanced, &key, source, target_language)?,
        )
    } else if settings.basic.enabled {
        let key = session_key("basic")?;
        (
            format!("dedicated:{}", settings.basic.provider),
            translate_with_basic(&settings.basic, key.as_deref(), source, target_language)?,
        )
    } else {
        return Ok(TranslationResult {
            content: source.to_string(),
            status: "unavailable",
        });
    };
    write_cached_translation(home, source, target_language, &engine, &translated)?;
    Ok(TranslationResult {
        content: translated,
        status: "translated",
    })
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
        assert_eq!(settings.schema_version, 2);
        assert_eq!(settings.basic.provider, "azure");
        assert_eq!(settings.advanced.provider, "gemini");
    }
    #[test]
    fn remote_http_endpoint_is_rejected() {
        assert!(validate_endpoint("http://api.example.com", false).is_err());
    }
    #[test]
    fn empty_endpoint_is_rejected() {
        assert!(validate_endpoint("", false).is_err());
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
    fn enabling_both_engines_is_rejected() {
        let mut request = SaveTranslationSettingsRequest::example_with_keys();
        request.basic.enabled = true;
        request.advanced.enabled = true;
        assert!(sanitize_request(request, 0).is_err());
    }
    #[test]
    fn translation_cache_is_keyed_by_source_and_language() {
        let directory = std::env::temp_dir().join(format!(
            "skill-manager-translation-cache-test-{}",
            uuid::Uuid::new_v4()
        ));
        write_cached_translation(&directory, "Hello", "zh-CN", "test", "你好").unwrap();
        assert_eq!(
            read_cached_translation(&directory, "Hello", "zh-CN").unwrap(),
            Some("你好".into())
        );
        assert_eq!(
            read_cached_translation(&directory, "Changed", "zh-CN").unwrap(),
            None
        );
        assert_eq!(
            read_cached_translation(&directory, "Hello", "ja-JP").unwrap(),
            None
        );
        fs::remove_dir_all(directory).expect("cleanup");
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
