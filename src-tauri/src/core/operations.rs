use crate::core::tasks::{self, TaskHandle};
use crate::database::{self, open_connection};
use crate::models::{ApplyPlanResult, OperationHistoryItem, PlannedOperation, TaskProgress};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

static NEXT_COPY_ID: AtomicU64 = AtomicU64::new(1);
static APPLY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub async fn apply_plan(
    app_handle: tauri::AppHandle,
    plan_id: String,
) -> Result<ApplyPlanResult, String> {
    let task = tasks::create("organize");
    let task_for_worker = task.clone();
    let error_handle = app_handle.clone();
    let joined = tokio::task::spawn_blocking(move || {
        apply_plan_blocking(app_handle, plan_id, task_for_worker)
    })
    .await;
    let result = match joined {
        Ok(result) => result,
        Err(error) => Err(format!("执行整理计划的任务异常终止: {error}")),
    };
    if let Err(error) = &result {
        tasks::emit_error(&error_handle, &task.id, "organize", error);
    }
    tasks::finish(&task);
    result
}

fn apply_plan_blocking(
    app_handle: tauri::AppHandle,
    plan_id: String,
    task: TaskHandle,
) -> Result<ApplyPlanResult, String> {
    let _apply_guard = APPLY_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "整理计划互斥锁不可用。".to_string())?;
    let connection = open_connection()?;
    let operations = database::load_plan_operations(&connection, &plan_id)?;
    if operations.is_empty() {
        return Err(format!("找不到计划或计划为空: {plan_id}"));
    }
    validate_operations_for_apply(&operations)?;

    let total = operations.len() as u64;
    let total_bytes = operations
        .iter()
        .filter_map(|operation| operation.source_size)
        .sum();
    let started_at = Instant::now();
    let mut completed = 0u64;
    let mut failed = 0u64;
    let mut last_emit = Instant::now() - Duration::from_millis(200);

    for operation in &operations {
        if tasks::is_cancelled(&task) {
            break;
        }

        let was_skipped = operation.status == "skipped";
        let result = if was_skipped {
            database::update_operation(&connection, &operation.id, "skipped", None, None)?;
            Ok(())
        } else if operation.operation_type == "tag" {
            database::apply_tags(&connection, &operation.source_path, &operation.tags)
        } else if operation.operation_type == "move" {
            let destination = operation
                .destination_path
                .as_ref()
                .map(PathBuf::from)
                .ok_or_else(|| "计划缺少目标路径。".to_string())?;
            if same_volume(Path::new(&operation.source_path), &destination) {
                execute_one(operation)
            } else {
                execute_cross_volume_move(&connection, operation)
            }
        } else {
            execute_one(operation)
        };
        match result {
            Ok(()) => {
                completed += 1;
                let undo_status = if !was_skipped && operation.operation_type == "move" {
                    Some("available")
                } else {
                    None
                };
                database::update_operation(
                    &connection,
                    &operation.id,
                    if was_skipped { "skipped" } else { "completed" },
                    None,
                    undo_status,
                )?;
            }
            Err(error) => {
                failed += 1;
                database::update_operation(
                    &connection,
                    &operation.id,
                    "failed",
                    Some(&error),
                    Some("unavailable"),
                )?;
            }
        }

        if last_emit.elapsed() >= Duration::from_millis(100) || completed + failed == total {
            last_emit = Instant::now();
            tasks::emit_progress(
                &app_handle,
                TaskProgress {
                    task_id: task.id.clone(),
                    task_type: task.task_type.clone(),
                    phase: "moving".to_string(),
                    completed_items: completed + failed,
                    total_items: total,
                    completed_bytes: operations
                        .iter()
                        .take((completed + failed) as usize)
                        .filter_map(|operation| operation.source_size)
                        .sum(),
                    total_bytes,
                    current_path: Some(operation.source_path.clone()),
                    speed_bytes_per_second: Some(bytes_per_second(
                        completed.saturating_add(failed),
                        started_at.elapsed(),
                    )),
                    eta_seconds: None,
                },
            );
        }
    }

    let status = if tasks::is_cancelled(&task) {
        "cancelled"
    } else if failed == 0 {
        "completed"
    } else {
        "completed_with_errors"
    };
    database::update_plan(&connection, &plan_id, status)?;
    tasks::emit_completed(&app_handle, &task.id, &task.task_type, status);

    Ok(ApplyPlanResult {
        task_id: task.id,
        plan_id: plan_id.clone(),
        status: status.to_string(),
        completed,
        failed,
        operations: database::load_plan_operations(&connection, &plan_id)?,
    })
}

