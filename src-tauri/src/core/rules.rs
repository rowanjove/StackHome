use crate::database::{self, open_connection};
use crate::models::{FileRecord, RuleAction, RuleDefinition, RuleRecord};
use regex::Regex;
use serde_json::Value;

pub fn list() -> Result<Vec<RuleRecord>, String> {
    let connection = open_connection()?;
    let mut rules = database::list_rules(&connection)?;
    let existing_ids = rules
        .iter()
        .map(|rule| rule.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    for rule in built_in_rules() {
        if !existing_ids.contains(rule.id.as_str()) {
            database::upsert_rule(&connection, &rule)?;
        }
    }
    rules = database::list_rules(&connection)?;
    Ok(rules)
}

pub fn save(mut rule: RuleRecord) -> Result<RuleRecord, String> {
    validate_action(&rule.definition.action)?;
    if rule.name.trim().is_empty() {
        return Err("规则名称不能为空。".to_string());
    }
    rule.name = rule.name.trim().to_string();
    let now = database::now_millis();
    if rule.created_at == 0 {
        rule.created_at = now;
    }
    rule.updated_at = now;
    let connection = open_connection()?;
    database::upsert_rule(&connection, &rule)?;
    Ok(rule)
}

pub fn remove(rule_id: String) -> Result<(), String> {
    if rule_id.starts_with("builtin-") {
        return Err("内置规则不能删除，请改为关闭。".to_string());
    }
    let connection = open_connection()?;
    database::delete_rule(&connection, &rule_id)
}

pub fn find(rule_id: &str) -> Result<Option<RuleRecord>, String> {
    let _ = list()?;
    let connection = open_connection()?;
    database::find_rule(&connection, rule_id)
}

pub fn matches(rule: &RuleRecord, file: &FileRecord) -> bool {
    if !rule.enabled || !matches_source(rule.definition.source.as_ref(), file) {
        return false;
    }
    evaluate_condition(&rule.definition.condition, file)
}

pub fn action(rule: &RuleRecord) -> &RuleAction {
    &rule.definition.action
}

fn matches_source(source: Option<&crate::models::RuleSource>, file: &FileRecord) -> bool {
    let Some(source) = source else { return true };
    if let Some(source_type) = &source.source_type {
        if file.source_type.as_deref() != Some(source_type.as_str()) {
            return false;
        }
    }
    if let Some(path_contains) = &source.path_contains {
        if !file
            .path
            .to_ascii_lowercase()
            .contains(&path_contains.to_ascii_lowercase())
        {
            return false;
        }
    }
    true
}

fn evaluate_condition(condition: &Value, file: &FileRecord) -> bool {
    let Some(object) = condition.as_object() else {
        return true;
    };
    if let Some(all) = object.get("all").and_then(Value::as_array) {
        return all.iter().all(|item| evaluate_condition(item, file));
    }
    if let Some(any) = object.get("any").and_then(Value::as_array) {
        return any.iter().any(|item| evaluate_condition(item, file));
    }
    if let Some(not) = object.get("not") {
        return !evaluate_condition(not, file);
    }
    let field = object
        .get("field")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let operator = object
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or("equals");
    let expected = object.get("value").unwrap_or(&Value::Null);
    compare(field_value(file, field), operator, expected)
}

fn field_value<'a>(file: &'a FileRecord, field: &str) -> Option<Value> {
    let value = match field {
        "category" => Value::String(file.category.clone()),
        "extension" => Value::String(file.extension.clone()),
        "mime" => file.mime.clone().map(Value::String)?,
        "filename" => Value::String(file.filename.clone()),
        "stem" => Value::String(file.stem.clone()),
        "path" | "directory" => Value::String(file.path.clone()),
        "sourceType" | "source_type" => Value::String(file.source_type.clone().unwrap_or_default()),
        "size" => Value::from(file.size),
        "created" | "createdAt" | "created_at" => file.created_at.map(Value::from)?,
        "modified" | "modifiedAt" | "modified_at" => file.modified_at.map(Value::from)?,
        "parent" => Value::String(
            std::path::Path::new(&file.path)
                .parent()
                .map(|value| value.display().to_string())
                .unwrap_or_default(),
        ),
        "image.width" => file.metadata.as_ref()?.width.map(Value::from)?,
        "image.height" => file.metadata.as_ref()?.height.map(Value::from)?,
        "exif.date" => Value::String(
            file.metadata
                .as_ref()?
                .exif_date
                .clone()
                .unwrap_or_default(),
        ),
        "exif.camera" => Value::String(
            file.metadata
                .as_ref()?
                .camera_model
                .clone()
                .unwrap_or_default(),
        ),
        "audio.artist" => Value::String(file.metadata.as_ref()?.artist.clone().unwrap_or_default()),
        "audio.album" => Value::String(file.metadata.as_ref()?.album.clone().unwrap_or_default()),
        "audio.title" => Value::String(file.metadata.as_ref()?.title.clone().unwrap_or_default()),
        "audio.track" => file.metadata.as_ref()?.track.map(Value::from)?,
        _ => return None,
    };
    Some(value)
}

