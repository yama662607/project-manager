use super::models::{AppConfig, Project};
use anyhow::{Context, Result};
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

/// 設定ファイルのパスを取得
pub fn config_path() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".project-manager.json"))
        .ok_or_else(|| anyhow::anyhow!("ホームディレクトリが見つかりません"))
}

/// 設定ファイルを保存（atomic write）
pub fn save_config_file(config: &AppConfig) -> Result<()> {
    let path = config_path()?;
    let tmp_path = path.with_extension("json.tmp");

    // JSONにシリアライズ
    let data = serde_json::to_vec_pretty(config)
        .context("設定ファイルのシリアライズに失敗しました")?;

    // 一時ファイルに書き込み
    let mut file = File::create(&tmp_path)
        .with_context(|| format!("一時ファイルの作成に失敗: {}", tmp_path.display()))?;
    file.write_all(&data)
        .context("一時ファイルへの書き込みに失敗しました")?;
    file.sync_all()
        .context("一時ファイルの同期に失敗しました")?;

    // 一時ファイルを本ファイルにリネーム（atomic操作）
    fs::rename(&tmp_path, &path)
        .context("設定ファイルのリネームに失敗しました")?;

    Ok(())
}

/// 設定ファイルを読み込み（フォールバック付き）
pub fn load_config() -> AppConfig {
    if let Ok(path) = config_path() {
        if let Ok(data) = fs::read(&path) {
            if let Ok(config) = serde_json::from_slice::<AppConfig>(&data) {
                return config;
            }
        }
    }

    // フォールバック: 空の設定を返す
    AppConfig::default()
}

/// 指定されたパスが既に登録済みかチェック
pub fn is_project_registered(config: &AppConfig, path: &str) -> bool {
    config.projects.iter().any(|p| {
        p.path == path
            || fs::canonicalize(&p.path)
                .ok()
                .and_then(|p1| fs::canonicalize(path).ok().map(|p2| p1 == p2))
                .unwrap_or(false)
    })
}

/// プロジェクトを追加
pub fn add_project(config: &mut AppConfig, project: Project) -> Result<()> {
    config.projects.push(project);
    save_config_file(config)?;
    Ok(())
}

/// プロジェクトを更新
pub fn update_project(config: &mut AppConfig, index: usize, project: Project) -> Result<()> {
    if index < config.projects.len() {
        config.projects[index] = project;
        save_config_file(config)?;
        Ok(())
    } else {
        Err(anyhow::anyhow!("無効なインデックス: {}", index))
    }
}

/// プロジェクトを削除
pub fn delete_project(config: &mut AppConfig, index: usize) -> Result<()> {
    if index < config.projects.len() {
        config.projects.remove(index);
        save_config_file(config)?;
        Ok(())
    } else {
        Err(anyhow::anyhow!("無効なインデックス: {}", index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_path() {
        let path = config_path();
        assert!(path.is_ok());
        let path = path.unwrap();
        assert!(path.ends_with(".project-manager.json"));
    }

    #[test]
    fn test_is_project_registered() {
        let mut config = AppConfig::default();
        config.projects.push(Project::new(
            "Test".into(),
            "/tmp/test".into(),
            vec![],
            vec![],
            None,
        ));

        assert!(is_project_registered(&config, "/tmp/test"));
        assert!(!is_project_registered(&config, "/tmp/other"));
    }
}
