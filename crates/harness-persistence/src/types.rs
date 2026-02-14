//! Core domain types for Harness v2 - Hive Mind.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(Uuid);

impl AgentId {
    /// Create a new random agent ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create an AgentId from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "agent-{}", &self.0.to_string()[..8])
    }
}

/// Unique identifier for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(Uuid);

impl SessionId {
    /// Create a new random session ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a SessionId from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(Uuid);

impl TaskId {
    /// Create a new random task ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a TaskId from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "task-{}", &self.0.to_string()[..8])
    }
}

/// Unique identifier for a knowledge entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KnowledgeId(Uuid);

impl KnowledgeId {
    /// Create a new random knowledge ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a KnowledgeId from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for KnowledgeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for KnowledgeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "know-{}", &self.0.to_string()[..8])
    }
}

/// Unique identifier for a direct message between agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DirectMessageId(Uuid);

impl DirectMessageId {
    /// Create a new random direct message ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a DirectMessageId from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for DirectMessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DirectMessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "dm-{}", &self.0.to_string()[..8])
    }
}

/// Unique identifier for a product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProductId(Uuid);

impl ProductId {
    /// Create a new random product ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a ProductId from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for ProductId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ProductId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "product-{}", &self.0.to_string()[..8])
    }
}

/// Task priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Low priority - nice to have.
    Low,
    /// Medium priority - should be done.
    Medium,
    /// High priority - needs attention.
    High,
    /// Critical priority - blocking other work.
    Critical,
}

/// Task status lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Task created, waiting for an agent to claim it.
    Pending,
    /// An agent has claimed this task.
    Claimed,
    /// Work is actively being done.
    InProgress,
    /// Task completed successfully.
    Completed,
    /// Task failed.
    Failed,
}

/// Product status lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductStatus {
    /// Product in concept phase.
    Concept,
    /// Product is active.
    Active,
    /// Product in maintenance phase.
    Maintenance,
    /// Product is archived.
    Archived,
}

/// Unique identifier for a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(Uuid);

impl ProjectId {
    /// Create a new random project ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a ProjectId from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "project-{}", &self.0.to_string()[..8])
    }
}

/// Project status lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    /// Project in planning phase.
    Planning,
    /// Project is active.
    Active,
    /// Project is on hold.
    OnHold,
    /// Project is completed.
    Completed,
    /// Project is archived.
    Archived,
}

/// Unique identifier for a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlanId(Uuid);

impl PlanId {
    /// Create a new random plan ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a PlanId from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for PlanId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PlanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "plan-{}", &self.0.to_string()[..8])
    }
}

/// Unique identifier for a task execution pattern in the ReasoningBank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PatternId(Uuid);

impl PatternId {
    /// Create a new random pattern ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a PatternId from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for PatternId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PatternId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pat-{}", &self.0.to_string()[..8])
    }
}

/// Plan status lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    /// Plan in draft phase.
    Draft,
    /// Plan has been approved.
    Approved,
    /// Plan is currently in execution.
    InExecution,
    /// Plan is paused.
    Paused,
    /// Plan is completed.
    Completed,
    /// Plan was abandoned.
    Abandoned,
}

/// What kind of knowledge this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeKind {
    /// Routine activity: "claimed task", "started compiling".
    Activity,
    /// Finding: "race condition in pool.rs".
    Discovery,
    /// Choice: "using JWT over sessions because...".
    Decision,
    /// Problem: "can't proceed, need API key".
    Blocker,
}