fn validate_operations_for_apply(operations: &[PlannedOperation]) -> Result<(), String> {
    if operations.iter().any(|operation| {
        !matches!(operation.status.as_str(), "ready" | "skipped")
            && !(operation.status == "copy_verified" && operation.operation_type == "move")
    }) {
        return Err("计划已执行、正在恢复或包含未解决冲突，不能重复应用。".to_string());
    }
    Ok(())
}

fn execute_one(operation: &PlannedOperation) -> Result<(), String> {
    if operation.status != "ready" {
        return Err(format!("计划操作未就绪: {}", operation.status));
    }
    let source = Path::new(&operation.source_path);
    validate_source_snapshot(operation)?;
    if operation.operation_type == "recycle" {
        return recycle_file(source);
    }
    let destination = operation
        .destination_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| "计划缺少目标路径。".to_string())?;
    if destination.exists() {
        return Err("conflict: 目标文件在应用前已存在。".to_string());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| classify_io_error("destination", error))?;
    }

    match operation.operation_type.as_str() {
        "rename" => {
            fs::rename(source, &destination).map_err(|error| classify_io_error("rename", error))
        }
        "move" => move_file(source, &destination),
        "copy" | "restore" => copy_file(source, &destination),
        other => Err(format!("unsupported_operation: 不支持的操作类型 {other}")),
    }
}

fn validate_source_snapshot(operation: &PlannedOperation) -> Result<(), String> {
    let source = Path::new(&operation.source_path);
    let metadata = fs::metadata(source).map_err(|error| classify_io_error("source", error))?;
    if let Some(size) = operation.source_size {
        if metadata.len() != size {
            return Err("source_changed: 源文件大小在预览后发生变化。".to_string());
        }
    }
    if let Some(modified_at) = operation.source_modified_at {
        let current_modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as i64);
        if current_modified != Some(modified_at) {
            return Err("source_changed: 源文件修改时间在预览后发生变化。".to_string());
        }
    }
    Ok(())
}

fn execute_cross_volume_move(
    connection: &rusqlite::Connection,
    operation: &PlannedOperation,
) -> Result<(), String> {
    let source = Path::new(&operation.source_path);
    let destination = operation
        .destination_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| "计划缺少目标路径。".to_string())?;
    if operation.status == "copy_verified" {
        if !destination.is_file() {
            return Err("recovery_failed: 已校验副本不存在，未删除源文件。".to_string());
        }
        if source.exists() {
            if !same_file_content(source, &destination) {
                return Err(
                    "recovery_failed: 已校验副本与源文件内容不一致，未删除源文件。".to_string(),
                );
            }
            fs::remove_file(source)
                .map_err(|error| classify_io_error("delete_source_after_recovery", error))?;
        }
        return Ok(());
    }
    validate_source_snapshot(operation)?;
    if destination.exists() {
        return Err("conflict: 目标文件在应用前已存在。".to_string());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| classify_io_error("destination", error))?;
    }
    copy_file(source, &destination)?;
    database::update_operation(
        connection,
        &operation.id,
        "copy_verified",
        None,
        Some("copy_verified"),
    )?;
    fs::remove_file(source).map_err(|error| classify_io_error("delete_source_after_verify", error))
}

fn recycle_file(source: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::UI::Shell::{
            SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FO_DELETE,
            SHFILEOPSTRUCTW,
        };
        let mut path: Vec<u16> = source.as_os_str().encode_wide().collect();
        path.extend_from_slice(&[0, 0]);
        let operation = SHFILEOPSTRUCTW {
            wFunc: FO_DELETE,
            pFrom: windows::core::PCWSTR(path.as_ptr()),
            fFlags: (FOF_ALLOWUNDO.0 | FOF_NOCONFIRMATION.0 | FOF_NOERRORUI.0) as u16,
            ..Default::default()
        };
        let result = unsafe { SHFileOperationW(&operation as *const _ as *mut _) };
        if result != 0 {
            return Err(format!(
                "recycle_failed: 移至 Windows 回收站失败，错误码 {result}"
            ));
        }
        if source.exists() {
            return Err("recycle_failed: 回收站操作后源文件仍存在。".to_string());
        }
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        let _ = source;
        Err("unsupported_operation: 当前平台不支持 Windows 回收站。".to_string())
    }
}

