use crate::models::{FileMetadata, MetadataReadResult};
use chrono::{DateTime, Utc};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MAX_READ_BYTES: usize = 8 * 1024 * 1024;

pub fn read_file(path: &Path) -> Result<FileMetadata, String> {
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let mut file = File::open(path).map_err(|error| format!("打开 metadata 文件失败: {error}"))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_READ_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("读取 metadata 失败: {error}"))?;

    let mut metadata = FileMetadata::default();
    let actual_kind = actual_kind(&bytes);
    metadata.extension_mismatch = extension_kind(&extension)
        .zip(actual_kind)
        .is_some_and(|(expected, actual)| expected != actual);

    if matches!(actual_kind, Some("image")) {
        if let Some((width, height)) = image_dimensions(&bytes, &extension) {
            metadata.width = Some(width);
            metadata.height = Some(height);
        }
        read_exif(&bytes, &mut metadata);
    }
    if matches!(actual_kind, Some("audio")) || extension == "mp3" {
        read_id3(&bytes, &mut metadata);
        if extension == "wav" {
            metadata.duration_seconds = wav_duration(&bytes);
        }
    }
    if matches!(actual_kind, Some("video")) {
        read_mp4_metadata(&bytes, &mut metadata);
    }
    if actual_kind.is_none() && !bytes.is_empty() {
        metadata.unsupported = true;
    }

    Ok(metadata)
}

pub fn read_result(path: &Path) -> Result<MetadataReadResult, String> {
    Ok(MetadataReadResult {
        path: path.display().to_string(),
        metadata: read_file(path)?,
    })
}

fn actual_kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || bytes.starts_with(b"BM")
        || bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP")
        || bytes.starts_with(b"\xff\xd8\xff")
    {
        return Some("image");
    }
    if bytes.starts_with(b"MZ") {
        return Some("installer");
    }
    if bytes.starts_with(b"%PDF") {
        return Some("document");
    }
    if bytes.starts_with(b"ID3")
        || bytes.starts_with(b"\xff\xfb")
        || bytes.starts_with(b"\xff\xfa")
        || bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE")
    {
        return Some("audio");
    }
    if bytes.len() > 12 && bytes.get(4..8) == Some(b"ftyp") {
        return Some("video");
    }
    None
}

fn extension_kind(extension: &str) -> Option<&'static str> {
    match extension {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "heic" => Some("image"),
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" => Some("audio"),
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "wmv" => Some("video"),
        "exe" | "msi" | "bat" | "cmd" | "com" => Some("installer"),
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => Some("document"),
        _ => None,
    }
}

fn image_dimensions(bytes: &[u8], extension: &str) -> Option<(u32, u32)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        return Some((
            u32::from_be_bytes(bytes[16..20].try_into().ok()?),
            u32::from_be_bytes(bytes[20..24].try_into().ok()?),
        ));
    }
    if (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) && bytes.len() >= 10 {
        return Some((
            u16::from_le_bytes(bytes[6..8].try_into().ok()?) as u32,
            u16::from_le_bytes(bytes[8..10].try_into().ok()?) as u32,
        ));
    }
    if bytes.starts_with(b"BM") && bytes.len() >= 26 {
        return Some((
            i32::from_le_bytes(bytes[18..22].try_into().ok()?).unsigned_abs(),
            i32::from_le_bytes(bytes[22..26].try_into().ok()?).unsigned_abs(),
        ));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") && bytes.len() >= 30 {
        match bytes.get(12..16) {
            Some(b"VP8X") if bytes.len() >= 30 => {
                let width =
                    1 + (bytes[24] as u32 | (bytes[25] as u32) << 8 | (bytes[26] as u32) << 16);
                let height =
                    1 + (bytes[27] as u32 | (bytes[28] as u32) << 8 | (bytes[29] as u32) << 16);
                return Some((width, height));
            }
            _ => {}
        }
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return jpeg_dimensions(bytes);
    }
    if extension == "heic" {
        return None;
    }
    None
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut cursor = 2usize;
    while cursor + 9 < bytes.len() {
        if bytes[cursor] != 0xff {
            cursor += 1;
            continue;
        }
        while cursor < bytes.len() && bytes[cursor] == 0xff {
            cursor += 1;
        }
        let marker = *bytes.get(cursor)?;
        cursor += 1;
        if matches!(marker, 0xd8 | 0xd9) {
            continue;
        }
        let length = u16::from_be_bytes(bytes.get(cursor..cursor + 2)?.try_into().ok()?) as usize;
        if length < 2 || cursor + length > bytes.len() {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            let height = u16::from_be_bytes(bytes.get(cursor + 3..cursor + 5)?.try_into().ok()?);
            let width = u16::from_be_bytes(bytes.get(cursor + 5..cursor + 7)?.try_into().ok()?);
            return Some((width as u32, height as u32));
        }
        cursor += length;
    }
    None
}

