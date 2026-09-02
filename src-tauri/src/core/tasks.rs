use crate::models::TaskProgress;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);
static TASKS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Clone)]
pub struct TaskHandle {
    pub id: String,
    pub task_type: String,
    cancelled: Arc<AtomicBool>,
}

pub fn create(task_type: &str) -> TaskHandle {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default();
    let id = format!(
        "{}-{}-{}",
        task_type,
        timestamp,
        NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed)
    );
    let cancelled = Arc::new(AtomicBool::new(false));
    registry()
        .lock()
        .expect("task registry poisoned")
        .insert(id.clone(), cancelled.clone());
    TaskHandle {
        id,
        task_type: task_type.to_string(),
        cancelled,
    }
}

pub fn cancel(task_id: &str) -> Result<(), String> {
    let tasks = registry()
        .lock()
        .map_err(|_| "任务注册表不可用。".to_string())?;
    match tasks.get(task_id) {
        Some(token) => {
            token.store(true, Ordering::SeqCst);
            Ok(())
        }
        None => Err(format!("任务不存在或已结束: {task_id}")),
    }
}

pub fn is_cancelled(task: &TaskHandle) -> bool {
    task.cancelled.load(Ordering::SeqCst)
}

pub fn finish(task: &TaskHandle) {
    if let Ok(mut tasks) = registry().lock() {
        tasks.remove(&task.id);
    }
}

pub fn emit_progress(app: &AppHandle, progress: TaskProgress) {
    let _ = app.emit("task-progress", progress);
}

pub fn emit_completed(app: &AppHandle, task_id: &str, task_type: &str, status: &str) {
    let _ = app.emit(
        "task-completed",
        serde_json::json!({
            "taskId": task_id,
            "taskType": task_type,
            "status": status
        }),
    );
}

#[allow(dead_code)]
pub fn emit_error(app: &AppHandle, task_id: &str, task_type: &str, error: &str) {
    let _ = app.emit(
        "task-error",
        serde_json::json!({
            "taskId": task_id,
            "taskType": task_type,
            "error": error
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::{cancel, create, finish, is_cancelled};

    #[test]
    fn cancellation_is_scoped_to_one_task() {
        let first = create("scan");
        let second = create("backup");
        cancel(&first.id).unwrap();
        assert!(is_cancelled(&first));
        assert!(!is_cancelled(&second));
        finish(&first);
        finish(&second);
    }
}
