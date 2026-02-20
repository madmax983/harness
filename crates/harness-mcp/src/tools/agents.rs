//! Agent tool request/response types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request to register an agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegisterAgentRequest {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    /// For spawned agents: pre-created agent ID to activate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// Response from register_agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegisterAgentResponse {
    pub agent_id: String,
}

/// Request to list agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListAgentsRequest {
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Agent info in list response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub role: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    pub is_strategoi: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
}

/// Response from list_agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListAgentsResponse {
    pub agents: Vec<AgentInfo>,
}

/// Request to get full hive status.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetHiveStatusRequest {
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Task summary counts.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskSummary {
    pub pending: usize,
    pub claimed: usize,
    pub in_progress: usize,
    pub completed: usize,
    pub failed: usize,
}

/// Response from get_hive_status.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetHiveStatusResponse {
    pub agents: Vec<AgentInfo>,
    pub task_summary: TaskSummary,
    pub recent_knowledge: Vec<super::knowledge::KnowledgeResult>,
    /// Message inbox for current agent (if registered).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inbox: Option<MessageInbox>,
    /// Active conversation threads in the hive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_threads: Option<Vec<ActiveThread>>,
}

/// Request to disconnect an agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DisconnectAgentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from disconnect_agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DisconnectAgentResponse {
    pub success: bool,
}

/// Request to refresh MCP session context and rebind agent identity.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RefreshSessionRequest {
    /// Optional explicit agent ID to bind to this session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from refresh_session.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RefreshSessionResponse {
    /// Current Harness session ID.
    pub session_id: String,
    /// Current MCP transport session ID, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_session_id: Option<String>,
    /// Bound agent ID after refresh, if one was restored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Whether the refresh operation rebound an agent identity.
    pub rebound: bool,
}

/// Request to spawn a new agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnAgentRequest {
    /// Role for the spawned agent (MVP: only "developer" supported).
    pub role: String,
    /// Name for the spawned teammate.
    pub name: String,
    /// CLI command to execute (e.g., "claude", "codex", "gemini").
    pub cli_command: String,
    /// CLI arguments with {PROMPT} placeholder for system prompt injection.
    pub cli_args: Vec<String>,
    /// Custom instructions to include in the generated system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_prompt: Option<String>,
    /// Strategoi directive appended to the standard agent template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
    /// Polling interval in seconds for auto-polling get_messages and get_hive_status.
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    /// Optional task to assign immediately after spawn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_task_id: Option<String>,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

fn default_poll_interval() -> u64 {
    30
}

fn default_handshake_mode() -> String {
    "ring".to_string()
}

fn default_stale_after_secs() -> u64 {
    300
}

fn default_recent_knowledge_limit() -> usize {
    200
}

fn default_artifact_max_chars() -> usize {
    1200
}

/// Request to spawn a team of agents and seed handshake DMs between them.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnTeamAndHandshakeRequest {
    /// Role for spawned agents (MVP: only "developer" supported).
    pub role: String,
    /// Number of agents to spawn.
    pub agent_count: usize,
    /// CLI command to execute (e.g., "claude", "codex", "gemini").
    pub cli_command: String,
    /// CLI arguments with {PROMPT} placeholder for system prompt injection.
    pub cli_args: Vec<String>,
    /// Custom instructions to include in each generated system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_prompt: Option<String>,
    /// Strategoi directive appended to each agent template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
    /// Polling interval in seconds for auto-polling get_messages and get_hive_status.
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    /// Handshake topology: "ring" or "full_mesh".
    #[serde(default = "default_handshake_mode")]
    pub handshake_mode: String,
    /// Optional custom message body used for seeded handshake DMs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handshake_message: Option<String>,
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from spawn_team_and_handshake.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnTeamAndHandshakeResponse {
    /// Spawned Harness agent IDs.
    pub agent_ids: Vec<String>,
    /// IDs of direct messages created to establish handshake links.
    pub message_ids: Vec<String>,
    /// Handshake topology used.
    pub handshake_mode: String,
}

/// Spawn summary for one template team member.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemplateSpawnedMember {
    pub agent_id: String,
    pub name: String,
    pub role: String,
    pub cli_command: String,
}

/// Request to spawn a team from a named template.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnTeamFromTemplateRequest {
    /// Template name: feature, bugfix, incident.
    pub template: String,
    /// Optional path to a TOML template file (supports custom template names).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_path: Option<String>,
    /// Optional global CLI override for all members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_command: Option<String>,
    /// Optional global CLI args override for all members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_args: Option<Vec<String>>,
    /// Optional directive appended to each member's default directive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
    /// Polling interval in seconds for auto-polling get_messages and get_hive_status.
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    /// Handshake topology to seed after spawning.
    #[serde(default = "default_handshake_mode")]
    pub handshake_mode: String,
    /// Optional custom handshake DM body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handshake_message: Option<String>,
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from spawn_team_from_template.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnTeamFromTemplateResponse {
    pub template: String,
    pub members: Vec<TemplateSpawnedMember>,
    pub message_ids: Vec<String>,
    pub handshake_mode: String,
}

