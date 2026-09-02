use crate::core::catalog;
use crate::core::metadata as metadata_engine;
use crate::core::tasks::{self, TaskHandle};
use crate::models::{SimilarGroup, SimilarScanRequest, SimilarScanResult};
use image::imageops::FilterType;
use image::ImageReader;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

pub fn scan(
    app_handle: tauri::AppHandle,
    request: SimilarScanRequest,
    task: TaskHandle,
) -> Result<SimilarScanResult, String> {
    let root = PathBuf::from(request.root_path.trim());
    if !root.is_absolute() {
        return Err("相似图片扫描路径必须是绝对路径。".to_string());
    }
    if !root.is_dir() {
        return Err(format!("相似图片扫描路径不是目录: {}", root.display()));
    }

    let started_at = Instant::now();
    let mut images = Vec::new();
    for entry in WalkDir::new(&root).follow_links(false).into_iter() {
        if tasks::is_cancelled(&task) {
            break;
        }
        let entry = match entry {
            Ok(value) if value.file_type().is_file() => value,
            _ => continue,
        };
        let relative = entry.path().strip_prefix(&root).unwrap_or(entry.path());
        if !request.include_hidden
            && relative
                .components()
                .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
        {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(value) => value,
            Err(_) => continue,
        };
        let file = catalog::file_record(
            entry.path(),
            &metadata,
            Some("custom".to_string()),
            metadata_engine::read_file(entry.path()).ok(),
        );
        if file.category != "image" {
            continue;
        }
        let hash = match perceptual_hash(entry.path()) {
            Ok(value) => value,
            Err(_) => continue,
        };
        images.push((file, hash));
        if images.len() % 100 == 0 {
            tasks::emit_progress(
                &app_handle,
                crate::models::TaskProgress {
                    task_id: task.id.clone(),
                    task_type: task.task_type.clone(),
                    phase: "analyzing".to_string(),
                    completed_items: images.len() as u64,
                    total_items: 0,
                    completed_bytes: 0,
                    total_bytes: 0,
                    current_path: Some(entry.path().display().to_string()),
                    speed_bytes_per_second: Some(
                        images.len() as u64 / started_at.elapsed().as_secs().max(1),
                    ),
                    eta_seconds: None,
                },
            );
        }
    }

    let mut parent = (0..images.len()).collect::<Vec<_>>();
    let mut max_distance = vec![0u32; images.len()];
    let threshold = request.max_distance.min(64);
    let mut hash_index = Vec::new();
    let mut hash_root = None;
    for (index, (_, hash)) in images.iter().enumerate() {
        let mut matches = Vec::new();
        if let Some(root) = hash_root {
            query_hash_index(&hash_index, root, *hash, threshold, &mut matches);
        }
        for right in matches {
            let distance = (images[index].1 ^ images[right].1).count_ones();
            union(&mut parent, index, right);
            max_distance[index] = max_distance[index].max(distance);
            max_distance[right] = max_distance[right].max(distance);
        }
        if hash_root.is_none() {
            hash_root = Some(insert_hash_index(&mut hash_index, None, *hash, index));
        } else {
            insert_hash_index(&mut hash_index, hash_root, *hash, index);
        }
        if tasks::is_cancelled(&task) {
            break;
        }
    }

    let mut grouped: HashMap<usize, Vec<usize>> = HashMap::new();
    for index in 0..images.len() {
        let root_index = find(&mut parent, index);
        grouped.entry(root_index).or_default().push(index);
    }
    let mut groups = grouped
        .into_values()
        .filter(|indices| indices.len() > 1)
        .map(|indices| {
            let files = indices
                .iter()
                .map(|index| images[*index].0.clone())
                .collect::<Vec<_>>();
            let distance = indices
                .iter()
                .map(|index| max_distance[*index])
                .max()
                .unwrap_or_default();
            let reclaimable_size = files.iter().skip(1).map(|file| file.size).sum::<u64>();
            SimilarGroup {
                id: format!("similar-{}", files[0].id),
                distance,
                files,
                reclaimable_size,
            }
        })
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| right.reclaimable_size.cmp(&left.reclaimable_size));
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
                "analyzing"
            }
            .to_string(),
            completed_items: images.len() as u64,
            total_items: images.len() as u64,
            completed_bytes: 0,
            total_bytes: 0,
            current_path: None,
            speed_bytes_per_second: None,
            eta_seconds: Some(0),
        },
    );
    tasks::emit_completed(&app_handle, &task.id, &task.task_type, status);
    Ok(SimilarScanResult {
        task_id: task.id,
        root_path: root.display().to_string(),
        status: status.to_string(),
        total_images: images.len() as u64,
        groups,
    })
}

