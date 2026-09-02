use crate::core::tasks::{self, TaskHandle};
use crate::database::{self, open_connection};
use crate::models::{
    BackupJobRecord, BackupOptions, PlanPreview, RestorePlanRequest, SnapshotFileRecord,
    SnapshotManifest, SnapshotRecord, SnapshotVerifyResult,
};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub fn create_job(
    name: &str,
    items: &[crate::models::BackupItem],
    target_path: &str,
    options: &BackupOptions,
) -> Result<BackupJobRecord, String> {
    let name = if name.trim().is_empty() {
        "个人文件"
    } else {
        name.trim()
    };
    let id = format!(
        "backup-job-{}",
        blake3::hash(format!("{name}\n{target_path}").as_bytes()).to_hex()
    );
    let job = BackupJobRecord {
        id,
        name: name.to_string(),
        source_config: serde_json::to_value(items)
            .map_err(|error| format!("序列化备份来源失败: {error}"))?,
        target_path: target_path.to_string(),
        policy: serde_json::to_value(options)
            .map_err(|error| format!("序列化备份策略失败: {error}"))?,
        created_at: database::now_millis(),
    };
    let connection = open_connection()?;
    database::insert_backup_job(&connection, &job)?;
    Ok(job)
}

pub fn record_snapshot(
    job: &BackupJobRecord,
    backup_root: &Path,
    files: Vec<SnapshotFileRecord>,
    status: &str,
) -> Result<SnapshotRecord, String> {
    let snapshot_id = format!(
        "snapshot-{}-{}",
        database::now_millis(),
        blake3::hash(backup_root.to_string_lossy().as_bytes())
            .to_hex()
            .chars()
            .take(10)
            .collect::<String>()
    );
    let files = files
        .into_iter()
        .map(|mut file| {
            file.snapshot_id = snapshot_id.clone();
            file
        })
        .collect::<Vec<_>>();
    let manifest_path = backup_root.join("manifest.json");
    let manifest = SnapshotManifest {
        snapshot_id: snapshot_id.clone(),
        created_at: database::now_millis(),
        files: files.clone(),
    };
    write_manifest(&manifest_path, &manifest)?;
    let snapshot = SnapshotRecord {
        id: snapshot_id,
        backup_job_id: Some(job.id.clone()),
        snapshot_time: database::now_millis(),
        file_count: files.len() as u64,
        total_size: files.iter().map(|file| file.size).sum(),
        manifest_path: Some(manifest_path.display().to_string()),
        status: status.to_string(),
    };
    let connection = open_connection()?;
    database::insert_snapshot(&connection, &snapshot)?;
    for file in &files {
        database::insert_snapshot_file(&connection, file)?;
    }
    Ok(snapshot)
}

pub fn list(limit: u32) -> Result<Vec<SnapshotRecord>, String> {
    let connection = open_connection()?;
    database::list_snapshots(&connection, limit)
}

pub fn latest_files(job_id: &str) -> Result<HashMap<String, SnapshotFileRecord>, String> {
    let connection = open_connection()?;
    let snapshots = database::list_snapshots(&connection, 10_000)?;
    let Some(snapshot) = snapshots
        .into_iter()
        .find(|snapshot| snapshot.backup_job_id.as_deref() == Some(job_id))
    else {
        return Ok(HashMap::new());
    };
    Ok(database::list_snapshot_files(&connection, &snapshot.id)?
        .into_iter()
        .map(|file| (file.source_path.clone(), file))
        .collect())
}

pub fn verify_snapshot_files(snapshot_id: &str, mode: &str) -> Result<(u64, u64), String> {
    let connection = open_connection()?;
    let files = database::list_snapshot_files(&connection, snapshot_id)?;
    let full = mode == "full";
    let mut failed = 0u64;
    for file in &files {
        let result = fs::metadata(&file.backup_path)
            .map_err(|error| error.to_string())
            .and_then(|metadata| {
                if metadata.len() != file.size {
                    return Err("文件大小不匹配".to_string());
                }
                if full {
                    let actual = hash_file(Path::new(&file.backup_path))?;
                    if let Some(expected) = file.hash.as_deref() {
                        if actual != expected {
                            return Err("BLAKE3 Hash 不匹配".to_string());
                        }
                    } else {
                        database::update_snapshot_file_hash(
                            &connection,
                            snapshot_id,
                            &file.source_path,
                            &actual,
                        )?;
                    }
                }
                Ok(())
            });
        if result.is_err() {
            failed += 1;
        }
    }
    if full {
        refresh_manifest(&connection, snapshot_id)?;
    }
    database::update_snapshot_status(
        &connection,
        snapshot_id,
        if failed == 0 { "verified" } else { "failed" },
    )?;
    Ok((files.len() as u64, failed))
}

