use crate::models::{FileRecord, OperationHistoryItem, PlannedOperation};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SCHEMA_VERSION: i64 = 3;

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

pub fn database_path() -> Result<PathBuf, String> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("WindowsEasyBackup"));
    Ok(base.join("WindowsEasyBackup").join("app.db"))
}

pub fn open_connection() -> Result<Connection, String> {
    let path = database_path()?;
    open_connection_at(&path)
}

pub fn open_connection_at(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建数据库目录失败 {}: {error}", parent.display()))?;
    }

    let mut connection = Connection::open(path)
        .map_err(|error| format!("打开 SQLite 数据库失败 {}: {error}", path.display()))?;
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;",
        )
        .map_err(|error| format!("初始化 SQLite 参数失败: {error}"))?;
    migrate(&mut connection)?;
    Ok(connection)
}

fn migrate(connection: &mut Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL
            );",
        )
        .map_err(|error| format!("创建迁移表失败: {error}"))?;

    let current: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("读取数据库版本失败: {error}"))?;

    if current < 1 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("开启数据库迁移事务失败: {error}"))?;
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS files (
                    id TEXT PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    filename TEXT NOT NULL,
                    stem TEXT NOT NULL,
                    extension TEXT NOT NULL,
                    size INTEGER NOT NULL,
                    created_at INTEGER,
                    modified_at INTEGER,
                    accessed_at INTEGER,
                    mime TEXT,
                    category TEXT NOT NULL,
                    source_type TEXT,
                    hash TEXT,
                    indexed_at INTEGER NOT NULL,
                    last_seen_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_files_category ON files(category);
                CREATE INDEX IF NOT EXISTS idx_files_modified_at ON files(modified_at);
                CREATE TABLE IF NOT EXISTS metadata (
                    file_id TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                    metadata_type TEXT NOT NULL,
                    json_data TEXT NOT NULL,
                    PRIMARY KEY(file_id, metadata_type)
                );
                CREATE TABLE IF NOT EXISTS tags (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS file_tags (
                    file_id TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                    PRIMARY KEY(file_id, tag_id)
                );
                CREATE TABLE IF NOT EXISTS rules (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    enabled INTEGER NOT NULL,
                    priority INTEGER NOT NULL,
                    rule_type TEXT NOT NULL,
                    definition_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS plans (
                    id TEXT PRIMARY KEY,
                    task_id TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    status TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS operations (
                    id TEXT PRIMARY KEY,
                    plan_id TEXT REFERENCES plans(id) ON DELETE SET NULL,
                    task_id TEXT,
                    operation_type TEXT NOT NULL,
                    source_path TEXT NOT NULL,
                    destination_path TEXT,
                    source_hash TEXT,
                    source_size INTEGER,
                    source_modified_at INTEGER,
                    reason TEXT NOT NULL,
                    status TEXT NOT NULL,
                    error TEXT,
                    executed_at INTEGER,
                    undo_status TEXT NOT NULL DEFAULT 'available'
                );
                CREATE INDEX IF NOT EXISTS idx_operations_executed_at ON operations(executed_at);
                CREATE TABLE IF NOT EXISTS backup_jobs (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    source_config TEXT NOT NULL,
                    target_path TEXT NOT NULL,
                    policy_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS snapshots (
                    id TEXT PRIMARY KEY,
                    backup_job_id TEXT REFERENCES backup_jobs(id) ON DELETE SET NULL,
                    snapshot_time INTEGER NOT NULL,
                    file_count INTEGER NOT NULL,
                    total_size INTEGER NOT NULL,
                    manifest_path TEXT,
                    status TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS snapshot_files (
                    snapshot_id TEXT NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
                    source_path TEXT NOT NULL,
                    backup_path TEXT NOT NULL,
                    size INTEGER NOT NULL,
                    mtime INTEGER,
                    hash TEXT,
                    PRIMARY KEY(snapshot_id, source_path)
                );
                INSERT INTO schema_migrations(version, applied_at) VALUES (1, strftime('%s','now') * 1000);",
            )
            .map_err(|error| format!("执行数据库迁移失败: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("提交数据库迁移失败: {error}"))?;
    }

    if current < 2 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("开启数据库迁移事务失败: {error}"))?;
        transaction
            .execute_batch(
                "ALTER TABLE files ADD COLUMN hash_algorithm TEXT;
                 INSERT INTO schema_migrations(version, applied_at) VALUES (2, strftime('%s','now') * 1000);",
            )
            .map_err(|error| format!("执行数据库 v2 迁移失败: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("提交数据库 v2 迁移失败: {error}"))?;
    }

    if current < 3 {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("开启数据库迁移事务失败: {error}"))?;
        transaction
            .execute_batch(
                "ALTER TABLE operations ADD COLUMN payload_json TEXT;
                 INSERT INTO schema_migrations(version, applied_at) VALUES (3, strftime('%s','now') * 1000);",
            )
            .map_err(|error| format!("执行数据库 v3 迁移失败: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("提交数据库 v3 迁移失败: {error}"))?;
    }

    if current > SCHEMA_VERSION {
        return Err(format!(
            "数据库版本 {} 高于当前程序支持的版本 {}。",
            current, SCHEMA_VERSION
        ));
    }

    Ok(())
}

fn as_i64(value: u64, label: &str) -> Result<i64, String> {
    i64::try_from(value).map_err(|_| format!("{label} 超出 SQLite INTEGER 范围。"))
}

pub fn upsert_file(connection: &Connection, file: &FileRecord) -> Result<(), String> {
    let now = now_millis();
    connection
        .execute(
            "INSERT INTO files (
                id, path, filename, stem, extension, size, created_at, modified_at,
                accessed_at, mime, category, source_type, hash, hash_algorithm, indexed_at, last_seen_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)
            ON CONFLICT(path) DO UPDATE SET
                filename = excluded.filename,
                stem = excluded.stem,
                extension = excluded.extension,
                size = excluded.size,
                created_at = excluded.created_at,
                modified_at = excluded.modified_at,
                accessed_at = excluded.accessed_at,
                mime = excluded.mime,
                category = excluded.category,
                source_type = excluded.source_type,
                indexed_at = excluded.indexed_at,
                last_seen_at = excluded.last_seen_at",
            params![
                file.id,
                file.path,
                file.filename,
                file.stem,
                file.extension,
                as_i64(file.size, "文件大小")?,
                file.created_at,
                file.modified_at,
                file.accessed_at,
                file.mime,
                file.category,
                file.source_type,
                file.hash,
                file.hash_algorithm,
                now,
            ],
        )
        .map_err(|error| format!("写入文件索引失败 {}: {error}", file.path))?;
    Ok(())
}

pub fn file_is_unchanged(
    connection: &Connection,
    path: &str,
    size: u64,
    modified_at: Option<i64>,
    source_type: Option<&str>,
) -> Result<bool, String> {
    let existing: Option<(i64, Option<i64>, Option<String>)> = connection
        .query_row(
            "SELECT size, modified_at, source_type FROM files WHERE path = ?1",
            params![path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| format!("读取增量索引状态失败: {error}"))?;
    Ok(
        existing.is_some_and(|(stored_size, stored_modified, stored_source)| {
            u64::try_from(stored_size).ok() == Some(size)
                && stored_modified == modified_at
                && stored_source.as_deref() == source_type
        }),
    )
}

pub fn file_from_row(row: &Row<'_>) -> rusqlite::Result<FileRecord> {
    let size: i64 = row.get("size")?;
    Ok(FileRecord {
        id: row.get("id")?,
        path: row.get("path")?,
        filename: row.get("filename")?,
        stem: row.get("stem")?,
        extension: row.get("extension")?,
        size: u64::try_from(size).unwrap_or_default(),
        created_at: row.get("created_at")?,
        modified_at: row.get("modified_at")?,
        accessed_at: row.get("accessed_at")?,
        mime: row.get("mime")?,
        category: row.get("category")?,
        source_type: row.get("source_type")?,
        hash: row.get("hash")?,
        hash_algorithm: row.get("hash_algorithm")?,
        metadata: None,
        tags: Vec::new(),
    })
}

pub fn list_files(
    connection: &Connection,
    query: &crate::models::CatalogQuery,
) -> Result<Vec<FileRecord>, String> {
    let limit = if query.limit == 0 {
        200
    } else {
        query.limit.min(10_000)
    };
    let offset = query.offset;
    let search = format!("%{}%", query.search.trim().to_lowercase());
    let mut statement = connection
        .prepare(
            "SELECT id, path, filename, stem, extension, size, created_at, modified_at,
                    accessed_at, mime, category, source_type, hash, hash_algorithm
             FROM files
             WHERE (?1 = '%%' OR lower(path) LIKE ?1 OR lower(filename) LIKE ?1)
               AND (?2 IS NULL OR category = ?2)
               AND (?3 IS NULL OR source_type = ?3)
               AND (?4 IS NULL OR path = ?4 OR path LIKE (?4 || '%'))
             ORDER BY modified_at DESC NULLS LAST, path ASC
             LIMIT ?5 OFFSET ?6",
        )
        .map_err(|error| format!("准备文件查询失败: {error}"))?;
    let rows = statement
        .query_map(
            params![
                search,
                query.category,
                query.source_type,
                query.root_path,
                limit,
                offset
            ],
            file_from_row,
        )
        .map_err(|error| format!("查询文件索引失败: {error}"))?;
    let mut files: Vec<FileRecord> = rows
        .map(|row| row.map_err(|error| format!("读取文件索引失败: {error}")))
        .collect::<Result<_, _>>()?;
    for file in &mut files {
        file.metadata = load_metadata(connection, &file.id)?;
        file.tags = load_tags(connection, &file.id)?;
    }
    Ok(files)
}

fn load_tags(connection: &Connection, file_id: &str) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT tags.name FROM tags
             INNER JOIN file_tags ON file_tags.tag_id = tags.id
             WHERE file_tags.file_id = ?1 ORDER BY tags.name ASC",
        )
        .map_err(|error| format!("准备标签查询失败: {error}"))?;
    let rows = statement
        .query_map(params![file_id], |row| row.get(0))
        .map_err(|error| format!("查询文件标签失败: {error}"))?;
    rows.map(|row| row.map_err(|error| format!("读取文件标签失败: {error}")))
        .collect()
}

pub fn apply_tags(connection: &Connection, path: &str, tags: &[String]) -> Result<(), String> {
    let file_id: String = connection
        .query_row(
            "SELECT id FROM files WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取标签文件失败: {error}"))?
        .ok_or_else(|| format!("文件尚未进入 Catalog，无法添加标签: {path}"))?;
    for tag in tags
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        connection
            .execute(
                "INSERT INTO tags(name, created_at) VALUES (?1, ?2) ON CONFLICT(name) DO NOTHING",
                params![tag, now_millis()],
            )
            .map_err(|error| format!("创建标签失败 {tag}: {error}"))?;
        let tag_id: i64 = connection
            .query_row("SELECT id FROM tags WHERE name = ?1", params![tag], |row| {
                row.get(0)
            })
            .map_err(|error| format!("读取标签失败 {tag}: {error}"))?;
        connection
            .execute(
                "INSERT OR IGNORE INTO file_tags(file_id, tag_id) VALUES (?1, ?2)",
                params![file_id, tag_id],
            )
            .map_err(|error| format!("关联文件标签失败 {tag}: {error}"))?;
    }
    Ok(())
}

pub fn upsert_metadata(
    connection: &Connection,
    file_id: &str,
    metadata_type: &str,
    json_data: &str,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO metadata(file_id, metadata_type, json_data) VALUES (?1, ?2, ?3)
             ON CONFLICT(file_id, metadata_type) DO UPDATE SET json_data = excluded.json_data",
            params![file_id, metadata_type, json_data],
        )
        .map_err(|error| format!("写入文件 metadata 失败 {file_id}: {error}"))?;
    Ok(())
}

fn load_metadata(
    connection: &Connection,
    file_id: &str,
) -> Result<Option<crate::models::FileMetadata>, String> {
    let json_data: Option<String> = connection
        .query_row(
            "SELECT json_data FROM metadata WHERE file_id = ?1 AND metadata_type = 'file'",
            params![file_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取文件 metadata 失败 {file_id}: {error}"))?;
    json_data
        .map(|value| {
            serde_json::from_str(&value)
                .map_err(|error| format!("解析文件 metadata 失败 {file_id}: {error}"))
        })
        .transpose()
}

pub fn update_file_hash(
    connection: &Connection,
    path: &str,
    size: u64,
    modified_at: Option<i64>,
    hash: &str,
    algorithm: &str,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE files SET hash = ?1, hash_algorithm = ?2, size = ?3, modified_at = ?4,
                    last_seen_at = ?5 WHERE path = ?6",
            params![
                hash,
                algorithm,
                as_i64(size, "文件大小")?,
                modified_at,
                now_millis(),
                path
            ],
        )
        .map_err(|error| format!("写入文件 Hash 缓存失败 {path}: {error}"))?;
    Ok(())
}

pub fn cached_hash(
    connection: &Connection,
    path: &str,
    size: u64,
    modified_at: Option<i64>,
    algorithm: &str,
) -> Result<Option<String>, String> {
    let result: Option<(i64, Option<i64>, Option<String>, Option<String>)> = connection
        .query_row(
            "SELECT size, modified_at, hash, hash_algorithm FROM files WHERE path = ?1",
            params![path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| format!("读取 Hash 缓存失败 {path}: {error}"))?;
    Ok(
        result.and_then(|(cached_size, cached_mtime, hash, cached_algorithm)| {
            (u64::try_from(cached_size).ok() == Some(size)
                && cached_mtime == modified_at
                && cached_algorithm.as_deref() == Some(algorithm))
            .then_some(hash)
            .flatten()
        }),
    )
}

pub fn find_rule(
    connection: &Connection,
    rule_id: &str,
) -> Result<Option<crate::models::RuleRecord>, String> {
    connection
        .query_row(
            "SELECT id, name, enabled, priority, rule_type, definition_json, created_at, updated_at
             FROM rules WHERE id = ?1",
            params![rule_id],
            rule_from_row,
        )
        .optional()
        .map_err(|error| format!("读取规则失败 {rule_id}: {error}"))
}

fn rule_from_row(row: &Row<'_>) -> rusqlite::Result<crate::models::RuleRecord> {
    let definition_json: String = row.get("definition_json")?;
    let definition = serde_json::from_str(&definition_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            definition_json.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;
    Ok(crate::models::RuleRecord {
        id: row.get("id")?,
        name: row.get("name")?,
        enabled: row.get::<_, i64>("enabled")? != 0,
        priority: row.get("priority")?,
        rule_type: row.get("rule_type")?,
        definition,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn list_rules(connection: &Connection) -> Result<Vec<crate::models::RuleRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, name, enabled, priority, rule_type, definition_json, created_at, updated_at
             FROM rules ORDER BY priority ASC, name ASC",
        )
        .map_err(|error| format!("准备规则查询失败: {error}"))?;
    let rows = statement
        .query_map([], rule_from_row)
        .map_err(|error| format!("查询规则失败: {error}"))?;
    rows.map(|row| row.map_err(|error| format!("读取规则失败: {error}")))
        .collect()
}

pub fn upsert_rule(
    connection: &Connection,
    rule: &crate::models::RuleRecord,
) -> Result<(), String> {
    let definition = serde_json::to_string(&rule.definition)
        .map_err(|error| format!("序列化规则失败: {error}"))?;
    connection
        .execute(
            "INSERT INTO rules(id, name, enabled, priority, rule_type, definition_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, enabled = excluded.enabled,
                 priority = excluded.priority, rule_type = excluded.rule_type,
                 definition_json = excluded.definition_json, updated_at = excluded.updated_at",
            params![
                rule.id,
                rule.name,
                i64::from(rule.enabled),
                rule.priority,
                rule.rule_type,
                definition,
                rule.created_at,
                rule.updated_at,
            ],
        )
        .map_err(|error| format!("保存规则失败 {}: {error}", rule.name))?;
    Ok(())
}

pub fn delete_rule(connection: &Connection, rule_id: &str) -> Result<(), String> {
    connection
        .execute("DELETE FROM rules WHERE id = ?1", params![rule_id])
        .map_err(|error| format!("删除规则失败 {rule_id}: {error}"))?;
    Ok(())
}

pub fn insert_backup_job(
    connection: &Connection,
    job: &crate::models::BackupJobRecord,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO backup_jobs(id, name, source_config, target_path, policy_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, source_config = excluded.source_config,
                 target_path = excluded.target_path, policy_json = excluded.policy_json",
            params![
                job.id,
                job.name,
                serde_json::to_string(&job.source_config).map_err(|error| format!("序列化备份来源失败: {error}"))?,
                job.target_path,
                serde_json::to_string(&job.policy).map_err(|error| format!("序列化备份策略失败: {error}"))?,
                job.created_at,
            ],
        )
        .map_err(|error| format!("写入备份方案失败 {}: {error}", job.name))?;
    Ok(())
}

fn backup_job_from_row(row: &Row<'_>) -> rusqlite::Result<crate::models::BackupJobRecord> {
    let source_config: String = row.get("source_config")?;
    let policy: String = row.get("policy_json")?;
    Ok(crate::models::BackupJobRecord {
        id: row.get("id")?,
        name: row.get("name")?,
        source_config: serde_json::from_str(&source_config).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                source_config.len(),
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        target_path: row.get("target_path")?,
        policy: serde_json::from_str(&policy).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                policy.len(),
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        created_at: row.get("created_at")?,
    })
}

pub fn list_backup_jobs(
    connection: &Connection,
) -> Result<Vec<crate::models::BackupJobRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, name, source_config, target_path, policy_json, created_at
             FROM backup_jobs ORDER BY created_at DESC",
        )
        .map_err(|error| format!("准备备份方案查询失败: {error}"))?;
    let rows = statement
        .query_map([], backup_job_from_row)
        .map_err(|error| format!("查询备份方案失败: {error}"))?;
    rows.map(|row| row.map_err(|error| format!("读取备份方案失败: {error}")))
        .collect()
}

pub fn insert_snapshot(
    connection: &Connection,
    snapshot: &crate::models::SnapshotRecord,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO snapshots(id, backup_job_id, snapshot_time, file_count, total_size, manifest_path, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                snapshot.id,
                snapshot.backup_job_id,
                snapshot.snapshot_time,
                as_i64(snapshot.file_count, "Snapshot 文件数")?,
                as_i64(snapshot.total_size, "Snapshot 大小")?,
                snapshot.manifest_path,
                snapshot.status,
            ],
        )
        .map_err(|error| format!("写入 Snapshot 失败 {}: {error}", snapshot.id))?;
    Ok(())
}

fn snapshot_from_row(row: &Row<'_>) -> rusqlite::Result<crate::models::SnapshotRecord> {
    Ok(crate::models::SnapshotRecord {
        id: row.get("id")?,
        backup_job_id: row.get("backup_job_id")?,
        snapshot_time: row.get("snapshot_time")?,
        file_count: u64::try_from(row.get::<_, i64>("file_count")?).unwrap_or_default(),
        total_size: u64::try_from(row.get::<_, i64>("total_size")?).unwrap_or_default(),
        manifest_path: row.get("manifest_path")?,
        status: row.get("status")?,
    })
}

pub fn list_snapshots(
    connection: &Connection,
    limit: u32,
) -> Result<Vec<crate::models::SnapshotRecord>, String> {
    let limit = if limit == 0 { 200 } else { limit.min(10_000) };
    let mut statement = connection
        .prepare(
            "SELECT id, backup_job_id, snapshot_time, file_count, total_size, manifest_path, status
             FROM snapshots ORDER BY snapshot_time DESC LIMIT ?1",
        )
        .map_err(|error| format!("准备 Snapshot 查询失败: {error}"))?;
    let rows = statement
        .query_map(params![limit], snapshot_from_row)
        .map_err(|error| format!("查询 Snapshot 失败: {error}"))?;
    rows.map(|row| row.map_err(|error| format!("读取 Snapshot 失败: {error}")))
        .collect()
}

pub fn find_snapshot(
    connection: &Connection,
    snapshot_id: &str,
) -> Result<Option<crate::models::SnapshotRecord>, String> {
    connection
        .query_row(
            "SELECT id, backup_job_id, snapshot_time, file_count, total_size, manifest_path, status
             FROM snapshots WHERE id = ?1",
            params![snapshot_id],
            snapshot_from_row,
        )
        .optional()
        .map_err(|error| format!("读取 Snapshot 失败 {snapshot_id}: {error}"))
}

pub fn insert_snapshot_file(
    connection: &Connection,
    file: &crate::models::SnapshotFileRecord,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO snapshot_files(snapshot_id, source_path, backup_path, size, mtime, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                file.snapshot_id,
                file.source_path,
                file.backup_path,
                as_i64(file.size, "Snapshot 文件大小")?,
                file.mtime,
                file.hash,
            ],
        )
        .map_err(|error| format!("写入 Snapshot 文件记录失败: {error}"))?;
    Ok(())
}

pub fn update_snapshot_status(
    connection: &Connection,
    snapshot_id: &str,
    status: &str,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE snapshots SET status = ?1 WHERE id = ?2",
            params![status, snapshot_id],
        )
        .map_err(|error| format!("更新 Snapshot 状态失败 {snapshot_id}: {error}"))?;
    Ok(())
}