/// Health issue found during supervision.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentSupervisionIssue {
    pub agent_id: String,
    pub role: String,
    pub status: String,
    pub is_running: bool,
    pub issue: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_taken: Option<String>,
}

/// Request to supervise the current team and optionally auto-restart unhealthy agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SuperviseTeamRequest {
    /// Consider an agent stale if no heartbeat is observed for this many seconds.
    #[serde(default = "default_stale_after_secs")]
    pub stale_after_secs: u64,
    /// Number of recent knowledge entries to scan for heartbeat/activity.
    #[serde(default = "default_recent_knowledge_limit")]
    pub recent_knowledge_limit: usize,
    /// Restart non-running agents when spawn metadata is available.
    #[serde(default)]
    pub auto_restart: bool,
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from supervise_team.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SuperviseTeamResponse {
    pub inspected_at: String,
    pub total_agents: usize,
    pub healthy_agents: usize,
    pub restarted_agents: Vec<String>,
    pub issues: Vec<AgentSupervisionIssue>,
    pub escalations: Vec<String>,
}

/// Request to return the strict shared runbook protocol for team coordination.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TeamRunbookPromptRequest {
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from team_runbook_prompt.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TeamRunbookPromptResponse {
    pub protocol_version: String,
    pub runbook: String,
    pub sections: Vec<String>,
}

fn default_schedule_cadence_minutes() -> u64 {
    1440
}

fn default_schedule_auto_dispatch() -> bool {
    true
}

fn default_schedule_task_title_template() -> String {
    "Scheduled coding-agent run [{name}]".to_string()
}

fn default_schedule_task_priority() -> String {
    "medium".to_string()
}

fn default_schedule_max_schedules() -> usize {
    10
}

/// Request to register recurring coding-agent work (coverage, review, refactor).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleCodingAgentsRequest {
    pub name: String,
    #[serde(default = "default_schedule_cadence_minutes")]
    pub cadence_minutes: u64,
    /// Reusable task prompt template. Supports placeholders: {name}, {run_at}.
    pub prompt_template: String,
    /// Optional task title template. Supports placeholders: {name}, {run_at}.
    #[serde(default = "default_schedule_task_title_template")]
    pub task_title_template: String,
    /// Priority for generated tasks: low, medium, high, critical.
    #[serde(default = "default_schedule_task_priority")]
    pub task_priority: String,
    #[serde(default = "default_schedule_auto_dispatch")]
    pub auto_dispatch: bool,
    /// Optional RFC3339 timestamp for first run; defaults to now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from schedule_coding_agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleCodingAgentsResponse {
    pub schedule_id: String,
    pub name: String,
    pub cadence_minutes: u64,
    pub prompt_template: String,
    pub task_title_template: String,
    pub task_priority: String,
    pub auto_dispatch: bool,
    pub next_run_at: String,
}

/// Request to list coding-agent schedules.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListCodingAgentSchedulesRequest {
    #[serde(default)]
    pub enabled_only: bool,
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Schedule metadata returned by list_coding_agent_schedules.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodingAgentScheduleInfo {
    pub schedule_id: String,
    pub name: String,
    pub cadence_minutes: u64,
    pub prompt_template: String,
    pub task_title_template: String,
    pub task_priority: String,
    pub auto_dispatch: bool,
    pub enabled: bool,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<String>,
    pub next_run_at: String,
}

/// Response from list_coding_agent_schedules.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListCodingAgentSchedulesResponse {
    pub schedules: Vec<CodingAgentScheduleInfo>,
}

/// Request to execute due coding-agent schedules.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RunCodingAgentSchedulesRequest {
    /// Optional schedule ID to run a specific schedule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule_id: Option<String>,
    #[serde(default = "default_schedule_max_schedules")]
    pub max_schedules: usize,
    #[serde(default)]
    pub force_run: bool,
    #[serde(default)]
    pub dry_run: bool,
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Run output for one schedule execution.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodingAgentScheduleRunResult {
    pub schedule_id: String,
    pub name: String,
    pub executed: bool,
    pub created_task_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_run_id: Option<String>,
    pub dispatched_assignment_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spawned_agent_id: Option<String>,
    pub next_run_at: String,
}

/// Response from run_coding_agent_schedules.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RunCodingAgentSchedulesResponse {
    pub inspected_at: String,
    pub inspected_count: usize,
    pub executed_count: usize,
    /// Number of queued workflow runs consumed by the executor heartbeat.
    pub workflow_runs_consumed: usize,
    pub results: Vec<CodingAgentScheduleRunResult>,
}