pub fn prune(job_id: &str, keep: u32) -> Result<u64, String> {
    if job_id.trim().is_empty() {
        return Err("缺少备份方案 ID。".to_string());
    }
    if keep == 0 {
        return Err("至少保留一个 Snapshot。".to_string());
    }
    let connection = open_connection()?;
    let snapshots = database::list_snapshots(&connection, 10_000)?
        .into_iter()
        .filter(|snapshot| snapshot.backup_job_id.as_deref() == Some(job_id))
        .collect::<Vec<_>>();
    let mut removed = 0u64;
    for snapshot in snapshots.into_iter().skip(keep as usize) {
        let Some(manifest_path) = snapshot.manifest_path.as_deref() else {
            continue;
        };
        let root = Path::new(manifest_path)
            .parent()
            .ok_or_else(|| format!("Snapshot 路径无效: {manifest_path}"))?;
        let folder_name = root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !folder_name.starts_with("WindowsBackup_") || !root.is_dir() {
            return Err(format!("拒绝清理非工作台生成的目录: {}", root.display()));
        }
        fs::remove_dir_all(root)
            .map_err(|error| format!("清理旧 Snapshot 失败 {}: {error}", root.display()))?;
        database::delete_snapshot(&connection, &snapshot.id)?;
        removed += 1;
    }
    Ok(removed)
}

pub fn manifest(snapshot_id: String) -> Result<SnapshotManifest, String> {
    let connection = open_connection()?;
    let snapshot = database::find_snapshot(&connection, &snapshot_id)?
        .ok_or_else(|| format!("找不到 Snapshot: {snapshot_id}"))?;
    if let Some(path) = snapshot.manifest_path {
        if let Ok(bytes) = fs::read(&path) {
            return serde_json::from_slice(&bytes)
                .map_err(|error| format!("解析 manifest 失败: {error}"));
        }
    }
    Ok(SnapshotManifest {
        snapshot_id: snapshot.id.clone(),
        created_at: snapshot.snapshot_time,
        files: database::list_snapshot_files(&connection, &snapshot.id)?,
    })
}

pub async fn verify(
    app_handle: tauri::AppHandle,
    snapshot_id: String,
    mode: String,
) -> Result<SnapshotVerifyResult, String> {
    let task = tasks::create("verify");
    let worker_task = task.clone();
    let error_handle = app_handle.clone();
    let joined = tokio::task::spawn_blocking(move || {
        verify_blocking(app_handle, snapshot_id, mode, worker_task)
    })
    .await;
    let result = match joined {
        Ok(value) => value,
        Err(error) => Err(format!("Snapshot 校验任务异常终止: {error}")),
    };
    if let Err(error) = &result {
        tasks::emit_error(&error_handle, &task.id, "verify", error);
    }
    tasks::finish(&task);
    result
}