pub fn delete_snapshot(connection: &Connection, snapshot_id: &str) -> Result<(), String> {
    connection
        .execute("DELETE FROM snapshots WHERE id = ?1", params![snapshot_id])
        .map_err(|error| format!("删除 Snapshot 记录失败 {snapshot_id}: {error}"))?;
    Ok(())
}

fn snapshot_file_from_row(row: &Row<'_>) -> rusqlite::Result<crate::models::SnapshotFileRecord> {
    Ok(crate::models::SnapshotFileRecord {
        snapshot_id: row.get("snapshot_id")?,
        source_path: row.get("source_path")?,
        backup_path: row.get("backup_path")?,
        size: u64::try_from(row.get::<_, i64>("size")?).unwrap_or_default(),
        mtime: row.get("mtime")?,
        hash: row.get("hash")?,
    })
}

pub fn list_snapshot_files(
    connection: &Connection,
    snapshot_id: &str,
) -> Result<Vec<crate::models::SnapshotFileRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT snapshot_id, source_path, backup_path, size, mtime, hash
             FROM snapshot_files WHERE snapshot_id = ?1 ORDER BY source_path ASC",
        )
        .map_err(|error| format!("准备 Snapshot 文件查询失败: {error}"))?;
    let rows = statement
        .query_map(params![snapshot_id], snapshot_file_from_row)
        .map_err(|error| format!("查询 Snapshot 文件失败: {error}"))?;
    rows.map(|row| row.map_err(|error| format!("读取 Snapshot 文件失败: {error}")))
        .collect()
}