fn default_workflow_max_concurrency() -> usize {
    1
}

fn default_workflow_failure_policy() -> String {
    "fail_fast".to_string()
}

fn default_workflow_step_kind() -> String {
    "run_tool".to_string()
}

fn default_workflow_step_max_attempts() -> u32 {
    1
}

fn default_workflow_step_timeout_secs() -> u64 {
    300
}

fn default_workflow_step_backoff_secs() -> u64 {
    30
}

/// One workflow step definition from create_workflow input.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowStepDefinitionInput {
    pub step_id: String,
    #[serde(default = "default_workflow_step_kind")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
    #[serde(default = "default_workflow_step_max_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_workflow_step_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_workflow_step_backoff_secs")]
    pub backoff_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub red_evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub green_evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_verification: Option<String>,
}

/// Workflow definition payload from create_workflow.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowDefinitionInput {
    pub steps: Vec<WorkflowStepDefinitionInput>,
    #[serde(default = "default_workflow_max_concurrency")]
    pub max_concurrency: usize,
    #[serde(default = "default_workflow_failure_policy")]
    pub failure_policy: String,
    #[serde(default)]
    pub retries: u32,
    #[serde(default)]
    pub timeout_secs: u64,
    #[serde(default)]
    pub backoff_secs: u64,
}

/// Request to create a workflow definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub definition: WorkflowDefinitionInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from create_workflow.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateWorkflowResponse {
    pub workflow_id: String,
    pub name: String,
    pub status: String,
    pub step_count: usize,
    pub max_concurrency: usize,
    pub failure_policy: String,
}

/// Request to list workflow definitions.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListWorkflowsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// One workflow row from list_workflows.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowInfo {
    pub workflow_id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub step_count: usize,
    pub max_concurrency: usize,
    pub failure_policy: String,
    pub retries: u32,
    pub timeout_secs: u64,
    pub backoff_secs: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// Response from list_workflows.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListWorkflowsResponse {
    pub workflows: Vec<WorkflowInfo>,
}

/// Request to trigger a workflow run.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TriggerWorkflowRequest {
    pub workflow_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from trigger_workflow.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TriggerWorkflowResponse {
    pub workflow_run_id: String,
    pub workflow_id: String,
    pub status: String,
}

/// Request to list workflow runs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListWorkflowRunsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// One step run in a workflow run response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StepRunInfo {
    pub step_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
    pub status: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub timeout_secs: u64,
    pub backoff_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub red_evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub green_evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_verification: Option<String>,
}

/// One workflow run row from list/get responses.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowRunInfo {
    pub workflow_run_id: String,
    pub workflow_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    pub status: String,
    pub evidence_status: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub timeout_secs: u64,
    pub backoff_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub step_runs: Vec<StepRunInfo>,
}

/// Response from list_workflow_runs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListWorkflowRunsResponse {
    pub runs: Vec<WorkflowRunInfo>,
}

/// Request to get one workflow run.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetWorkflowRunRequest {
    pub workflow_run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from get_workflow_run.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetWorkflowRunResponse {
    pub run: WorkflowRunInfo,
}

/// Request to pause a workflow definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PauseWorkflowRequest {
    pub workflow_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from pause_workflow.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PauseWorkflowResponse {
    pub workflow_id: String,
    pub status: String,
}

/// Request to resume a paused workflow definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResumeWorkflowRequest {
    pub workflow_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from resume_workflow.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResumeWorkflowResponse {
    pub workflow_id: String,
    pub status: String,
}

/// Request to retry a blocked/failed step.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RetryStepRequest {
    pub workflow_run_id: String,
    pub step_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from retry_step.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RetryStepResponse {
    pub workflow_run_id: String,
    pub step_id: String,
    pub status: String,
    pub attempt: u32,
}

/// Request to backfill workflow runs for a historical interval.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackfillWorkflowRequest {
    pub workflow_id: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from backfill_workflow.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackfillWorkflowResponse {
    pub workflow_id: String,
    pub queued_run_ids: Vec<String>,
    pub queued_count: usize,
    pub dry_run: bool,
}

fn default_observability_window_minutes() -> u64 {
    60
}

fn default_observability_stale_task_minutes() -> u64 {
    30
}

fn default_observability_noisy_agent_threshold() -> usize {
    5
}

