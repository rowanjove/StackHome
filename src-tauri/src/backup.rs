use crate::archive::create_archive;
use crate::core::snapshots;
use crate::core::tasks::{self, TaskHandle};
use crate::filters::should_exclude_relative_path;
use crate::models::{
    ArchiveFormat, BackupError, BackupItem, BackupOptions, BackupSummary, SnapshotFileRecord,
    TaskProgress,
};
use crate::notify::notify_backup_result;
use crate::paths::{create_backup_root, probe_target_root, validate_target_root};
use crate::report::{render_report, write_utf8_bom_file};
use crate::scanner::scan_backup_items;
use chrono::Local;
use std::fs::{self, File};
use std::io::{Read, Write};
#[cfg(windows)]
use std::os::windows::fs::FileTimesExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::AppHandle;
use walkdir::WalkDir;

static CANCEL_FLAG: AtomicBool = AtomicBool::new(false);
static BACKUP_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const LARGE_FILE_THRESHOLD: u64 = 64 * 1024 * 1024;
const CHUNK_SIZE: usize = 2 * 1024 * 1024;
const THROTTLE_INTERVAL_MS: u128 = 80;

pub fn reset_cancel_flag() {
    CANCEL_FLAG.store(false, Ordering::SeqCst);
}

pub fn cancel_backup() {
    CANCEL_FLAG.store(true, Ordering::SeqCst);
}

fn legacy_cancel_requested() -> bool {
    CANCEL_FLAG.load(Ordering::SeqCst)
}

#[cfg(test)]
pub fn is_cancelled() -> bool {
    legacy_cancel_requested()
}

fn is_task_cancelled(task: &TaskHandle) -> bool {
    tasks::is_cancelled(task) || legacy_cancel_requested()
}

struct ProgressState {
    app_handle: AppHandle,
    last_emit: Instant,
    started_at: Instant,
    planned_files: u64,
    planned_bytes: u64,
    task_id: String,
}

impl ProgressState {
    fn new(
        app_handle: AppHandle,
        started_at: Instant,
        planned_files: u64,
        planned_bytes: u64,
        task_id: String,
    ) -> Self {
        Self {
            app_handle,
            last_emit: Instant::now() - Duration::from_millis(200),
            started_at,
            planned_files,
            planned_bytes,
            task_id,
        }
    }

    fn emit(
        &mut self,
        force: bool,
        phase: &str,
        current_folder: &str,
        current_file: &str,
        _current_file_size: Option<u64>,
        _current_file_copied: Option<u64>,
        copied_files: u64,
        copied_bytes: u64,
        _failed_files: u64,
        _status: &str,
    ) {
        if !force && self.last_emit.elapsed().as_millis() < THROTTLE_INTERVAL_MS {
            return;
        }

        self.last_emit = Instant::now();
        let elapsed = self.started_at.elapsed().as_secs().max(1);
        let speed = copied_bytes / elapsed;
        let remaining = self.planned_bytes.saturating_sub(copied_bytes);
        let estimated = if speed == 0 {
            -1
        } else {
            (remaining / speed) as i64
        };
        let task_phase = match phase {
            "copying" => "copying",
            "compressing" => "verifying",
            "cancelled" => "cancelled",
            "done" | "error" => "verifying",
            other => other,
        };
        tasks::emit_progress(
            &self.app_handle,
            TaskProgress {
                task_id: self.task_id.clone(),
                task_type: "backup".to_string(),
                phase: task_phase.to_string(),
                completed_items: copied_files,
                total_items: self.planned_files,
                completed_bytes: copied_bytes,
                total_bytes: self.planned_bytes,
                current_path: if current_file.is_empty() {
                    Some(current_folder.to_string())
                } else {
                    Some(current_file.to_string())
                },
                speed_bytes_per_second: Some(speed),
                eta_seconds: Some(estimated),
            },
        );
    }
}

pub async fn start_backup(
    app_handle: AppHandle,
    items: Vec<BackupItem>,
    target_root: String,
    options: BackupOptions,
) -> Result<BackupSummary, String> {
    validate_target_root(&items, Path::new(&target_root))?;
    let task = tasks::create("backup");
    let task_for_worker = task.clone();
    let error_handle = app_handle.clone();

    let joined = tokio::task::spawn_blocking(move || {
        perform_backup_blocking(app_handle, items, target_root, options, task_for_worker)
    })
    .await;
    let result = match joined {
        Ok(result) => result,
        Err(error) => Err(format!("备份任务异常终止: {error}")),
    };
    if let Err(error) = &result {
        tasks::emit_error(&error_handle, &task.id, "backup", error);
    }
    tasks::finish(&task);
    result
}

