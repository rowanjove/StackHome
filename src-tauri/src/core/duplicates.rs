use crate::core::catalog;
use crate::core::metadata as metadata_engine;
use crate::core::tasks::{self, TaskHandle};
use crate::database::{self, open_connection};
use crate::filters::should_exclude_relative_path;
use crate::models::{
    BackupOptions, DuplicateGroup, DuplicatePlanRequest, DuplicateScanRequest, DuplicateScanResult,
    FileRecord, PlanPreview, PlannedOperation,
};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

const PREHASH_BYTES: u64 = 64 * 1024;
const HASH_ALGORITHM: &str = "blake3";

pub fn scan(
    app_handle: tauri::AppHandle,
    request: DuplicateScanRequest,
    task: TaskHandle,
) -> Result<DuplicateScanResult, String> {
    let root = PathBuf::from(request.root_path.trim());
    if !root.is_absolute() {
        return Err("重复项扫描路径必须是绝对路径。".to_string());
    }
    if !root.is_dir() {
        return Err(format!("重复项扫描路径不是目录: {}", root.display()));
    }

    let connection = open_connection()?;
    let options = BackupOptions {
        custom_exclude_patterns: request.custom_exclude_patterns.clone(),
        ..BackupOptions::default()
    };
    let started_at = Instant::now();
    let mut by_size: HashMap<u64, Vec<FileRecord>> = HashMap::new();
    let mut total_files = 0u64;

    for entry in WalkDir::new(&root).follow_links(false).into_iter() {
        if tasks::is_cancelled(&task) {
            break;
        }
        let entry = match entry {
            Ok(value) => value,
            Err(_) => continue,
        };
        let path = entry.path();
        let relative = match path.strip_prefix(&root) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if !relative.as_os_str().is_empty()
            && ((!request.include_hidden && contains_hidden(relative))
                || (!request.include_system_files && is_system_file(path))
                || should_exclude_relative_path(relative, &options))
        {
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(value) => value,
            Err(_) => continue,
        };
        total_files += 1;
        let record = catalog::file_record(
            path,
            &metadata,
            Some("custom".to_string()),
            metadata_engine::read_file(path).ok(),
        );
        database::upsert_file(&connection, &record)?;
        if let Some(file_metadata) = &record.metadata {
            let json = serde_json::to_string(file_metadata)
                .map_err(|error| format!("序列化 metadata 失败: {error}"))?;
            database::upsert_metadata(&connection, &record.id, "file", &json)?;
        }
        by_size.entry(record.size).or_default().push(record);
        if total_files % 100 == 0 {
            tasks::emit_progress(
                &app_handle,
                crate::models::TaskProgress {
                    task_id: task.id.clone(),
                    task_type: task.task_type.clone(),
                    phase: "hashing".to_string(),
                    completed_items: total_files,
                    total_items: 0,
                    completed_bytes: 0,
                    total_bytes: 0,
                    current_path: Some(path.display().to_string()),
                    speed_bytes_per_second: Some(
                        total_files / started_at.elapsed().as_secs().max(1),
                    ),
                    eta_seconds: None,
                },
            );
        }
    }

    let mut groups = Vec::new();
    for candidates in by_size.into_values().filter(|files| files.len() > 1) {
        let mut by_prehash: HashMap<String, Vec<FileRecord>> = HashMap::new();
        for file in candidates {
            let hash = hash_file(Path::new(&file.path), Some(PREHASH_BYTES))?;
            by_prehash.entry(hash).or_default().push(file);
        }
        for candidates in by_prehash.into_values().filter(|files| files.len() > 1) {
            let mut by_hash: HashMap<String, Vec<FileRecord>> = HashMap::new();
            for mut file in candidates {
                let hash = match database::cached_hash(
                    &connection,
                    &file.path,
                    file.size,
                    file.modified_at,
                    HASH_ALGORITHM,
                )? {
                    Some(value) => value,
                    None => {
                        let value = hash_file(Path::new(&file.path), None)?;
                        database::update_file_hash(
                            &connection,
                            &file.path,
                            file.size,
                            file.modified_at,
                            &value,
                            HASH_ALGORITHM,
                        )?;
                        value
                    }
                };
                file.hash = Some(hash.clone());
                file.hash_algorithm = Some(HASH_ALGORITHM.to_string());
                by_hash.entry(hash).or_default().push(file);
            }
            for (hash, files) in by_hash.into_iter().filter(|(_, files)| files.len() > 1) {
                let size = files[0].size;
                groups.push(DuplicateGroup {
                    id: format!("duplicate-{hash}"),
                    hash,
                    size,
                    reclaimable_size: size.saturating_mul((files.len() - 1) as u64),
                    files,
                });
            }
        }
    }
    groups.sort_by(|left, right| right.reclaimable_size.cmp(&left.reclaimable_size));
    let duplicate_files = groups.iter().map(|group| group.files.len() as u64).sum();
    let reclaimable_size = groups.iter().map(|group| group.reclaimable_size).sum();
    let status = if tasks::is_cancelled(&task) {
        "cancelled"
    } else {
        "completed"
    };
    tasks::emit_progress(
        &app_handle,
        crate::models::TaskProgress {
            task_id: task.id.clone(),
            task_type: task.task_type.clone(),
            phase: if status == "cancelled" {
                "cancelled"
            } else {
                "hashing"
            }
            .to_string(),
            completed_items: total_files,
            total_items: total_files,
            completed_bytes: 0,
            total_bytes: 0,
            current_path: None,
            speed_bytes_per_second: None,
            eta_seconds: Some(0),
        },
    );
    tasks::emit_completed(&app_handle, &task.id, &task.task_type, status);
    Ok(DuplicateScanResult {
        task_id: task.id,
        root_path: root.display().to_string(),
        total_files,
        duplicate_files,
        reclaimable_size,
        groups,
        status: status.to_string(),
    })
}

