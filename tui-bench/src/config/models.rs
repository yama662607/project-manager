use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub open_paths: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub last_opened_at: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutConfig {
    #[serde(default)]
    pub modifiers: Vec<String>,
    #[serde(default = "default_shortcut_key")]
    pub key: String,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            modifiers: vec!["control".into()],
            key: "m".into(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub shortcut: ShortcutConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            projects: Vec::new(),
            shortcut: ShortcutConfig::default(),
        }
    }
}

fn default_language() -> String {
    "Project".into()
}

fn default_shortcut_key() -> String {
    "m".into()
}

impl Project {
    pub fn new(
        name: String,
        path: String,
        aliases: Vec<String>,
        tags: Vec<String>,
        language: Option<String>,
    ) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let id = format!("manual-{}", timestamp);
        Self {
            id,
            name,
            path,
            open_paths: Vec::new(),
            aliases,
            tags,
            language: language.unwrap_or_else(default_language),
            last_opened_at: String::new(),
        }
    }

    pub fn with_language(mut self, language: String) -> Self {
        self.language = language;
        self
    }

    /// 編集用: 既存のIDを保持してフィールドを更新
    pub fn update_from(
        &self,
        name: String,
        path: String,
        aliases: Vec<String>,
        tags: Vec<String>,
        language: Option<String>,
    ) -> Self {
        Self {
            id: self.id.clone(),
            name,
            path,
            open_paths: self.open_paths.clone(),
            aliases,
            tags,
            language: language.unwrap_or_else(|| self.language.clone()),
            last_opened_at: self.last_opened_at.clone(),
        }
    }

    pub fn display_name(&self) -> String {
        if self.aliases.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.aliases.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_new() {
        let project = Project::new(
            "Test Project".into(),
            "/path/to/project".into(),
            vec!["test".into()],
            vec!["dev".into()],
            None,
        );
        assert!(project.id.starts_with("manual-"));
        assert_eq!(project.name, "Test Project");
        assert_eq!(project.path, "/path/to/project");
        assert_eq!(project.aliases, vec!["test"]);
        assert_eq!(project.tags, vec!["dev"]);
        assert_eq!(project.language, "Project");
    }

    #[test]
    fn test_display_name() {
        let project = Project::new(
            "Test".into(),
            "/path".into(),
            vec!["t".into()],
            vec![],
            None,
        );
        assert_eq!(project.display_name(), "Test (t)");
    }
}