fn read_exif(bytes: &[u8], metadata: &mut FileMetadata) {
    let Some(exif_start) = bytes.windows(6).position(|value| value == b"Exif\0\0") else {
        return;
    };
    let tiff = exif_start + 6;
    if tiff + 8 > bytes.len() {
        return;
    }
    let little_endian = &bytes[tiff..tiff + 2] == b"II";
    if &bytes[tiff..tiff + 2] != b"II" && &bytes[tiff..tiff + 2] != b"MM" {
        return;
    }
    let read_u16 = |offset: usize| -> Option<u16> {
        let value = bytes.get(offset..offset + 2)?.try_into().ok()?;
        Some(if little_endian {
            u16::from_le_bytes(value)
        } else {
            u16::from_be_bytes(value)
        })
    };
    let read_u32 = |offset: usize| -> Option<u32> {
        let value = bytes.get(offset..offset + 4)?.try_into().ok()?;
        Some(if little_endian {
            u32::from_le_bytes(value)
        } else {
            u32::from_be_bytes(value)
        })
    };
    let ifd_offset = read_u32(tiff + 4).unwrap_or_default() as usize;
    let ifd = tiff + ifd_offset;
    let count = read_u16(ifd).unwrap_or_default() as usize;
    let mut gps_ifd = None;
    for index in 0..count {
        let entry = ifd + 2 + index * 12;
        let Some(tag) = read_u16(entry) else { break };
        let Some(value_type) = read_u16(entry + 2) else {
            break;
        };
        let Some(value_count) = read_u32(entry + 4) else {
            break;
        };
        let value_size = value_type_size(value_type).saturating_mul(value_count as usize);
        let value_offset = if value_size <= 4 {
            entry + 8
        } else {
            tiff + read_u32(entry + 8).unwrap_or_default() as usize
        };
        match tag {
            0x010f => metadata.camera_make = read_ascii(bytes, value_offset, value_count as usize),
            0x0110 => metadata.camera_model = read_ascii(bytes, value_offset, value_count as usize),
            0x0112 if value_type == 3 => {
                metadata.orientation = read_u16(value_offset);
            }
            0x9003 => metadata.exif_date = read_ascii(bytes, value_offset, value_count as usize),
            0x8825 if value_type == 4 => {
                gps_ifd =
                    read_u32(value_offset).and_then(|offset| tiff.checked_add(offset as usize));
            }
            _ => {}
        }
    }

    let Some(gps_ifd) = gps_ifd else {
        return;
    };
    let gps_count = read_u16(gps_ifd).unwrap_or_default() as usize;
    let mut latitude_ref = None;
    let mut longitude_ref = None;
    let mut latitude = None;
    let mut longitude = None;
    for index in 0..gps_count {
        let entry = gps_ifd + 2 + index * 12;
        let Some(tag) = read_u16(entry) else { break };
        let Some(value_type) = read_u16(entry + 2) else {
            break;
        };
        let Some(value_count) = read_u32(entry + 4) else {
            break;
        };
        let value_size = value_type_size(value_type).saturating_mul(value_count as usize);
        let value_offset = if value_size <= 4 {
            entry + 8
        } else {
            tiff + read_u32(entry + 8).unwrap_or_default() as usize
        };
        match tag {
            0x0001 => latitude_ref = read_ascii(bytes, value_offset, value_count as usize),
            0x0002 if value_type == 5 && value_count >= 3 => {
                latitude = read_gps_coordinate(bytes, value_offset, little_endian)
            }
            0x0003 => longitude_ref = read_ascii(bytes, value_offset, value_count as usize),
            0x0004 if value_type == 5 && value_count >= 3 => {
                longitude = read_gps_coordinate(bytes, value_offset, little_endian)
            }
            _ => {}
        }
    }
    metadata.gps_latitude = format_gps_coordinate(latitude_ref.as_deref(), latitude);
    metadata.gps_longitude = format_gps_coordinate(longitude_ref.as_deref(), longitude);
}