pub fn update_snapshot_file_hash(
    connection: &Connection,
    snapshot_id: &str,
    source_path: &str,
    hash: &str,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE snapshot_files SET hash = ?1 WHERE snapshot_id = ?2 AND source_path = ?3",
            params![hash, snapshot_id, source_path],
        )
        .map_err(|error| format!("更新 Snapshot Hash 失败: {error}"))?;
    Ok(())
}

pub fn insert_plan(
    connection: &Connection,
    id: &str,
    task_id: &str,
    status: &str,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO plans(id, task_id, created_at, status) VALUES (?1, ?2, ?3, ?4)",
            params![id, task_id, now_millis(), status],
        )
        .map_err(|error| format!("写入计划失败: {error}"))?;
    Ok(())
}

pub fn insert_operation(
    connection: &Connection,
    plan_id: &str,
    task_id: &str,
    operation: &PlannedOperation,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO operations(
                id, plan_id, task_id, operation_type, source_path, destination_path,
                source_size, source_modified_at, reason, status, payload_json, undo_status
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'available')",
            params![
                operation.id,
                plan_id,
                task_id,
                operation.operation_type,
                operation.source_path,
                operation.destination_path,
                operation
                    .source_size
                    .map(|value| as_i64(value, "文件大小"))
                    .transpose()?,
                operation.source_modified_at,
                operation.reason,
                operation.status,
                if operation.tags.is_empty() {
                    None
                } else {
                    Some(
                        serde_json::to_string(&operation.tags)
                            .map_err(|error| format!("序列化操作标签失败: {error}"))?,
                    )
                },
            ],
        )
        .map_err(|error| format!("写入操作计划失败: {error}"))?;
    Ok(())
}