/// Request to compute a high-level hive observability snapshot.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HiveObservabilitySnapshotRequest {
    #[serde(default = "default_observability_window_minutes")]
    pub window_minutes: u64,
    #[serde(default = "default_observability_stale_task_minutes")]
    pub stale_task_minutes: u64,
    #[serde(default = "default_observability_noisy_agent_threshold")]
    pub noisy_agent_threshold: usize,
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Throughput metrics for recent hive activity.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SnapshotThroughput {
    pub completed_tasks_last_window: usize,
    pub completion_rate_per_hour: f64,
    pub knowledge_events_last_window: usize,
}

/// One task considered stuck by observability snapshot heuristics.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SnapshotStuckTask {
    pub task_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_to: Option<String>,
    pub minutes_since_activity: u64,
}

/// One noisy agent entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SnapshotNoisyAgent {
    pub agent_id: String,
    pub event_count: usize,
}

/// One failed-command signal extracted from recent process output.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SnapshotFailedCommand {
    pub agent_id: String,
    pub signal: String,
}

/// Coordination latency metrics derived from task-thread message exchange.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SnapshotCoordinationLatency {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_secs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p95_secs: Option<f64>,
    pub sample_count: usize,
}

/// Response from hive_observability_snapshot.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HiveObservabilitySnapshotResponse {
    pub inspected_at: String,
    pub throughput: SnapshotThroughput,
    pub stuck_tasks: Vec<SnapshotStuckTask>,
    pub noisy_agents: Vec<SnapshotNoisyAgent>,
    pub failed_commands: Vec<SnapshotFailedCommand>,
    pub coordination_latency: SnapshotCoordinationLatency,
}

/// Request to list running processes.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListProcessesRequest {
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Process information.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProcessInfo {
    pub agent_id: String,
    pub is_running: bool,
    pub status: String,
}

/// Response from list_processes.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListProcessesResponse {
    pub processes: Vec<ProcessInfo>,
    pub total_count: usize,
}

/// Request to kill a process.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KillProcessRequest {
    pub agent_id: String,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from kill_process.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KillProcessResponse {
    pub success: bool,
}

/// Request to cleanup stale agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CleanupStaleAgentsRequest {
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from cleanup_stale_agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CleanupStaleAgentsResponse {
    pub cleaned_count: usize,
    pub agent_ids: Vec<String>,
}

/// Request to get process output.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetProcessOutputRequest {
    pub agent_id: String,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from get_process_output.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetProcessOutputResponse {
    pub agent_id: String,
    pub stdout: String,
    pub stderr: String,
    pub is_running: bool,
}

/// Request to collect structured knowledge artifacts from an agent's stdout/stderr output.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CollectAgentArtifactsRequest {
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default = "default_artifact_max_chars")]
    pub max_chars: usize,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Structured knowledge artifact extracted from one output stream.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentArtifact {
    pub stream: String,
    pub kind: String,
    pub knowledge_id: String,
    pub summary: String,
}

/// Response from collect_agent_artifacts.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CollectAgentArtifactsResponse {
    pub agent_id: String,
    pub is_running: bool,
    pub created_count: usize,
    pub artifacts: Vec<AgentArtifact>,
}

/// Request to command an agent with a new prompt.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandAgentRequest {
    pub agent_id: String,
    /// Raw prompt to send to the agent (legacy path).
    /// Optional when using `directive`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Strategoi directive to wrap with the command prompt template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
    pub cli_command: String,
    pub cli_args: Vec<String>,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from command_agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandAgentResponse {
    pub success: bool,
    pub agent_id: String,
}

/// Response from spawn_agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnAgentResponse {
    /// Harness agent ID.
    pub agent_id: String,
    /// Process ID of spawned CLI agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
    /// CLI command that was executed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_command: Option<String>,
    /// Claude Code teammate ID (from Task tool) - DEPRECATED.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teammate_id: Option<String>,
}

// === Enhanced Message Visibility ===

/// Direct message info for inbox display.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DirectMessageInfo {
    /// Message ID.
    pub id: String,
    /// Sending agent ID.
    pub from_agent: String,
    /// Sending agent's role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_agent_role: Option<String>,
    /// Sending agent's project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_agent_project: Option<String>,
    /// Receiving agent ID.
    pub to_agent: String,
    /// Message content.
    pub content: String,
    /// Task context for threading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// When sent.
    pub created_at: String,
}

/// Message inbox showing recent DMs for current agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageInbox {
    /// Recent messages (last N).
    pub recent_messages: Vec<DirectMessageInfo>,
    /// Total message count shown.
    pub count: usize,
}

/// Active conversation thread grouped by task or participants.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActiveThread {
    /// Task context (None for general DM threads).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Task title if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
    /// Agent IDs participating in thread.
    pub participants: Vec<String>,
    /// Total messages in thread.
    pub message_count: usize,
    /// Most recent message timestamp.
    pub last_message_at: String,
    /// Preview of last message (first 100 chars).
    pub last_message_preview: String,
}