fn value_type_size(value_type: u16) -> usize {
    match value_type {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 => 4,
        5 | 10 | 12 => 8,
        _ => 0,
    }
}

fn read_ascii(bytes: &[u8], offset: usize, length: usize) -> Option<String> {
    let value = bytes.get(offset..offset.saturating_add(length))?;
    let value = value.split(|byte| *byte == 0).next().unwrap_or_default();
    Some(String::from_utf8_lossy(value).trim().to_string())
}

fn read_gps_coordinate(bytes: &[u8], offset: usize, little_endian: bool) -> Option<[f64; 3]> {
    let mut values = [0.0; 3];
    for (index, value) in values.iter_mut().enumerate() {
        let item = offset.checked_add(index.checked_mul(8)?)?;
        let numerator = read_u32_endian(bytes, item, little_endian)? as f64;
        let denominator = read_u32_endian(bytes, item + 4, little_endian)? as f64;
        if denominator == 0.0 {
            return None;
        }
        *value = numerator / denominator;
    }
    Some(values)
}

fn format_gps_coordinate(reference: Option<&str>, value: Option<[f64; 3]>) -> Option<String> {
    let [degrees, minutes, seconds] = value?;
    let coordinate = degrees + minutes / 60.0 + seconds / 3600.0;
    if !coordinate.is_finite() {
        return None;
    }
    let signed = if matches!(reference, Some("S") | Some("W")) {
        -coordinate
    } else {
        coordinate
    };
    Some(format!("{signed:.6}"))
}

fn read_id3(bytes: &[u8], metadata: &mut FileMetadata) {
    if !bytes.starts_with(b"ID3") || bytes.len() < 10 {
        return;
    }
    let tag_size = synchsafe(&bytes[6..10]) as usize;
    let end = (10 + tag_size).min(bytes.len());
    let mut cursor = 10usize;
    while cursor + 10 <= end {
        let frame_id = &bytes[cursor..cursor + 4];
        let size = u32::from_be_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if size == 0 || cursor + 10 + size > end {
            break;
        }
        let value = id3_text(&bytes[cursor + 10..cursor + 10 + size]);
        match frame_id {
            b"TIT2" => metadata.title = value,
            b"TPE1" => metadata.artist = value,
            b"TALB" => metadata.album = value,
            b"TRCK" => metadata.track = value.as_deref().and_then(parse_track),
            b"TDRC" | b"TYER" => metadata.year = value.as_deref().and_then(parse_year),
            b"TCON" => metadata.genre = value,
            _ => {}
        }
        cursor += 10 + size;
    }
}

fn synchsafe(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .take(4)
        .fold(0, |value, byte| (value << 7) | u32::from(byte & 0x7f))
}

fn id3_text(bytes: &[u8]) -> Option<String> {
    let (_, text) = bytes.split_first()?;
    let text = text.split(|byte| *byte == 0).next().unwrap_or_default();
    Some(String::from_utf8_lossy(text).trim().to_string()).filter(|value| !value.is_empty())
}

