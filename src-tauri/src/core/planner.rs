use crate::core::rules;
use crate::core::tasks;
use crate::database::{self, open_connection};
use crate::models::{ConflictInfo, CreatePlanRequest, FileRecord, PlanPreview, PlannedOperation};
use chrono::{DateTime, NaiveDateTime};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_PLAN_ID: AtomicU64 = AtomicU64::new(1);

pub fn create_plan(request: CreatePlanRequest) -> Result<PlanPreview, String> {
    if request.files.is_empty() {
        return Err("没有可生成计划的文件。请先扫描一个目录。".to_string());
    }
    if !matches!(request.operation_type.as_str(), "rename" | "move" | "copy") {
        return Err(format!("不支持的操作类型: {}", request.operation_type));
    }

    let destination_root = PathBuf::from(request.destination_root.trim());
    if !destination_root.is_absolute() {
        return Err("计划目标目录必须是绝对路径。".to_string());
    }

    let task = tasks::create("organize");
    let plan_id = format!(
        "plan-{}-{}",
        database::now_millis(),
        NEXT_PLAN_ID.fetch_add(1, Ordering::Relaxed)
    );
    let mut operations = Vec::with_capacity(request.files.len());
    let mut reserved_destinations = HashSet::new();
    let selected_rule = request
        .rule_id
        .as_deref()
        .map(rules::find)
        .transpose()?
        .flatten();
    if request.rule_id.is_some() && selected_rule.is_none() {
        return Err("找不到所选整理规则。".to_string());
    }

    let mut sequence = 0u64;
    for (index, file) in request.files.iter().enumerate() {
        let mut effective_request = request.clone();
        let (destination_override, rule_tags) = if let Some(rule) = &selected_rule {
            if !rules::matches(rule, file) {
                continue;
            }
            let action = rules::action(rule);
            if action.action_type == "ignore" {
                continue;
            }
            effective_request.operation_type = action.action_type.clone();
            effective_request.rename_template = action
                .rename_template
                .clone()
                .or_else(|| request.rename_template.clone());
            effective_request.reason = format!("规则：{}", rule.name);
            sequence += 1;
            (
                action.destination_template.as_deref().map(|template| {
                    resolve_destination_template(template, file, sequence, &destination_root)
                }),
                Some(action.tags.clone()),
            )
        } else {
            sequence = index as u64 + 1;
            (None, None)
        };
        let mut operation = build_operation(
            file,
            &destination_root,
            &effective_request,
            sequence,
            destination_override.as_deref(),
            &mut reserved_destinations,
        );
        if let Some(tags) = rule_tags {
            operation.tags = tags;
        }
        operations.push(operation);
    }

    let status = if operations
        .iter()
        .any(|operation| operation.status == "invalid")
    {
        "invalid"
    } else if operations
        .iter()
        .any(|operation| operation.status == "conflict")
    {
        "conflict"
    } else {
        "ready"
    };

    let connection = open_connection()?;
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

fn build_operation(
    file: &FileRecord,
    destination_root: &Path,
    request: &CreatePlanRequest,
    sequence: u64,
    destination_override: Option<&Path>,
    reserved_destinations: &mut HashSet<String>,
) -> PlannedOperation {
    let source = PathBuf::from(&file.path);
    let operation_id = format!(
        "operation-{}-{}-{}",
        database::now_millis(),
        sequence,
        NEXT_PLAN_ID.fetch_add(1, Ordering::Relaxed)
    );
    let reason = if request.reason.trim().is_empty() {
        "按 Catalog 生成整理计划".to_string()
    } else {
        request.reason.trim().to_string()
    };
    let source_metadata = fs::metadata(&source).ok();
    let source_size = source_metadata.as_ref().map(fs::Metadata::len);
    let source_modified_at = source_metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64);

    let mut operation = PlannedOperation {
        id: operation_id,
        operation_type: request.operation_type.clone(),
        source_path: file.path.clone(),
        destination_path: None,
        reason,
        rule_id: request.rule_id.clone(),
        conflict: None,
        status: "ready".to_string(),
        source_size,
        source_modified_at,
        tags: Vec::new(),
    };

    if source_metadata.is_none() {
        operation.status = "invalid".to_string();
        operation.conflict = Some(ConflictInfo {
            kind: "source_missing".to_string(),
            message: "源文件在生成计划时不存在。".to_string(),
            suggested_path: None,
        });
        return operation;
    }
    if !source.is_absolute() {
        operation.status = "invalid".to_string();
        operation.conflict = Some(ConflictInfo {
            kind: "invalid_source_path".to_string(),
            message: "源文件路径必须是绝对路径。".to_string(),
            suggested_path: None,
        });
        return operation;
    }

    if request.operation_type == "tag" {
        if request.rule_id.is_none() {
            operation.status = "invalid".to_string();
            operation.conflict = Some(ConflictInfo {
                kind: "missing_rule".to_string(),
                message: "标签操作必须来自整理规则。".to_string(),
                suggested_path: None,
            });
        }
        return operation;
    }

    let filename = request
        .rename_template
        .as_deref()
        .map(|template| render_template(template, file, sequence))
        .unwrap_or_else(|| file.filename.clone());
    if let Err(message) = validate_filename(&filename) {
        operation.status = "invalid".to_string();
        operation.conflict = Some(ConflictInfo {
            kind: "invalid_filename".to_string(),
            message,
            suggested_path: None,
        });
        return operation;
    }

    let destination = if request.operation_type == "rename" {
        source
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(filename)
    } else if let Some(destination_override) = destination_override {
        destination_override.join(filename)
    } else {
        destination_root.join(&file.category).join(filename)
    };
    if same_path(&source, &destination) {
        operation.status = "invalid".to_string();
        operation.conflict = Some(ConflictInfo {
            kind: "same_path".to_string(),
            message: "源文件与目标文件相同，无需执行操作。".to_string(),
            suggested_path: None,
        });
        return operation;
    }

    if request.operation_type == "move" && is_inside(&destination, &source) {
        operation.status = "invalid".to_string();
        operation.conflict = Some(ConflictInfo {
            kind: "recursive_target".to_string(),
            message: "目标路径位于源文件目录内，可能导致递归整理。".to_string(),
            suggested_path: None,
        });
        return operation;
    }

    let mut final_destination = destination.clone();
    let destination_key = path_key(&final_destination);
    let is_existing =
        final_destination.exists() || reserved_destinations.contains(&destination_key);
    if is_existing {
        let reserved = reserved_destinations.contains(&destination_key);
        if !reserved && same_file_content(&source, &final_destination) {
            operation.status = "skipped".to_string();
            operation.conflict = Some(ConflictInfo {
                kind: "duplicate".to_string(),
                message: "目标文件与源文件内容相同，已跳过以避免创建重复副本。".to_string(),
                suggested_path: Some(final_destination.display().to_string()),
            });
        } else {
            match request.conflict_policy.as_str() {
                "auto_number" | "sequence" => {
                    final_destination = auto_number_path(&destination, reserved_destinations);
                    operation.conflict = Some(ConflictInfo {
                        kind: "auto_numbered".to_string(),
                        message: "目标已存在，已按默认策略自动编号。".to_string(),
                        suggested_path: Some(final_destination.display().to_string()),
                    });
                }
                "skip" => {
                    operation.status = "skipped".to_string();
                    operation.conflict = Some(ConflictInfo {
                        kind: "existing_target".to_string(),
                        message: "目标已存在，按跳过策略保留现状。".to_string(),
                        suggested_path: None,
                    });
                }
                "keep_newer" | "keep_larger" if !reserved => {
                    let existing_metadata = fs::metadata(&final_destination).ok();
                    let source_wins = match request.conflict_policy.as_str() {
                        "keep_newer" => compare_modified_time(&source, &final_destination),
                        "keep_larger" => existing_metadata
                            .as_ref()
                            .zip(source_size)
                            .map(|(metadata, size)| size > metadata.len())
                            .unwrap_or(false),
                        _ => false,
                    };
                    if source_wins {
                        operation.status = "conflict".to_string();
                        operation.conflict = Some(ConflictInfo {
                            kind: "source_preferred_requires_confirmation".to_string(),
                            message: format!(
                                "源文件按 {} 策略更优，但为避免静默覆盖，需确认后再处理目标文件。",
                                if request.conflict_policy == "keep_newer" {
                                    "更新时间"
                                } else {
                                    "文件大小"
                                }
                            ),
                            suggested_path: Some(final_destination.display().to_string()),
                        });
                    } else {
                        operation.status = "skipped".to_string();
                        operation.conflict = Some(ConflictInfo {
                            kind: "existing_target_preferred".to_string(),
                            message: format!(
                                "目标文件按 {} 策略更优，已跳过源文件。",
                                if request.conflict_policy == "keep_newer" {
                                    "更新时间"
                                } else {
                                    "文件大小"
                                }
                            ),
                            suggested_path: None,
                        });
                    }
                }
                "replace" => {
                    operation.status = "conflict".to_string();
                    operation.conflict = Some(ConflictInfo {
                        kind: "replace_requires_confirmation".to_string(),
                        message: "替换策略要求显式确认；当前执行器不会静默覆盖已有文件。"
                            .to_string(),
                        suggested_path: Some(final_destination.display().to_string()),
                    });
                }
                _ => {
                    operation.status = "conflict".to_string();
                    operation.conflict = Some(ConflictInfo {
                        kind: if reserved {
                            "duplicate_target"
                        } else {
                            "existing_target"
                        }
                        .to_string(),
                        message: "目标文件已存在，请选择冲突处理方式。".to_string(),
                        suggested_path: None,
                    });
                }
            }
        }
    }

    reserved_destinations.insert(path_key(&final_destination));
    operation.destination_path = Some(final_destination.display().to_string());
    operation
}

