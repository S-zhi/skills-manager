use std::fs;
use std::path::Path;
use uuid::Uuid;

fn unquote(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        if (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'')
        {
            return trimmed[1..trimmed.len() - 1].trim();
        }
    }
    trimmed
}

pub fn normalize_skill_uuid(value: &str) -> Option<String> {
    Uuid::parse_str(unquote(value))
        .ok()
        .map(|value| value.hyphenated().to_string())
}

#[cfg(test)]
fn is_valid_skill_uuid(value: &str) -> bool {
    normalize_skill_uuid(value).is_some()
}

pub fn skill_uuid_from_content(content: &str) -> Option<String> {
    let normalized = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = normalized.lines();
    if lines.next().map(str::trim) != Some("---") {
        return None;
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if line.chars().next().is_some_and(char::is_whitespace) || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            if key.trim() == "uuid" {
                return normalize_skill_uuid(value);
            }
        }
    }
    None
}

pub fn skill_name_from_content(content: &str) -> Option<String> {
    let normalized = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = normalized.lines();
    if lines.next().map(str::trim) != Some("---") {
        return None;
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if line.chars().next().is_some_and(char::is_whitespace) || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            if key.trim() == "name" {
                let name = unquote(value).to_string();
                return (!name.is_empty()).then_some(name);
            }
        }
    }
    None
}

pub fn read_skill_uuid(skill_dir: &Path) -> Option<String> {
    let content = fs::read_to_string(skill_dir.join("SKILL.md")).ok()?;
    skill_uuid_from_content(&content)
}

pub fn read_skill_name(skill_dir: &Path) -> Option<String> {
    let content = fs::read_to_string(skill_dir.join("SKILL.md")).ok()?;
    skill_name_from_content(&content)
}

fn replace_or_insert_uuid(content: &str, uuid: &str) -> String {
    let has_bom = content.starts_with('\u{feff}');
    let body = content.strip_prefix('\u{feff}').unwrap_or(content);
    let newline = if body.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<&str> = body.lines().collect();

    if lines.first().map(|line| line.trim()) == Some("---") {
        if let Some(end) = lines
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(index, line)| (line.trim() == "---").then_some(index))
        {
            if let Some(index) = (1..end).find(|index| {
                let line = lines[*index];
                !line.chars().next().is_some_and(char::is_whitespace)
                    && line
                        .trim()
                        .split_once(':')
                        .is_some_and(|(key, _)| key.trim() == "uuid")
            }) {
                lines[index] = "";
                let mut output = Vec::with_capacity(lines.len());
                for (line_index, line) in lines.iter().enumerate() {
                    if line_index == index {
                        output.push(format!("uuid: {uuid}"));
                    } else {
                        output.push((*line).to_string());
                    }
                }
                let mut result = output.join(newline);
                if body.ends_with('\n') {
                    result.push_str(newline);
                }
                return if has_bom {
                    format!("\u{feff}{result}")
                } else {
                    result
                };
            }

            let mut output = Vec::with_capacity(lines.len() + 1);
            for (index, line) in lines.iter().enumerate() {
                if index == end {
                    output.push(format!("uuid: {uuid}"));
                }
                output.push((*line).to_string());
            }
            let mut result = output.join(newline);
            if body.ends_with('\n') {
                result.push_str(newline);
            }
            return if has_bom {
                format!("\u{feff}{result}")
            } else {
                result
            };
        }
    }

    let prefix = format!("---{newline}uuid: {uuid}{newline}---{newline}");
    if has_bom {
        format!("\u{feff}{prefix}{body}")
    } else {
        format!("{prefix}{body}")
    }
}

pub fn write_skill_uuid(skill_dir: &Path, uuid: &str) -> Result<String, String> {
    let normalized = normalize_skill_uuid(uuid).ok_or_else(|| "Invalid skill UUID".to_string())?;
    let skill_md = skill_dir.join("SKILL.md");
    let content = fs::read_to_string(&skill_md).map_err(|err| err.to_string())?;
    let updated = replace_or_insert_uuid(&content, &normalized);
    fs::write(&skill_md, updated).map_err(|err| err.to_string())?;
    Ok(normalized)
}

pub fn ensure_skill_uuid(skill_dir: &Path, preferred: Option<&str>) -> Result<String, String> {
    if let Some(existing) = read_skill_uuid(skill_dir) {
        return Ok(existing);
    }
    let uuid = preferred
        .and_then(normalize_skill_uuid)
        .unwrap_or_else(|| Uuid::new_v4().hyphenated().to_string());
    write_skill_uuid(skill_dir, &uuid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("skill-identity-{label}-{unique}"))
    }

    #[test]
    fn preserves_an_existing_valid_uuid() {
        let dir = test_dir("existing");
        fs::create_dir_all(&dir).expect("create dir");
        let expected = "550e8400-e29b-41d4-a716-446655440000";
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: sample\nuuid: {expected}\n---\n"),
        )
        .expect("write skill");
        assert_eq!(
            ensure_skill_uuid(&dir, None).expect("ensure uuid"),
            expected
        );
        assert_eq!(read_skill_uuid(&dir).as_deref(), Some(expected));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn adds_frontmatter_when_it_is_missing() {
        let dir = test_dir("frontmatter");
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(dir.join("SKILL.md"), "# Compatible skill\n").expect("write skill");
        let generated = ensure_skill_uuid(&dir, None).expect("ensure uuid");
        assert!(is_valid_skill_uuid(&generated));
        let updated = fs::read_to_string(dir.join("SKILL.md")).expect("read skill");
        assert!(updated.starts_with("---\nuuid: "));
        assert!(updated.contains("# Compatible skill"));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn explicit_write_replaces_a_remote_uuid_during_update() {
        let dir = test_dir("update");
        fs::create_dir_all(&dir).expect("create dir");
        let old_uuid = "550e8400-e29b-41d4-a716-446655440000";
        let remote_uuid = "c56a4180-65aa-42ec-a945-5fd21dec0538";
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: sample\nuuid: {remote_uuid}\n---\n"),
        )
        .expect("write skill");
        write_skill_uuid(&dir, old_uuid).expect("restore identity");
        assert_eq!(read_skill_uuid(&dir).as_deref(), Some(old_uuid));
        fs::remove_dir_all(dir).expect("cleanup");
    }
}