fn perform_backup_blocking(
    app_handle: AppHandle,
    items: Vec<BackupItem>,
    target_root: String,
    options: BackupOptions,
    task: TaskHandle,
) -> Result<BackupSummary, String> {
    let _backup_guard = BACKUP_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "备份任务互斥锁不可用。".to_string())?;
    reset_cancel_flag();
    let now = Local::now();
    let target_root = PathBuf::from(target_root);
    let preflight = probe_target_root(&target_root)?;
    let refreshed_scan = scan_backup_items(&items, &target_root, &options)?;
    let backup_items = refreshed_scan.items.clone();
    let job = snapshots::create_job(
        &options.job_name,
        &items,
        &target_root.display().to_string(),
        &options,
    )?;
    let previous_files = if options.incremental {
        snapshots::latest_files(&job.id)?
    } else {
        std::collections::HashMap::new()
    };
    let backup_root = create_backup_root(&target_root, now)?;
    let source_root = backup_items
        .iter()
        .find(|item| item.enabled)
        .and_then(|item| Path::new(&item.source_path).parent())
        .map(|path| path.display().to_string())
        .unwrap_or_default();

    let planned_files = refreshed_scan.total_files;
    let planned_bytes = refreshed_scan.total_bytes;
    let skipped_by_rule_count = refreshed_scan.skipped_by_rule_count;

    let started_at = Instant::now();
    let mut progress = ProgressState::new(
        app_handle.clone(),
        started_at,
        planned_files,
        planned_bytes,
        task.id.clone(),
    );

    let mut copied_files = 0u64;
    let mut copied_bytes = 0u64;
    let mut failed_files = 0u64;
    let mut errors = Vec::new();
    let mut snapshot_files = Vec::new();
    let mut logs = vec![format!(
        "[{}] [INFO] 开始备份，目标类型: {}",
        now.format("%Y-%m-%d %H:%M:%S"),
        preflight.kind
    )];

    for warning in preflight
        .warnings
        .iter()
        .chain(refreshed_scan.warnings.iter())
        .chain(refreshed_scan.source_warnings.iter())
    {
        logs.push(format!(
            "[{}] [INFO] {}",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            warning
        ));
    }

    progress.emit(
        true,
        "copying",
        "初始化备份",
        "",
        None,
        None,
        0,
        0,
        0,
        "copying",
    );

    for item in backup_items.iter().filter(|item| item.enabled) {
        if is_task_cancelled(&task) {
            break;
        }

        let source_root_path = Path::new(&item.source_path);

        for entry in WalkDir::new(&item.source_path).into_iter() {
            if is_task_cancelled(&task) {
                break;
            }

            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    failed_files += 1;
                    errors.push(BackupError {
                        source_path: item.source_path.clone(),
                        target_path: backup_root.display().to_string(),
                        reason: format!("遍历失败: {error}"),
                    });
                    continue;
                }
            };

            let path = entry.path().to_path_buf();
            let relative_path = match path.strip_prefix(source_root_path) {
                Ok(path) => path.to_path_buf(),
                Err(_) => continue,
            };

            if !relative_path.as_os_str().is_empty()
                && should_exclude_relative_path(&relative_path, &options)
            {
                continue;
            }

            if !entry.file_type().is_file() {
                continue;
            }

            let target_path = backup_root.join(&item.target_name).join(&relative_path);

            if let Some(parent) = target_path.parent() {
                if let Err(error) = fs::create_dir_all(parent) {
                    failed_files += 1;
                    errors.push(BackupError {
                        source_path: path.display().to_string(),
                        target_path: target_path.display().to_string(),
                        reason: format!("无法创建目录: {error}"),
                    });
                    continue;
                }
            }

            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string();

            let source_mtime = file_mtime(&path);
            let previous = previous_files
                .get(&path.display().to_string())
                .filter(|previous| {
                    previous.size == entry.metadata().map(|meta| meta.len()).unwrap_or_default()
                })
                .filter(|previous| previous.mtime == source_mtime)
                .filter(|previous| previous.hash.is_some())
                .filter(|previous| Path::new(&previous.backup_path).is_file());
            let copy_result = if let Some(previous) = previous {
                copy_reused_snapshot_file(
                    &path,
                    Path::new(&previous.backup_path),
                    &target_path,
                    previous.size,
                    previous.hash.as_deref().unwrap_or_default(),
                    &options.metadata_preserve_level,
                )
            } else {
                copy_single_file_with_throttle(
                    &path,
                    &target_path,
                    &item.label,
                    &file_name,
                    &mut progress,
                    &task,
                    copied_files,
                    copied_bytes,
                    failed_files,
                    &options.metadata_preserve_level,
                )
            };
            let copy_result = copy_result.and_then(|bytes| {
                let hash = verify_copied_file(
                    &path,
                    &target_path,
                    bytes,
                    source_mtime,
                    options.verify_mode == "full",
                )?;
                snapshot_files.push(SnapshotFileRecord {
                    snapshot_id: String::new(),
                    source_path: path.display().to_string(),
                    backup_path: target_path.display().to_string(),
                    size: bytes,
                    mtime: source_mtime,
                    hash,
                });
                Ok(bytes)
            });
            match copy_result {
                Ok(bytes) => {
                    copied_files += 1;
                    copied_bytes += bytes;
                    logs.push(format!(
                        "[{}] [OK] {}",
                        Local::now().format("%Y-%m-%d %H:%M:%S"),
                        path.display()
                    ));
                    progress.emit(
                        false,
                        "copying",
                        &item.label,
                        &file_name,
                        Some(bytes),
                        Some(bytes),
                        copied_files,
                        copied_bytes,
                        failed_files,
                        if is_task_cancelled(&task) {
                            "cancelled"
                        } else {
                            "copying"
                        },
                    );
                }
                Err(error) => {
                    failed_files += 1;
                    errors.push(BackupError {
                        source_path: path.display().to_string(),
                        target_path: target_path.display().to_string(),
                        reason: error.clone(),
                    });
                    logs.push(format!(
                        "[{}] [SKIP] {} - {}",
                        Local::now().format("%Y-%m-%d %H:%M:%S"),
                        path.display(),
                        error
                    ));
                }
            }
        }
    }

    let status = if is_task_cancelled(&task) {
        "cancelled"
    } else if failed_files > 0 {
        "error"
    } else {
        "done"
    }
    .to_string();
    let duration_seconds = started_at.elapsed().as_secs();
    let report_path = backup_root.join("backup_report.txt");
    let log_path = backup_root.join("backup_log.txt");

    let mut summary = BackupSummary {
        status,
        total_files: planned_files,
        success_files: copied_files,
        failed_files,
        skipped_by_rule_count,
        total_bytes: planned_bytes,
        copied_bytes,
        duration_seconds,
        errors,
        archive_format: None,
        archive_error: None,
        report_path: report_path.display().to_string(),
        log_path: log_path.display().to_string(),
        backup_root: backup_root.display().to_string(),
        archive_path: None,
        snapshot_id: None,
        manifest_path: None,
        verify_status: None,
    };

    let snapshot = snapshots::record_snapshot(&job, &backup_root, snapshot_files, &summary.status)?;
    summary.snapshot_id = Some(snapshot.id.clone());
    summary.manifest_path = snapshot.manifest_path.clone();
    if summary.status == "cancelled" {
        summary.verify_status = Some("cancelled".to_string());
    } else {
        let (verify_mode, verify_status) = if options.verify_mode == "full" {
            ("full", "full")
        } else {
            ("fast", "fast")
        };
        let (_, failed) = snapshots::verify_snapshot_files(&snapshot.id, verify_mode)?;
        summary.verify_status = Some(if failed == 0 {
            format!("{verify_status}:passed")
        } else {
            format!("{verify_status}:failed")
        });
        if failed > 0 {
            summary.status = "error".to_string();
        }
    }

    if options.compress_after_backup && !is_task_cancelled(&task) && backup_root.exists() {
        let archive_format = match options.archive_format {
            ArchiveFormat::Zip => "zip",
            ArchiveFormat::SevenZ => "7z",
        };
        summary.archive_format = Some(archive_format.to_string());

        progress.emit(
            true,
            "compressing",
            "压缩归档",
            "正在打包生成压缩文件...",
            None,
            None,
            copied_files,
            copied_bytes,
            failed_files,
            "compressing",
        );

        match create_archive(&backup_root, &options, &mut |label, processed, total| {
            progress.emit(
                false,
                "compressing",
                "压缩归档",
                &label,
                Some(total),
                Some(processed),
                copied_files,
                copied_bytes,
                failed_files,
                "compressing",
            );
            Ok(())
        }) {
            Ok(path) => {
                logs.push(format!(
                    "[{}] [OK] 归档完成 {}",
                    Local::now().format("%Y-%m-%d %H:%M:%S"),
                    path.display()
                ));
                summary.archive_path = Some(path.display().to_string());
            }
            Err(error) => {
                logs.push(format!(
                    "[{}] [WARN] 压缩失败 - {}",
                    Local::now().format("%Y-%m-%d %H:%M:%S"),
                    error
                ));
                summary.archive_error = Some(error);
            }
        }
    }

    let report = render_report(
        Local::now(),
        &source_root,
        &backup_root,
        &backup_items,
        &summary,
    );
    write_utf8_bom_file(&report_path, &report)?;
    write_utf8_bom_file(&log_path, &logs.join("\r\n"))?;

    progress.emit(
        true,
        &summary.status,
        "",
        "",
        None,
        None,
        copied_files,
        copied_bytes,
        failed_files,
        &summary.status,
    );

    if options.send_notification {
        let _ = notify_backup_result(&summary);
    }

    tasks::emit_completed(&progress.app_handle, &task.id, "backup", &summary.status);

    Ok(summary)
}

