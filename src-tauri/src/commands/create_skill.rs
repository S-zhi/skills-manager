use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSkillRequest {
    name: String,
    description: String,
    body: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedSkill {
    uuid: String,
    path: String,
}

fn create(home: &Path, request: CreateSkillRequest) -> Result<CreatedSkill, String> {
    let name = request.name.trim();
    if name.is_empty()
        || name.len() > 64
        || name.starts_with('-')
        || name.ends_with('-')
        || name.contains("--")
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(
            "名称需为 1–64 位小写字母、数字或单连字符，连字符不能位于首尾 / Invalid skill name"
                .into(),
        );
    }
    let reserved = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    if reserved.contains(&name) {
        return Err("此名称是 Windows 保留名称 / Reserved Windows name".into());
    }
    let description = request.description.trim();
    if description.is_empty() || description.chars().count() > 1024 {
        return Err("描述需为 1–1024 个字符 / Invalid description".into());
    }
    if request.body.trim().is_empty() || request.body.len() > 1_048_576 {
        return Err("正文不能为空，且不能超过 1 MB / Invalid body".into());
    }
    let root = home.join("Skill Manager/Skills");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let target = root.join(name);
    // create_dir reserves the name without ever overwriting an existing skill.
    fs::create_dir(&target).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            "同名目录已存在，请更换名称 / Directory already exists".into()
        } else {
            e.to_string()
        }
    })?;
    let uuid = Uuid::new_v4().to_string();
    let description_lines = description
        .lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let document = format!(
        "---\nname: {name}\nuuid: {uuid}\ndescription: |-\n{description_lines}\n---\n\n{}\n",
        request.body.trim()
    );
    let file = target.join("SKILL.md");
    let result = (|| {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)
            .map_err(|e| e.to_string())?;
        output
            .write_all(document.as_bytes())
            .map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())
    })();
    if let Err(err) = result {
        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir(&target);
        return Err(err);
    }
    Ok(CreatedSkill {
        uuid,
        path: target.display().to_string(),
    })
}

#[tauri::command]
pub fn create_local_skill(request: CreateSkillRequest) -> Result<CreatedSkill, String> {
    create(
        &dirs::home_dir().ok_or("Cannot determine home directory")?,
        request,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::skill_identity::read_skill_uuid;

    #[test]
    fn creates_identity_and_preserves_existing_files() {
        let root = std::env::temp_dir().join(format!("create-skill-test-{}", Uuid::new_v4()));
        let request = |name: &str| CreateSkillRequest {
            name: name.into(),
            description: "Description: \"quoted\"\n---\nname: other".into(),
            body: "# Instructions\nDo useful work.".into(),
        };
        for invalid in ["../outside", "UPPER", "con", "bad--name", ""] {
            assert!(create(&root, request(invalid)).is_err());
        }
        assert!(!root.exists());
        let result = create(&root, request("sample-skill")).unwrap();
        let path = Path::new(&result.path);
        assert_eq!(read_skill_uuid(path).unwrap(), result.uuid);
        let original = fs::read_to_string(path.join("SKILL.md")).unwrap();
        assert!(original.contains("\n  ---\n  name: other\n---\n"));
        assert!(create(&root, request("sample-skill")).is_err());
        assert_eq!(fs::read_to_string(path.join("SKILL.md")).unwrap(), original);
        fs::remove_dir_all(root).unwrap();
    }
}