fn parse_track(value: &str) -> Option<u32> {
    value.split('/').next()?.parse().ok()
}

fn parse_year(value: &str) -> Option<u32> {
    value.get(..4)?.parse().ok()
}

fn wav_duration(bytes: &[u8]) -> Option<u64> {
    if bytes.len() < 44 || !bytes.starts_with(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return None;
    }
    let channels = u16::from_le_bytes(bytes[22..24].try_into().ok()?) as f64;
    let sample_rate = u32::from_le_bytes(bytes[24..28].try_into().ok()?) as f64;
    let bits = u16::from_le_bytes(bytes[34..36].try_into().ok()?) as f64;
    let data_size = bytes
        .get(40..44)
        .and_then(|value| Some(u32::from_le_bytes(value.try_into().ok()?) as f64))?;
    let bytes_per_second = sample_rate * channels * bits / 8.0;
    (bytes_per_second > 0.0).then_some((data_size / bytes_per_second).round() as u64)
}

fn read_mp4_metadata(bytes: &[u8], metadata: &mut FileMetadata) {
    parse_mp4_boxes(bytes, 0, bytes.len(), metadata);
}

fn parse_mp4_boxes(bytes: &[u8], start: usize, end: usize, metadata: &mut FileMetadata) {
    let mut cursor = start;
    while cursor.saturating_add(8) <= end {
        let Some(size32) = read_be_u32(bytes, cursor) else {
            break;
        };
        let Some(kind) = bytes.get(cursor + 4..cursor + 8) else {
            break;
        };
        let (header_size, box_size) = if size32 == 1 {
            let Some(size64) = read_be_u64(bytes, cursor + 8) else {
                break;
            };
            (16usize, size64)
        } else if size32 == 0 {
            (8usize, (end - cursor) as u64)
        } else {
            (8usize, size32 as u64)
        };
        let Ok(box_size) = usize::try_from(box_size) else {
            break;
        };
        if box_size < header_size {
            break;
        }
        let Some(box_end) = cursor.checked_add(box_size) else {
            break;
        };
        if box_end > end || cursor.checked_add(header_size).is_none() {
            break;
        }
        let payload_start = cursor + header_size;
        if kind == b"mvhd" {
            parse_mvhd(bytes, payload_start, metadata);
        } else if kind == b"tkhd" {
            parse_tkhd(bytes, payload_start, metadata);
        } else if kind == b"moov" || kind == b"trak" {
            parse_mp4_boxes(bytes, payload_start, box_end, metadata);
        }
        if box_end == cursor {
            break;
        }
        cursor = box_end;
    }
}

fn parse_mvhd(bytes: &[u8], payload: usize, metadata: &mut FileMetadata) {
    let Some(version) = bytes.get(payload).copied() else {
        return;
    };
    if version == 0 {
        let creation = read_be_u32(bytes, payload + 4).map(u64::from);
        let timescale = read_be_u32(bytes, payload + 12);
        let duration = read_be_u32(bytes, payload + 16).map(u64::from);
        if let (Some(timescale), Some(duration)) = (timescale, duration) {
            if timescale > 0 {
                metadata.duration_seconds =
                    Some((duration as f64 / timescale as f64).round() as u64);
            }
        }
        metadata.creation_time = creation.and_then(mp4_time_to_string);
    } else if version == 1 {
        let creation = read_be_u64(bytes, payload + 4);
        let timescale = read_be_u32(bytes, payload + 20);
        let duration = read_be_u64(bytes, payload + 24);
        if let (Some(timescale), Some(duration)) = (timescale, duration) {
            if timescale > 0 {
                metadata.duration_seconds =
                    Some((duration as f64 / timescale as f64).round() as u64);
            }
        }
        metadata.creation_time = creation.and_then(mp4_time_to_string);
    }
}

