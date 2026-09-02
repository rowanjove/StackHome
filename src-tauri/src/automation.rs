use crate::config;
use crate::core::{catalog, operations, planner, rules};
use crate::database;
use crate::models::{AutomationConfig, AutomationStatus, BackupItem, CreatePlanRequest};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use walkdir::WalkDir;

const AUTOMATION_DIR_NAME: &str = "WindowsEasyBackup";
const AUTOMATION_FILE_NAME: &str = "automation.json";
const DEFAULT_INTERVAL_MINUTES: u32 = 60;
const WATCH_POLL_INTERVAL: Duration = Duration::from_secs(2);

static CONTROL: OnceLock<Mutex<Option<Arc<AtomicBool>>>> = OnceLock::new();

fn control_slot() -> &'static Mutex<Option<Arc<AtomicBool>>> {
    CONTROL.get_or_init(|| Mutex::new(None))
}

pub fn default_config() -> AutomationConfig {
    AutomationConfig {
        scheduled_backup_interval_minutes: DEFAULT_INTERVAL_MINUTES,
        ..AutomationConfig::default()
    }
}

pub fn load_config() -> Result<AutomationConfig, String> {
    let path = automation_file_path()?;
    if !path.exists() {
        return Ok(default_config());
    }
    let content = fs::read_to_string(&path)
        .map_err(|error| format!("读取自动化配置失败 {}: {error}", path.display()))?;
    let mut config: AutomationConfig = serde_json::from_str(&content)
        .map_err(|error| format!("解析自动化配置失败 {}: {error}", path.display()))?;
    normalize(&mut config);
    Ok(config)
}

pub fn save_config(config: &AutomationConfig) -> Result<(), String> {
    let path = automation_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建自动化配置目录失败 {}: {error}", parent.display()))?;
    }
    let mut config = config.clone();
    normalize(&mut config);
    let content = serde_json::to_string_pretty(&config)
        .map_err(|error| format!("序列化自动化配置失败: {error}"))?;
    fs::write(&path, content)
        .map_err(|error| format!("写入自动化配置失败 {}: {error}", path.display()))
}

pub fn save_and_apply(
    app_handle: tauri::AppHandle,
    mut config: AutomationConfig,
) -> Result<AutomationConfig, String> {
    normalize(&mut config);
    validate(&config)?;
    save_config(&config)?;
    start(app_handle, config.clone())?;
    Ok(config)
}

pub fn start_saved(app_handle: tauri::AppHandle) -> Result<(), String> {
    let config = load_config()?;
    validate(&config)?;
    start(app_handle, config)
}

pub fn stop() {
    if let Ok(mut slot) = control_slot().lock() {
        if let Some(flag) = slot.take() {
            flag.store(true, Ordering::Release);
        }
    }
}

pub fn status(config: &AutomationConfig) -> AutomationStatus {
    AutomationStatus {
        watch_running: config.watch_enabled,
        scheduled_backup_running: config.scheduled_backup_enabled,
        watch_path: config.watch_path.clone(),
        next_scheduled_run_at: config.scheduled_backup_enabled.then(|| {
            database::now_millis()
                .saturating_add(i64::from(config.scheduled_backup_interval_minutes) * 60_000)
        }),
    }
}

pub fn minimize_to_tray_enabled() -> bool {
    load_config()
        .map(|config| config.minimize_to_tray)
        .unwrap_or(false)
}

fn start(app_handle: tauri::AppHandle, config: AutomationConfig) -> Result<(), String> {
    stop();
    let stop_flag = Arc::new(AtomicBool::new(false));
    if let Ok(mut slot) = control_slot().lock() {
        *slot = Some(stop_flag.clone());
    }

    let mut status = status(&config);
    let _ = app_handle.emit("automation-status", &status);

    if config.watch_enabled {
        let watch_handle = app_handle.clone();
        let watch_config = config.clone();
        let watch_stop = stop_flag.clone();
        thread::Builder::new()
            .name("windows-easy-backup-watch".to_string())
            .spawn(move || watch_loop(watch_handle, watch_config, watch_stop))
            .map_err(|error| format!("启动 Watch Folder 失败: {error}"))?;
    }

    if config.scheduled_backup_enabled {
        let schedule_handle = app_handle.clone();
        let schedule_config = config.clone();
        let schedule_stop = stop_flag.clone();
        thread::Builder::new()
            .name("windows-easy-backup-schedule".to_string())
            .spawn(move || scheduled_backup_loop(schedule_handle, schedule_config, schedule_stop))
            .map_err(|error| format!("启动定时备份失败: {error}"))?;
    }

    status.watch_running = config.watch_enabled;
    status.scheduled_backup_running = config.scheduled_backup_enabled;
    let _ = app_handle.emit("automation-status", &status);
    Ok(())
}

