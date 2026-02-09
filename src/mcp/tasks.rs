//! MCP Tasks — asynchronous task management for long-running operations.
//!
//! Implements the MCP Tasks protocol (SEP-1686) using rmcp's built-in
//! `OperationProcessor`. Clients can enqueue long-running operations (like
//! directory indexing) and poll for completion instead of blocking.
//!
//! # Flow
//!
//! 1. Client sends `tools/call` with a `task` field → rmcp routes to `enqueue_task`
//! 2. Server creates a task, spawns the operation, returns `CreateTaskResult`
//! 3. Client polls via `tasks/get` to check progress
//! 4. Client retrieves result via `tasks/result` when completed
//! 5. Client can cancel via `tasks/cancel`

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rmcp::model::{
    self as mcp, CallToolResult, CreateTaskResult, GetTaskInfoParams, GetTaskInfoResult,
    GetTaskResultParams, ListTasksResult, PaginatedRequestParams, Task, TaskStatus,
};
use rmcp::task_manager::current_timestamp;
use rmcp::ErrorData as McpError;

// ---------------------------------------------------------------------------
// Task state — our internal bookkeeping beyond what rmcp tracks
// ---------------------------------------------------------------------------

/// Internal state for a managed task.
#[derive(Debug, Clone)]
pub struct TaskEntry {
    /// The rmcp Task metadata (id, status, timestamps, etc.).
    pub task: Task,
    /// Human-readable operation description (e.g., "index /path/to/project").
    pub operation: String,
    /// Progress percentage (0–100). Only meaningful while status is Working.
    pub progress: u8,
    /// The completed tool result, stored here once the background work finishes.
    pub result: Option<CallToolResult>,
    /// Error message if the task failed.
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// TaskManager — thread-safe task registry
// ---------------------------------------------------------------------------

/// Manages the lifecycle of asynchronous MCP tasks.
///
/// All methods acquire the inner lock briefly and return — they never hold the
/// lock across an `.await` boundary.
#[derive(Clone, Default)]
pub struct TaskManager {
    inner: Arc<Mutex<TaskManagerInner>>,
}

#[derive(Default)]
struct TaskManagerInner {
    /// Monotonically increasing counter for task IDs.
    next_id: u64,
    /// Active and recently completed tasks keyed by task ID.
    tasks: HashMap<String, TaskEntry>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new task in `Working` status and return its ID.
    ///
    /// Automatically evicts expired terminal tasks before creating the new one.
    pub fn create_task(&self, operation: &str) -> String {
        self.evict_expired();
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let id = format!("task-{}", inner.next_id);
        inner.next_id += 1;

        let now = current_timestamp();
        let task = Task {
            task_id: id.clone(),
            status: TaskStatus::Working,
            status_message: Some(format!("Starting: {operation}")),
            created_at: now.clone(),
            last_updated_at: Some(now),
            ttl: Some(300_000),         // 5 minutes
            poll_interval: Some(1_000), // 1 second
        };

        inner.tasks.insert(
            id.clone(),
            TaskEntry {
                task,
                operation: operation.to_string(),
                progress: 0,
                result: None,
                error: None,
            },
        );

        id
    }

    /// Update progress for a running task.
    pub fn update_progress(&self, task_id: &str, progress: u8, message: Option<&str>) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = inner.tasks.get_mut(task_id) {
            if entry.task.status == TaskStatus::Working {
                entry.progress = progress.min(100);
                entry.task.last_updated_at = Some(current_timestamp());
                if let Some(msg) = message {
                    entry.task.status_message = Some(msg.to_string());
                }
            }
        }
    }