fn parse_tkhd(bytes: &[u8], payload: usize, metadata: &mut FileMetadata) {
    let Some(version) = bytes.get(payload).copied() else {
        return;
    };
    let (width_offset, height_offset) = if version == 0 {
        (payload + 76, payload + 80)
    } else if version == 1 {
        (payload + 88, payload + 92)
    } else {
        return;
    };
    let Some(width) = read_be_u32(bytes, width_offset) else {
        return;
    };
    let Some(height) = read_be_u32(bytes, height_offset) else {
        return;
    };
    let width = width >> 16;
    let height = height >> 16;
    if width > 0 && height > 0 {
        metadata.width = Some(width);
        metadata.height = Some(height);
    }
}

fn mp4_time_to_string(seconds: u64) -> Option<String> {
    let unix_seconds = i64::try_from(seconds).ok()?.checked_sub(2_208_988_800)?;
    DateTime::<Utc>::from_timestamp(unix_seconds, 0).map(|value| value.to_rfc3339())
}

fn read_be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_be_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn read_u32_endian(bytes: &[u8], offset: usize, little_endian: bool) -> Option<u32> {
    let value = bytes.get(offset..offset + 4)?.try_into().ok()?;
    Some(if little_endian {
        u32::from_le_bytes(value)
    } else {
        u32::from_be_bytes(value)
    })
}

#[cfg(test)]
mod tests {
    use super::{
        format_gps_coordinate, image_dimensions, read_exif, read_id3, read_mp4_metadata,
        wav_duration,
    };
    use crate::models::FileMetadata;