fn validate(config: &AutomationConfig) -> Result<(), String> {
    if config.watch_enabled {
        let watch_path = config
            .watch_path
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "启用 Watch Folder 前必须选择监控目录。".to_string())?;
        let watch_path = Path::new(watch_path);
        if !watch_path.is_absolute() || !watch_path.is_dir() {
            return Err("Watch Folder 必须是已存在的绝对目录。".to_string());
        }
        let destination = config
            .watch_destination_root
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Watch Folder 必须指定整理目标目录。".to_string())?;
        if !Path::new(destination).is_absolute() {
            return Err("Watch Folder 整理目标必须是绝对路径。".to_string());
        }
        if config.watch_auto_apply && config.watch_rule_id.as_deref().is_none_or(str::is_empty) {
            return Err("自动应用 Watch Folder 前必须指定一条规则。".to_string());
        }
        if let Some(rule_id) = config.watch_rule_id.as_deref() {
            if rules::find(rule_id)?.is_none() {
                return Err("Watch Folder 选择的规则不存在。".to_string());
            }
        }
    }
    if config.scheduled_backup_enabled
        && !(1..=7 * 24 * 60).contains(&config.scheduled_backup_interval_minutes)
    {
        return Err("定时备份间隔必须在 1 分钟至 7 天之间。".to_string());
    }
    Ok(())
}

fn normalize(config: &mut AutomationConfig) {
    if config.scheduled_backup_interval_minutes == 0 {
        config.scheduled_backup_interval_minutes = DEFAULT_INTERVAL_MINUTES;
    }
    config.watch_path = normalized_optional(config.watch_path.take());
    config.watch_destination_root = normalized_optional(config.watch_destination_root.take());
    config.watch_rule_id = normalized_optional(config.watch_rule_id.take());
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn automation_file_path() -> Result<PathBuf, String> {
    let appdata =
        std::env::var("APPDATA").map_err(|error| format!("无法获取 APPDATA 配置目录: {error}"))?;
    Ok(PathBuf::from(appdata)
        .join(AUTOMATION_DIR_NAME)
        .join(AUTOMATION_FILE_NAME))
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    size: u64,
    modified_millis: u128,
}

fn watch_loop(app_handle: tauri::AppHandle, config: AutomationConfig, stop_flag: Arc<AtomicBool>) {
    let root = PathBuf::from(config.watch_path.as_deref().unwrap_or_default());
    let mut known = snapshot_files(&root);
    let mut pending: HashMap<PathBuf, (Fingerprint, u8)> = HashMap::new();
    let _ = app_handle.emit(
        "automation-watch-status",
        serde_json::json!({"status":"running","path":root.display().to_string()}),
    );

    while !stop_flag.load(Ordering::Acquire) {
        let current = snapshot_files(&root);
        for (path, fingerprint) in &current {
            if known.contains_key(path) {
                pending.remove(path);
                continue;
            }
            let ready = match pending.get_mut(path) {
                Some((previous, stable_polls)) if *previous == *fingerprint => {
                    *stable_polls = stable_polls.saturating_add(1);
                    *stable_polls >= 2
                }
                _ => {
                    pending.insert(path.clone(), (*fingerprint, 1));
                    false
                }
            };
            if ready {
                pending.remove(path);
                process_watch_file(&app_handle, &config, path);
            }
        }
        known = current;
        sleep_or_stop(&stop_flag, WATCH_POLL_INTERVAL);
    }

    let _ = app_handle.emit(
        "automation-watch-status",
        serde_json::json!({"status":"stopped","path":root.display().to_string()}),
    );
}

fn snapshot_files(root: &Path) -> HashMap<PathBuf, Fingerprint> {
    if !root.is_dir() {
        return HashMap::new();
    }
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let modified_millis = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_millis())
                .unwrap_or_default();
            Some((
                entry.path().to_path_buf(),
                Fingerprint {
                    size: metadata.len(),
                    modified_millis,
                },
            ))
        })
        .collect()
}

