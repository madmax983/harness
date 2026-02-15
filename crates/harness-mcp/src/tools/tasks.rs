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
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
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
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
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
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
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
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
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
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from assign_task.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssignTaskResponse {
    pub success: bool,
}

/// Request to dispatch ready pending tasks to eligible agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DispatchReadyTasksRequest {
    /// Maximum number of assignments to create in this pass.
    #[serde(default = "default_dispatch_limit")]
    pub max_assignments: usize,
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

fn default_dispatch_limit() -> usize {
    10
}

/// A single task dispatch assignment.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskDispatchAssignment {
    pub task_id: String,
    pub agent_id: String,
}

/// Response from dispatch_ready_tasks.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DispatchReadyTasksResponse {
    pub inspected_at: String,
    pub ready_task_count: usize,
    pub eligible_agent_count: usize,
    pub assignment_count: usize,
    pub assignments: Vec<TaskDispatchAssignment>,
}

/// Request to nudge an assigned agent or replan stalled task ownership.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NudgeOrReplanRequest {
    pub task_id: String,
    #[serde(default = "default_nudge_mode")]
    pub mode: String,
    #[serde(default = "default_nudge_inactivity_minutes")]
    pub inactivity_minutes: u64,
    #[serde(default = "default_nudge_max_actions")]
    pub max_actions: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nudge_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

fn default_nudge_mode() -> String {
    "auto".to_string()
}

fn default_nudge_inactivity_minutes() -> u64 {
    15
}

fn default_nudge_max_actions() -> usize {
    10
}

/// One action considered or taken while nudging/replanning tasks.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NudgeOrReplanAction {
    pub task_id: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Response from nudge_or_replan.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NudgeOrReplanResponse {
    pub task_id: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    pub inspected_at: String,
    pub stale_task_count: usize,
    pub nudged_count: usize,
    pub reassigned_count: usize,
    pub actions: Vec<NudgeOrReplanAction>,
}

/// Individual completion check result supplied to task_completion_gate.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CompletionCheckResult {
    pub name: String,
    pub passed: bool,
}

/// Request to validate required completion checks before closing a task.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskCompletionGateRequest {
    pub task_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub checks: Vec<CompletionCheckResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_checks: Option<Vec<String>>,
    #[serde(default)]
    pub finalize: bool,
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from task_completion_gate.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskCompletionGateResponse {
    pub task_id: String,
    pub allowed: bool,
    pub missing_summary_fields: Vec<String>,
    pub missing_checks: Vec<String>,
    pub failed_checks: Vec<String>,
    pub required_checks: Vec<String>,
    pub validated_checks: usize,
    pub finalized: bool,
}

/// Request to get task context.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTaskContextRequest {
    pub task_id: String,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
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
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
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
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from remove_task_dependency.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoveTaskDependencyResponse {
    pub success: bool,
}

// === Graph Traversal Tools ===

/// Request to find a path between tasks in the dependency graph.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FindPathRequest {
    pub from_task_id: String,
    pub to_task_id: String,
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

fn default_max_depth() -> usize {
    10
}

/// A single step in a task dependency path.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PathStep {
    pub task_id: String,
    pub title: String,
    pub relationship: String, // "blocks" or "blocked_by"
}

/// Response from find_path.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FindPathResponse {
    pub found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<PathStep>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance: Option<usize>,
}

// === Task Statistics Tools ===

/// Request to get task statistics for analytics dashboard.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskStatisticsRequest {
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Task counts by status.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StatusCounts {
    pub pending: usize,
    pub claimed: usize,
    pub in_progress: usize,
    pub completed: usize,
    pub failed: usize,
    pub total: usize,
}

/// Task counts by priority.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PriorityCounts {
    pub low: usize,
    pub medium: usize,
    pub high: usize,
    pub critical: usize,
}

/// Completion metrics.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CompletionMetrics {
    /// Percentage of tasks completed (0-100)
    pub completion_rate: f64,
    /// Average time to completion in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_completion_time_secs: Option<f64>,
    /// Median time to completion in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_completion_time_secs: Option<f64>,
}

/// Response from task_statistics.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskStatisticsResponse {
    pub status_counts: StatusCounts,
    pub priority_counts: PriorityCounts,
    pub completion_metrics: CompletionMetrics,
    /// Number of tasks with assigned agents
    pub assigned_count: usize,
    /// Number of tasks with blocking dependencies
    pub blocked_count: usize,
}

// === Task History Tools ===

/// Request to get task history.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTaskHistoryRequest {
    pub task_id: String,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Information about a specific version of a task.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskVersionInfo {
    /// Sequential version number (1, 2, 3, ...)
    pub version_number: u64,
    /// Task data at this version
    pub task: TaskInfo,
    /// When this version became valid (ISO 8601)
    pub valid_from: String,
    /// When this version was recorded (ISO 8601)
    pub transaction_time: String,
    /// Properties that changed in this version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<TaskVersionChanges>,
}

/// Changes made in a specific version.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskVersionChanges {
    /// Fields that were added or modified
    pub modified_fields: Vec<String>,
    /// Count of property changes
    pub change_count: usize,
}