    /// Mark a task as completed with a result.
    pub fn complete_task(&self, task_id: &str, result: CallToolResult) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = inner.tasks.get_mut(task_id) {
            entry.task.status = TaskStatus::Completed;
            entry.task.status_message = Some("Completed successfully".to_string());
            entry.task.last_updated_at = Some(current_timestamp());
            entry.progress = 100;
            entry.result = Some(result);
        }
    }

    /// Mark a task as failed.
    pub fn fail_task(&self, task_id: &str, error: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = inner.tasks.get_mut(task_id) {
            entry.task.status = TaskStatus::Failed;
            entry.task.status_message = Some(format!("Failed: {error}"));
            entry.task.last_updated_at = Some(current_timestamp());
            entry.error = Some(error.to_string());
        }
    }

    /// Cancel a task. Returns `true` if the task existed and was cancellable.
    pub fn cancel_task(&self, task_id: &str) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = inner.tasks.get_mut(task_id) {
            match entry.task.status {
                TaskStatus::Working => {
                    entry.task.status = TaskStatus::Cancelled;
                    entry.task.status_message = Some("Cancelled by client".to_string());
                    entry.task.last_updated_at = Some(current_timestamp());
                    true
                }
                _ => false, // Already terminal
            }
        } else {
            false
        }
    }

    /// Get the Task metadata for a given task ID.
    pub fn get_task(&self, task_id: &str) -> Option<TaskEntry> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.tasks.get(task_id).cloned()
    }

    /// List all tasks (optionally paginated via cursor).
    pub fn list_tasks(&self, cursor: Option<&str>) -> (Vec<Task>, u64) {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let total = inner.tasks.len() as u64;

        // Simple cursor-based pagination: cursor is the task ID to start after.
        let mut tasks: Vec<&TaskEntry> = inner.tasks.values().collect();
        tasks.sort_by(|a, b| a.task.task_id.cmp(&b.task.task_id));

        let tasks = if let Some(cursor_id) = cursor {
            tasks
                .into_iter()
                .skip_while(|e| e.task.task_id.as_str() <= cursor_id)
                .take(50)
                .map(|e| e.task.clone())
                .collect()
        } else {
            tasks.into_iter().take(50).map(|e| e.task.clone()).collect()
        };

        (tasks, total)
    }

    /// Take the completed result for a task (consumes it — subsequent calls return None).
    pub fn take_result(&self, task_id: &str) -> Option<CallToolResult> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.tasks.get_mut(task_id).and_then(|e| e.result.take())
    }

    /// Check if a task has been cancelled (for cooperative cancellation).
    pub fn is_cancelled(&self, task_id: &str) -> bool {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .tasks
            .get(task_id)
            .map(|e| e.task.status == TaskStatus::Cancelled)
            .unwrap_or(false)
    }

    /// Remove completed, failed, and cancelled tasks that have exceeded their TTL.
    ///
    /// Called automatically when creating new tasks to prevent unbounded growth.
    /// Only evicts terminal tasks (Completed/Failed/Cancelled); Working tasks
    /// are never removed.
    pub fn evict_expired(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let now_ms = current_timestamp().parse::<u64>().unwrap_or(0);

        inner.tasks.retain(|_, entry| {
            // Never evict working tasks
            if entry.task.status == TaskStatus::Working {
                return true;
            }
            // Keep tasks within their TTL
            let ttl_ms = entry.task.ttl.unwrap_or(300_000);
            let updated_ms = entry
                .task
                .last_updated_at
                .as_ref()
                .and_then(|ts| ts.parse::<u64>().ok())
                .unwrap_or(0);
            now_ms.saturating_sub(updated_ms) < ttl_ms
        });
    }
}

// ---------------------------------------------------------------------------
// ServerHandler helpers — convert TaskManager state into rmcp response types
// ---------------------------------------------------------------------------

/// Build a `CreateTaskResult` from a newly created task.
pub fn make_create_result(
    manager: &TaskManager,
    task_id: &str,
) -> Result<CreateTaskResult, McpError> {
    manager
        .get_task(task_id)
        .map(|entry| CreateTaskResult { task: entry.task })
        .ok_or_else(|| McpError::internal_error("Task creation failed".to_string(), None))
}

/// Build a `GetTaskInfoResult` for a task.
pub fn make_task_info(
    manager: &TaskManager,
    params: GetTaskInfoParams,
) -> Result<GetTaskInfoResult, McpError> {
    let entry = manager.get_task(&params.task_id);
    // Return the task if it exists, or None (the field is Option<Task>).
    Ok(GetTaskInfoResult {
        task: entry.map(|e| e.task),
    })
}

/// Build a `ListTasksResult`.
pub fn make_list_result(
    manager: &TaskManager,
    params: Option<PaginatedRequestParams>,
) -> Result<ListTasksResult, McpError> {
    let cursor = params.and_then(|p| p.cursor);
    let (tasks, total) = manager.list_tasks(cursor.as_deref());
    Ok(ListTasksResult {
        tasks,
        next_cursor: None,
        total: Some(total),
    })
}

