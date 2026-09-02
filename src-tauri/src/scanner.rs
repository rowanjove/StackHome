use crate::filters::should_exclude_relative_path;
use crate::models::{BackupItem, BackupOptions, ScanResult};
use crate::paths::{
    collect_source_warnings, get_drive_free_space, probe_target_root, protected_path_warning,
    validate_target_root,
};
use std::path::Path;
use walkdir::WalkDir;

pub fn scan_backup_items(
    items: &[BackupItem],
    target_root: &Path,
    options: &BackupOptions,
) -> Result<ScanResult, String> {
    validate_target_root(items, target_root)?;

    let mut updated_items = Vec::with_capacity(items.len());
    let mut total_files = 0u64;
    let mut total_bytes = 0u64;
    let mut skipped_by_rule_count = 0u64;
    let mut warnings = Vec::new();

    if let Some(message) = protected_path_warning(target_root) {
        warnings.push(message);
    }

    let target_probe = probe_target_root(target_root)?;
    warnings.extend(target_probe.warnings.iter().cloned());

    for item in items {
        if !item.enabled {
            updated_items.push(item.clone());
            continue;
        }

        let mut file_count = 0u64;
        let mut item_bytes = 0u64;
        let source_root = Path::new(&item.source_path);

        for entry in WalkDir::new(&item.source_path).into_iter() {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    let relative_path = match path.strip_prefix(source_root) {
                        Ok(path) => path,
                        Err(_) => continue,
                    };

                    if !relative_path.as_os_str().is_empty()
                        && should_exclude_relative_path(relative_path, options)
                    {
                        skipped_by_rule_count += 1;
                        continue;
                    }

                    if entry.file_type().is_file() {
                        file_count += 1;
                        if let Ok(metadata) = entry.metadata() {
                            item_bytes += metadata.len();
                        }
                    }
                }
                Err(error) => {
                    warnings.push(format!("扫描时跳过不可访问项: {error}"));
                }
            }
        }

        total_files += file_count;
        total_bytes += item_bytes;

        let mut next = item.clone();
        next.file_count = Some(file_count);
        next.total_size = Some(item_bytes);
        updated_items.push(next);
    }

    let source_warnings = collect_source_warnings(items)?;

    let (target_drive_name, target_drive_free_bytes) = match get_drive_free_space(target_root) {
        Ok((name, bytes)) => (Some(name), Some(bytes)),
        Err(error) => {
            warnings.push(error);
            (None, None)
        }
    };

    Ok(ScanResult {
        items: updated_items,
        total_files,
        total_bytes,
        target_drive_free_bytes,
        target_drive_name,
        target_accessible: target_probe.accessible,
        target_writable: target_probe.writable,
        target_kind: target_probe.kind,
        warnings,
        source_warnings,
        skipped_by_rule_count,
    })
}

#[cfg(test)]
mod tests {
    use super::scan_backup_items;
    use crate::models::{BackupItem, BackupOptions};
    use std::fs;

    #[test]
    fn skips_excluded_entries_during_scan() {
        let root =
            std::env::temp_dir().join(format!("windows-easy-backup-scan-{}", std::process::id()));
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(source.join("node_modules")).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(source.join("keep.txt"), b"1234").unwrap();
        fs::write(source.join("node_modules").join("skip.txt"), b"12").unwrap();

        let item = BackupItem {
            id: "documents".to_string(),
            label: "文档".to_string(),
            source_path: source.to_string_lossy().to_string(),
            target_name: "文档".to_string(),
            enabled: true,
            category: "system".to_string(),
            description: None,
            is_custom: false,
            file_count: None,
            total_size: None,
        };

        let options = BackupOptions {
            enable_smart_exclude: true,
            custom_exclude_patterns: vec![],
            compress_after_backup: false,
            archive_format: crate::models::ArchiveFormat::Zip,
            compression_level: 6,
            send_notification: false,
            ..BackupOptions::default()
        };

        let result = scan_backup_items(&[item], &target, &options).unwrap();

        assert_eq!(result.total_files, 1);
        assert_eq!(result.skipped_by_rule_count, 2);
        assert!(result.target_accessible);
        assert!(result.target_writable);

        let _ = fs::remove_dir_all(&root);
    }
}