pub fn update_plan(connection: &Connection, plan_id: &str, status: &str) -> Result<(), String> {
    connection
        .execute(
            "UPDATE plans SET status = ?1 WHERE id = ?2",
            params![status, plan_id],
        )
        .map_err(|error| format!("更新计划状态失败: {error}"))?;
    Ok(())
}

pub fn load_plan_operations(
    connection: &Connection,
    plan_id: &str,
) -> Result<Vec<PlannedOperation>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, operation_type, source_path, destination_path, source_size,
                    source_modified_at, reason, status, error, payload_json
             FROM operations WHERE plan_id = ?1 ORDER BY rowid ASC",
        )
        .map_err(|error| format!("准备计划读取失败: {error}"))?;
    let rows = statement
        .query_map(params![plan_id], |row| {
            let status: String = row.get("status")?;
            let payload: Option<String> = row.get("payload_json")?;
            let tags = payload
                .as_deref()
                .and_then(|value| serde_json::from_str(value).ok())
                .unwrap_or_default();
            Ok(PlannedOperation {
                id: row.get("id")?,
                operation_type: row.get("operation_type")?,
                source_path: row.get("source_path")?,
                destination_path: row.get("destination_path")?,
                reason: row.get("reason")?,
                rule_id: None,
                conflict: None,
                status,
                source_size: row
                    .get::<_, Option<i64>>("source_size")?
                    .and_then(|value| u64::try_from(value).ok()),
                source_modified_at: row.get("source_modified_at")?,
                tags,
            })
        })
        .map_err(|error| format!("读取计划操作失败: {error}"))?;
    rows.map(|row| row.map_err(|error| format!("读取计划操作失败: {error}")))
        .collect()
}