fn copy_single_file_with_throttle(
    source_path: &Path,
    target_path: &Path,
    folder_label: &str,
    file_name: &str,
    progress: &mut ProgressState,
    task: &TaskHandle,
    copied_files: u64,
    copied_bytes: u64,
    failed_files: u64,
    metadata_level: &str,
) -> Result<u64, String> {
    let metadata =
        fs::metadata(source_path).map_err(|error| format!("读取源文件信息失败: {error}"))?;
    let file_size = metadata.len();

    if target_path.exists() {
        if let Ok(target_meta) = fs::metadata(target_path) {
            let mut permissions = target_meta.permissions();
            if permissions.readonly() {
                permissions.set_readonly(false);
                let _ = fs::set_permissions(target_path, permissions);
            }
        }
    }

    if file_size <= LARGE_FILE_THRESHOLD {
        fs::copy(source_path, target_path).map_err(|error| format!("复制文件失败: {error}"))?;
        preserve_file_times(source_path, target_path, &metadata, metadata_level);
        return Ok(file_size);
    }

    let mut source = File::open(source_path).map_err(|error| format!("打开源文件失败: {error}"))?;
    let mut target =
        File::create(target_path).map_err(|error| format!("创建目标文件失败: {error}"))?;
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut written = 0u64;

    loop {
        if is_task_cancelled(task) {
            break;
        }

        let read = source
            .read(&mut buffer)
            .map_err(|error| format!("读取源文件失败: {error}"))?;
        if read == 0 {
            break;
        }
        target
            .write_all(&buffer[..read])
            .map_err(|error| format!("写入目标文件失败: {error}"))?;
        written += read as u64;

        progress.emit(
            false,
            "copying",
            folder_label,
            file_name,
            Some(file_size),
            Some(written),
            copied_files,
            copied_bytes + written,
            failed_files,
            "copying",
        );
    }

    drop(target);

    if is_task_cancelled(task) {
        let _ = fs::remove_file(target_path);
        return Err("cancelled: 任务已取消，已清理未完成的目标文件。".to_string());
    }

    preserve_file_times(source_path, target_path, &metadata, metadata_level);

    Ok(file_size)
}