fn move_file(source: &Path, destination: &Path) -> Result<(), String> {
    if same_volume(source, destination) {
        return fs::rename(source, destination).map_err(|error| classify_io_error("move", error));
    }
    copy_file(source, destination)?;
    fs::remove_file(source).map_err(|error| classify_io_error("delete_source_after_verify", error))
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    let source_metadata =
        fs::metadata(source).map_err(|error| classify_io_error("copy_source", error))?;
    let modified_time = source_metadata.modified().ok();
    let temporary = temporary_copy_path(destination);

    let copied = match fs::copy(source, &temporary) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(classify_io_error("copy", error));
        }
    };
    let target_size = match fs::metadata(&temporary) {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(classify_io_error("verify", error));
        }
    };
    if copied != target_size || copied != source_metadata.len() {
        let _ = fs::remove_file(&temporary);
        return Err("verify_failed: 复制后的文件大小校验失败，已保留源文件。".to_string());
    }
    if !same_file_content(source, &temporary) {
        let _ = fs::remove_file(&temporary);
        return Err("verify_failed: 源文件与复制结果内容不一致，已保留源文件。".to_string());
    }
    if let Some(mtime) = modified_time {
        if let Ok(target_file) = fs::File::options().write(true).open(&temporary) {
            let _ = target_file.set_times(std::fs::FileTimes::new().set_modified(mtime));
        }
    }
    if let Err(error) = fs::rename(&temporary, destination) {
        let _ = fs::remove_file(&temporary);
        if destination.exists() {
            return Err("conflict: 目标文件在复制完成前已存在，未覆盖。".to_string());
        }
        return Err(classify_io_error("place_copy", error));
    }
    Ok(())
}

fn temporary_copy_path(destination: &Path) -> PathBuf {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let id = NEXT_COPY_ID.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".{name}.windows-easy-backup-{}-{id}.tmp",
        std::process::id()
    ))
}

fn same_volume(source: &Path, destination: &Path) -> bool {
    let source_prefix = source.components().next();
    let destination_prefix = destination.components().next();
    match (source_prefix, destination_prefix) {
        (Some(Component::Prefix(source)), Some(Component::Prefix(destination))) => {
            source == destination
        }
        _ => true,
    }
}

fn classify_io_error(operation: &str, error: io::Error) -> String {
    let kind = match error.kind() {
        io::ErrorKind::NotFound => "source_missing",
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::AlreadyExists => "conflict",
        io::ErrorKind::WouldBlock => "file_locked",
        _ => "io_error",
    };
    format!("{kind}: {operation} 操作失败: {error}")
}

fn bytes_per_second(items: u64, elapsed: Duration) -> u64 {
    items
        .checked_div(elapsed.as_secs().max(1))
        .unwrap_or_default()
}

pub fn undo(operation_id: String) -> Result<OperationHistoryItem, String> {
    let connection = open_connection()?;
    undo_with_connection(&connection, &operation_id)
}

