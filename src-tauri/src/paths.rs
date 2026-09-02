use crate::models::BackupItem;
use chrono::{DateTime, Local};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

#[cfg(windows)]
use windows::core::PWSTR;
#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;
#[cfg(windows)]
use windows::Win32::System::Com::CoTaskMemFree;
#[cfg(windows)]
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music, FOLDERID_Pictures,
    FOLDERID_Videos, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetVolumePathNameW};

pub struct TargetProbe {
    pub accessible: bool,
    pub writable: bool,
    pub kind: String,
    pub warnings: Vec<String>,
}

pub fn build_backup_folder_name(now: DateTime<Local>) -> String {
    format!("WindowsBackup_{}", now.format("%Y-%m-%d_%H%M"))
}

pub fn is_unc_path(path: &Path) -> bool {
    path.to_string_lossy().starts_with("\\\\")
}

pub fn normalize_path(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        std::fs::canonicalize(path)
            .map_err(|error| format!("无法规范化路径 {}: {error}", path.display()))
    } else if let Some(parent) = path.parent() {
        if parent.exists() {
            let canonical_parent = std::fs::canonicalize(parent)
                .map_err(|error| format!("无法规范化父目录 {}: {error}", parent.display()))?;
            Ok(canonical_parent.join(path.file_name().unwrap_or_default()))
        } else {
            Ok(path.to_path_buf())
        }
    } else {
        Ok(path.to_path_buf())
    }
}

pub fn validate_selected_sources(items: &[BackupItem]) -> Result<(), String> {
    let mut seen = HashSet::new();

    for item in items.iter().filter(|item| item.enabled) {
        let lowered = item.source_path.to_ascii_lowercase();
        if !seen.insert(lowered) {
            return Err(format!("重复的源目录: {}", item.source_path));
        }
    }

    Ok(())
}

pub fn collect_source_warnings(items: &[BackupItem]) -> Result<Vec<String>, String> {
    let enabled: Vec<_> = items.iter().filter(|item| item.enabled).collect();
    let mut warnings = Vec::new();

    for (index, item) in enabled.iter().enumerate() {
        let item_path = normalize_path(Path::new(&item.source_path))?;

        for other in enabled.iter().skip(index + 1) {
            let other_path = normalize_path(Path::new(&other.source_path))?;
            if item_path.starts_with(&other_path) || other_path.starts_with(&item_path) {
                warnings.push(format!(
                    "目录可能重复备份：{} 与 {} 存在包含关系。",
                    item.source_path, other.source_path
                ));
            }
        }
    }

    Ok(warnings)
}

pub fn validate_target_root(items: &[BackupItem], target_root: &Path) -> Result<(), String> {
    validate_selected_sources(items)?;

    if !target_root.is_absolute() && !is_unc_path(target_root) {
        return Err("备份目标路径必须是绝对路径或有效的网络共享路径。".to_string());
    }

    let normalized_target = normalize_path(target_root)?;

    for item in items.iter().filter(|item| item.enabled) {
        let normalized_source = normalize_path(Path::new(&item.source_path))?;

        if normalized_target == normalized_source {
            return Err(format!("备份目标不能与源目录相同: {}", item.source_path));
        }

        if normalized_target.starts_with(&normalized_source) {
            return Err(format!("备份目标不能位于源目录内部: {}", item.source_path));
        }
    }

    Ok(())
}

pub fn protected_path_warning(target_root: &Path) -> Option<String> {
    let lowered = target_root.to_string_lossy().to_ascii_lowercase();
    if lowered.starts_with("c:\\windows") || lowered.starts_with("c:\\program files") {
        Some("目标位置位于系统保护目录，建议改用数据盘、移动硬盘或网络共享目录。".to_string())
    } else {
        None
    }
}