fn preserve_file_times(
    _source_path: &Path,
    target_path: &Path,
    metadata: &fs::Metadata,
    level: &str,
) {
    let mut times = std::fs::FileTimes::new();
    if let Some(modified) = metadata.modified().ok() {
        times = times.set_modified(modified);
    }
    if level != "standard" {
        if let Some(accessed) = metadata.accessed().ok() {
            times = times.set_accessed(accessed);
        }
        if let Some(created) = metadata.created().ok() {
            times = times.set_created(created);
        }
    }
    if let Ok(target_file) = File::options().write(true).open(target_path) {
        let _ = target_file.set_times(times);
    }
    if level != "standard" {
        preserve_windows_attributes(target_path, metadata);
    }
}

#[cfg(windows)]
fn preserve_windows_attributes(target_path: &Path, metadata: &fs::Metadata) {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::SetFileAttributesW;

    let wide = target_path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        let _ = SetFileAttributesW(wide.as_ptr(), metadata.file_attributes());
    }
}

#[cfg(not(windows))]
fn preserve_windows_attributes(_target_path: &Path, _metadata: &fs::Metadata) {}

fn copy_reused_snapshot_file(
    source: &Path,
    previous_backup: &Path,
    target: &Path,
    expected_size: u64,
    expected_hash: &str,
    metadata_level: &str,
) -> Result<u64, String> {
    let source_hash = hash_file(source)?;
    if source_hash != expected_hash {
        return Err("source_changed: 增量复用前源文件内容已变化。".to_string());
    }
    let bytes =
        fs::copy(previous_backup, target).map_err(|error| format!("复用增量文件失败: {error}"))?;
    if bytes != expected_size {
        let _ = fs::remove_file(target);
        return Err("verify_failed: 增量文件大小校验失败。".to_string());
    }
    let target_hash = hash_file(target)?;
    if target_hash != expected_hash {
        let _ = fs::remove_file(target);
        return Err("verify_failed: 增量文件 Hash 校验失败。".to_string());
    }
    if let Ok(source_metadata) = fs::metadata(source) {
        preserve_file_times(source, target, &source_metadata, metadata_level);
    }
    Ok(bytes)
}