fn undo_with_connection(
    connection: &rusqlite::Connection,
    operation_id: &str,
) -> Result<OperationHistoryItem, String> {
    let operation = database::find_operation(&connection, &operation_id)?
        .ok_or_else(|| format!("找不到操作日志: {operation_id}"))?;
    if !matches!(operation.operation_type.as_str(), "rename" | "move") {
        return Err("当前只支持撤销 rename 和 move 操作。".to_string());
    }
    if operation.status != "completed" {
        return Err("只有已完成的操作可以撤销。".to_string());
    }
    if operation.undo_status == "undone" {
        return Err("该操作已经撤销。".to_string());
    }
    let source = Path::new(
        operation
            .destination_path
            .as_deref()
            .ok_or_else(|| "操作日志缺少目标路径。".to_string())?,
    );
    let destination = Path::new(&operation.source_path);
    if operation.undo_status == "undo_copy_verified" && source.exists() && destination.exists() {
        if !same_file_content(source, destination) {
            return Err("undo_conflict: 撤销副本与目标内容已不一致，未删除任何文件。".to_string());
        }
        fs::remove_file(source)
            .map_err(|error| classify_io_error("undo_delete_source_after_verify", error))?;
        database::update_operation(
            &connection,
            &operation.id,
            "completed",
            None,
            Some("undone"),
        )?;
        return database::find_operation(&connection, &operation.id)?
            .ok_or_else(|| "撤销后无法读取操作日志。".to_string());
    }
    if destination.exists() {
        database::update_operation(
            &connection,
            &operation.id,
            "completed",
            Some("undo_conflict: 原位置已有文件，未覆盖。"),
            Some("undo_conflict"),
        )?;
        return Err("undo_conflict: 原位置已有文件，未覆盖。".to_string());
    }
    if !source.exists() {
        database::update_operation(
            &connection,
            &operation.id,
            "completed",
            Some("source_missing: 找不到已执行操作的目标文件。"),
            Some("unavailable"),
        )?;
        return Err("source_missing: 找不到已执行操作的目标文件。".to_string());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| classify_io_error("undo_create_parent", error))?;
    }
    if same_volume(source, destination) {
        fs::rename(source, destination).map_err(|error| classify_io_error("undo", error))?;
    } else {
        copy_file(source, destination)?;
        database::update_operation(
            &connection,
            &operation.id,
            "completed",
            None,
            Some("undo_copy_verified"),
        )?;
        fs::remove_file(source)
            .map_err(|error| classify_io_error("undo_delete_source_after_verify", error))?;
    }
    database::update_operation(
        &connection,
        &operation.id,
        "completed",
        None,
        Some("undone"),
    )?;
    database::find_operation(&connection, &operation.id)?
        .ok_or_else(|| "撤销后无法读取操作日志。".to_string())
}

fn same_file_content(source: &Path, destination: &Path) -> bool {
    let Ok(source_metadata) = fs::metadata(source) else {
        return false;
    };
    let Ok(destination_metadata) = fs::metadata(destination) else {
        return false;
    };
    if !source_metadata.is_file()
        || !destination_metadata.is_file()
        || source_metadata.len() != destination_metadata.len()
    {
        return false;
    }
    let Ok(mut source_file) = fs::File::open(source) else {
        return false;
    };
    let Ok(mut destination_file) = fs::File::open(destination) else {
        return false;
    };
    let mut source_hasher = blake3::Hasher::new();
    let mut destination_hasher = blake3::Hasher::new();
    let mut source_buffer = vec![0u8; 1024 * 1024];
    let mut destination_buffer = vec![0u8; 1024 * 1024];
    loop {
        let Ok(source_read) = source_file.read(&mut source_buffer) else {
            return false;
        };
        let Ok(destination_read) = destination_file.read(&mut destination_buffer) else {
            return false;
        };
        if source_read != destination_read {
            return false;
        }
        if source_read == 0 {
            break;
        }
        source_hasher.update(&source_buffer[..source_read]);
        destination_hasher.update(&destination_buffer[..destination_read]);
    }
    source_hasher.finalize() == destination_hasher.finalize()
}

pub fn history(limit: u32) -> Result<Vec<OperationHistoryItem>, String> {
    let connection = open_connection()?;
    database::list_history(&connection, limit)
}