pub fn probe_target_root(target_root: &Path) -> Result<TargetProbe, String> {
    let mut warnings = Vec::new();
    let kind = if is_unc_path(target_root) {
        String::from("network")
    } else {
        String::from("local")
    };

    if is_unc_path(target_root) && !target_root.exists() {
        return Err(format!(
            "网络目标当前不可访问，请确认共享路径已连接并且有权限访问: {}",
            target_root.display()
        ));
    }

    let writable_root = if target_root.exists() {
        target_root.to_path_buf()
    } else {
        fs::create_dir_all(target_root)
            .map_err(|error| format!("无法创建目标目录 {}: {error}", target_root.display()))?;
        warnings.push(format!(
            "目标目录不存在，已自动创建: {}",
            target_root.display()
        ));
        target_root.to_path_buf()
    };

    let probe_file = writable_root.join(".web_target_probe.tmp");
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&probe_file)
        .map_err(|error| format!("目标位置当前不可写 {}: {error}", writable_root.display()))?;
    let _ = fs::remove_file(&probe_file);

    Ok(TargetProbe {
        accessible: true,
        writable: true,
        kind,
        warnings,
    })
}

pub fn create_backup_root(target_root: &Path, now: DateTime<Local>) -> Result<PathBuf, String> {
    let base_name = build_backup_folder_name(now);
    for suffix in 0..10_000u32 {
        let name = if suffix == 0 {
            base_name.clone()
        } else {
            format!("{base_name}_{suffix:04}")
        };
        let backup_root = target_root.join(name);
        match std::fs::create_dir(&backup_root) {
            Ok(()) => return Ok(backup_root),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "无法创建备份目录 {}: {error}",
                    backup_root.display()
                ));
            }
        }
    }
    Err(format!(
        "无法为备份生成唯一目录，目标位置冲突过多: {}",
        target_root.display()
    ))
}

#[cfg(windows)]
pub fn get_drive_free_space(path: &Path) -> Result<(String, u64), String> {
    use std::os::windows::ffi::OsStrExt;

    if is_unc_path(path) && !path.exists() {
        return Err("网络目标路径当前不可访问，无法获取剩余空间。".to_string());
    }

    let mut volume_buffer = vec![0u16; 260];
    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();

    let volume_ok = unsafe {
        GetVolumePathNameW(
            wide_path.as_ptr(),
            volume_buffer.as_mut_ptr(),
            volume_buffer.len() as u32,
        )
    };

    if volume_ok == 0 {
        return Err(format!("无法确定目标盘符或共享根路径: {}", path.display()));
    }

    let end = volume_buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(volume_buffer.len());
    let volume_path = String::from_utf16_lossy(&volume_buffer[..end]);
    let volume_wide: Vec<u16> = volume_path.encode_utf16().chain(Some(0)).collect();

    let mut available = 0u64;
    let mut total = 0u64;
    let mut free = 0u64;

    let free_ok =
        unsafe { GetDiskFreeSpaceExW(volume_wide.as_ptr(), &mut available, &mut total, &mut free) };

    if free_ok == 0 {
        return Err(format!("无法获取目标位置剩余空间: {}", path.display()));
    }

    Ok((volume_path.trim_end_matches('\\').to_string(), available))
}

#[cfg(not(windows))]
pub fn get_drive_free_space(_path: &Path) -> Result<(String, u64), String> {
    Ok(("".to_string(), 0))
}

#[cfg(windows)]
fn known_folder_path(id: &windows::core::GUID) -> Result<String, String> {
    unsafe {
        let raw: PWSTR = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, Some(HANDLE::default()))
            .map_err(|error| format!("获取系统目录失败: {error}"))?;
        let text = raw
            .to_string()
            .map_err(|error| format!("系统目录编码转换失败: {error}"))?;
        CoTaskMemFree(Some(raw.0 as _));
        Ok(text)
    }
}

