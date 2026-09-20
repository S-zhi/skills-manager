use crate::types::RemoteSkillView;
use serde::{Deserialize, Serialize};
use std::{io::Read, time::Duration};

const ENDPOINT: &str = "https://skillsmp.com/api/v1/skills/search";
const MAX_RESPONSE: u64 = 4 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsMpResponse {
    pub skills: Vec<RemoteSkillView>,
    pub total: u64,
    pub limit: u64,
    pub offset: u64,
    pub has_next: bool,
    pub daily_remaining: Option<u64>,
}

#[derive(Deserialize)]
struct Envelope {
    success: bool,
    data: Option<SearchData>,
}
#[derive(Deserialize)]
struct SearchData {
    skills: Vec<OnlineSkill>,
    pagination: Pagination,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pagination {
    page: u64,
    limit: u64,
    total: u64,
    has_next: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OnlineSkill {
    id: String,
    name: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    github_url: String,
    #[serde(default)]
    stars: u64,
}

fn validate_request(query: &str, limit: u64, offset: u64) -> Result<(), String> {
    let query = query.trim();
    if query.is_empty()
        || query.chars().count() > 200
        || query.contains('*')
        || query.chars().any(char::is_control)
    {
        return Err("SkillsMP：请输入 1–200 个字符的搜索词，不支持通配符 * / Enter a keyword, not a wildcard".into());
    }
    if !(1..=50).contains(&limit) || offset % limit != 0 || offset / limit >= 50 {
        return Err("SkillsMP：分页参数无效（最多 50 页）/ Invalid pagination".into());
    }
    Ok(())
}

fn supported_github_url(url: &str) -> bool {
    let Some(path) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    if path.contains(['?', '#', '\\']) {
        return false;
    }
    let parts: Vec<_> = path.trim_end_matches('/').split('/').collect();
    if !(parts.len() == 2 || (parts.len() >= 5 && parts[2] == "tree")) {
        return false;
    }
    parts.iter().all(|part| {
        !part.is_empty()
            && *part != "."
            && *part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    })
}

fn parse_response(
    raw: &str,
    limit: u64,
    offset: u64,
    daily_remaining: Option<u64>,
) -> Result<SkillsMpResponse, String> {
    let envelope: Envelope =
        serde_json::from_str(raw).map_err(|_| "SkillsMP 返回格式异常 / Invalid API response")?;
    if !envelope.success {
        return Err("SkillsMP 未能完成搜索，请稍后重试 / Search failed".into());
    }
    let data = envelope
        .data
        .ok_or("SkillsMP 响应缺少数据 / Missing response data")?;
    if data.pagination.page != offset / limit + 1
        || data.pagination.limit != limit
        || data.skills.len() > limit as usize
    {
        return Err("SkillsMP 分页响应不一致，请刷新重试 / Unexpected pagination".into());
    }
    let skills = data
        .skills
        .into_iter()
        .map(|item| RemoteSkillView {
            id: format!("skillsmp:{}", item.id),
            name: item.name,
            author: item.author,
            source_url: if supported_github_url(&item.github_url) {
                item.github_url
            } else {
                String::new()
            },
            description: item.description,
            description_zh: String::new(),
            namespace: String::new(),
            stars: item.stars,
            installs: 0,
            market_id: "skillsmp".into(),
            market_label: "SkillsMP · Online".into(),
        })
        .collect();
    Ok(SkillsMpResponse {
        skills,
        total: data.pagination.total,
        limit,
        offset,
        has_next: data.pagination.has_next && offset / limit + 1 < 50,
        daily_remaining,
    })
}

fn fetch_search(
    endpoint: &str,
    query: &str,
    limit: u64,
    offset: u64,
) -> Result<SkillsMpResponse, String> {
    validate_request(query, limit, offset)?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(25))
        .redirects(0)
        .build();
    let response = agent.get(endpoint).query("q", query.trim())
        .query("page", &(offset / limit + 1).to_string()).query("limit", &limit.to_string())
        .query("sortBy", "stars").set("Accept", "application/json")
        .set("User-Agent", "Skill-Manager/0.4.1")
        .call().map_err(|error| match error {
            ureq::Error::Status(429, _) => "SkillsMP：请求过于频繁或今日配额已用尽，请稍后重试 / Rate limit or daily quota exceeded".to_string(),
            ureq::Error::Status(401 | 403, _) => "SkillsMP：访问被拒绝，请检查平台访问政策 / Access denied".to_string(),
            ureq::Error::Status(code, _) => format!("SkillsMP 服务返回 HTTP {code}，请稍后重试 / Service error"),
            _ => "SkillsMP：连接失败或超时，请检查网络 / Connection failed or timed out".to_string(),
        })?;
    let remaining = response
        .header("X-RateLimit-Daily-Remaining")
        .and_then(|v| v.parse().ok());
    let mut body = String::new();
    response
        .into_reader()
        .take(MAX_RESPONSE + 1)
        .read_to_string(&mut body)
        .map_err(|_| "SkillsMP：无法读取响应 / Cannot read response")?;
    if body.len() as u64 > MAX_RESPONSE {
        return Err("SkillsMP 响应过大 / Response too large".into());
    }
    parse_response(&body, limit, offset, remaining)
}

#[tauri::command]
pub async fn search_skillsmp(
    query: String,
    limit: u64,
    offset: u64,
) -> Result<SkillsMpResponse, String> {
    tauri::async_runtime::spawn_blocking(move || fetch_search(ENDPOINT, &query, limit, offset))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    #[ignore = "Requires external network; consumes one anonymous SkillsMP request"]
    fn live_anonymous_search() {
        let result = fetch_search(ENDPOINT, "pdf", 2, 0).unwrap();
        assert!(!result.skills.is_empty());
        assert!(result
            .skills
            .iter()
            .all(|item| item.market_id == "skillsmp"));
        assert!(result
            .skills
            .iter()
            .any(|item| item.source_url.starts_with("https://github.com/")));
    }

    #[test]
    fn http_client_encodes_query_reads_quota_and_reports_rate_limit() {
        use std::io::{BufRead, BufReader, Write};
        for status in [200, 429] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}/search", listener.local_addr().unwrap());
            let response = fixture().to_string();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let request_line = line.clone();
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                }
                write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nX-RateLimit-Daily-Remaining: 48\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
                request_line
            });
            let result = fetch_search(&endpoint, "pdf & docs", 20, 0);
            let request = server.join().unwrap();
            assert!(request.contains("%26"));
            assert!(request.contains("page=1"));
            if status == 200 {
                assert_eq!(result.unwrap().daily_remaining, Some(48));
            } else {
                assert!(result.unwrap_err().contains("配额"));
            }
        }
    }

    fn fixture() -> serde_json::Value {
        json!({"success":true,"data":{"skills":[{"id":"demo","name":"PDF","author":"alice","description":"PDF tools","githubUrl":"https://github.com/alice/skills/tree/main/pdf","stars":12}],"pagination":{"page":1,"limit":20,"total":1,"hasNext":true,"totalIsExact":false}}})
    }
    #[test]
    fn maps_online_results_and_uses_has_next_instead_of_estimated_total() {
        let result = parse_response(&fixture().to_string(), 20, 0, Some(49)).unwrap();
        assert!(result.has_next);
        assert_eq!(result.skills[0].market_id, "skillsmp");
        assert_eq!(
            result.skills[0].source_url,
            "https://github.com/alice/skills/tree/main/pdf"
        );
        assert_eq!(result.skills[0].stars, 12);
        assert_eq!(result.daily_remaining, Some(49));
    }
    #[test]
    fn rejects_bad_queries_and_pagination_before_network_requests() {
        for q in ["", "   ", "*", "a*b"] {
            assert!(validate_request(q, 20, 0).is_err());
        }
        assert!(validate_request("中文 PDF", 20, 0).is_ok());
        assert!(validate_request("pdf", 0, 0).is_err());
        assert!(validate_request("pdf", 51, 0).is_err());
        assert!(validate_request("pdf", 20, 1).is_err());
        assert!(validate_request("pdf", 20, 1000).is_err());
    }
    #[test]
    fn disables_unsafe_download_links_without_hiding_search_results() {
        for url in [
            "https://github.com.evil.test/a/b",
            "file:///secret",
            "https://github.com/a/b/blob/main/SKILL.md",
            "https://github.com/a/b/tree/main/%2e%2e/private",
        ] {
            let mut value = fixture();
            value["data"]["skills"][0]["githubUrl"] = json!(url);
            let result = parse_response(&value.to_string(), 20, 0, None).unwrap();
            assert_eq!(result.skills.len(), 1);
            assert!(result.skills[0].source_url.is_empty());
        }
    }
    #[test]
    fn rejects_malformed_or_error_responses() {
        for raw in [
            "<html>blocked</html>",
            "{}",
            r#"{"success":false,"error":{"code":"DAILY_QUOTA_EXCEEDED"}}"#,
        ] {
            assert!(parse_response(raw, 20, 0, None).is_err());
        }
    }
}