fn perceptual_hash(path: &Path) -> Result<u64, String> {
    let image = ImageReader::open(path)
        .map_err(|error| format!("打开图片失败: {error}"))?
        .with_guessed_format()
        .map_err(|error| format!("识别图片格式失败: {error}"))?
        .decode()
        .map_err(|error| format!("解码图片失败: {error}"))?;
    let image = image.resize_exact(8, 8, FilterType::Triangle).to_luma8();
    let average = image.pixels().map(|pixel| u64::from(pixel[0])).sum::<u64>() / 64;
    Ok(image
        .pixels()
        .enumerate()
        .fold(0u64, |hash, (index, pixel)| {
            if u64::from(pixel[0]) >= average {
                hash | (1u64 << index)
            } else {
                hash
            }
        }))
}

fn find(parent: &mut [usize], value: usize) -> usize {
    if parent[value] != value {
        let root = find(parent, parent[value]);
        parent[value] = root;
    }
    parent[value]
}

fn union(parent: &mut [usize], left: usize, right: usize) {
    let left_root = find(parent, left);
    let right_root = find(parent, right);
    if left_root != right_root {
        parent[right_root] = left_root;
    }
}

struct HashIndexNode {
    hash: u64,
    indices: Vec<usize>,
    children: HashMap<u32, usize>,
}

fn insert_hash_index(
    nodes: &mut Vec<HashIndexNode>,
    root: Option<usize>,
    hash: u64,
    index: usize,
) -> usize {
    let Some(mut node_index) = root else {
        nodes.push(HashIndexNode {
            hash,
            indices: vec![index],
            children: HashMap::new(),
        });
        return 0;
    };
    loop {
        let distance = (hash ^ nodes[node_index].hash).count_ones();
        if distance == 0 {
            nodes[node_index].indices.push(index);
            return node_index;
        }
        if let Some(child) = nodes[node_index].children.get(&distance).copied() {
            node_index = child;
            continue;
        }
        let child = nodes.len();
        nodes.push(HashIndexNode {
            hash,
            indices: vec![index],
            children: HashMap::new(),
        });
        nodes[node_index].children.insert(distance, child);
        return child;
    }
}

fn query_hash_index(
    nodes: &[HashIndexNode],
    node_index: usize,
    target: u64,
    threshold: u32,
    matches: &mut Vec<usize>,
) {
    let node = &nodes[node_index];
    let distance = (target ^ node.hash).count_ones();
    if distance <= threshold {
        matches.extend(node.indices.iter().copied());
    }
    let minimum = distance.saturating_sub(threshold);
    let maximum = distance.saturating_add(threshold).min(64);
    for (edge, child) in &node.children {
        if (*edge >= minimum) && (*edge <= maximum) {
            query_hash_index(nodes, *child, target, threshold, matches);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{insert_hash_index, perceptual_hash, query_hash_index};
    use image::{ImageBuffer, Rgba};
    use std::fs;

    #[test]
    fn perceptual_hash_is_stable_for_same_pixels() {
        let path = std::env::temp_dir().join(format!(
            "windows-easy-backup-similar-{}.png",
            std::process::id()
        ));
        let image = ImageBuffer::<Rgba<u8>, _>::from_pixel(8, 8, Rgba([200, 100, 50, 255]));
        image.save(&path).unwrap();
        assert_eq!(
            perceptual_hash(&path).unwrap(),
            perceptual_hash(&path).unwrap()
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn hash_index_finds_values_within_hamming_threshold() {
        let mut nodes = Vec::new();
        let root = insert_hash_index(&mut nodes, None, 0, 0);
        insert_hash_index(&mut nodes, Some(root), 0b11, 1);
        insert_hash_index(&mut nodes, Some(root), 1u64 << 20, 2);
        let mut matches = Vec::new();
        query_hash_index(&nodes, root, 0b1, 1, &mut matches);
        assert_eq!(matches, vec![0, 1]);
    }
}