fn compare(actual: Option<Value>, operator: &str, expected: &Value) -> bool {
    let Some(actual) = actual else { return false };
    match operator {
        "equals" => normalize_string(&actual) == normalize_string(expected),
        "not_equals" => normalize_string(&actual) != normalize_string(expected),
        "contains" => normalize_string(&actual).contains(&normalize_string(expected)),
        "starts_with" => normalize_string(&actual).starts_with(&normalize_string(expected)),
        "ends_with" => normalize_string(&actual).ends_with(&normalize_string(expected)),
        "regex" => Regex::new(expected.as_str().unwrap_or_default())
            .map(|regex| regex.is_match(&normalize_string(&actual)))
            .unwrap_or(false),
        "greater_than" => numeric(&actual) > numeric(expected),
        "less_than" => numeric(&actual) < numeric(expected),
        "greater_or_equals" => numeric(&actual) >= numeric(expected),
        "less_or_equals" => numeric(&actual) <= numeric(expected),
        "in" => expected.as_array().is_some_and(|values| {
            values
                .iter()
                .any(|value| normalize_string(&actual) == normalize_string(value))
        }),
        _ => false,
    }
}

fn normalize_string(value: &Value) -> String {
    value
        .as_str()
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| value.to_string().to_ascii_lowercase())
}

fn numeric(value: &Value) -> f64 {
    value.as_f64().unwrap_or(f64::NAN)
}

fn validate_action(action: &RuleAction) -> Result<(), String> {
    if !matches!(
        action.action_type.as_str(),
        "rename" | "move" | "copy" | "tag" | "ignore"
    ) {
        return Err(format!("不支持的规则动作: {}", action.action_type));
    }
    if matches!(action.action_type.as_str(), "move" | "copy")
        && action
            .destination_template
            .as_deref()
            .is_none_or(str::is_empty)
    {
        return Err("移动或复制规则必须提供目标模板。".to_string());
    }
    Ok(())
}

pub fn built_in_rules() -> Vec<RuleRecord> {
    let now = database::now_millis();
    vec![
        built_in(
            "builtin-download-images",
            "Downloads 图片",
            10,
            serde_json::json!({"all":[{"field":"category","operator":"equals","value":"image"}]}),
            RuleAction {
                action_type: "move".to_string(),
                destination_template: Some("Pictures\\Downloads\\Images".to_string()),
                rename_template: None,
                tags: vec![],
            },
            now,
        ),
        built_in(
            "builtin-download-videos",
            "Downloads 视频",
            20,
            serde_json::json!({"field":"category","operator":"equals","value":"video"}),
            RuleAction {
                action_type: "move".to_string(),
                destination_template: Some("Videos\\Downloads".to_string()),
                rename_template: None,
                tags: vec![],
            },
            now,
        ),
        built_in(
            "builtin-download-documents",
            "Downloads 文档",
            30,
            serde_json::json!({"field":"category","operator":"equals","value":"document"}),
            RuleAction {
                action_type: "move".to_string(),
                destination_template: Some("Documents\\Downloads".to_string()),
                rename_template: None,
                tags: vec![],
            },
            now,
        ),
        built_in(
            "builtin-screenshots",
            "截图整理",
            5,
            serde_json::json!({"any":[{"field":"filename","operator":"contains","value":"screenshot"},{"field":"filename","operator":"contains","value":"屏幕截图"},{"field":"filename","operator":"contains","value":"截屏"},{"field":"filename","operator":"contains","value":"snipaste"}]}),
            RuleAction {
                action_type: "move".to_string(),
                destination_template: Some("Pictures\\Screenshots".to_string()),
                rename_template: None,
                tags: vec![],
            },
            now,
        ),
    ]
}

fn built_in(
    id: &str,
    name: &str,
    priority: i32,
    condition: Value,
    action: RuleAction,
    now: i64,
) -> RuleRecord {
    RuleRecord {
        id: id.to_string(),
        name: name.to_string(),
        enabled: true,
        priority,
        rule_type: "organize".to_string(),
        definition: RuleDefinition {
            source: Some(crate::models::RuleSource {
                source_type: Some("downloads".to_string()),
                path_contains: None,
            }),
            condition,
            action,
        },
        created_at: now,
        updated_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::matches;
    use crate::models::{FileMetadata, FileRecord, RuleAction, RuleDefinition, RuleRecord};

    fn file() -> FileRecord {
        FileRecord {
            id: "file".to_string(),
            path: "C:\\Downloads\\Screenshot.png".to_string(),
            filename: "Screenshot.png".to_string(),
            stem: "Screenshot".to_string(),
            extension: "png".to_string(),
            size: 10,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            mime: Some("image/png".to_string()),
            category: "image".to_string(),
            source_type: Some("downloads".to_string()),
            hash: None,
            hash_algorithm: None,
            metadata: Some(FileMetadata {
                width: Some(100),
                ..FileMetadata::default()
            }),
            tags: vec![],
        }
    }

    #[test]
    fn evaluates_nested_and_or_not_conditions() {
        let rule = RuleRecord {
            id: "test".to_string(),
            name: "test".to_string(),
            enabled: true,
            priority: 1,
            rule_type: "organize".to_string(),
            definition: RuleDefinition {
                source: None,
                condition: serde_json::json!({"all":[{"field":"category","operator":"equals","value":"image"},{"not":{"field":"filename","operator":"contains","value":"draft"}},{"any":[{"field":"image.width","operator":"greater_than","value":80},{"field":"filename","operator":"contains","value":"fallback"}]}]}),
                action: RuleAction::default(),
            },
            created_at: 0,
            updated_at: 0,
        };
        assert!(matches(&rule, &file()));
    }
}
