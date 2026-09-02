use crate::models::{AppConfig, BackupItem};
use std::fs;
use std::path::Component;
use std::path::{Path, PathBuf};

const CONFIG_DIR_NAME: &str = "WindowsEasyBackup";
const CONFIG_FILE_NAME: &str = "app_config.json";
const LEGACY_CONFIG_FILE_NAME: &str = "config.json";

pub fn load_app_config() -> Result<AppConfig, String> {
    let config_path = config_file_path()?;
    if config_path.exists() {
        return load_app_config_from_path(&config_path);
    }
    let legacy_path = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(LEGACY_CONFIG_FILE_NAME);
    load_app_config_from_path(&legacy_path)
}

pub fn save_app_config(config: &AppConfig) -> Result<(), String> {
    let config_path = config_file_path()?;
    save_app_config_to_path(&config_path, config)
}

fn config_file_path() -> Result<PathBuf, String> {
    let appdata =
        std::env::var("APPDATA").map_err(|error| format!("无法获取 APPDATA 配置目录: {error}"))?;
    Ok(PathBuf::from(appdata)
        .join(CONFIG_DIR_NAME)
        .join(CONFIG_FILE_NAME))
}

fn load_app_config_from_path(path: &Path) -> Result<AppConfig, String> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let content = fs::read_to_string(path)
        .map_err(|error| format!("读取配置文件失败 {}: {error}", path.display()))?;
    let mut config: AppConfig = serde_json::from_str(&content)
        .map_err(|error| format!("解析配置文件失败 {}: {error}", path.display()))?;

    config.items = sanitize_items(config.items);
    Ok(config)
}

fn save_app_config_to_path(path: &Path, config: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建配置目录失败 {}: {error}", parent.display()))?;
    }

    let mut config = config.clone();
    config.items = sanitize_items(config.items);

    let content = serde_json::to_string_pretty(&config)
        .map_err(|error| format!("序列化配置失败: {error}"))?;
    fs::write(path, content)
        .map_err(|error| format!("写入配置文件失败 {}: {error}", path.display()))
}

fn sanitize_items(items: Vec<BackupItem>) -> Vec<BackupItem> {
    let mut seen = std::collections::HashSet::new();

    items
        .into_iter()
        .filter(|item| !item.source_path.trim().is_empty())
        .filter(|item| seen.insert(item.source_path.to_ascii_lowercase()))
        .map(|mut item| {
            item.label = if item.label.trim().is_empty() {
                fallback_label(&item.source_path)
            } else {
                item.label.trim().to_string()
            };
            item.target_name = if item.target_name.trim().is_empty() {
                item.label.clone()
            } else {
                item.target_name.trim().to_string()
            };
            item.file_count = None;
            item.total_size = None;
            item
        })
        .collect()
}

fn fallback_label(path: &str) -> String {
    let path = Path::new(path);
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            path.components()
                .rev()
                .find_map(|component| match component {
                    Component::Prefix(prefix) => {
                        Some(prefix.as_os_str().to_string_lossy().to_string())
                    }
                    _ => None,
                })
        })
        .unwrap_or_else(|| String::from("Custom Folder"))
}

#[cfg(test)]
mod tests {
    use super::{load_app_config_from_path, save_app_config_to_path};
    use crate::models::{AppConfig, ArchiveFormat, BackupItem, BackupOptions};

    #[test]
    fn round_trips_config_file() {
        let root =
            std::env::temp_dir().join(format!("windows-easy-backup-config-{}", std::process::id()));
        let config_path = root.join("config.json");
        let config = AppConfig {
            target_root: String::from(r"D:\Backup"),
            options: BackupOptions {
                enable_smart_exclude: false,
                custom_exclude_patterns: vec![String::from("dist"), String::from("*.log")],
                compress_after_backup: true,
                archive_format: ArchiveFormat::SevenZ,
                compression_level: 9,
                send_notification: false,
                ..BackupOptions::default()
            },
            items: vec![BackupItem {
                id: String::from("custom:d:/projects"),
                label: String::from("Projects"),
                source_path: String::from(r"D:\Projects"),
                target_name: String::from("Projects"),
                enabled: true,
                category: String::from("custom"),
                description: None,
                is_custom: true,
                file_count: Some(1),
                total_size: Some(2),
            }],
        };

        save_app_config_to_path(&config_path, &config).unwrap();
        let loaded = load_app_config_from_path(&config_path).unwrap();

        assert_eq!(loaded.target_root, config.target_root);
        assert_eq!(loaded.options, config.options);
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].label, "Projects");
        assert_eq!(loaded.items[0].file_count, None);

        let _ = std::fs::remove_dir_all(root);
    }
}