fn resolve_destination_template(
    template: &str,
    file: &FileRecord,
    sequence: u64,
    destination_root: &Path,
) -> PathBuf {
    let rendered = render_template_value(template, file, sequence);
    let path = PathBuf::from(rendered);
    if path.is_absolute() {
        path
    } else {
        destination_root.join(path)
    }
}

pub fn render_template(template: &str, file: &FileRecord, sequence: u64) -> String {
    let mut rendered = render_template_value(template, file, sequence);
    if !rendered.contains('.') && !file.extension.is_empty() {
        rendered.push('.');
        rendered.push_str(&file.extension);
    }
    rendered
}

fn render_template_value(template: &str, file: &FileRecord, sequence: u64) -> String {
    let mut rendered = String::with_capacity(template.len());
    let mut cursor = 0;
    while let Some(start_offset) = template[cursor..].find("{{") {
        let start = cursor + start_offset;
        rendered.push_str(&template[cursor..start]);
        let Some(end_offset) = template[start + 2..].find("}}") else {
            rendered.push_str(&template[start..]);
            cursor = template.len();
            break;
        };
        let end = start + 2 + end_offset;
        let token = &template[start + 2..end];
        rendered.push_str(&resolve_template_token(token, file, sequence));
        cursor = end + 2;
    }
    if cursor < template.len() {
        rendered.push_str(&template[cursor..]);
    }
    rendered
}