pub fn create_plan(request: DuplicatePlanRequest) -> Result<PlanPreview, String> {
    if request.files.is_empty() {
        return Err("没有选择要移至回收站的重复文件。".to_string());
    }
    let task = tasks::create("duplicate");
    let plan_id = format!("duplicate-plan-{}", database::now_millis());
    let connection = open_connection()?;
    database::insert_plan(&connection, &plan_id, &task.id, "ready")?;
    let operations = request
        .files
        .into_iter()
        .enumerate()
        .map(|(index, file)| PlannedOperation {
            id: format!("duplicate-operation-{}-{index}", database::now_millis()),
            operation_type: "recycle".to_string(),
            source_path: file.path,
            destination_path: None,
            reason: if request.reason.trim().is_empty() {
                "重复文件清理：移至 Windows 回收站".to_string()
            } else {
                request.reason.trim().to_string()
            },
            rule_id: None,
            conflict: None,
            status: "ready".to_string(),
            source_size: Some(file.size),
            source_modified_at: file.modified_at,
            tags: Vec::new(),
        })
        .collect::<Vec<_>>();
    for operation in &operations {
        database::insert_operation(&connection, &plan_id, &task.id, operation)?;
    }
    tasks::finish(&task);
    Ok(PlanPreview {
        id: plan_id,
        task_id: task.id,
        created_at: database::now_millis(),
        status: "ready".to_string(),
        operations,
    })
}

fn hash_file(path: &Path, limit: Option<u64>) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("打开 Hash 文件失败 {}: {error}", path.display()))?;
    if limit.is_some() {
        file.seek(SeekFrom::Start(0))
            .map_err(|error| format!("定位 Hash 文件失败: {error}"))?;
    }
    let mut hasher = blake3::Hasher::new();
    let mut remaining = limit.unwrap_or(u64::MAX);
    let mut buffer = vec![0u8; 1024 * 1024];
    while remaining > 0 {
        let requested = remaining.min(buffer.len() as u64) as usize;
        let read = file
            .read(&mut buffer[..requested])
            .map_err(|error| format!("读取 Hash 文件失败 {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn contains_hidden(path: &Path) -> bool {
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

#[cfg(test)]
mod tests {
    use super::hash_file;
    use std::fs;

    #[test]
    fn hash_cache_input_is_deterministic() {
        let path =
            std::env::temp_dir().join(format!("windows-easy-backup-hash-{}", std::process::id()));
        fs::write(&path, b"same content").unwrap();
        assert_eq!(
            hash_file(&path, None).unwrap(),
            hash_file(&path, None).unwrap()
        );
        let _ = fs::remove_file(path);
    }
}
