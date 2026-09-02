use crate::models::{ArchiveFormat, BackupOptions};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

type ProgressCallback<'a> = &'a mut dyn FnMut(String, u64, u64) -> Result<(), String>;

pub fn create_archive(
    backup_root: &Path,
    options: &BackupOptions,
    on_progress: ProgressCallback<'_>,
) -> Result<PathBuf, String> {
    match options.archive_format {
        ArchiveFormat::Zip => {
            create_zip_archive(backup_root, options.compression_level, on_progress)
        }
        ArchiveFormat::SevenZ => {
            create_7z_archive(backup_root, options.compression_level, on_progress)
        }
    }
}

fn create_zip_archive(
    backup_root: &Path,
    compression_level: u8,
    on_progress: ProgressCallback<'_>,
) -> Result<PathBuf, String> {
    let archive_path = backup_root.with_extension("zip");
    let archive_file = File::create(&archive_path)
        .map_err(|error| format!("无法创建 ZIP 文件 {}: {error}", archive_path.display()))?;
    let mut zip = ZipWriter::new(archive_file);
    let level = compression_level.min(9);
    let method = if level == 0 {
        CompressionMethod::Stored
    } else {
        CompressionMethod::Deflated
    };
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .compression_level(Some(i64::from(level)));
    let files = collect_files(backup_root)?;
    let total_bytes = files.iter().map(|(_, _, size)| *size).sum::<u64>().max(1);
    let mut processed_bytes = 0u64;

    for (path, archive_name, size) in files {
        on_progress(
            format!("正在压缩 {}", archive_name),
            processed_bytes,
            total_bytes,
        )?;
        zip.start_file(&archive_name, options)
            .map_err(|error| format!("写入 ZIP 文件头失败: {error}"))?;

        let mut source = File::open(&path)
            .map_err(|error| format!("打开待压缩文件失败 {}: {error}", path.display()))?;
        let mut buffer = [0u8; 1024 * 1024];

        loop {
            let read = source
                .read(&mut buffer)
                .map_err(|error| format!("读取待压缩文件失败 {}: {error}", path.display()))?;
            if read == 0 {
                break;
            }
            zip.write_all(&buffer[..read])
                .map_err(|error| format!("写入 ZIP 文件内容失败: {error}"))?;
            processed_bytes += read as u64;
            on_progress(archive_name.clone(), processed_bytes, total_bytes)?;
        }

        if size == 0 {
            on_progress(archive_name.clone(), processed_bytes, total_bytes)?;
        }
    }

    zip.finish()
        .map_err(|error| format!("完成 ZIP 文件失败: {error}"))?;

    Ok(archive_path)
}

fn create_7z_archive(
    backup_root: &Path,
    compression_level: u8,
    on_progress: ProgressCallback<'_>,
) -> Result<PathBuf, String> {
    let archive_path = backup_root.with_extension("7z");
    let executable = find_7z_executable()?;
    let files = collect_files(backup_root)?;
    let total_files = files.len() as u64;

    on_progress(String::from("正在准备 7Z 压缩"), 0, total_files.max(1))?;

    let mut command = Command::new(&executable);
    command
        .arg("a")
        .arg("-t7z")
        .arg(format!("-mx={}", compression_level.min(9)))
        .arg("-bsp1")
        .arg("-bso1")
        .arg("-bse1")
        .arg(&archive_path)
        .arg(backup_root)
        .current_dir(
            backup_root
                .parent()
                .ok_or_else(|| String::from("无法定位压缩目录的父目录"))?,
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("启动 7Z 压缩失败 {}: {error}", executable.display()))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| String::from("无法读取 7Z 标准输出"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| String::from("无法读取 7Z 错误输出"))?;

    let mut stdout_reader = BufReader::new(stdout);
    let stderr_handle = thread::spawn(move || {
        let mut stderr_reader = BufReader::new(stderr);
        let mut stderr_text = String::new();
        let _ = stderr_reader.read_to_string(&mut stderr_text);
        stderr_text
    });

    let mut line = String::new();
    let mut processed_files = 0u64;

    loop {
        line.clear();
        let read = stdout_reader
            .read_line(&mut line)
            .map_err(|error| format!("读取 7Z 进度失败: {error}"))?;
        if read == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.contains('%') || trimmed.starts_with("Compressing") {
            processed_files = (processed_files + 1).min(total_files.max(1));
            on_progress(trimmed.to_string(), processed_files, total_files.max(1))?;
        }
    }

    let status = child
        .wait()
        .map_err(|error| format!("等待 7Z 进程结束失败: {error}"))?;

    let stderr_text = stderr_handle.join().unwrap_or_default();

    if !status.success() {
        return Err(format!(
            "7Z 压缩失败{}",
            if stderr_text.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", stderr_text.trim())
            }
        ));
    }

    on_progress(
        String::from("7Z 压缩完成"),
        total_files.max(1),
        total_files.max(1),
    )?;
    Ok(archive_path)
}

fn collect_files(backup_root: &Path) -> Result<Vec<(PathBuf, String, u64)>, String> {
    let mut files = Vec::new();

    for entry in WalkDir::new(backup_root) {
        let entry = entry.map_err(|error| format!("遍历压缩目录失败: {error}"))?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path().to_path_buf();
        let relative = path
            .strip_prefix(backup_root)
            .map_err(|error| format!("计算压缩相对路径失败: {error}"))?;
        let archive_name = relative.to_string_lossy().replace('\\', "/");
        let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        files.push((path, archive_name, size));
    }

    Ok(files)
}

fn find_7z_executable() -> Result<PathBuf, String> {
    let candidates = ["7z.exe", "7za.exe", "7zr.exe"];
    let output = Command::new("where")
        .args(candidates)
        .output()
        .map_err(|error| format!("无法查找 7Z 可执行文件: {error}"))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(first) = stdout.lines().find(|line| !line.trim().is_empty()) {
            return Ok(PathBuf::from(first.trim()));
        }
    }

    Err(String::from(
        "未找到 7Z 可执行文件，请先安装 7-Zip 或确保 7za.exe 可用。",
    ))
}

#[cfg(test)]
mod tests {
    use super::{create_archive, find_7z_executable};
    use crate::models::{ArchiveFormat, BackupOptions};
    use std::fs;

    fn build_options(format: ArchiveFormat) -> BackupOptions {
        BackupOptions {
            enable_smart_exclude: true,
            custom_exclude_patterns: vec![],
            compress_after_backup: true,
            archive_format: format,
            compression_level: 6,
            send_notification: false,
            ..BackupOptions::default()
        }
    }

    #[test]
    fn creates_zip_from_backup_root() {
        let root = std::env::temp_dir().join(format!(
            "windows-easy-backup-archive-{}",
            std::process::id()
        ));
        let backup_root = root.join("WindowsBackup_2026-05-07_1430");
        fs::create_dir_all(backup_root.join("docs")).unwrap();
        fs::write(backup_root.join("docs").join("a.txt"), b"hello zip").unwrap();

        let mut progress_calls = 0u64;
        let archive = create_archive(
            &backup_root,
            &build_options(ArchiveFormat::Zip),
            &mut |_, _, _| {
                progress_calls += 1;
                Ok(())
            },
        )
        .unwrap();

        assert!(archive.exists());
        assert!(progress_calls > 0);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_file(&archive);
    }

    #[test]
    fn finds_7z_when_available() {
        let result = find_7z_executable();
        if let Ok(path) = result {
            assert!(path.exists());
        }
    }
}