fn resolve_template_token(token: &str, file: &FileRecord, sequence: u64) -> String {
    let (name, argument) = token
        .split_once(':')
        .map(|(name, argument)| (name, Some(argument)))
        .unwrap_or((token, None));
    match name {
        "name" => file.filename.clone(),
        "stem" => file.stem.clone(),
        "ext" => file.extension.clone(),
        "parent" => Path::new(&file.path)
            .parent()
            .and_then(|path| path.file_name())
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default(),
        "seq" => argument
            .and_then(|width| width.parse::<usize>().ok())
            .map(|width| format!("{sequence:0width$}"))
            .unwrap_or_else(|| sequence.to_string()),
        "hash" => {
            let hash = file.hash.as_deref().unwrap_or("unknown-hash");
            argument
                .and_then(|width| width.parse::<usize>().ok())
                .map(|width| hash.chars().take(width).collect())
                .unwrap_or_else(|| hash.to_string())
        }
        "created" => format_timestamp(file.created_at, argument),
        "modified" => format_timestamp(file.modified_at, argument),
        "date" => format_timestamp(file.modified_at, argument),
        "year" => timestamp_part(file.modified_at, "%Y"),
        "month" => timestamp_part(file.modified_at, "%m"),
        "day" => timestamp_part(file.modified_at, "%d"),
        "image.width" => metadata_value(file.metadata.as_ref().and_then(|metadata| metadata.width)),
        "image.height" => {
            metadata_value(file.metadata.as_ref().and_then(|metadata| metadata.height))
        }
        "exif.date" => format_exif_date(
            file.metadata
                .as_ref()
                .and_then(|metadata| metadata.exif_date.as_deref()),
            argument,
        ),
        "exif.camera" => file
            .metadata
            .as_ref()
            .map(|metadata| {
                [
                    metadata.camera_make.as_deref(),
                    metadata.camera_model.as_deref(),
                ]
                .into_iter()
                .flatten()
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
            })
            .unwrap_or_default(),
        "audio.artist" => metadata_text(
            file.metadata
                .as_ref()
                .and_then(|metadata| metadata.artist.as_deref()),
        ),
        "audio.album" => metadata_text(
            file.metadata
                .as_ref()
                .and_then(|metadata| metadata.album.as_deref()),
        ),
        "audio.title" => metadata_text(
            file.metadata
                .as_ref()
                .and_then(|metadata| metadata.title.as_deref()),
        ),
        "audio.track" => metadata_value(file.metadata.as_ref().and_then(|metadata| metadata.track)),
        _ => format!("{{{{{token}}}}}"),
    }
}