/// Build a `TaskResult` for a completed task.
pub fn make_task_result(
    manager: &TaskManager,
    params: GetTaskResultParams,
) -> Result<mcp::TaskResult, McpError> {
    let entry = manager.get_task(&params.task_id).ok_or_else(|| {
        McpError::invalid_params(format!("Unknown task: {}", params.task_id), None)
    })?;

    match entry.task.status {
        TaskStatus::Completed => {
            // Try to take the result (may have been consumed already)
            let result = manager.take_result(&params.task_id);
            let value = match result {
                Some(tool_result) => serde_json::to_value(&tool_result).unwrap_or_default(),
                None => serde_json::json!({"note": "Result already consumed"}),
            };
            Ok(mcp::TaskResult {
                content_type: "application/json".to_string(),
                value,
                summary: entry.task.status_message,
            })
        }
        TaskStatus::Failed => Err(McpError::internal_error(
            entry.error.unwrap_or_else(|| "Task failed".to_string()),
            None,
        )),
        TaskStatus::Cancelled => Err(McpError::internal_error(
            "Task was cancelled".to_string(),
            None,
        )),
        _ => Err(McpError::internal_error(
            format!(
                "Task {} is still running (status: {:?})",
                params.task_id, entry.task.status
            ),
            None,
        )),
    }
}

