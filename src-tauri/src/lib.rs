mod archive;
mod automation;
mod backup;
mod config;
mod core;
mod database;
mod filters;
mod models;
mod notify;
mod paths;
mod report;
mod scanner;

use backup::start_backup as run_backup;
use core::catalog;
use core::duplicates;
use core::metadata;
use core::operations;
use core::rules;
use core::similar;
use core::snapshots;
use models::{
    AppConfig, ApplyPlanResult, AutomationConfig, AutomationStatus, BackupItem, BackupJobRecord,
    BackupOptions, BackupSummary, CatalogQuery, CatalogScanRequest, CatalogScanResult,
    CreatePlanRequest, DuplicatePlanRequest, DuplicateScanRequest, DuplicateScanResult, FileRecord,
    MetadataReadResult, OperationHistoryItem, PlanPreview, RestorePlanRequest, RuleRecord,
    ScanResult, SimilarScanRequest, SimilarScanResult, SnapshotManifest, SnapshotRecord,
    SnapshotVerifyResult,
};
use tauri::{AppHandle, Manager};

#[tauri::command]
fn get_default_backup_items() -> Result<Vec<BackupItem>, String> {
    paths::get_default_backup_items()
}

#[tauri::command]
fn get_app_config() -> Result<AppConfig, String> {
    config::load_app_config()
}

#[tauri::command]
fn save_app_config(config: AppConfig) -> Result<(), String> {
    config::save_app_config(&config)
}

#[tauri::command]
fn automation_get_config() -> Result<AutomationConfig, String> {
    automation::load_config()
}

#[tauri::command]
fn automation_get_status() -> Result<AutomationStatus, String> {
    let config = automation::load_config()?;
    Ok(automation::status(&config))
}

#[tauri::command]
fn automation_save_config(
    app_handle: AppHandle,
    config: AutomationConfig,
) -> Result<AutomationConfig, String> {
    automation::save_and_apply(app_handle, config)
}

#[tauri::command]
fn automation_stop() -> Result<(), String> {
    automation::stop();
    Ok(())
}

#[tauri::command]
fn scan_backup_items(
    items: Vec<BackupItem>,
    target_root: String,
    options: BackupOptions,
) -> Result<ScanResult, String> {
    scanner::scan_backup_items(&items, std::path::Path::new(&target_root), &options)
}

#[tauri::command]
async fn catalog_scan(
    app_handle: AppHandle,
    request: CatalogScanRequest,
) -> Result<CatalogScanResult, String> {
    let task = core::tasks::create("scan");
    let task_for_worker = task.clone();
    let error_handle = app_handle.clone();
    let joined =
        tokio::task::spawn_blocking(move || catalog::scan(app_handle, request, task_for_worker))
            .await;
    let result = match joined {
        Ok(result) => result,
        Err(error) => Err(format!("Catalog 扫描任务异常终止: {error}")),
    };
    if let Err(error) = &result {
        core::tasks::emit_error(&error_handle, &task.id, "scan", error);
    }
    core::tasks::finish(&task);
    result
}

#[tauri::command]
fn catalog_query(request: CatalogQuery) -> Result<Vec<FileRecord>, String> {
    catalog::query(request)
}

#[tauri::command]
fn metadata_read(path: String) -> Result<MetadataReadResult, String> {
    metadata::read_result(std::path::Path::new(path.trim()))
}

#[tauri::command]
async fn duplicate_scan(
    app_handle: AppHandle,
    request: DuplicateScanRequest,
) -> Result<DuplicateScanResult, String> {
    let task = core::tasks::create("duplicate");
    let worker_task = task.clone();
    let error_handle = app_handle.clone();
    let joined =
        tokio::task::spawn_blocking(move || duplicates::scan(app_handle, request, worker_task))
            .await;
    let result = match joined {
        Ok(value) => value,
        Err(error) => Err(format!("重复项扫描任务异常终止: {error}")),
    };
    if let Err(error) = &result {
        core::tasks::emit_error(&error_handle, &task.id, "duplicate", error);
    }
    core::tasks::finish(&task);
    result
}

#[tauri::command]
fn duplicate_create_plan(request: DuplicatePlanRequest) -> Result<PlanPreview, String> {
    duplicates::create_plan(request)
}