fn verify_blocking(
    app_handle: tauri::AppHandle,
    snapshot_id: String,
    mode: String,
    task: TaskHandle,
) -> Result<SnapshotVerifyResult, String> {
    let connection = open_connection()?;
    let files = database::list_snapshot_files(&connection, &snapshot_id)?;
    if files.is_empty() {
        return Err(format!("Snapshot 没有文件记录: {snapshot_id}"));
    }
    let full = mode == "full";
    let started_at = Instant::now();
    let mut failed_files = 0u64;
    let mut checked_files = 0u64;
    let mut errors = Vec::new();
    for file in &files {
        if tasks::is_cancelled(&task) {
            break;
        }
        checked_files += 1;
        let path = Path::new(&file.backup_path);
        let result = fs::metadata(path)
            .map_err(|error| format!("{}: {error}", path.display()))
            .and_then(|metadata| {
                if metadata.len() != file.size {
                    return Err(format!(
                        "大小不匹配，期望 {}，实际 {}",
                        file.size,
                        metadata.len()
                    ));
                }
                if full {
                    let actual = hash_file(path)?;
                    if let Some(expected) = file.hash.as_deref() {
                        if actual != expected {
                            return Err("BLAKE3 Hash 不匹配".to_string());
                        }
                    } else {
                        database::update_snapshot_file_hash(
                            &connection,
                            &snapshot_id,
                            &file.source_path,
                            &actual,
                        )?;
                    }
                }
                Ok(())
            });
        if let Err(error) = result {
            failed_files += 1;
            errors.push(format!("{}: {error}", file.source_path));
        }
        tasks::emit_progress(
            &app_handle,
            crate::models::TaskProgress {
                task_id: task.id.clone(),
                task_type: task.task_type.clone(),
                phase: "verifying".to_string(),
                completed_items: checked_files,
                total_items: files.len() as u64,
                completed_bytes: files
                    .iter()
                    .take(checked_files as usize)
                    .map(|item| item.size)
                    .sum(),
                total_bytes: files.iter().map(|item| item.size).sum(),
                current_path: Some(file.backup_path.clone()),
                speed_bytes_per_second: Some(
                    files
                        .iter()
                        .take(checked_files as usize)
                        .map(|item| item.size)
                        .sum::<u64>()
                        / started_at.elapsed().as_secs().max(1),
                ),
                eta_seconds: None,
            },
        );
    }
    if full {
        refresh_manifest(&connection, &snapshot_id)?;
    }
    let status = if tasks::is_cancelled(&task) {
        "cancelled"
    } else if failed_files == 0 {
        "verified"
    } else {
        "failed"
    };
    database::update_snapshot_status(&connection, &snapshot_id, status)?;
    tasks::emit_completed(&app_handle, &task.id, &task.task_type, status);
    Ok(SnapshotVerifyResult {
        task_id: task.id,
        snapshot_id,
        mode,
        checked_files,
        failed_files,
        status: status.to_string(),
        errors,
    })
}

pub fn restore_plan(request: RestorePlanRequest) -> Result<PlanPreview, String> {
    let connection = open_connection()?;
    let snapshot = database::find_snapshot(&connection, &request.snapshot_id)?
        .ok_or_else(|| format!("找不到 Snapshot: {}", request.snapshot_id))?;
    let files = database::list_snapshot_files(&connection, &request.snapshot_id)?;
    if files.is_empty() {
        return Err("Snapshot 没有可恢复文件。".to_string());
    }
    let destination_root = request.destination_root.as_deref().map(PathBuf::from);
    if let Some(root) = &destination_root {
        if !root.is_absolute() {
            return Err("恢复目标目录必须是绝对路径。".to_string());
        }
    }
    let task = tasks::create("restore");
    let plan_id = format!("restore-plan-{}", database::now_millis());
    let selected_paths = request
        .source_paths
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let files = files
        .into_iter()
        .filter(|file| {
            selected_paths.is_empty() || selected_paths.contains(file.source_path.as_str())
        })
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Err("所选 Snapshot 内容为空。".to_string());
    }
    let common_parent = common_parent(&files);
    let mut operations = Vec::with_capacity(files.len());
    let mut reserved = HashSet::new();
    for (index, file) in files.into_iter().enumerate() {
        let mut destination = match &destination_root {
            Some(root) => root.join(relative_path(&file.source_path, common_parent.as_deref())),
            None => PathBuf::from(&file.source_path),
        };
        let destination_key = path_key(&destination);
        let (status, conflict) = if destination.exists() || reserved.contains(&destination_key) {
            match request.conflict_policy.as_str() {
                "auto_number" | "sequence" => {
                    destination = auto_number_path(&destination, &reserved);
                    (
                        "ready".to_string(),
                        Some(crate::models::ConflictInfo {
                            kind: "auto_numbered".to_string(),
                            message: "恢复目标已存在，已按自动编号策略生成新目标。".to_string(),
                            suggested_path: Some(destination.display().to_string()),
                        }),
                    )
                }
                "skip" => (
                    "skipped".to_string(),
                    Some(crate::models::ConflictInfo {
                        kind: "existing_target".to_string(),
                        message: "恢复目标已存在，按跳过策略保留现状。".to_string(),
                        suggested_path: None,
                    }),
                ),
                _ => (
                    "conflict".to_string(),
                    Some(crate::models::ConflictInfo {
                        kind: "existing_target".to_string(),
                        message: "恢复目标已存在，未允许覆盖。".to_string(),
                        suggested_path: None,
                    }),
                ),
            }
        } else {
            ("ready".to_string(), None)
        };
        reserved.insert(path_key(&destination));
        operations.push(crate::models::PlannedOperation {
            id: format!("restore-operation-{}-{index}", database::now_millis()),
            operation_type: "restore".to_string(),
            source_path: file.backup_path,
            destination_path: Some(destination.display().to_string()),
            reason: format!("从 Snapshot {} 恢复", snapshot.id),
            rule_id: None,
            conflict,
            status,
            source_size: Some(file.size),
            source_modified_at: None,
            tags: Vec::new(),
        });
    }
    let status = if operations.iter().any(|op| op.status == "conflict") {
        "conflict"
    } else {
        "ready"
    };
    database::insert_plan(&connection, &plan_id, &task.id, status)?;
    for operation in &operations {
        database::insert_operation(&connection, &plan_id, &task.id, operation)?;
    }
    tasks::finish(&task);
    Ok(PlanPreview {
        id: plan_id,
        task_id: task.id,
        created_at: database::now_millis(),
        status: status.to_string(),
        operations,
    })
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase()
}

