use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveFormat {
    Zip,
    SevenZ,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct BackupOptions {
    pub enable_smart_exclude: bool,
    pub custom_exclude_patterns: Vec<String>,
    pub compress_after_backup: bool,
    pub archive_format: ArchiveFormat,
    pub compression_level: u8,
    pub send_notification: bool,
    #[serde(default = "default_verify_mode")]
    pub verify_mode: String,
    #[serde(default = "default_metadata_preserve_level")]
    pub metadata_preserve_level: String,
    #[serde(default)]
    pub incremental: bool,
    #[serde(default = "default_job_name")]
    pub job_name: String,
}

fn default_verify_mode() -> String {
    "fast".to_string()
}

fn default_metadata_preserve_level() -> String {
    "windows".to_string()
}

fn default_job_name() -> String {
    "个人文件".to_string()
}

impl Default for BackupOptions {
    fn default() -> Self {
        Self {
            enable_smart_exclude: true,
            custom_exclude_patterns: Vec::new(),
            compress_after_backup: false,
            archive_format: ArchiveFormat::Zip,
            compression_level: 6,
            send_notification: true,
            verify_mode: default_verify_mode(),
            metadata_preserve_level: default_metadata_preserve_level(),
            incremental: false,
            job_name: default_job_name(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupItem {
    pub id: String,
    pub label: String,
    pub source_path: String,
    pub target_name: String,
    pub enabled: bool,
    #[serde(default = "default_item_category")]
    pub category: String,
    #[serde(default)]
    pub description: Option<String>,
    pub is_custom: bool,
    pub file_count: Option<u64>,
    pub total_size: Option<u64>,
}

fn default_item_category() -> String {
    "system".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub items: Vec<BackupItem>,
    pub total_files: u64,
    pub total_bytes: u64,
    pub target_drive_free_bytes: Option<u64>,
    pub target_drive_name: Option<String>,
    pub target_accessible: bool,
    pub target_writable: bool,
    pub target_kind: String,
    pub warnings: Vec<String>,
    pub source_warnings: Vec<String>,
    pub skipped_by_rule_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct BackupProgress {
    #[serde(default)]
    pub task_id: Option<String>,
    pub phase: String,
    pub current_folder: String,
    pub current_file: String,
    pub current_file_size: Option<u64>,
    pub current_file_copied: Option<u64>,
    pub total_files: u64,
    pub copied_files: u64,
    pub total_bytes: u64,
    pub copied_bytes: u64,
    pub failed_files: u64,
    pub skipped_by_rule_count: u64,
    pub speed_bytes_per_sec: u64,
    pub estimated_seconds_left: i64,
    pub percent: u8,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub id: String,
    pub path: String,
    pub filename: String,
    pub stem: String,
    pub extension: String,
    pub size: u64,
    pub created_at: Option<i64>,
    pub modified_at: Option<i64>,
    pub accessed_at: Option<i64>,
    pub mime: Option<String>,
    pub category: String,
    pub source_type: Option<String>,
    pub hash: Option<String>,
    #[serde(default)]
    pub hash_algorithm: Option<String>,
    #[serde(default)]
    pub metadata: Option<FileMetadata>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct FileMetadata {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub orientation: Option<u16>,
    pub exif_date: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub gps_latitude: Option<String>,
    pub gps_longitude: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub title: Option<String>,
    pub track: Option<u32>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub duration_seconds: Option<u64>,
    pub creation_time: Option<String>,
    pub codec: Option<String>,
    pub extension_mismatch: bool,
    pub unsupported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogScanRequest {
    pub root_path: String,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub include_hidden: bool,
    #[serde(default)]
    pub include_system_files: bool,
    #[serde(default)]
    pub custom_exclude_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogScanResult {
    pub task_id: String,
    pub root_path: String,
    pub total_files: u64,
    pub total_bytes: u64,
    pub indexed_files: u64,
    pub skipped_files: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetadataReadResult {
    pub path: String,
    pub metadata: FileMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct CatalogQuery {
    pub search: String,
    pub root_path: Option<String>,
    pub category: Option<String>,
    pub source_type: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    pub task_id: String,
    pub task_type: String,
    pub phase: String,
    pub completed_items: u64,
    pub total_items: u64,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    pub current_path: Option<String>,
    pub speed_bytes_per_second: Option<u64>,
    pub eta_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlannedOperation {
    pub id: String,
    #[serde(rename = "type")]
    pub operation_type: String,
    pub source_path: String,
    pub destination_path: Option<String>,
    pub reason: String,
    pub rule_id: Option<String>,
    pub conflict: Option<ConflictInfo>,
    pub status: String,
    pub source_size: Option<u64>,
    pub source_modified_at: Option<i64>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub kind: String,
    pub message: String,
    pub suggested_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuleRecord {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub rule_type: String,
    pub definition: RuleDefinition,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct RuleDefinition {
    pub source: Option<RuleSource>,
    pub condition: serde_json::Value,
    pub action: RuleAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct RuleSource {
    pub source_type: Option<String>,
    pub path_contains: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct RuleAction {
    #[serde(rename = "type")]
    pub action_type: String,
    pub destination_template: Option<String>,
    pub rename_template: Option<String>,
    pub tags: Vec<String>,
}

impl Default for RuleAction {
    fn default() -> Self {
        Self {
            action_type: "move".to_string(),
            destination_template: None,
            rename_template: None,
            tags: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateScanRequest {
    pub root_path: String,
    #[serde(default)]
    pub include_hidden: bool,
    #[serde(default)]
    pub include_system_files: bool,
    #[serde(default)]
    pub custom_exclude_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub id: String,
    pub hash: String,
    pub size: u64,
    pub files: Vec<FileRecord>,
    pub reclaimable_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateScanResult {
    pub task_id: String,
    pub root_path: String,
    pub status: String,
    pub total_files: u64,
    pub duplicate_files: u64,
    pub reclaimable_size: u64,
    pub groups: Vec<DuplicateGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DuplicatePlanRequest {
    pub files: Vec<FileRecord>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SimilarScanRequest {
    pub root_path: String,
    #[serde(default = "default_similar_distance")]
    pub max_distance: u32,
    #[serde(default)]
    pub include_hidden: bool,
}

fn default_similar_distance() -> u32 {
    8
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SimilarGroup {
    pub id: String,
    pub distance: u32,
    pub files: Vec<FileRecord>,
    pub reclaimable_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SimilarScanResult {
    pub task_id: String,
    pub root_path: String,
    pub status: String,
    pub total_images: u64,
    pub groups: Vec<SimilarGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupJobRecord {
    pub id: String,
    pub name: String,
    pub source_config: serde_json::Value,
    pub target_path: String,
    pub policy: serde_json::Value,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRecord {
    pub id: String,
    pub backup_job_id: Option<String>,
    pub snapshot_time: i64,
    pub file_count: u64,
    pub total_size: u64,
    pub manifest_path: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFileRecord {
    pub snapshot_id: String,
    pub source_path: String,
    pub backup_path: String,
    pub size: u64,
    pub mtime: Option<i64>,
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotManifest {
    pub snapshot_id: String,
    pub created_at: i64,
    pub files: Vec<SnapshotFileRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotVerifyResult {
    pub task_id: String,
    pub snapshot_id: String,
    pub mode: String,
    pub checked_files: u64,
    pub failed_files: u64,
    pub status: String,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestorePlanRequest {
    pub snapshot_id: String,
    #[serde(default)]
    pub source_paths: Vec<String>,
    pub destination_root: Option<String>,
    #[serde(default = "default_conflict_policy")]
    pub conflict_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreatePlanRequest {
    pub files: Vec<FileRecord>,
    pub destination_root: String,
    #[serde(default = "default_operation_type")]
    pub operation_type: String,
    pub rename_template: Option<String>,
    #[serde(default = "default_conflict_policy")]
    pub conflict_policy: String,
    #[serde(default)]
    pub reason: String,
    pub rule_id: Option<String>,
}

fn default_operation_type() -> String {
    "move".to_string()
}

fn default_conflict_policy() -> String {
    "auto_number".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanPreview {
    pub id: String,
    pub task_id: String,
    pub created_at: i64,
    pub status: String,
    pub operations: Vec<PlannedOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyPlanResult {
    pub task_id: String,
    pub plan_id: String,
    pub status: String,
    pub completed: u64,
    pub failed: u64,
    pub operations: Vec<PlannedOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationHistoryItem {
    pub id: String,
    pub plan_id: Option<String>,
    pub task_id: Option<String>,
    #[serde(rename = "type")]
    pub operation_type: String,
    pub source_path: String,
    pub destination_path: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub executed_at: Option<i64>,
    pub undo_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupError {
    pub source_path: String,
    pub target_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub status: String,
    pub total_files: u64,
    pub success_files: u64,
    pub failed_files: u64,
    pub skipped_by_rule_count: u64,
    pub total_bytes: u64,
    pub copied_bytes: u64,
    pub duration_seconds: u64,
    pub errors: Vec<BackupError>,
    pub archive_format: Option<String>,
    pub archive_error: Option<String>,
    pub report_path: String,
    pub log_path: String,
    pub backup_root: String,
    pub archive_path: Option<String>,
    #[serde(default)]
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub manifest_path: Option<String>,
    #[serde(default)]
    pub verify_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AppConfig {
    pub target_root: String,
    #[serde(default)]
    pub options: BackupOptions,
    #[serde(default, alias = "customItems")]
    pub items: Vec<BackupItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AutomationConfig {
    pub watch_enabled: bool,
    pub watch_path: Option<String>,
    pub watch_destination_root: Option<String>,
    pub watch_rule_id: Option<String>,
    pub watch_auto_apply: bool,
    pub scheduled_backup_enabled: bool,
    pub scheduled_backup_interval_minutes: u32,
    pub minimize_to_tray: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AutomationStatus {
    pub watch_running: bool,
    pub scheduled_backup_running: bool,
    pub watch_path: Option<String>,
    pub next_scheduled_run_at: Option<i64>,
}