#[tauri::command]
async fn similar_scan(
    app_handle: AppHandle,
    request: SimilarScanRequest,
) -> Result<SimilarScanResult, String> {
    let task = core::tasks::create("similar");
    let worker_task = task.clone();
    let error_handle = app_handle.clone();
    let joined =
        tokio::task::spawn_blocking(move || similar::scan(app_handle, request, worker_task)).await;
    let result = match joined {
        Ok(value) => value,
        Err(error) => Err(format!("相似图片分析任务异常终止: {error}")),
    };
    if let Err(error) = &result {
        core::tasks::emit_error(&error_handle, &task.id, "similar", error);
    }
    core::tasks::finish(&task);
    result
}

#[tauri::command]
fn similar_create_plan(request: DuplicatePlanRequest) -> Result<PlanPreview, String> {
    duplicates::create_plan(request)
}

#[tauri::command]
fn rules_list() -> Result<Vec<RuleRecord>, String> {
    rules::list()
}

#[tauri::command]
fn rules_save(rule: RuleRecord) -> Result<RuleRecord, String> {
    rules::save(rule)
}

#[tauri::command]
fn rules_delete(rule_id: String) -> Result<(), String> {
    rules::remove(rule_id)
}

#[tauri::command]
fn organizer_create_plan(request: CreatePlanRequest) -> Result<PlanPreview, String> {
    core::planner::create_plan(request)
}

#[tauri::command]
async fn organizer_apply_plan(
    app_handle: AppHandle,
    plan_id: String,
) -> Result<ApplyPlanResult, String> {
    operations::apply_plan(app_handle, plan_id).await
}

#[tauri::command]
fn backup_job_list() -> Result<Vec<BackupJobRecord>, String> {
    let connection = database::open_connection()?;
    database::list_backup_jobs(&connection)
}

#[tauri::command]
fn snapshot_list(limit: u32) -> Result<Vec<SnapshotRecord>, String> {
    snapshots::list(limit)
}

#[tauri::command]
fn snapshot_manifest(snapshot_id: String) -> Result<SnapshotManifest, String> {
    snapshots::manifest(snapshot_id)
}

#[tauri::command]
fn snapshot_prune(job_id: String, keep: u32) -> Result<u64, String> {
    snapshots::prune(&job_id, keep)
}

#[tauri::command]
async fn snapshot_verify(
    app_handle: AppHandle,
    snapshot_id: String,
    mode: String,
) -> Result<SnapshotVerifyResult, String> {
    snapshots::verify(app_handle, snapshot_id, mode).await
}

#[tauri::command]
fn restore_create_plan(request: RestorePlanRequest) -> Result<PlanPreview, String> {
    snapshots::restore_plan(request)
}

#[tauri::command]
fn operation_undo(operation_id: String) -> Result<OperationHistoryItem, String> {
    operations::undo(operation_id)
}

#[tauri::command]
fn history_list(limit: u32) -> Result<Vec<OperationHistoryItem>, String> {
    operations::history(limit)
}

#[tauri::command]
fn task_cancel(task_id: String) -> Result<(), String> {
    core::tasks::cancel(&task_id)
}

#[tauri::command]
async fn start_backup(
    app_handle: AppHandle,
    items: Vec<BackupItem>,
    target_root: String,
    options: BackupOptions,
) -> Result<BackupSummary, String> {
    run_backup(app_handle, items, target_root, options).await
}

#[tauri::command]
fn cancel_backup(task_id: Option<String>) -> Result<(), String> {
    if let Some(task_id) = task_id {
        core::tasks::cancel(&task_id)
    } else {
        backup::cancel_backup();
        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            if let Err(error) = automation::start_saved(app.handle().clone()) {
                eprintln!("自动化配置未启动: {error}");
            }
            #[cfg(desktop)]
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if automation::minimize_to_tray_enabled() {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_default_backup_items,
            get_app_config,
            save_app_config,
            automation_get_config,
            automation_get_status,
            automation_save_config,
            automation_stop,
            scan_backup_items,
            catalog_scan,
            catalog_query,
            metadata_read,
            duplicate_scan,
            duplicate_create_plan,
            similar_scan,
            similar_create_plan,
            rules_list,
            rules_save,
            rules_delete,
            organizer_create_plan,
            organizer_apply_plan,
            backup_job_list,
            snapshot_list,
            snapshot_manifest,
            snapshot_prune,
            snapshot_verify,
            restore_create_plan,
            operation_undo,
            history_list,
            task_cancel,
            start_backup,
            cancel_backup
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(desktop)]
fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let open = MenuItem::with_id(app, "open", "打开工作台", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("归栈")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    let _tray = builder.build(app)?;
    Ok(())
}