fn auto_number_path(path: &Path, reserved: &HashSet<String>) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    let extension = path.extension().map(|value| value.to_string_lossy());
    for number in 2..100_000u32 {
        let filename = match extension.as_deref() {
            Some(extension) if !extension.is_empty() => format!("{stem} ({number}).{extension}"),
            _ => format!("{stem} ({number})"),
        };
        let candidate = parent.join(filename);
        if !candidate.exists() && !reserved.contains(&path_key(&candidate)) {
            return candidate;
        }
    }
    path.to_path_buf()
}

fn write_manifest(path: &Path, manifest: &SnapshotManifest) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("生成 manifest 失败: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    let mut file = fs::File::create(&temporary)
        .map_err(|error| format!("创建 manifest 临时文件失败: {error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("写入 manifest 失败: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("刷新 manifest 失败: {error}"))?;
    fs::rename(&temporary, path).map_err(|error| format!("落位 manifest 失败: {error}"))
}

fn refresh_manifest(connection: &rusqlite::Connection, snapshot_id: &str) -> Result<(), String> {
    let Some(snapshot) = database::find_snapshot(connection, snapshot_id)? else {
        return Ok(());
    };
    let Some(path) = snapshot.manifest_path else {
        return Ok(());
    };
    write_manifest(
        Path::new(&path),
        &SnapshotManifest {
            snapshot_id: snapshot.id,
            created_at: snapshot.snapshot_time,
            files: database::list_snapshot_files(connection, snapshot_id)?,
        },
    )
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|error| format!("打开校验文件失败: {error}"))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("读取校验文件失败: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn common_parent(files: &[SnapshotFileRecord]) -> Option<PathBuf> {
    let mut parent = Path::new(files.first()?.source_path.as_str())
        .parent()?
        .to_path_buf();
    for file in files.iter().skip(1) {
        let current = Path::new(&file.source_path).parent()?;
        while !current.starts_with(&parent) {
            parent = parent.parent()?.to_path_buf();
        }
    }
    Some(parent)
}

fn relative_path(source: &str, common_parent: Option<&Path>) -> PathBuf {
    let path = Path::new(source);
    common_parent
        .and_then(|parent| path.strip_prefix(parent).ok())
        .map(Path::to_path_buf)
        .or_else(|| path.file_name().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("restored-file"))
}

#[cfg(test)]
mod tests {
    use super::relative_path;
    use std::path::Path;

    #[test]
    fn restores_relative_path_without_string_replacement() {
        let result = relative_path(
            r"C:\Users\A\Documents\a.txt",
            Some(Path::new(r"C:\Users\A")),
        );
        assert_eq!(result, Path::new(r"Documents\a.txt"));
    }
}
