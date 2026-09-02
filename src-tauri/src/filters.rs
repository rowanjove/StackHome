use crate::models::BackupOptions;
use std::path::Path;

const BUILTIN_PATTERNS: &[&str] = &[
    "node_modules",
    ".git",
    ".cache",
    "Thumbs.db",
    "Desktop.ini",
    "*.tmp",
    "~$*",
    "Temp",
    "tmp",
];

pub fn builtin_patterns() -> Vec<String> {
    BUILTIN_PATTERNS
        .iter()
        .map(|pattern| pattern.to_string())
        .collect()
}

pub fn effective_patterns(options: &BackupOptions) -> Vec<String> {
    let mut patterns = Vec::new();

    if options.enable_smart_exclude {
        patterns.extend(builtin_patterns());
    }

    patterns.extend(
        options
            .custom_exclude_patterns
            .iter()
            .map(|pattern| pattern.trim())
            .filter(|pattern| !pattern.is_empty())
            .map(|pattern| pattern.to_string()),
    );

    patterns
}

pub fn should_exclude_path(path: &Path, patterns: &[String]) -> bool {
    let lowered_parts: Vec<String> = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect();
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    patterns.iter().any(|pattern| {
        let lowered_pattern = pattern.to_ascii_lowercase();

        lowered_parts
            .iter()
            .any(|part| wildcard_match(part, &lowered_pattern))
            || wildcard_match(&file_name, &lowered_pattern)
    })
}

pub fn should_exclude_relative_path(relative_path: &Path, options: &BackupOptions) -> bool {
    let patterns = effective_patterns(options);
    should_exclude_path(relative_path, &patterns)
}

fn wildcard_match(value: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if !pattern.contains('*') {
        return value == pattern;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    let mut remaining = value;

    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }

        if index == 0 && !pattern.starts_with('*') {
            if !remaining.starts_with(part) {
                return false;
            }
            remaining = &remaining[part.len()..];
            continue;
        }

        if index == parts.len() - 1 && !pattern.ends_with('*') {
            return remaining.ends_with(part);
        }

        if let Some(found) = remaining.find(part) {
            remaining = &remaining[(found + part.len())..];
        } else {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::{effective_patterns, should_exclude_path};
    use crate::models::{ArchiveFormat, BackupOptions};
    use std::path::PathBuf;

    #[test]
    fn excludes_node_modules_directory() {
        let options = BackupOptions {
            enable_smart_exclude: true,
            custom_exclude_patterns: vec![],
            compress_after_backup: false,
            archive_format: ArchiveFormat::Zip,
            compression_level: 6,
            send_notification: false,
            ..BackupOptions::default()
        };

        let path = PathBuf::from("D:\\Projects\\demo\\node_modules\\react\\index.js");
        let patterns = effective_patterns(&options);

        assert!(should_exclude_path(&path, &patterns));
    }

    #[test]
    fn excludes_custom_tmp_pattern() {
        let options = BackupOptions {
            enable_smart_exclude: false,
            custom_exclude_patterns: vec!["*.log".to_string()],
            compress_after_backup: false,
            archive_format: ArchiveFormat::Zip,
            compression_level: 6,
            send_notification: false,
            ..BackupOptions::default()
        };

        let path = PathBuf::from("D:\\Backup\\debug.log");
        let patterns = effective_patterns(&options);

        assert!(should_exclude_path(&path, &patterns));
    }
}