    #[test]
    fn reads_png_dimensions() {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&640u32.to_be_bytes());
        bytes.extend_from_slice(&480u32.to_be_bytes());
        bytes.extend_from_slice(&[8, 2, 0, 0, 0]);
        assert_eq!(image_dimensions(&bytes, "png"), Some((640, 480)));
    }

    #[test]
    fn reads_basic_id3_frames() {
        let mut bytes = b"ID3\x04\0\0\0\0\0\x28".to_vec();
        bytes.extend_from_slice(b"TIT2\0\0\0\x06\0\0\0Test\0");
        let mut metadata = FileMetadata::default();
        read_id3(&bytes, &mut metadata);
        assert_eq!(metadata.title.as_deref(), Some("Test"));
    }

    #[test]
    fn reads_wav_duration() {
        let mut bytes = vec![0u8; 44];
        bytes[0..4].copy_from_slice(b"RIFF");
        bytes[8..12].copy_from_slice(b"WAVE");
        bytes[22..24].copy_from_slice(&2u16.to_le_bytes());
        bytes[24..28].copy_from_slice(&48_000u32.to_le_bytes());
        bytes[34..36].copy_from_slice(&16u16.to_le_bytes());
        bytes[40..44].copy_from_slice(&192_000u32.to_le_bytes());
        assert_eq!(wav_duration(&bytes), Some(1));
    }

    #[test]
    fn reads_mp4_duration_and_dimensions() {
        fn box_bytes(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
            let mut value = Vec::with_capacity(payload.len() + 8);
            value.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
            value.extend_from_slice(kind);
            value.extend_from_slice(payload);
            value
        }

        let mut mvhd = vec![0u8; 20];
        mvhd[12..16].copy_from_slice(&1_000u32.to_be_bytes());
        mvhd[16..20].copy_from_slice(&5_000u32.to_be_bytes());
        let mut tkhd = vec![0u8; 84];
        tkhd[76..80].copy_from_slice(&(1_920u32 << 16).to_be_bytes());
        tkhd[80..84].copy_from_slice(&(1_080u32 << 16).to_be_bytes());
        let trak = box_bytes(b"trak", &box_bytes(b"tkhd", &tkhd));
        let moov_payload = [box_bytes(b"mvhd", &mvhd), trak].concat();
        let bytes = box_bytes(b"moov", &moov_payload);
        let mut metadata = FileMetadata::default();
        read_mp4_metadata(&bytes, &mut metadata);

        assert_eq!(metadata.duration_seconds, Some(5));
        assert_eq!(metadata.width, Some(1_920));
        assert_eq!(metadata.height, Some(1_080));
        assert!(metadata.creation_time.is_some());
    }

    #[test]
    fn formats_signed_gps_coordinates() {
        assert_eq!(
            format_gps_coordinate(Some("S"), Some([1.0, 2.0, 3.0])),
            Some("-1.034167".to_string())
        );
    }

    #[test]
    fn reads_exif_gps_coordinates() {
        let mut bytes = vec![0u8; 260];
        bytes[0..6].copy_from_slice(b"Exif\0\0");
        let tiff = 6;
        bytes[tiff..tiff + 2].copy_from_slice(b"II");
        bytes[tiff + 2..tiff + 4].copy_from_slice(&42u16.to_le_bytes());
        bytes[tiff + 4..tiff + 8].copy_from_slice(&8u32.to_le_bytes());

        let ifd = tiff + 8;
        bytes[ifd..ifd + 2].copy_from_slice(&1u16.to_le_bytes());
        let gps_pointer = ifd + 2;
        bytes[gps_pointer..gps_pointer + 2].copy_from_slice(&0x8825u16.to_le_bytes());
        bytes[gps_pointer + 2..gps_pointer + 4].copy_from_slice(&4u16.to_le_bytes());
        bytes[gps_pointer + 4..gps_pointer + 8].copy_from_slice(&1u32.to_le_bytes());
        bytes[gps_pointer + 8..gps_pointer + 12].copy_from_slice(&100u32.to_le_bytes());

        let gps_ifd = tiff + 100;
        bytes[gps_ifd..gps_ifd + 2].copy_from_slice(&4u16.to_le_bytes());
        let entry = gps_ifd + 2;
        bytes[entry..entry + 2].copy_from_slice(&1u16.to_le_bytes());
        bytes[entry + 2..entry + 4].copy_from_slice(&2u16.to_le_bytes());
        bytes[entry + 4..entry + 8].copy_from_slice(&2u32.to_le_bytes());
        bytes[entry + 8..entry + 10].copy_from_slice(b"N\0");

        let entry = gps_ifd + 14;
        bytes[entry..entry + 2].copy_from_slice(&2u16.to_le_bytes());
        bytes[entry + 2..entry + 4].copy_from_slice(&5u16.to_le_bytes());
        bytes[entry + 4..entry + 8].copy_from_slice(&3u32.to_le_bytes());
        bytes[entry + 8..entry + 12].copy_from_slice(&160u32.to_le_bytes());

        let entry = gps_ifd + 26;
        bytes[entry..entry + 2].copy_from_slice(&3u16.to_le_bytes());
        bytes[entry + 2..entry + 4].copy_from_slice(&2u16.to_le_bytes());
        bytes[entry + 4..entry + 8].copy_from_slice(&2u32.to_le_bytes());
        bytes[entry + 8..entry + 10].copy_from_slice(b"W\0");

        let entry = gps_ifd + 38;
        bytes[entry..entry + 2].copy_from_slice(&4u16.to_le_bytes());
        bytes[entry + 2..entry + 4].copy_from_slice(&5u16.to_le_bytes());
        bytes[entry + 4..entry + 8].copy_from_slice(&3u32.to_le_bytes());
        bytes[entry + 8..entry + 12].copy_from_slice(&184u32.to_le_bytes());

        for (offset, value) in [
            (166, 12),
            (174, 34),
            (182, 56),
            (190, 98),
            (198, 45),
            (206, 30),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&(value as u32).to_le_bytes());
            bytes[offset + 4..offset + 8].copy_from_slice(&1u32.to_le_bytes());
        }

        let mut metadata = FileMetadata::default();
        read_exif(&bytes, &mut metadata);
        assert_eq!(metadata.gps_latitude.as_deref(), Some("12.582222"));
        assert_eq!(metadata.gps_longitude.as_deref(), Some("-98.758333"));
    }
}
