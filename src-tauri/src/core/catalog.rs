use crate::core::metadata as metadata_engine;
use crate::core::tasks::{self, TaskHandle};
use crate::database::{self, file_from_row, open_connection};
use crate::filters::should_exclude_relative_path;
use crate::models::{
    BackupOptions, CatalogQuery, CatalogScanRequest, CatalogScanResult, FileRecord, TaskProgress,
};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use walkdir::WalkDir;

pub fn scan(
    app_handle: tauri::AppHandle,
    request: CatalogScanRequest,
    task: TaskHandle,
) -> Result<CatalogScanResult, String> {
    let root = PathBuf::from(request.root_path.trim());
    if !root.is_absolute() {
        return Err("Catalog 扫描路径必须是绝对路径。".to_string());
    }
    if !root.exists() {
        return Err(format!("扫描路径不存在: {}", root.display()));
    }
    if !root.is_dir() {
        return Err(format!("扫描路径不是目录: {}", root.display()));
    }

    let connection = open_connection()?;
    let options = BackupOptions {
        custom_exclude_patterns: request.custom_exclude_patterns.clone(),
        ..BackupOptions::default()
    };
    let started_at = Instant::now();
    let mut last_emit = Instant::now() - Duration::from_millis(200);
    let mut total_files = 0u64;
    let mut total_bytes = 0u64;
    let mut indexed_files = 0u64;
    let mut skipped_files = 0u64;
    let mut warnings = Vec::new();

    for entry in WalkDir::new(&root).follow_links(false).into_iter() {
        if tasks::is_cancelled(&task) {
            break;
        }

        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(format!("扫描时跳过不可访问项: {error}"));
                continue;
            }
        };
        let path = entry.path();
        let relative_path = match path.strip_prefix(&root) {
            Ok(value) => value,
            Err(_) => continue,
        };

        if !relative_path.as_os_str().is_empty()
            && (!request.include_hidden && contains_hidden_component(relative_path)
                || !request.include_system_files && is_system_file(path))
        {
            skipped_files += 1;
            continue;
        }

        if !relative_path.as_os_str().is_empty()
            && should_exclude_relative_path(relative_path, &options)
        {
            skipped_files += 1;
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(value) => value,
            Err(error) => {
                warnings.push(format!("无法读取文件元数据 {}: {error}", path.display()));
                continue;
            }
        };
        let path_string = path.to_string_lossy().to_string();
        let modified_at = file_time(metadata.modified());
        let unchanged = database::file_is_unchanged(
            &connection,
            &path_string,
            metadata.len(),
            modified_at,
            request.source_type.as_deref(),
        )?;
        let record = if unchanged {
            None
        } else {
            let file_metadata = match metadata_engine::read_file(path) {
                Ok(value) => Some(value),
                Err(error) => {
                    warnings.push(format!(
                        "读取文件 metadata 失败 {}: {error}",
                        path.display()
                    ));
                    None
                }
            };
            Some(file_record(
                path,
                &metadata,
                request.source_type.clone(),
                file_metadata,
            ))
        };
        total_files += 1;
        total_bytes = total_bytes.saturating_add(metadata.len());
        if let Some(record) = record {
            let file_id = record.id.clone();
            let file_metadata = record.metadata.clone();
            database::upsert_file(&connection, &record)?;
            if let Some(file_metadata) = &file_metadata {
                let json_data = serde_json::to_string(file_metadata)
                    .map_err(|error| format!("序列化文件 metadata 失败: {error}"))?;
                database::upsert_metadata(&connection, &file_id, "file", &json_data)?;
            }
        }
        indexed_files += 1;

        if last_emit.elapsed() >= Duration::from_millis(150) {
            last_emit = Instant::now();
            tasks::emit_progress(
                &app_handle,
                TaskProgress {
                    task_id: task.id.clone(),
                    task_type: task.task_type.clone(),
                    phase: "scanning".to_string(),
                    completed_items: total_files,
                    total_items: 0,
                    completed_bytes: total_bytes,
                    total_bytes: 0,
                    current_path: Some(path.display().to_string()),
                    speed_bytes_per_second: Some(bytes_per_second(
                        total_bytes,
                        started_at.elapsed(),
                    )),
                    eta_seconds: None,
                },
            );
        }
    }

    let status = if tasks::is_cancelled(&task) {
        "cancelled"
    } else {
        "completed"
    };
    tasks::emit_progress(
        &app_handle,
        TaskProgress {
            task_id: task.id.clone(),
            task_type: task.task_type.clone(),
            phase: if status == "cancelled" {
                "cancelled"
            } else {
                "scanning"
            }
            .to_string(),
            completed_items: total_files,
            total_items: total_files,
            completed_bytes: total_bytes,
            total_bytes,
            current_path: None,
            speed_bytes_per_second: Some(bytes_per_second(total_bytes, started_at.elapsed())),
            eta_seconds: Some(0),
        },
    );
    tasks::emit_completed(&app_handle, &task.id, &task.task_type, status);
    let _ = app_handle.emit(
        "catalog-updated",
        serde_json::json!({
            "rootPath": root.display().to_string(),
            "status": status,
            "indexedFiles": indexed_files
        }),
    );
    tasks::finish(&task);

    Ok(CatalogScanResult {
        task_id: task.id,
        root_path: root.display().to_string(),
        total_files,
        total_bytes,
        indexed_files,
        skipped_files,
        warnings,
    })
}