/// BMAD agent roles in the hive mind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    /// The coordinator - dispatches work, never implements. Human's interface.
    Strategoi,
    /// Gathers requirements through structured interviews, produces PRDs.
    BusinessAnalyst,
    /// Prioritizes features, manages roadmap, bridges BA and technical roles.
    ProductManager,
    /// Designs systems, defines technical specs from requirements.
    Architect,
    /// Implements code from designs and specs.
    Developer,
    /// Validates implementations, writes and runs tests.
    Tester,
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Strategoi => write!(f, "Strategoi"),
            Self::BusinessAnalyst => write!(f, "BA"),
            Self::ProductManager => write!(f, "PM"),
            Self::Architect => write!(f, "Architect"),
            Self::Developer => write!(f, "Developer"),
            Self::Tester => write!(f, "Tester"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_display_is_short() {
        let id = AgentId::new();
        let display = id.to_string();
        assert!(display.starts_with("agent-"));
        assert_eq!(display.len(), 14); // "agent-" + 8 chars
    }

    #[test]
    fn task_id_display_format() {
        let id = TaskId::new();
        let display = id.to_string();
        assert!(display.starts_with("task-"));
        assert_eq!(display.len(), 13); // "task-" + 8 chars
    }

    #[test]
    fn knowledge_id_display_format() {
        let id = KnowledgeId::new();
        let display = id.to_string();
        assert!(display.starts_with("know-"));
        assert_eq!(display.len(), 13); // "know-" + 8 chars
    }

    #[test]
    fn direct_message_id_display_format() {
        let id = DirectMessageId::new();
        let display = id.to_string();
        assert!(display.starts_with("dm-"));
        assert_eq!(display.len(), 11); // "dm-" + 8 chars
    }

    #[test]
    fn task_status_serde_roundtrip() {
        let statuses = vec![
            TaskStatus::Pending,
            TaskStatus::Claimed,
            TaskStatus::InProgress,
            TaskStatus::Completed,
            TaskStatus::Failed,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: TaskStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::Low < Priority::Medium);
        assert!(Priority::Medium < Priority::High);
        assert!(Priority::High < Priority::Critical);
    }

    #[test]
    fn knowledge_kind_serde() {
        let kinds = vec![
            KnowledgeKind::Activity,
            KnowledgeKind::Discovery,
            KnowledgeKind::Decision,
            KnowledgeKind::Blocker,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let deserialized: KnowledgeKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, deserialized);
        }
    }

    #[test]
    fn agent_role_serde_roundtrip() {
        let roles = vec![
            AgentRole::Strategoi,
            AgentRole::BusinessAnalyst,
            AgentRole::ProductManager,
            AgentRole::Architect,
            AgentRole::Developer,
            AgentRole::Tester,
        ];
        for role in roles {
            let json = serde_json::to_string(&role).unwrap();
            let deserialized: AgentRole = serde_json::from_str(&json).unwrap();
            assert_eq!(role, deserialized);
        }
    }

    #[test]
    fn agent_role_display() {
        assert_eq!(AgentRole::Strategoi.to_string(), "Strategoi");
        assert_eq!(AgentRole::BusinessAnalyst.to_string(), "BA");
        assert_eq!(AgentRole::ProductManager.to_string(), "PM");
        assert_eq!(AgentRole::Architect.to_string(), "Architect");
        assert_eq!(AgentRole::Developer.to_string(), "Developer");
        assert_eq!(AgentRole::Tester.to_string(), "Tester");
    }

    #[test]
    fn id_roundtrip_through_uuid() {
        let original = TaskId::new();
        let uuid = original.as_uuid();
        let restored = TaskId::from_uuid(uuid);
        assert_eq!(original, restored);
    }

    #[test]
    fn product_id_display_format() {
        let id = ProductId::new();
        let display = id.to_string();
        assert!(display.starts_with("product-"));
        assert_eq!(display.len(), 16); // "product-" + 8 chars
    }

    #[test]
    fn product_id_roundtrip() {
        let original = ProductId::new();
        let uuid = original.as_uuid();
        let restored = ProductId::from_uuid(uuid);
        assert_eq!(original, restored);
    }

    #[test]
    fn product_status_serde_roundtrip() {
        let statuses = vec![
            ProductStatus::Concept,
            ProductStatus::Active,
            ProductStatus::Maintenance,
            ProductStatus::Archived,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: ProductStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[test]
    fn project_id_display_format() {
        let id = ProjectId::new();
        let display = id.to_string();
        assert!(display.starts_with("project-"));
        assert_eq!(display.len(), 16); // "project-" + 8 chars
    }

    #[test]
    fn project_id_roundtrip() {
        let original = ProjectId::new();
        let uuid = original.as_uuid();
        let restored = ProjectId::from_uuid(uuid);
        assert_eq!(original, restored);
    }

    #[test]
    fn project_status_serde_roundtrip() {
        let statuses = vec![
            ProjectStatus::Planning,
            ProjectStatus::Active,
            ProjectStatus::OnHold,
            ProjectStatus::Completed,
            ProjectStatus::Archived,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: ProjectStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[test]
    fn plan_id_display_format() {
        let id = PlanId::new();
        let display = id.to_string();
        assert!(display.starts_with("plan-"));
        assert_eq!(display.len(), 13); // "plan-" + 8 chars
    }

    #[test]
    fn plan_id_roundtrip() {
        let original = PlanId::new();
        let uuid = original.as_uuid();
        let restored = PlanId::from_uuid(uuid);
        assert_eq!(original, restored);
    }

    #[test]
    fn plan_status_serde_roundtrip() {
        let statuses = vec![
            PlanStatus::Draft,
            PlanStatus::Approved,
            PlanStatus::InExecution,
            PlanStatus::Paused,
            PlanStatus::Completed,
            PlanStatus::Abandoned,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: PlanStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }
}