#[cfg(test)]
mod tests {
    use super::{
        classify_io_error, execute_one, undo_with_connection, validate_operations_for_apply,
    };
    use crate::database::{insert_operation, insert_plan, open_connection_at, update_operation};
    use crate::models::PlannedOperation;
    use std::fs;
    use std::io;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "windows-easy-backup-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn operation(
        operation_type: &str,
        source: &std::path::Path,
        destination: &std::path::Path,
    ) -> PlannedOperation {
        let metadata = fs::metadata(source).unwrap();
        let modified_at = metadata
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        PlannedOperation {
            id: format!("operation-{operation_type}"),
            operation_type: operation_type.to_string(),
            source_path: source.to_string_lossy().to_string(),
            destination_path: Some(destination.to_string_lossy().to_string()),
            reason: "test".to_string(),
            rule_id: None,
            conflict: None,
            status: "ready".to_string(),
            source_size: Some(metadata.len()),
            source_modified_at: Some(modified_at),
            tags: Vec::new(),
        }
    }

    #[test]
    fn classifies_permission_errors() {
        let message = classify_io_error("copy", io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(message.starts_with("permission_denied:"));
    }

    #[test]
    fn refuses_to_apply_a_completed_plan_again() {
        let root = test_root("completed-plan");
        let source = root.join("source");
        let target = root.join("target");
        fs::write(&source, b"content").unwrap();
        let mut operation = operation("rename", &source, &target);
        operation.status = "completed".to_string();
        let error = validate_operations_for_apply(&[operation]).unwrap_err();
        assert!(error.contains("不能重复应用"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn executor_rename_changes_only_after_a_plan_exists() {
        let root = test_root("executor-rename");
        let source = root.join("before.txt");
        let destination = root.join("after.txt");
        fs::write(&source, b"content").unwrap();
        let operation = operation("rename", &source, &destination);
        execute_one(&operation).unwrap();
        assert!(!source.exists());
        assert_eq!(fs::read(&destination).unwrap(), b"content");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn executor_copy_verifies_and_preserves_source() {
        let root = test_root("executor-copy");
        let source = root.join("source.txt");
        let destination = root.join("nested").join("copy.txt");
        fs::write(&source, b"copy me").unwrap();
        let operation = operation("copy", &source, &destination);

        execute_one(&operation).unwrap();

        assert_eq!(fs::read(&source).unwrap(), b"copy me");
        assert_eq!(fs::read(&destination).unwrap(), b"copy me");
        let temporary_files = fs::read_dir(root.join("nested"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".copy.txt.windows-easy-backup-")
            })
            .count();
        assert_eq!(temporary_files, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn executor_rejects_source_changes_before_touching_destination() {
        let root = test_root("executor-source-changed");
        let source = root.join("source.txt");
        let destination = root.join("destination.txt");
        fs::write(&source, b"before").unwrap();
        let operation = operation("move", &source, &destination);
        fs::write(&source, b"after changed").unwrap();

        let error = execute_one(&operation).unwrap_err();

        assert!(error.starts_with("source_changed:"));
        assert!(source.exists());
        assert!(!destination.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn executor_never_overwrites_an_existing_destination() {
        let root = test_root("executor-conflict");
        let source = root.join("source.txt");
        let destination = root.join("destination.txt");
        fs::write(&source, b"source").unwrap();
        fs::write(&destination, b"existing").unwrap();
        let operation = operation("move", &source, &destination);

        let error = execute_one(&operation).unwrap_err();

        assert!(error.starts_with("conflict:"));
        assert_eq!(fs::read(&source).unwrap(), b"source");
        assert_eq!(fs::read(&destination).unwrap(), b"existing");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn undo_restores_a_completed_move() {
        let root = test_root("undo");
        let db_path = root.join("app.db");
        let source = root.join("source.txt");
        let destination = root.join("destination.txt");
        fs::write(&source, b"restore me").unwrap();
        let operation = operation("move", &source, &destination);
        let connection = open_connection_at(&db_path).unwrap();
        insert_plan(&connection, "plan-undo", "task-undo", "ready").unwrap();
        insert_operation(&connection, "plan-undo", "task-undo", &operation).unwrap();
        update_operation(&connection, &operation.id, "completed", None, None).unwrap();

        execute_one(&operation).unwrap();
        let history = undo_with_connection(&connection, &operation.id).unwrap();

        assert!(!destination.exists());
        assert_eq!(fs::read(&source).unwrap(), b"restore me");
        assert_eq!(history.undo_status, "undone");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn undo_conflict_never_overwrites_the_original_location() {
        let root = test_root("undo-conflict");
        let db_path = root.join("app.db");
        let source = root.join("source.txt");
        let destination = root.join("destination.txt");
        fs::write(&source, b"original").unwrap();
        let operation = operation("move", &source, &destination);
        let connection = open_connection_at(&db_path).unwrap();
        insert_plan(
            &connection,
            "plan-undo-conflict",
            "task-undo-conflict",
            "ready",
        )
        .unwrap();
        insert_operation(
            &connection,
            "plan-undo-conflict",
            "task-undo-conflict",
            &operation,
        )
        .unwrap();
        update_operation(&connection, &operation.id, "completed", None, None).unwrap();

        execute_one(&operation).unwrap();
        fs::write(&source, b"new user file").unwrap();
        let error = undo_with_connection(&connection, &operation.id).unwrap_err();
        let history = crate::database::find_operation(&connection, &operation.id)
            .unwrap()
            .unwrap();

        assert!(error.starts_with("undo_conflict:"));
        assert_eq!(fs::read(&source).unwrap(), b"new user file");
        assert_eq!(fs::read(&destination).unwrap(), b"original");
        assert_eq!(history.undo_status, "undo_conflict");
        let _ = fs::remove_dir_all(root);
    }
}
