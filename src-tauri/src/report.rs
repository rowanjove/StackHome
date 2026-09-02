use crate::models::{BackupError, BackupItem, BackupSummary};
use chrono::{DateTime, Local};
use std::fs;
use std::path::{Path, PathBuf};

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

pub fn render_report(
    generated_at: DateTime<Local>,
    source_root: &str,
    target_root: &Path,
    items: &[BackupItem],
    summary: &BackupSummary,
) -> String {
    let status_text = match summary.status.as_str() {
        "done" => "完成",
        "cancelled" => "用户已取消",
        _ => "异常终止",
    };

    let selected_items = items
        .iter()
        .filter(|item| item.enabled)
        .map(|item| format!("  [x] {}", item.label))
        .collect::<Vec<_>>()
        .join("\n");

    let error_lines = if summary.errors.is_empty() {
        String::from("  无")
    } else {
        render_errors(&summary.errors)
    };

    let archive_lines = match (
        &summary.archive_path,
        &summary.archive_format,
        &summary.archive_error,
    ) {
        (Some(path), Some(format), _) => format!("  压缩格式：{format}\r\n  压缩文件：{path}"),
        (None, Some(format), Some(error)) => {
            format!("  压缩格式：{format}\r\n  压缩结果：失败\r\n  失败原因：{error}")
        }
        _ => String::from("  未启用压缩"),
    };
    let snapshot_lines = format!(
        "  Snapshot：{}\r\n  manifest：{}\r\n  校验：{}",
        summary.snapshot_id.as_deref().unwrap_or("未生成"),
        summary.manifest_path.as_deref().unwrap_or("未生成"),
        summary.verify_status.as_deref().unwrap_or("未执行")
    );

    format!(
        "================================\r\nStackHome · 归栈备份报告\r\n================================\r\n\r\n备份时间：{}\r\n备份状态：{}\r\n耗时：{} 秒\r\n源用户目录：{}\r\n目标备份目录：{}\r\n\r\n备份项目：\r\n{}\r\n\r\n统计：\r\n  总文件数：{}\r\n  成功文件数：{}\r\n  失败文件数：{}\r\n  规则跳过：{}\r\n  总大小：{} bytes\r\n  已复制大小：{} bytes\r\n\r\nSnapshot：\r\n{}\r\n\r\n压缩结果：\r\n{}\r\n\r\n失败文件列表：\r\n{}\r\n\r\n================================\r\n",
        generated_at.format("%Y-%m-%d %H:%M:%S"),
        status_text,
        summary.duration_seconds,
        source_root,
        target_root.display(),
        selected_items,
        summary.total_files,
        summary.success_files,
        summary.failed_files,
        summary.skipped_by_rule_count,
        summary.total_bytes,
        summary.copied_bytes,
        snapshot_lines,
        archive_lines,
        error_lines
    )
}

fn render_errors(errors: &[BackupError]) -> String {
    errors
        .iter()
        .enumerate()
        .map(|(index, error)| format!("  {}. {} - {}", index + 1, error.source_path, error.reason))
        .collect::<Vec<_>>()
        .join("\r\n")
}

pub fn write_utf8_bom_file(path: &Path, content: &str) -> Result<PathBuf, String> {
    let mut bytes = Vec::with_capacity(UTF8_BOM.len() + content.len());
    bytes.extend_from_slice(UTF8_BOM);
    bytes.extend_from_slice(content.as_bytes());
    fs::write(path, bytes).map_err(|error| format!("写入文件失败 {}: {error}", path.display()))?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::render_report;
    use crate::models::{BackupError, BackupItem, BackupSummary};
    use chrono::{Local, TimeZone};
    use std::path::PathBuf;

    #[test]
    fn renders_cancelled_report_with_error_list() {
        let generated_at = Local.with_ymd_and_hms(2026, 5, 7, 14, 30, 22).unwrap();
        let summary = BackupSummary {
            status: "cancelled".to_string(),
            total_files: 1200,
            success_files: 1198,
            failed_files: 2,
            skipped_by_rule_count: 3,
            total_bytes: 1024,
            copied_bytes: 1000,
            duration_seconds: 522,
            errors: vec![BackupError {
                source_path: "C:\\Users\\Admin\\Downloads\\temp.lock".to_string(),
                target_path: "D:\\Backup\\下载\\temp.lock".to_string(),
                reason: "文件被占用".to_string(),
            }],
            archive_format: Some("zip".to_string()),
            archive_error: Some("压缩中断".to_string()),
            report_path: String::new(),
            log_path: String::new(),
            backup_root: "D:\\Backup".to_string(),
            archive_path: None,
            snapshot_id: None,
            manifest_path: None,
            verify_status: None,
        };

        let report = render_report(
            generated_at,
            "C:\\Users\\Admin",
            &PathBuf::from("D:\\Backup"),
            &[BackupItem {
                id: "downloads".to_string(),
                label: "下载".to_string(),
                source_path: "C:\\Users\\Admin\\Downloads".to_string(),
                target_name: "下载".to_string(),
                enabled: true,
                category: "system".to_string(),
                description: None,
                is_custom: false,
                file_count: Some(1),
                total_size: Some(1),
            }],
            &summary,
        );

        assert!(report.contains("用户已取消"));
        assert!(report.contains("C:\\Users\\Admin\\Downloads\\temp.lock - 文件被占用"));
        assert!(report.contains("压缩结果"));
    }
}
