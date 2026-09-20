use crate::types::{RemoteSkillView, RemoteSkillsViewResponse};
use serde::Deserialize;
use std::{io::Read, time::Duration};

const ENDPOINT: &str = "https://clawhub.ai/api/v1/search";
const MAX_RESPONSE: u64 = 4 * 1024 * 1024;

#[derive(Deserialize)]
struct SearchEnvelope {
    #[serde(default)]
    results: Vec<SearchItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchItem {
    id: String,
    slug: String,
    display_name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    source: String,
    #[serde(default)]
    canonical_url: String,
    #[serde(default)]
    owner_handle: String,
    owner: Option<Owner>,
    native: Option<NativeSkill>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Owner {
    #[serde(default)]
    handle: String,
    #[serde(default)]
    display_name: String,
}

#[derive(Deserialize)]
struct NativeSkill {
    skill: NativeSkillData,
}

#[derive(Deserialize)]
struct NativeSkillData {
    #[serde(default)]
    categories: Vec<String>,
    stats: Option<NativeStats>,
}

#[derive(Deserialize)]
struct NativeStats {
    #[serde(default)]
    stars: u64,
}

fn validate_request(query: &str, limit: u64, offset: u64) -> Result<(), String> {
    let query = query.trim();
    if query.is_empty() || query.chars().count() > 200 || query.chars().any(char::is_control) {
        return Err("ClawHub：请输入 1–200 个字符的搜索词 / Enter a search keyword".into());
    }
    if !(1..=50).contains(&limit) || offset != 0 {
        return Err("ClawHub：该搜索接口不支持继续翻页 / Search pagination is unavailable".into());
    }
    Ok(())
}

fn parse_response(raw: &str, limit: u64) -> Result<RemoteSkillsViewResponse, String> {
    let envelope: SearchEnvelope =
        serde_json::from_str(raw).map_err(|_| "ClawHub 返回格式异常 / Invalid API response")?;
    let mut skills = Vec::new();
    for item in envelope
        .results
        .into_iter()
        .filter(|item| item.source == "clawhub")
    {
        if skills.len() >= limit as usize {
            break;
        }
        let author = item
            .owner
            .as_ref()
            .map(|owner| {
                if owner.display_name.trim().is_empty() {
                    owner.handle.clone()
                } else {
                    owner.display_name.clone()
                }
            })
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| item.owner_handle.clone());
        let namespace = item
            .native
            .as_ref()
            .and_then(|native| native.skill.categories.first())
            .cloned()
            .unwrap_or_default();
        let stars = item
            .native
            .as_ref()
            .and_then(|native| native.skill.stats.as_ref())
            .map(|stats| stats.stars)
            .unwrap_or(0);
        let detail_url = if item.canonical_url.starts_with('/') {
            format!("https://clawhub.ai{}", item.canonical_url)
        } else {
            format!(
                "https://clawhub.ai/{}/skills/{}",
                item.owner_handle, item.slug
            )
        };
        skills.push(RemoteSkillView {
            id: format!("clawhub:{}", item.id),
            name: item.display_name,
            namespace,
            source_url: format!(
                "https://clawhub.ai/api/v1/download?slug={}",
                urlencoding::encode(&item.slug)
            ),
            detail_url,
            description: item.summary,
            description_zh: String::new(),
            author,
            installs: item.downloads,
            stars,
            market_id: "clawhub".into(),
            market_label: "ClawHub".into(),
        });
    }
    let total = skills.len() as u64;
    Ok(RemoteSkillsViewResponse {
        skills,
        total,
        limit,
        offset: 0,
        has_next: false,
        daily_remaining: None,
    })
}

fn fetch_search(
    endpoint: &str,
    query: &str,
    limit: u64,
    offset: u64,
) -> Result<RemoteSkillsViewResponse, String> {
    validate_request(query, limit, offset)?;
    let response = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(25))
        .redirects(0)
        .build()
        .get(endpoint)
        .query("q", query.trim())
        .set("Accept", "application/json")
        .set("User-Agent", "Skill-Manager/0.4.1")
        .call()
        .map_err(|error| match error {
            ureq::Error::Status(429, _) => {
                "ClawHub：请求过于频繁，请稍后重试 / Rate limit exceeded".to_string()
            }
            ureq::Error::Status(code, _) => format!("ClawHub 服务返回 HTTP {code} / Service error"),
            _ => "ClawHub：连接失败或超时 / Connection failed or timed out".to_string(),
        })?;
    let mut body = String::new();
    response
        .into_reader()
        .take(MAX_RESPONSE + 1)
        .read_to_string(&mut body)
        .map_err(|_| "ClawHub：无法读取响应 / Cannot read response")?;
    if body.len() as u64 > MAX_RESPONSE {
        return Err("ClawHub 响应过大 / Response too large".into());
    }
    parse_response(&body, limit)
}

#[tauri::command]
pub async fn search_clawhub(
    query: String,
    limit: u64,
    offset: u64,
) -> Result<RemoteSkillsViewResponse, String> {
    tauri::async_runtime::spawn_blocking(move || fetch_search(ENDPOINT, &query, limit, offset))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_only_native_clawhub_results() {
        let raw = r#"{"results":[{"id":"clawhub:one","slug":"react","displayName":"React","summary":"React workflow","downloads":42,"source":"clawhub","canonicalUrl":"/alice/skills/react","ownerHandle":"alice","owner":{"handle":"alice","displayName":"Alice"},"native":{"skill":{"categories":["development"],"stats":{"stars":3}}}},{"id":"skills-sh:x/y/z","slug":"z","displayName":"z","source":"skills-sh"}]}"#;
        let result = parse_response(raw, 20).unwrap();
        assert_eq!(result.skills.len(), 1);
        assert_eq!(result.skills[0].market_id, "clawhub");
        assert_eq!(
            result.skills[0].source_url,
            "https://clawhub.ai/api/v1/download?slug=react"
        );
        assert_eq!(
            result.skills[0].detail_url,
            "https://clawhub.ai/alice/skills/react"
        );
        assert_eq!(result.skills[0].installs, 42);
        assert_eq!(result.skills[0].stars, 3);
    }

    #[test]
    fn rejects_empty_queries_and_pagination() {
        assert!(validate_request("", 20, 0).is_err());
        assert!(validate_request("react", 20, 20).is_err());
        assert!(validate_request("react", 20, 0).is_ok());
    }
}