fn process_watch_file(app_handle: &tauri::AppHandle, config: &AutomationConfig, path: &Path) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    let file_metadata = crate::core::metadata::read_file(path).ok();
    let file = catalog::file_record(path, &metadata, Some("watch".to_string()), file_metadata);
    if let Ok(connection) = database::open_connection() {
        let _ = database::upsert_file(&connection, &file);
        if let Some(file_metadata) = &file.metadata {
            if let Ok(json_data) = serde_json::to_string(file_metadata) {
                let _ = database::upsert_metadata(&connection, &file.id, "file", &json_data);
            }
        }
    }
    let _ = app_handle.emit(
        "catalog-updated",
        serde_json::json!({"rootPath":config.watch_path,"status":"watch-file","indexedFiles":1}),
    );
    let Some(destination_root) = config.watch_destination_root.as_deref() else {
        return;
    };
    let request = CreatePlanRequest {
        files: vec![file.clone()],
        destination_root: destination_root.to_string(),
        operation_type: "move".to_string(),
        rename_template: None,
        conflict_policy: "auto_number".to_string(),
        reason: "Watch Folder 自动规则".to_string(),
        rule_id: config.watch_rule_id.clone(),
    };
    match planner::create_plan(request) {
        Ok(plan) => {
            let _ = app_handle.emit(
                "automation-watch-plan",
                serde_json::json!({"path":path.display().to_string(),"plan":plan,"autoApply":config.watch_auto_apply}),
            );
            if config.watch_auto_apply && plan.status == "ready" && !plan.operations.is_empty() {
                let plan_id = plan.id.clone();
                let apply_handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    let result =
                        operations::apply_plan(apply_handle.clone(), plan_id.clone()).await;
                    let _ = apply_handle.emit(
                        "automation-watch-applied",
                        serde_json::json!({"planId":plan_id,"success":result.is_ok(),"error":result.err()}),
                    );
                });
            }
        }
        Err(error) => {
            let _ = app_handle.emit(
                "automation-error",
                serde_json::json!({"kind":"watch-plan","path":path.display().to_string(),"message":error}),
            );
        }
    }
}

fn scheduled_backup_loop(
    app_handle: tauri::AppHandle,
    config: AutomationConfig,
    stop_flag: Arc<AtomicBool>,
) {
    let interval = Duration::from_secs(u64::from(config.scheduled_backup_interval_minutes) * 60);
    let mut next_run = SystemTime::now() + interval;
    emit_schedule_status(&app_handle, &config, next_run);
    let running = Arc::new(AtomicBool::new(false));

    while !stop_flag.load(Ordering::Acquire) {
        if SystemTime::now() >= next_run {
            next_run = SystemTime::now() + interval;
            if !running.swap(true, Ordering::AcqRel) {
                let backup_handle = app_handle.clone();
                let backup_running = running.clone();
                let _ = backup_handle.emit(
                    "automation-scheduled-backup",
                    serde_json::json!({"status":"started"}),
                );
                tauri::async_runtime::spawn(async move {
                    let result = run_configured_backup(backup_handle.clone()).await;
                    let _ = backup_handle.emit(
                        "automation-scheduled-backup",
                        serde_json::json!({"status":if result.is_ok(){"completed"}else{"failed"},"error":result.err()}),
                    );
                    backup_running.store(false, Ordering::Release);
                });
            } else {
                let _ = app_handle.emit(
                    "automation-error",
                    serde_json::json!({"kind":"scheduled-backup","message":"上一次定时备份仍在运行，本次已跳过。"}),
                );
            }
            emit_schedule_status(&app_handle, &config, next_run);
        }
        sleep_or_stop(&stop_flag, Duration::from_secs(1));
    }
}

async fn run_configured_backup(app_handle: tauri::AppHandle) -> Result<(), String> {
    let app_config = config::load_app_config()?;
    let items: Vec<BackupItem> = app_config
        .items
        .into_iter()
        .filter(|item| item.enabled)
        .collect();
    if items.is_empty() {
        return Err("定时备份没有启用的来源目录。".to_string());
    }
    if app_config.target_root.trim().is_empty() {
        return Err("定时备份尚未设置目标目录。".to_string());
    }
    let _summary = crate::backup::start_backup(
        app_handle,
        items,
        app_config.target_root,
        app_config.options,
    )
    .await?;
    Ok(())
}

fn emit_schedule_status(
    app_handle: &tauri::AppHandle,
    config: &AutomationConfig,
    next_run: SystemTime,
) {
    let next_run_at = next_run
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|value| value.as_millis() as i64);
    let _ = app_handle.emit(
        "automation-status",
        AutomationStatus {
            watch_running: config.watch_enabled,
            scheduled_backup_running: true,
            watch_path: config.watch_path.clone(),
            next_scheduled_run_at: next_run_at,
        },
    );
}

fn sleep_or_stop(stop_flag: &AtomicBool, duration: Duration) {
    let slices = (duration.as_millis() / 200).max(1);
    for _ in 0..slices {
        if stop_flag.load(Ordering::Acquire) {
            return;
        }
        thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests {
    use super::{default_config, normalize};

    #[test]
    fn defaults_schedule_interval_for_old_config() {
        let mut config = default_config();
        config.scheduled_backup_interval_minutes = 0;
        normalize(&mut config);
        assert_eq!(config.scheduled_backup_interval_minutes, 60);
    }

    #[test]
    fn trims_empty_watch_values() {
        let mut config = default_config();
        config.watch_path = Some("  C:\\Watch  ".to_string());
        config.watch_destination_root = Some("  ".to_string());
        normalize(&mut config);
        assert_eq!(config.watch_path.as_deref(), Some("C:\\Watch"));
        assert_eq!(config.watch_destination_root, None);
    }
}