pub fn query(request: CatalogQuery) -> Result<Vec<FileRecord>, String> {
    let connection = open_connection()?;
    database::list_files(&connection, &request)
}

pub(crate) fn file_record(
    path: &Path,
    metadata: &fs::Metadata,
    source_type: Option<String>,
    file_metadata: Option<crate::models::FileMetadata>,
) -> FileRecord {
    let filename = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| filename.clone());
    let category = detect_category(path, &extension);
    FileRecord {
        id: path.to_string_lossy().to_string(),
        path: path.to_string_lossy().to_string(),
        filename,
        stem,
        extension: extension.clone(),
        size: metadata.len(),
        created_at: file_time(metadata.created()),
        modified_at: file_time(metadata.modified()),
        accessed_at: file_time(metadata.accessed()),
        mime: Some(mime_for(&category, &extension)),
        category,
        source_type,
        hash: None,
        hash_algorithm: None,
        metadata: file_metadata,
        tags: Vec::new(),
    }
}

fn file_time(value: std::io::Result<SystemTime>) -> Option<i64> {
    value
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
}

fn bytes_per_second(bytes: u64, elapsed: Duration) -> u64 {
    bytes
        .checked_div(elapsed.as_secs().max(1))
        .unwrap_or_default()
}

fn contains_hidden_component(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
}

fn is_system_file(path: &Path) -> bool {
    path.file_name()
        .map(|value| {
            matches!(
                value.to_string_lossy().to_ascii_lowercase().as_str(),
                "desktop.ini" | "thumbs.db"
            )
        })
        .unwrap_or(false)
}

fn detect_category(path: &Path, extension: &str) -> String {
    let category = match extension {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "heic" => "image",
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "wmv" => "video",
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" => "audio",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "rtf" | "csv" => {
            "document"
        }
        "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" => "archive",
        "exe" | "msi" | "bat" | "cmd" | "com" => "installer",
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "c" | "cpp" | "h" | "hpp"
        | "json" | "yaml" | "yml" | "toml" => "code",
        _ => "other",
    };

    if category == "other" && has_magic(path, b"MZ") {
        return "installer".to_string();
    }
    if category == "other" && has_magic(path, b"%PDF") {
        return "document".to_string();
    }
    category.to_string()
}

fn has_magic(path: &Path, magic: &[u8]) -> bool {
    let mut buffer = vec![0u8; magic.len()];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut buffer))
        .is_ok()
        && buffer == magic
}

fn mime_for(category: &str, extension: &str) -> String {
    match (category, extension) {
        ("image", "jpg") | ("image", "jpeg") => "image/jpeg",
        ("image", "png") => "image/png",
        ("image", "gif") => "image/gif",
        ("video", "mp4") => "video/mp4",
        ("audio", "mp3") => "audio/mpeg",
        ("document", "pdf") => "application/pdf",
        ("archive", "zip") => "application/zip",
        ("installer", "exe") => "application/vnd.microsoft.portable-executable",
        ("code", _) => "text/plain",
        _ => "application/octet-stream",
    }
    .to_string()
}

#[allow(dead_code)]
fn _row_reader(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileRecord> {
    file_from_row(row)
}