fn format_timestamp(timestamp: Option<i64>, format: Option<&str>) -> String {
    let format = format.unwrap_or("yyyy-MM-dd");
    timestamp
        .and_then(DateTime::from_timestamp_millis)
        .map(|value| value.format(&translate_date_format(format)).to_string())
        .unwrap_or_else(|| "unknown-date".to_string())
}

fn timestamp_part(timestamp: Option<i64>, format: &str) -> String {
    timestamp
        .and_then(DateTime::from_timestamp_millis)
        .map(|value| value.format(format).to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_exif_date(value: Option<&str>, format: Option<&str>) -> String {
    let Some(value) = value else {
        return "unknown-date".to_string();
    };
    let Some(format) = format else {
        return value.to_string();
    };
    NaiveDateTime::parse_from_str(value, "%Y:%m:%d %H:%M:%S")
        .map(|date| date.format(&translate_date_format(format)).to_string())
        .unwrap_or_else(|_| value.to_string())
}

fn translate_date_format(format: &str) -> String {
    format
        .replace("yyyy", "%Y")
        .replace("MM", "%m")
        .replace("dd", "%d")
        .replace("HH", "%H")
        .replace("mm", "%M")
        .replace("ss", "%S")
}

fn metadata_value<T: ToString>(value: Option<T>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn metadata_text(value: Option<&str>) -> String {
    value.unwrap_or("unknown").to_string()
}

fn compare_modified_time(source: &Path, destination: &Path) -> bool {
    let source_time = fs::metadata(source)
        .ok()
        .and_then(|metadata| metadata.modified().ok());
    let destination_time = fs::metadata(destination)
        .ok()
        .and_then(|metadata| metadata.modified().ok());
    source_time
        .zip(destination_time)
        .map(|(source, destination)| source > destination)
        .unwrap_or(false)
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

fn validate_filename(filename: &str) -> Result<(), String> {
    if filename.trim().is_empty() || filename == "." || filename == ".." {
        return Err("文件名不能为空或为保留目录名。".to_string());
    }
    if filename
        .chars()
        .any(|character| "<>:\"/\\|?*".contains(character))
    {
        return Err("文件名包含 Windows 非法字符。".to_string());
    }
    let stem = filename
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && matches!(&stem[..3], "COM" | "LPT")
            && stem.as_bytes()[3].is_ascii_digit())
    {
        return Err("文件名使用了 Windows 保留设备名。".to_string());
    }
    Ok(())
}

fn same_path(source: &Path, destination: &Path) -> bool {
    path_key(source) == path_key(destination)
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase()
}

fn is_inside(path: &Path, directory: &Path) -> bool {
    let normalized_path = normalize_for_compare(path);
    let normalized_directory = normalize_for_compare(directory);
    normalized_path.starts_with(&normalized_directory)
}

fn normalize_for_compare(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn auto_number_path(path: &Path, reserved: &HashSet<String>) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_string());
    for number in 2..100_000u32 {
        let candidate_name = match extension.as_deref() {
            Some(value) if !value.is_empty() => format!("{stem} ({number}).{value}"),
            _ => format!("{stem} ({number})"),
        };
        let candidate = parent.join(candidate_name);
        if !candidate.exists() && !reserved.contains(&path_key(&candidate)) {
            return candidate;
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::{build_operation, render_template, validate_filename};
    use crate::models::{CreatePlanRequest, FileMetadata, FileRecord};
    use std::collections::HashSet;
    use std::fs;

    fn record() -> FileRecord {
        FileRecord {
            id: "id".to_string(),
            path: r"C:\Downloads\photo.jpg".to_string(),
            filename: "photo.jpg".to_string(),
            stem: "photo".to_string(),
            extension: "jpg".to_string(),
            size: 10,
            created_at: Some(1_700_000_000_000),
            modified_at: Some(1_700_000_000_000),
            accessed_at: None,
            mime: Some("image/jpeg".to_string()),
            category: "image".to_string(),
            source_type: Some("downloads".to_string()),
            hash: None,
            hash_algorithm: None,
            metadata: None,
            tags: vec![],
        }
    }

    #[test]
    fn renders_sequence_and_extension() {
        assert_eq!(
            render_template("{{year}}-{{seq:03}}", &record(), 7),
            "2023-007.jpg"
        );
    }

    #[test]
    fn renders_metadata_hash_and_date_formats() {
        let mut file = record();
        file.hash = Some("abcdef123456".to_string());
        file.metadata = Some(FileMetadata {
            width: Some(1920),
            height: Some(1080),
            exif_date: Some("2024:02:03 04:05:06".to_string()),
            camera_make: Some("OpenAI".to_string()),
            camera_model: Some("Cam 1".to_string()),
            artist: Some("Artist".to_string()),
            ..FileMetadata::default()
        });
        assert_eq!(
            render_template(
                "{{created:yyyy-MM-dd}}-{{exif.date:yyyyMMdd_HHmmss}}-{{hash:8}}-{{image.width}}-{{image.height}}-{{exif.camera}}-{{audio.artist}}",
                &file,
                1
            ),
            "2023-11-14-20240203_040506-abcdef12-1920-1080-OpenAI Cam 1-Artist.jpg"
        );
    }

    #[test]
    fn rejects_windows_reserved_names() {
        assert!(validate_filename("CON.txt").is_err());
        assert!(validate_filename("a:b.txt").is_err());
    }

    #[test]
    fn planner_only_reads_source_files() {
        let root =
            std::env::temp_dir().join(format!("windows-easy-backup-plan-{}", std::process::id()));
        let source = root.join("source.txt");
        let destination = root.join("destination");
        fs::create_dir_all(&destination).unwrap();
        fs::write(&source, b"keep me").unwrap();
        let file = FileRecord {
            path: source.to_string_lossy().to_string(),
            id: source.to_string_lossy().to_string(),
            filename: "source.txt".to_string(),
            stem: "source".to_string(),
            extension: "txt".to_string(),
            size: 7,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            mime: Some("text/plain".to_string()),
            category: "document".to_string(),
            source_type: None,
            hash: None,
            hash_algorithm: None,
            metadata: None,
            tags: vec![],
        };
        let request = CreatePlanRequest {
            files: vec![],
            destination_root: destination.to_string_lossy().to_string(),
            operation_type: "move".to_string(),
            rename_template: None,
            conflict_policy: "auto_number".to_string(),
            reason: String::new(),
            rule_id: None,
        };
        let operation =
            build_operation(&file, &destination, &request, 1, None, &mut HashSet::new());
        assert_eq!(operation.status, "ready");
        assert_eq!(fs::read(&source).unwrap(), b"keep me");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn planner_skips_existing_file_with_same_content() {
        let root = std::env::temp_dir().join(format!(
            "windows-easy-backup-duplicate-content-{}",
            std::process::id()
        ));
        let source = root.join("source.txt");
        let destination = root.join("destination");
        let target = destination.join("document").join("source.txt");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&source, b"identical content").unwrap();
        fs::write(&target, b"identical content").unwrap();

        let mut file = record();
        file.id = source.to_string_lossy().to_string();
        file.path = source.to_string_lossy().to_string();
        file.filename = "source.txt".to_string();
        file.stem = "source".to_string();
        file.extension = "txt".to_string();
        file.size = 16;
        file.category = "document".to_string();
        let request = CreatePlanRequest {
            files: vec![],
            destination_root: destination.to_string_lossy().to_string(),
            operation_type: "copy".to_string(),
            rename_template: None,
            conflict_policy: "auto_number".to_string(),
            reason: String::new(),
            rule_id: None,
        };
        let operation =
            build_operation(&file, &destination, &request, 1, None, &mut HashSet::new());
        assert_eq!(operation.status, "skipped");
        assert_eq!(operation.conflict.unwrap().kind, "duplicate");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn planner_auto_numbers_existing_target() {
        let root = std::env::temp_dir().join(format!(
            "windows-easy-backup-conflict-{}",
            std::process::id()
        ));
        let source = root.join("source.txt");
        let destination = root.join("destination");
        fs::create_dir_all(destination.join("document")).unwrap();
        fs::write(&source, b"source").unwrap();
        fs::write(destination.join("document").join("source.txt"), b"existing").unwrap();
        let file = FileRecord {
            id: source.to_string_lossy().to_string(),
            path: source.to_string_lossy().to_string(),
            filename: "source.txt".to_string(),
            stem: "source".to_string(),
            extension: "txt".to_string(),
            size: 6,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            mime: None,
            category: "document".to_string(),
            source_type: None,
            hash: None,
            hash_algorithm: None,
            metadata: None,
            tags: vec![],
        };
        let request = CreatePlanRequest {
            files: vec![],
            destination_root: destination.to_string_lossy().to_string(),
            operation_type: "move".to_string(),
            rename_template: None,
            conflict_policy: "auto_number".to_string(),
            reason: String::new(),
            rule_id: None,
        };
        let operation =
            build_operation(&file, &destination, &request, 1, None, &mut HashSet::new());
        assert_eq!(operation.status, "ready");
        assert!(operation
            .destination_path
            .unwrap()
            .contains("source (2).txt"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn planner_keep_larger_skips_smaller_existing_target() {
        let root = std::env::temp_dir().join(format!(
            "windows-easy-backup-keep-larger-{}",
            std::process::id()
        ));
        let source = root.join("source.txt");
        let destination = root.join("destination");
        fs::create_dir_all(destination.join("document")).unwrap();
        fs::write(&source, b"small").unwrap();
        fs::write(
            destination.join("document").join("source.txt"),
            b"larger existing",
        )
        .unwrap();
        let mut file = record();
        file.id = source.to_string_lossy().to_string();
        file.path = source.to_string_lossy().to_string();
        file.filename = "source.txt".to_string();
        file.stem = "source".to_string();
        file.extension = "txt".to_string();
        file.size = 5;
        file.category = "document".to_string();
        let request = CreatePlanRequest {
            files: vec![],
            destination_root: destination.to_string_lossy().to_string(),
            operation_type: "copy".to_string(),
            rename_template: None,
            conflict_policy: "keep_larger".to_string(),
            reason: String::new(),
            rule_id: None,
        };
        let operation =
            build_operation(&file, &destination, &request, 1, None, &mut HashSet::new());
        assert_eq!(operation.status, "skipped");
        assert_eq!(
            operation.conflict.unwrap().kind,
            "existing_target_preferred"
        );
        let _ = fs::remove_dir_all(root);
    }
}