/// Handle a cancel request.
pub fn handle_cancel(manager: &TaskManager, task_id: &str) -> Result<(), McpError> {
    if manager.cancel_task(task_id) {
        Ok(())
    } else {
        let entry = manager.get_task(task_id);
        match entry {
            Some(e) => Err(McpError::internal_error(
                format!(
                    "Task {} cannot be cancelled (status: {:?})",
                    task_id, e.task.status
                ),
                None,
            )),
            None => Err(McpError::invalid_params(
                format!("Unknown task: {task_id}"),
                None,
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::Content;

    #[test]
    fn create_and_complete_task() {
        let mgr = TaskManager::new();
        let id = mgr.create_task("index /tmp/project");

        // Freshly created → Working
        let entry = mgr.get_task(&id).unwrap();
        assert_eq!(entry.task.status, TaskStatus::Working);
        assert_eq!(entry.progress, 0);
        assert!(entry.task.task_id.starts_with("task-"));

        // Update progress
        mgr.update_progress(&id, 50, Some("Parsing files..."));
        let entry = mgr.get_task(&id).unwrap();
        assert_eq!(entry.progress, 50);
        assert_eq!(
            entry.task.status_message.as_deref(),
            Some("Parsing files...")
        );

        // Complete
        let result = CallToolResult::success(vec![Content::text("Indexed 42 files")]);
        mgr.complete_task(&id, result);
        let entry = mgr.get_task(&id).unwrap();
        assert_eq!(entry.task.status, TaskStatus::Completed);
        assert_eq!(entry.progress, 100);
    }

    #[test]
    fn fail_task() {
        let mgr = TaskManager::new();
        let id = mgr.create_task("index /nonexistent");

        mgr.fail_task(&id, "Directory not found");
        let entry = mgr.get_task(&id).unwrap();
        assert_eq!(entry.task.status, TaskStatus::Failed);
        assert_eq!(entry.error.as_deref(), Some("Directory not found"));
    }

    #[test]
    fn cancel_task() {
        let mgr = TaskManager::new();
        let id = mgr.create_task("index /tmp/big-project");

        assert!(mgr.cancel_task(&id));
        let entry = mgr.get_task(&id).unwrap();
        assert_eq!(entry.task.status, TaskStatus::Cancelled);

        // Cancelling again returns false (already terminal)
        assert!(!mgr.cancel_task(&id));
    }

    #[test]
    fn cancel_nonexistent_task() {
        let mgr = TaskManager::new();
        assert!(!mgr.cancel_task("task-999"));
    }

    #[test]
    fn is_cancelled_check() {
        let mgr = TaskManager::new();
        let id = mgr.create_task("index /tmp/project");

        assert!(!mgr.is_cancelled(&id));
        mgr.cancel_task(&id);
        assert!(mgr.is_cancelled(&id));
    }

    #[test]
    fn list_tasks_empty() {
        let mgr = TaskManager::new();
        let (tasks, total) = mgr.list_tasks(None);
        assert!(tasks.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn list_tasks_multiple() {
        let mgr = TaskManager::new();
        let _id1 = mgr.create_task("op1");
        let _id2 = mgr.create_task("op2");
        let _id3 = mgr.create_task("op3");

        let (tasks, total) = mgr.list_tasks(None);
        assert_eq!(tasks.len(), 3);
        assert_eq!(total, 3);
    }

    #[test]
    fn take_result_consumes() {
        let mgr = TaskManager::new();
        let id = mgr.create_task("index /tmp");
        let result = CallToolResult::success(vec![Content::text("done")]);
        mgr.complete_task(&id, result);

        // First take succeeds
        assert!(mgr.take_result(&id).is_some());
        // Second take returns None (consumed)
        assert!(mgr.take_result(&id).is_none());
    }

    #[test]
    fn progress_clamped_to_100() {
        let mgr = TaskManager::new();
        let id = mgr.create_task("index /tmp");
        mgr.update_progress(&id, 200, None);
        let entry = mgr.get_task(&id).unwrap();
        assert_eq!(entry.progress, 100);
    }

    #[test]
    fn progress_ignored_after_completion() {
        let mgr = TaskManager::new();
        let id = mgr.create_task("index /tmp");
        let result = CallToolResult::success(vec![Content::text("done")]);
        mgr.complete_task(&id, result);

        // Progress update on completed task is a no-op
        mgr.update_progress(&id, 50, Some("late update"));
        let entry = mgr.get_task(&id).unwrap();
        assert_eq!(entry.progress, 100); // stays at 100
    }

    #[test]
    fn monotonic_task_ids() {
        let mgr = TaskManager::new();
        let id1 = mgr.create_task("a");
        let id2 = mgr.create_task("b");
        let id3 = mgr.create_task("c");
        assert_eq!(id1, "task-0");
        assert_eq!(id2, "task-1");
        assert_eq!(id3, "task-2");
    }

    #[test]
    fn make_create_result_works() {
        let mgr = TaskManager::new();
        let id = mgr.create_task("test op");
        let result = make_create_result(&mgr, &id).unwrap();
        assert_eq!(result.task.task_id, id);
        assert_eq!(result.task.status, TaskStatus::Working);
    }

    #[test]
    fn make_task_info_unknown_returns_none() {
        let mgr = TaskManager::new();
        let result = make_task_info(
            &mgr,
            GetTaskInfoParams {
                meta: None,
                task_id: "nonexistent".to_string(),
            },
        )
        .unwrap();
        assert!(result.task.is_none());
    }

    #[test]
    fn make_task_result_not_ready() {
        let mgr = TaskManager::new();
        let id = mgr.create_task("running");
        let err = make_task_result(
            &mgr,
            GetTaskResultParams {
                meta: None,
                task_id: id,
            },
        );
        assert!(err.is_err());
    }

    #[test]
    fn make_task_result_completed() {
        let mgr = TaskManager::new();
        let id = mgr.create_task("index");
        let tool_result = CallToolResult::success(vec![Content::text("42 files")]);
        mgr.complete_task(&id, tool_result);

        let result = make_task_result(
            &mgr,
            GetTaskResultParams {
                meta: None,
                task_id: id,
            },
        )
        .unwrap();
        assert_eq!(result.content_type, "application/json");
    }

    #[test]
    fn handle_cancel_works() {
        let mgr = TaskManager::new();
        let id = mgr.create_task("op");
        assert!(handle_cancel(&mgr, &id).is_ok());

        // Second cancel fails (already cancelled)
        assert!(handle_cancel(&mgr, &id).is_err());
    }

    #[test]
    fn handle_cancel_unknown() {
        let mgr = TaskManager::new();
        assert!(handle_cancel(&mgr, "nope").is_err());
    }

    #[test]
    fn concurrent_access() {
        use std::thread;

        let mgr = TaskManager::new();
        let mgr_clone = mgr.clone();

        let handle = thread::spawn(move || {
            for i in 0..50 {
                let id = mgr_clone.create_task(&format!("op-{i}"));
                mgr_clone.update_progress(&id, (i * 2) as u8, None);
            }
        });

        for i in 50..100 {
            let id = mgr.create_task(&format!("op-{i}"));
            mgr.update_progress(&id, (i * 2) as u8, None);
        }

        handle.join().unwrap();

        let (tasks, total) = mgr.list_tasks(None);
        assert_eq!(total, 100);
        // list_tasks caps at 50 per page
        assert_eq!(tasks.len(), 50);
    }
}
