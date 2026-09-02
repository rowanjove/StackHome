use crate::models::BackupSummary;
use std::process::Command;

pub fn notify_backup_result(summary: &BackupSummary) -> Result<(), String> {
    #[cfg(windows)]
    {
        let title = match summary.status.as_str() {
            "done" if summary.archive_error.is_some() => "StackHome · 备份完成，压缩失败",
            "done" => "StackHome · 备份完成",
            "cancelled" => "StackHome · 备份已取消",
            _ => "StackHome · 备份异常",
        };

        let mut lines = Vec::new();
        lines.push(format!("成功 {} 个文件", summary.success_files));
        if summary.failed_files > 0 {
            lines.push(format!("失败 {} 个文件", summary.failed_files));
        }
        if summary.skipped_by_rule_count > 0 {
            lines.push(format!("规则跳过 {} 项", summary.skipped_by_rule_count));
        }
        if let Some(format) = &summary.archive_format {
            if let Some(path) = &summary.archive_path {
                lines.push(format!("已生成 {} 压缩包", format.to_uppercase()));
                lines.push(shorten_path(path));
            } else if let Some(error) = &summary.archive_error {
                lines.push(format!("{} 压缩失败", format.to_uppercase()));
                lines.push(error.clone());
            }
        }

        let body = escape_ps_single_quote(&lines.join(" | "));
        let title = escape_ps_single_quote(title);

        std::thread::spawn(move || {
            let script = format!(
                "[reflection.assembly]::LoadWithPartialName('System.Windows.Forms') | Out-Null; \
                 [reflection.assembly]::LoadWithPartialName('System.Drawing') | Out-Null; \
                 $n=New-Object System.Windows.Forms.NotifyIcon; \
                 $n.Icon=[System.Drawing.SystemIcons]::Information; \
                 $n.BalloonTipTitle='{title}'; \
                 $n.BalloonTipText='{body}'; \
                 $n.Visible=$true; \
                 $n.ShowBalloonTip(4000); \
                 Start-Sleep -Seconds 4; \
                 $n.Dispose();"
            );

            let _ = Command::new("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
                .output();
        });
    }

    Ok(())
}

fn escape_ps_single_quote(text: &str) -> String {
    text.replace('\'', "''")
}

fn shorten_path(path: &str) -> String {
    const MAX_LEN: usize = 80;
    if path.chars().count() <= MAX_LEN {
        return path.to_string();
    }

    let tail_len = 46;
    let tail: String = path
        .chars()
        .rev()
        .take(tail_len)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{}", tail)
}
