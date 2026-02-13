//! Task tool request/response types.

use serde::{Deserialize, Serialize};

/// Request to create a task.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub description: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_task: Option<String>,
}

fn default_priority() -> String {
    "medium".into()
}

/// Response from create_task.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateTaskResponse {
    pub task_id: String,
}

/// Request to list tasks.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListTasksRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Task info in list response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskInfo {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_to: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_by: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<String>>,
}

/// Response from list_tasks.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListTasksResponse {
    pub tasks: Vec<TaskInfo>,
}

/// Request to claim a task.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaimTaskRequest {
    pub task_id: String,
}

/// Response from claim_task.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaimTaskResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Request to update task status.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateTaskStatusRequest {
    pub task_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Response from update_task_status.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateTaskStatusResponse {
    pub success: bool,
}

/// Request to assign a task.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssignTaskRequest {
    pub task_id: String,
    pub agent_id: String,
}

/// Response from assign_task.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssignTaskResponse {
    pub success: bool,
}

/// Request to get task context.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTaskContextRequest {
    pub task_id: String,
}

/// Response from get_task_context.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTaskContextResponse {
    pub task: TaskInfo,
    pub knowledge: Vec<super::knowledge::KnowledgeResult>,
    pub subtasks: Vec<TaskInfo>,
}


// === Task Dependency Tools ===

/// Request to add a task dependency.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddTaskDependencyRequest {
    pub task_id: String,
    pub blocked_task_id: String,
}

/// Response from add_task_dependency.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddTaskDependencyResponse {
    pub success: bool,
}

/// Request to remove a task dependency.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoveTaskDependencyRequest {
    pub task_id: String,
    pub blocked_task_id: String,
}

/// Response from remove_task_dependency.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoveTaskDependencyResponse {
    pub success: bool,
}
