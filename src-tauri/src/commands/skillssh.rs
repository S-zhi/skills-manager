use crate::types::{RemoteSkillView, RemoteSkillsViewResponse};
use serde::Deserialize;
use std::{io::Read, time::Duration};

const ENDPOINT: &str = "https://skills.sh/api/search";
const MAX_RESPONSE: u64 = 2 * 1024 * 1024;

#[derive(Deserialize)]
struct SearchEnvelope {
    #[serde(default)]
    skills: Vec<SearchSkill>,
}

#[derive(Deserialize)]
struct SearchSkill {
    id: String,
    name: String,
    #[serde(default)]
    installs: u64,
    source: String,
}

fn validate_request(query: &str, limit: u64, offset: u64) -> Result<(), String> {
    let query = query.trim();
    if query.chars().count() < 2
        || query.chars().count() > 200
        || query.chars().any(char::is_control)
    {
        return Err("skills.sh：请输入 2–200 个字符的搜索词 / Enter at least 2 characters".into());
    }
    if !(1..=50).contains(&limit) || offset != 0 {
        return Err(
            "skills.sh：匿名搜索不支持继续翻页 / Anonymous search pagination is unavailable".into(),
        );
    }
    Ok(())
}

fn valid_source(source: &str) -> bool {
    let parts: Vec<_> = source.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && *part != "."
                && *part != ".."
                && part
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || "-_.".contains(ch))
        })
}

fn parse_response(raw: &str, limit: u64) -> Result<RemoteSkillsViewResponse, String> {
    let envelope: SearchEnvelope =
        serde_json::from_str(raw).map_err(|_| "skills.sh 返回格式异常 / Invalid API response")?;
    let skills: Vec<_> = envelope
        .skills
        .into_iter()
        .filter(|item| valid_source(&item.source))
        .take(limit as usize)
        .map(|item| {
            let author = item
                .source
                .split('/')
                .next()
                .unwrap_or_default()
                .to_string();
            RemoteSkillView {
                id: format!("skillssh:{}", item.id),
                name: item.name,
                namespace: String::new(),
                source_url: format!("https://github.com/{}", item.source),
                detail_url: format!("https://skills.sh/{}", item.id),
                description: format!("Skill published from {}", item.source),
                description_zh: String::new(),
                author,
                installs: item.installs,
                stars: 0,
                market_id: "skillssh".into(),
                market_label: "skills.sh".into(),
            }
        })
        .collect();
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
        .query("limit", &limit.to_string())
        .set("Accept", "application/json")
        .set("User-Agent", "Skill-Manager/0.4.1")
        .call()
        .map_err(|error| match error {
            ureq::Error::Status(429, _) => {
                "skills.sh：请求过于频繁，请稍后重试 / Rate limit exceeded".to_string()
            }
            ureq::Error::Status(code, _) => {
                format!("skills.sh 服务返回 HTTP {code} / Service error")
            }
            _ => "skills.sh：连接失败或超时 / Connection failed or timed out".to_string(),
        })?;
    let mut body = String::new();
    response
        .into_reader()
        .take(MAX_RESPONSE + 1)
        .read_to_string(&mut body)
        .map_err(|_| "skills.sh：无法读取响应 / Cannot read response")?;
    if body.len() as u64 > MAX_RESPONSE {
        return Err("skills.sh 响应过大 / Response too large".into());
    }
    parse_response(&body, limit)
}

#[tauri::command]
pub async fn search_skillssh(
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
    fn maps_anonymous_compatibility_results() {
        let raw = r#"{"skills":[{"id":"vercel-labs/agent-skills/react","name":"react","installs":123,"source":"vercel-labs/agent-skills"}]}"#;
        let result = parse_response(raw, 20).unwrap();
        assert_eq!(
            result.skills[0].source_url,
            "https://github.com/vercel-labs/agent-skills"
        );
        assert_eq!(
            result.skills[0].detail_url,
            "https://skills.sh/vercel-labs/agent-skills/react"
        );
        assert_eq!(result.skills[0].installs, 123);
    }

    #[test]
    fn rejects_untrusted_sources() {
        let raw = r#"{"skills":[{"id":"evil","name":"evil","source":"../evil"}]}"#;
        assert!(parse_response(raw, 20).unwrap().skills.is_empty());
    }
}