fn verify_copied_file(
    source: &Path,
    target: &Path,
    expected_size: u64,
    expected_mtime: Option<i64>,
    full_hash: bool,
) -> Result<Option<String>, String> {
    let source_metadata = fs::metadata(source)
        .map_err(|error| format!("source_changed: 复制后无法读取源文件: {error}"))?;
    let target_metadata = fs::metadata(target)
        .map_err(|error| format!("verify_failed: 复制后无法读取目标文件: {error}"))?;
    if target_metadata.len() != expected_size {
        return Err("verify_failed: 复制后的目标文件大小校验失败。".to_string());
    }
    if source_metadata.len() != expected_size
        || expected_mtime.is_some() && file_mtime(source) != expected_mtime
    {
        return Err("source_changed: 复制过程中源文件发生变化。".to_string());
    }
    if !full_hash {
        return Ok(None);
    }
    let source_hash = hash_file(source)?;
    let target_hash = hash_file(target)?;
    if source_hash != target_hash {
        return Err("verify_failed: 源文件与目标文件 Hash 不一致。".to_string());
    }
    Ok(Some(source_hash))
}

fn file_mtime(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("打开校验文件失败: {error}"))?;
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

#[cfg(test)]
mod tests {
    use super::{cancel_backup, file_mtime, is_cancelled, reset_cancel_flag, verify_copied_file};
    use std::fs;

    #[test]
    fn stops_after_current_file_when_cancelled() {
        reset_cancel_flag();
        assert!(!is_cancelled());
        cancel_backup();
        assert!(is_cancelled());
        reset_cancel_flag();
    }

    #[test]
    fn full_copy_verification_compares_source_and_target_content() {
        let root = std::env::temp_dir().join(format!(
            "windows-easy-backup-copy-verify-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.bin");
        let target = root.join("target.bin");
        fs::write(&source, b"source").unwrap();
        fs::write(&target, b"target").unwrap();
        let result = verify_copied_file(&source, &target, 6, file_mtime(&source), true);
        assert!(result.unwrap_err().starts_with("verify_failed:"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn incremental_reuse_copies_without_linking_snapshots() {
        let root = std::env::temp_dir().join(format!(
            "windows-easy-backup-incremental-copy-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.bin");
        let previous = root.join("previous.bin");
        let target = root.join("target.bin");
        fs::write(&source, b"same content").unwrap();
        fs::write(&previous, b"same content").unwrap();
        let expected_hash = super::hash_file(&previous).unwrap();
        super::copy_reused_snapshot_file(
            &source,
            &previous,
            &target,
            12,
            &expected_hash,
            "standard",
        )
        .unwrap();
        fs::write(&target, b"changed target").unwrap();
        assert_eq!(fs::read(&previous).unwrap(), b"same content");
        let _ = fs::remove_dir_all(root);
    }
}