pub fn update_operation(
    connection: &Connection,
    operation_id: &str,
    status: &str,
    error: Option<&str>,
    undo_status: Option<&str>,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE operations SET status = ?1, error = ?2, executed_at = ?3,
                    undo_status = COALESCE(?4, undo_status)
             WHERE id = ?5",
            params![status, error, now_millis(), undo_status, operation_id],
        )
        .map_err(|error| format!("更新操作日志失败: {error}"))?;
    Ok(())
}

pub fn find_operation(
    connection: &Connection,
    operation_id: &str,
) -> Result<Option<OperationHistoryItem>, String> {
    connection
        .query_row(
            "SELECT id, plan_id, task_id, operation_type, source_path, destination_path,
                    status, error, executed_at, undo_status
             FROM operations WHERE id = ?1",
            params![operation_id],
            |row| {
                Ok(OperationHistoryItem {
                    id: row.get("id")?,
                    plan_id: row.get("plan_id")?,
                    task_id: row.get("task_id")?,
                    operation_type: row.get("operation_type")?,
                    source_path: row.get("source_path")?,
                    destination_path: row.get("destination_path")?,
                    status: row.get("status")?,
                    error: row.get("error")?,
                    executed_at: row.get("executed_at")?,
                    undo_status: row.get("undo_status")?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("读取操作日志失败: {error}"))
}

pub fn list_history(
    connection: &Connection,
    limit: u32,
) -> Result<Vec<OperationHistoryItem>, String> {
    let limit = if limit == 0 { 200 } else { limit.min(10_000) };
    let mut statement = connection
        .prepare(
            "SELECT id, plan_id, task_id, operation_type, source_path, destination_path,
                    status, error, executed_at, undo_status
             FROM operations ORDER BY COALESCE(executed_at, 0) DESC, rowid DESC LIMIT ?1",
        )
        .map_err(|error| format!("准备历史查询失败: {error}"))?;
    let rows = statement
        .query_map(params![limit], |row| {
            Ok(OperationHistoryItem {
                id: row.get("id")?,
                plan_id: row.get("plan_id")?,
                task_id: row.get("task_id")?,
                operation_type: row.get("operation_type")?,
                source_path: row.get("source_path")?,
                destination_path: row.get("destination_path")?,
                status: row.get("status")?,
                error: row.get("error")?,
                executed_at: row.get("executed_at")?,
                undo_status: row.get("undo_status")?,
            })
        })
        .map_err(|error| format!("查询历史失败: {error}"))?;
    rows.map(|row| row.map_err(|error| format!("读取历史失败: {error}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        apply_tags, file_is_unchanged, insert_operation, insert_plan, list_files,
        load_plan_operations, open_connection_at, upsert_file, SCHEMA_VERSION,
    };
    use crate::models::{CatalogQuery, FileRecord, PlannedOperation};
    use std::path::PathBuf;

    #[test]
    fn creates_schema_and_enables_wal() {
        let root =
            std::env::temp_dir().join(format!("windows-easy-backup-db-{}", std::process::id()));
        let path = root.join("app.db");
        let connection = open_connection_at(&path).unwrap();
        let version: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        let journal: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(journal.to_ascii_lowercase(), "wal");
        drop(connection);
        let _ = std::fs::remove_dir_all(PathBuf::from(root));
    }

    #[test]
    fn detects_unchanged_file_for_incremental_scan() {
        let root = std::env::temp_dir().join(format!(
            "windows-easy-backup-incremental-{}",
            std::process::id()
        ));
        let path = root.join("app.db");
        let connection = open_connection_at(&path).unwrap();
        let file = FileRecord {
            id: "C:\\demo.txt".to_string(),
            path: "C:\\demo.txt".to_string(),
            filename: "demo.txt".to_string(),
            stem: "demo".to_string(),
            extension: "txt".to_string(),
            size: 12,
            created_at: None,
            modified_at: Some(42),
            accessed_at: None,
            mime: Some("text/plain".to_string()),
            category: "document".to_string(),
            source_type: Some("custom".to_string()),
            hash: None,
            hash_algorithm: None,
            metadata: None,
            tags: vec![],
        };
        upsert_file(&connection, &file).unwrap();
        assert!(file_is_unchanged(&connection, &file.path, 12, Some(42), Some("custom")).unwrap());
        assert!(!file_is_unchanged(&connection, &file.path, 13, Some(42), Some("custom")).unwrap());
        drop(connection);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn schema_contains_payload_and_tags_round_trip() {
        let root = std::env::temp_dir().join(format!(
            "windows-easy-backup-db-round-trip-{}",
            std::process::id()
        ));
        let path = root.join("app.db");
        let connection = open_connection_at(&path).unwrap();
        let payload_column: String = connection
            .query_row(
                "SELECT name FROM pragma_table_info('operations') WHERE name = 'payload_json'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(payload_column, "payload_json");

        let file = FileRecord {
            id: "C:\\demo-tagged.txt".to_string(),
            path: "C:\\demo-tagged.txt".to_string(),
            filename: "demo-tagged.txt".to_string(),
            stem: "demo-tagged".to_string(),
            extension: "txt".to_string(),
            size: 12,
            created_at: None,
            modified_at: Some(42),
            accessed_at: None,
            mime: Some("text/plain".to_string()),
            category: "document".to_string(),
            source_type: Some("custom".to_string()),
            hash: None,
            hash_algorithm: None,
            metadata: None,
            tags: vec![],
        };
        upsert_file(&connection, &file).unwrap();
        apply_tags(
            &connection,
            &file.path,
            &["工作".to_string(), "待整理".to_string()],
        )
        .unwrap();
        let files = list_files(
            &connection,
            &CatalogQuery {
                root_path: Some(file.path.clone()),
                limit: 10,
                ..CatalogQuery::default()
            },
        )
        .unwrap();
        assert_eq!(
            files[0].tags,
            vec!["工作".to_string(), "待整理".to_string()]
        );

        insert_plan(&connection, "plan-test", "task-test", "ready").unwrap();
        let operation = PlannedOperation {
            id: "operation-test".to_string(),
            operation_type: "tag".to_string(),
            source_path: file.path,
            destination_path: None,
            reason: "测试标签计划".to_string(),
            rule_id: Some("rule-test".to_string()),
            conflict: None,
            status: "ready".to_string(),
            source_size: Some(12),
            source_modified_at: Some(42),
            tags: vec!["测试".to_string()],
        };
        insert_operation(&connection, "plan-test", "task-test", &operation).unwrap();
        let loaded = load_plan_operations(&connection, "plan-test").unwrap();
        assert_eq!(loaded[0].tags, vec!["测试".to_string()]);

        drop(connection);
        let _ = std::fs::remove_dir_all(PathBuf::from(root));
    }
}