#[cfg(windows)]
pub fn get_default_backup_items() -> Result<Vec<BackupItem>, String> {
    let mut items = vec![
        BackupItem {
            id: "desktop".to_string(),
            label: "桌面".to_string(),
            source_path: known_folder_path(&FOLDERID_Desktop)?,
            target_name: "桌面".to_string(),
            enabled: true,
            category: "system".to_string(),
            description: Some("桌面放置的文件、快捷方式与文档".to_string()),
            is_custom: false,
            file_count: None,
            total_size: None,
        },
        BackupItem {
            id: "downloads".to_string(),
            label: "下载".to_string(),
            source_path: known_folder_path(&FOLDERID_Downloads)?,
            target_name: "下载".to_string(),
            enabled: true,
            category: "system".to_string(),
            description: Some("浏览器与软件默认下载的文件".to_string()),
            is_custom: false,
            file_count: None,
            total_size: None,
        },
        BackupItem {
            id: "documents".to_string(),
            label: "文档".to_string(),
            source_path: known_folder_path(&FOLDERID_Documents)?,
            target_name: "文档".to_string(),
            enabled: true,
            category: "system".to_string(),
            description: Some("各类软件数据、个人文档与档案".to_string()),
            is_custom: false,
            file_count: None,
            total_size: None,
        },
        BackupItem {
            id: "pictures".to_string(),
            label: "图片".to_string(),
            source_path: known_folder_path(&FOLDERID_Pictures)?,
            target_name: "图片".to_string(),
            enabled: true,
            category: "system".to_string(),
            description: Some("相册、壁纸与截图".to_string()),
            is_custom: false,
            file_count: None,
            total_size: None,
        },
        BackupItem {
            id: "videos".to_string(),
            label: "视频".to_string(),
            source_path: known_folder_path(&FOLDERID_Videos)?,
            target_name: "视频".to_string(),
            enabled: true,
            category: "system".to_string(),
            description: Some("个人录屏与视频剪辑".to_string()),
            is_custom: false,
            file_count: None,
            total_size: None,
        },
        BackupItem {
            id: "music".to_string(),
            label: "音乐".to_string(),
            source_path: known_folder_path(&FOLDERID_Music)?,
            target_name: "音乐".to_string(),
            enabled: true,
            category: "system".to_string(),
            description: Some("下载与本地存储的音频文件".to_string()),
            is_custom: false,
            file_count: None,
            total_size: None,
        },
    ];

    let userprofile = std::env::var("USERPROFILE").unwrap_or_default();
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_default();

    if let Ok(docs_path) = known_folder_path(&FOLDERID_Documents) {
        let wechat_path = PathBuf::from(&docs_path).join("WeChat Files");
        if wechat_path.exists() {
            items.push(BackupItem {
                id: "preset_wechat".to_string(),
                label: "微信聊天文件".to_string(),
                source_path: wechat_path.to_string_lossy().to_string(),
                target_name: "应用数据/微信文件".to_string(),
                enabled: true,
                category: "app".to_string(),
                description: Some("聊天中接收的文档、表格与图片".to_string()),
                is_custom: false,
                file_count: None,
                total_size: None,
            });
        }

        let qq_path = PathBuf::from(&docs_path).join("Tencent Files");
        if qq_path.exists() {
            items.push(BackupItem {
                id: "preset_qq".to_string(),
                label: "QQ 接收文件".to_string(),
                source_path: qq_path.to_string_lossy().to_string(),
                target_name: "应用数据/QQ文件".to_string(),
                enabled: true,
                category: "app".to_string(),
                description: Some("QQ接收的离线与传输文件".to_string()),
                is_custom: false,
                file_count: None,
                total_size: None,
            });
        }
    }

    if !userprofile.is_empty() {
        let ssh_path = PathBuf::from(&userprofile).join(".ssh");
        if ssh_path.exists() {
            items.push(BackupItem {
                id: "preset_ssh".to_string(),
                label: "SSH 密钥与配置".to_string(),
                source_path: ssh_path.to_string_lossy().to_string(),
                target_name: "开发配置/SSH密钥".to_string(),
                enabled: true,
                category: "dev".to_string(),
                description: Some("id_rsa、公钥与 SSH config 配置文件".to_string()),
                is_custom: false,
                file_count: None,
                total_size: None,
            });
        }
    }

    if !appdata.is_empty() {
        let vscode_path = PathBuf::from(&appdata).join("Code").join("User");
        if vscode_path.exists() {
            items.push(BackupItem {
                id: "preset_vscode".to_string(),
                label: "VS Code 用户设置".to_string(),
                source_path: vscode_path.to_string_lossy().to_string(),
                target_name: "开发配置/VSCode设置".to_string(),
                enabled: true,
                category: "dev".to_string(),
                description: Some("settings.json、按键映射与自定义代码片段".to_string()),
                is_custom: false,
                file_count: None,
                total_size: None,
            });
        }
    }

    if !localappdata.is_empty() {
        let edge_path = PathBuf::from(&localappdata)
            .join("Microsoft")
            .join("Edge")
            .join("User Data")
            .join("Default");
        if edge_path.join("Bookmarks").exists() {
            items.push(BackupItem {
                id: "preset_edge".to_string(),
                label: "Edge 浏览器书签与配置".to_string(),
                source_path: edge_path.to_string_lossy().to_string(),
                target_name: "应用数据/Edge配置".to_string(),
                enabled: false,
                category: "app".to_string(),
                description: Some("书签、扩展设置与本地首选项".to_string()),
                is_custom: false,
                file_count: None,
                total_size: None,
            });
        }

        let chrome_path = PathBuf::from(&localappdata)
            .join("Google")
            .join("Chrome")
            .join("User Data")
            .join("Default");
        if chrome_path.join("Bookmarks").exists() {
            items.push(BackupItem {
                id: "preset_chrome".to_string(),
                label: "Chrome 浏览器书签与配置".to_string(),
                source_path: chrome_path.to_string_lossy().to_string(),
                target_name: "应用数据/Chrome配置".to_string(),
                enabled: false,
                category: "app".to_string(),
                description: Some("书签、扩展数据与用户配置文件".to_string()),
                is_custom: false,
                file_count: None,
                total_size: None,
            });
        }
    }

    Ok(items)
}