/// Response from get_task_history.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTaskHistoryResponse {
    /// All versions of the task, ordered chronologically (oldest first)
    pub versions: Vec<TaskVersionInfo>,
    /// Total number of versions
    pub version_count: usize,
}

// === Semantic Search Tools ===

/// Request to search tasks semantically using vector embeddings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SemanticSearchTasksRequest {
    /// Natural language search query
    pub query: String,
    /// Maximum number of results to return
    #[serde(default = "default_search_limit")]
    pub limit: usize,
    /// Optional status filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

fn default_search_limit() -> usize {
    10
}

/// A task search result with similarity score.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskSearchResult {
    /// The task information
    pub task: TaskInfo,
    /// Semantic similarity score (0.0 to 1.0, higher is more similar)
    pub similarity: f32,
}

/// Response from semantic_search_tasks.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SemanticSearchTasksResponse {
    /// Search results ordered by descending similarity
    pub results: Vec<TaskSearchResult>,
    /// Query used for the search
    pub query: String,
}

/// Request to retrieve task state at a specific point in bi-temporal time.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTaskAsOfRequest {
    pub task_id: String,
    /// Valid time (when the fact was true in reality) in RFC3339 format
    pub valid_time: String,
    /// Transaction time (when the fact was recorded) in RFC3339 format
    /// If not provided, uses current transaction time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_time: Option<String>,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from get_task_as_of.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTaskAsOfResponse {
    pub task: TaskInfo,
    pub valid_time: String,
    pub transaction_time: String,
}

// === Task Tree Tools ===

/// Request to get hierarchical task tree.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTaskTreeRequest {
    /// Optional root task ID. If provided, returns tree rooted at this task.
    /// If not provided, returns all top-level tasks (tasks with no parent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_task_id: Option<String>,
    /// Maximum depth to traverse (default: unlimited)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// A node in the task tree with nested children.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskNode {
    /// Task information
    pub task: TaskInfo,
    /// Child tasks (subtasks)
    pub children: Vec<TaskNode>,
    /// Depth in the tree (0 = root)
    pub depth: usize,
}

/// Response from get_task_tree.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTaskTreeResponse {
    /// Root nodes (top-level tasks or specified root)
    pub roots: Vec<TaskNode>,
    /// Total number of tasks in the tree
    pub total_tasks: usize,
    /// Maximum depth in the tree
    pub max_depth: usize,
}

// === Cold Storage Query Tools ===

/// Request to query cold storage statistics and historical data availability.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ColdStorageQueryRequest {
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Cold storage statistics.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ColdStorageStats {
    /// Total number of node versions stored in cold storage
    pub node_versions_stored: u64,
    /// Total number of edge versions stored in cold storage
    pub edge_versions_stored: u64,
    /// Compression ratio (original/compressed)
    pub compression_ratio: f64,
    /// Total bytes written (after compression)
    pub bytes_stored_compressed: u64,
    /// Total bytes if uncompressed
    pub bytes_stored_raw: u64,
}

/// Tiered storage access metrics.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TieredStorageMetrics {
    /// Number of cache hits from hot tier (in-memory)
    pub hot_hits: u64,
    /// Number of cache hits from warm tier (LRU cache)
    pub warm_hits: u64,
    /// Number of cold tier accesses (disk reads)
    pub cold_hits: u64,
    /// Number of misses (data not found)
    pub misses: u64,
    /// Hot tier hit ratio (0.0 to 1.0)
    pub hot_ratio: f64,
    /// Warm tier cache efficiency (0.0 to 1.0)
    pub warm_ratio: f64,
}

/// Response from cold_storage_query.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ColdStorageQueryResponse {
    /// Whether cold storage is enabled and available
    pub available: bool,
    /// Cold storage statistics (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_stats: Option<ColdStorageStats>,
    /// Tiered storage access metrics (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiered_metrics: Option<TieredStorageMetrics>,
    /// Informational message
    pub message: String,
}

// === Graph Export Tools ===

/// Request to export project graph for visualization.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExportProjectGraphRequest {
    /// Export format: "dot" (Graphviz DOT) or "json"
    #[serde(default = "default_export_format")]
    pub format: String,
    /// Include task dependencies in the graph
    #[serde(default = "default_true")]
    pub include_tasks: bool,
    /// Include agent relationships in the graph
    #[serde(default = "default_true")]
    pub include_agents: bool,
    /// Include knowledge graph connections
    #[serde(default)]
    pub include_knowledge: bool,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

fn default_export_format() -> String {
    "dot".into()
}

fn default_true() -> bool {
    true
}

/// Response from export_project_graph.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExportProjectGraphResponse {
    /// The exported graph in the requested format
    pub graph: String,
    /// Format used
    pub format: String,
    /// Statistics about the exported graph
    pub stats: GraphExportStats,
}

/// Statistics about exported graph.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GraphExportStats {
    /// Number of nodes in the graph
    pub node_count: usize,
    /// Number of edges in the graph
    pub edge_count: usize,
    /// Number of tasks included
    pub task_count: usize,
    /// Number of agents included
    pub agent_count: usize,
    /// Number of knowledge nodes included
    pub knowledge_count: usize,
}