#[cfg(not(windows))]
pub fn get_default_backup_items() -> Result<Vec<BackupItem>, String> {
    Err("当前仅支持 Windows。".to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        build_backup_folder_name, collect_source_warnings, create_backup_root, is_unc_path,
        probe_target_root, validate_selected_sources, validate_target_root,
    };
    use crate::models::BackupItem;
    use chrono::{Local, TimeZone};
    use std::path::{Path, PathBuf};

    fn item(id: &str, source_path: &str) -> BackupItem {
        BackupItem {
            id: id.to_string(),
            label: id.to_string(),
            source_path: source_path.to_string(),
            target_name: id.to_string(),
            enabled: true,
            category: "custom".to_string(),
            description: None,
            is_custom: true,
            file_count: None,
            total_size: None,
        }
    }

    #[test]
    fn accepts_unc_target_path() {
        assert!(is_unc_path(Path::new(r"\\192.168.1.10\backup")));
    }

    #[test]
    fn rejects_duplicate_source_paths() {
        let items = vec![item("a", r"C:\Demo"), item("b", r"C:\Demo")];
        assert!(validate_selected_sources(&items).is_err());
    }

    #[test]
    fn rejects_target_inside_source() {
        let result = validate_target_root(
            &[item("desktop", r"C:\Users\Tester\Desktop")],
            &PathBuf::from(r"C:\Users\Tester\Desktop\Backup"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn creates_timestamped_backup_folder_name() {
        let time = Local.with_ymd_and_hms(2026, 5, 7, 14, 30, 0).unwrap();
        assert_eq!(
            build_backup_folder_name(time),
            "WindowsBackup_2026-05-07_1430"
        );
    }

    #[test]
    fn creates_unique_backup_roots_when_called_in_the_same_minute() {
        let root = std::env::temp_dir().join(format!(
            "windows-easy-backup-unique-root-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let time = Local.with_ymd_and_hms(2026, 5, 7, 14, 30, 0).unwrap();
        let first = create_backup_root(&root, time).unwrap();
        let second = create_backup_root(&root, time).unwrap();
        assert_ne!(first, second);
        assert!(first.is_dir());
        assert!(second.is_dir());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn warns_about_nested_sources() {
        let root =
            std::env::temp_dir().join(format!("windows-easy-backup-paths-{}", std::process::id()));
        let parent = root.join("Projects");
        let child = parent.join("Demo");
        std::fs::create_dir_all(&child).unwrap();

        let warnings = collect_source_warnings(&[
            item("parent", &parent.to_string_lossy()),
            item("child", &child.to_string_lossy()),
        ])
        .unwrap();
        assert!(!warnings.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn probes_local_target_root() {
        let root =
            std::env::temp_dir().join(format!("windows-easy-backup-probe-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();

        let probe = probe_target_root(&root).unwrap();

        assert!(probe.accessible);
        assert!(probe.writable);
        assert_eq!(probe.kind, "local");

        let _ = std::fs::remove_dir_all(&root);
    }
}
