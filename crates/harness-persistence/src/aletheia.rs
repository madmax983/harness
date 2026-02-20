//! AletheiaDB repository implementation for Harness v2 - Hive Mind.
//!
//! # Graph Model
//!
//! ```text
//! (Session)─[:CONTAINS_AGENT]────►(Agent)
//! (Session)─[:CONTAINS_TASK]─────►(Task)
//! (Session)─[:CONTAINS_PRODUCT]──►(Product)
//! (Session)─[:CONTAINS_PROJECT]──►(Project)
//! (Session)─[:CONTAINS_PLAN]─────►(Plan)
//! (Agent)──[:CLAIMS]──────────────►(Task)
//! (Agent)──[:SHARED]──────────────►(Knowledge)
//! (Knowledge)─[:ABOUT]───────────►(Task)
//! (Task)──[:SUBTASK_OF]───────────►(Task)
//! (Task)──[:BLOCKS]───────────────►(Task)
//! (Agent)──[:SENT_DM]────────────►(DirectMessage)
//! (DirectMessage)─[:DM_TO]───────►(Agent)
//! (DirectMessage)─[:DM_THREAD]───►(Task)
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use aletheiadb::api::transaction::{ReadTransaction, WriteTransaction};
use aletheiadb::core::Node;
use aletheiadb::core::id::NodeId;
use aletheiadb::core::property::PropertyMapBuilder;
use aletheiadb::index::VectorIndex;
use aletheiadb::index::vector::temporal::TemporalVectorConfig;
use aletheiadb::index::vector::{DistanceMetric, HnswConfig, HnswIndex, HnswIndexBuilder};
use aletheiadb::{AletheiaDB, ReadOps, WriteOps};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::{
    Agent, AgentId, AgentRole, AgentStatus, DirectMessage, DirectMessageId, Knowledge, KnowledgeId,
    KnowledgeKind, Plan, PlanId, PlanStatus, Priority, Product, ProductId, ProductStatus, Project,
    ProjectId, ProjectStatus, Repository, RepositoryError, RepositoryResult, Session, SessionId,
    Task, TaskId, TaskStatus,
};

const LABEL_SESSION: &str = "Session";
const LABEL_AGENT: &str = "Agent";
const LABEL_TASK: &str = "Task";
const LABEL_KNOWLEDGE: &str = "Knowledge";
const LABEL_DIRECT_MESSAGE: &str = "DirectMessage";
const LABEL_PRODUCT: &str = "Product";
const LABEL_PROJECT: &str = "Project";
const LABEL_PLAN: &str = "Plan";
const LABEL_TRAJECTORY: &str = "Trajectory";

const EDGE_CONTAINS_AGENT: &str = "CONTAINS_AGENT";
const EDGE_CONTAINS_TASK: &str = "CONTAINS_TASK";
const EDGE_CONTAINS_PRODUCT: &str = "CONTAINS_PRODUCT";
const EDGE_CONTAINS_PROJECT: &str = "CONTAINS_PROJECT";
const EDGE_CONTAINS_PLAN: &str = "CONTAINS_PLAN";
const EDGE_CLAIMS: &str = "CLAIMS";
const EDGE_SHARED: &str = "SHARED";
const EDGE_ABOUT: &str = "ABOUT";
const EDGE_SUBTASK_OF: &str = "SUBTASK_OF";
const EDGE_BLOCKS: &str = "BLOCKS";
const EDGE_SENT_DM: &str = "SENT_DM";
const EDGE_DM_TO: &str = "DM_TO";
const EDGE_DM_THREAD: &str = "DM_THREAD";
const EDGE_AGENT_TRAJECTORY: &str = "AGENT_TRAJECTORY";
const EDGE_TASK_TRAJECTORY: &str = "TASK_TRAJECTORY";
const EDGE_CONTAINS_TRAJECTORY: &str = "CONTAINS_TRAJECTORY";

const ENTITY_INDEX_FILE: &str = ".harness-entity-index.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersistedEntityIndex {
    entries: Vec<PersistedEntityIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedEntityIndexEntry {
    key: String,
    node_id: u64,
}

/// AletheiaDB-backed repository for production use.
pub struct AletheiaRepository {
    db: Arc<AletheiaDB>,
    entity_index: RwLock<HashMap<String, NodeId>>,
    entity_index_loaded: AtomicBool,
    /// When true, skip file I/O for the index node (for anonymous/in-memory DBs).
    skip_file_io: bool,
    /// Optional HNSW vector index for semantic knowledge search.
    vector_index: Option<Arc<HnswIndex>>,
}

impl AletheiaRepository {
    /// Create a new AletheiaRepository wrapping an AletheiaDB instance.
    pub fn new(db: Arc<AletheiaDB>) -> Self {
        Self {
            db,
            entity_index: RwLock::new(HashMap::new()),
            entity_index_loaded: AtomicBool::new(false),
            skip_file_io: false,
            vector_index: None,
        }
    }

    /// Create a new AletheiaRepository for an anonymous (in-memory) DB.
    /// Skips sidecar entity index file I/O so tests don't interfere with each other.
    pub fn new_anon(db: Arc<AletheiaDB>) -> Self {
        Self {
            db,
            entity_index: RwLock::new(HashMap::new()),
            entity_index_loaded: AtomicBool::new(false),
            skip_file_io: true,
            vector_index: None,
        }
    }

    /// Configure with an HNSW vector index for semantic knowledge search.
    ///
    /// This enables BOTH:
    /// 1. Harness's external HNSW index (for ask_hive semantic search)
    /// 2. AletheiaDB's built-in vector index (for Nova experimental features)
    pub fn with_vector_index(mut self, dimensions: usize) -> Result<Self, RepositoryError> {
        // 1. Create external HNSW index for ask_hive()
        let index = HnswIndexBuilder::new(dimensions, DistanceMetric::Cosine)
            .m(16)
            .ef_construction(200)
            .ef_search(64)
            .build()
            .map_err(|e| RepositoryError::Database(format!("HNSW index creation failed: {e}")))?;
        self.vector_index = Some(Arc::new(index));

        // 2. Enable AletheiaDB's built-in vector index for Nova features (Dreamer, Prophet, etc.)
        self.db
            .vector_index("embedding")
            .hnsw(HnswConfig::new(dimensions, DistanceMetric::Cosine))
            .temporal(TemporalVectorConfig::default())
            .enable()
            .map_err(|e| {
                RepositoryError::Database(format!("AletheiaDB vector index enable failed: {e}"))
            })?;

        Ok(self)
    }

    /// Get a reference to the underlying AletheiaDB instance.
    /// Useful for accessing experimental features like ConceptAlgebra.
    pub fn db(&self) -> &Arc<AletheiaDB> {
        &self.db
    }

    // --- DB helpers: force E = RepositoryError to avoid type inference ambiguity ---

    fn db_read<T>(
        &self,
        f: impl FnOnce(&ReadTransaction) -> RepositoryResult<T>,
    ) -> RepositoryResult<T> {
        self.db.read(f)
    }

    fn db_write<T>(
        &self,
        f: impl FnOnce(&mut WriteTransaction) -> RepositoryResult<T>,
    ) -> RepositoryResult<T> {
        self.db.write(f)
    }

    // --- Index management ---

    fn ensure_entity_index_loaded(&self) -> RepositoryResult<()> {
        if self.entity_index_loaded.load(Ordering::Acquire) {
            return Ok(());
        }

        let loaded = self.load_entity_index_from_disk()?;
        let mut cache = self.entity_index.write();
        if !self.entity_index_loaded.load(Ordering::Relaxed) {
            *cache = loaded;
            self.entity_index_loaded.store(true, Ordering::Release);
        }

        Ok(())
    }

    fn load_entity_index_from_disk(&self) -> RepositoryResult<HashMap<String, NodeId>> {
        if self.skip_file_io {
            return Ok(HashMap::new());
        }

        use std::fs;
        use std::path::Path;

        let path = Path::new(ENTITY_INDEX_FILE);
        if !path.exists() {
            return Ok(HashMap::new());
        }

        let raw = fs::read_to_string(path).map_err(|e| {
            RepositoryError::Database(format!(
                "Failed to read sidecar entity index '{}': {e}",
                path.display()
            ))
        })?;

        if raw.trim().is_empty() {
            return Ok(HashMap::new());
        }

        let persisted: PersistedEntityIndex = serde_json::from_str(&raw).map_err(|e| {
            RepositoryError::Database(format!(
                "Failed to parse sidecar entity index '{}': {e}",
                path.display()
            ))
        })?;

        let mut map = HashMap::with_capacity(persisted.entries.len());
        for entry in persisted.entries {
            match NodeId::new(entry.node_id) {
                Ok(node_id) => {
                    map.insert(entry.key, node_id);
                }
                Err(e) => {
                    tracing::warn!(
                        key = %entry.key,
                        node_id = entry.node_id,
                        error = %e,
                        "Skipping invalid sidecar entity index entry"
                    );
                }
            }
        }

        Ok(map)
    }

    fn persist_entity_index(&self) -> RepositoryResult<()> {
        if self.skip_file_io {
            return Ok(());
        }

        use std::fs;
        use std::path::Path;

        let mut entries: Vec<PersistedEntityIndexEntry> = self
            .entity_index
            .read()
            .iter()
            .map(|(key, node_id)| PersistedEntityIndexEntry {
                key: key.clone(),
                node_id: node_id.as_u64(),
            })
            .collect();
        entries.sort_by(|a, b| a.key.cmp(&b.key));

        let payload = PersistedEntityIndex { entries };
        let encoded = serde_json::to_string(&payload).map_err(|e| {
            RepositoryError::Database(format!("Failed to serialize sidecar entity index: {e}"))
        })?;

        let path = Path::new(ENTITY_INDEX_FILE);
        fs::write(path, encoded).map_err(|e| {
            RepositoryError::Database(format!(
                "Failed to write sidecar entity index '{}': {e}",
                path.display()
            ))
        })?;

        Ok(())
    }

    fn index_set(&self, key: &str, node_id: NodeId) -> RepositoryResult<()> {
        self.ensure_entity_index_loaded()?;
        self.entity_index.write().insert(key.to_string(), node_id);
        self.persist_entity_index()
    }

    fn index_get(&self, key: &str) -> RepositoryResult<NodeId> {
        self.ensure_entity_index_loaded()?;

        if let Some(node_id) = self.entity_index.read().get(key).copied() {
            return Ok(node_id);
        }

        Err(RepositoryError::NotFound {
            entity_type: "IndexEntry".into(),
            id: key.to_string(),
        })
    }

    // --- Index key formatters ---

    fn session_key(id: SessionId) -> String {
        format!("session:{}", id.as_uuid())
    }
    fn agent_key(id: AgentId) -> String {
        format!("agent:{}", id.as_uuid())
    }
    fn task_key(id: TaskId) -> String {
        format!("task:{}", id.as_uuid())
    }
    fn knowledge_key(id: KnowledgeId) -> String {
        format!("knowledge:{}", id.as_uuid())
    }
    fn dm_key(id: DirectMessageId) -> String {
        format!("dm:{}", id.as_uuid())
    }
    fn trajectory_key(id: crate::TrajectoryEventId) -> String {
        format!("trajectory:{}", id.as_uuid())
    }

    // --- Conversion helpers ---

    fn dt_to_ts(dt: DateTime<Utc>) -> i64 {
        dt.timestamp_nanos_opt().unwrap_or(0)
    }
    fn ts_to_dt(ts: i64) -> DateTime<Utc> {
        DateTime::from_timestamp_nanos(ts)
    }

    fn parse_uuid(s: &str) -> RepositoryResult<uuid::Uuid> {
        uuid::Uuid::parse_str(s)
            .map_err(|_| RepositoryError::Database(format!("Invalid UUID: {s}")))
    }

    fn agent_status_str(s: AgentStatus) -> &'static str {
        match s {
            AgentStatus::Pending => "pending",
            AgentStatus::Starting => "starting",
            AgentStatus::Active => "active",
            AgentStatus::Idle => "idle",
            AgentStatus::Finished => "finished",
            AgentStatus::Killed => "killed",
            AgentStatus::Crashed => "crashed",
        }
    }

    fn parse_agent_status(s: &str) -> RepositoryResult<AgentStatus> {
        match s {
            "pending" => Ok(AgentStatus::Pending),
            "starting" => Ok(AgentStatus::Starting),
            "active" => Ok(AgentStatus::Active),
            "idle" => Ok(AgentStatus::Idle),
            "finished" => Ok(AgentStatus::Finished),
            "killed" => Ok(AgentStatus::Killed),
            "crashed" => Ok(AgentStatus::Crashed),
            _ => Err(RepositoryError::Database(format!(
                "Invalid agent status: {s}"
            ))),
        }
    }

    fn task_status_str(s: TaskStatus) -> &'static str {
        match s {
            TaskStatus::Pending => "pending",
            TaskStatus::Claimed => "claimed",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
        }
    }

    fn parse_task_status(s: &str) -> RepositoryResult<TaskStatus> {
        match s {
            "pending" => Ok(TaskStatus::Pending),
            "claimed" => Ok(TaskStatus::Claimed),
            "in_progress" => Ok(TaskStatus::InProgress),
            "completed" => Ok(TaskStatus::Completed),
            "failed" => Ok(TaskStatus::Failed),
            _ => Err(RepositoryError::Database(format!(
                "Invalid task status: {s}"
            ))),
        }
    }

    fn priority_str(p: Priority) -> &'static str {
        match p {
            Priority::Low => "low",
            Priority::Medium => "medium",
            Priority::High => "high",
            Priority::Critical => "critical",
        }
    }

    fn parse_priority(s: &str) -> RepositoryResult<Priority> {
        match s {
            "low" => Ok(Priority::Low),
            "medium" => Ok(Priority::Medium),
            "high" => Ok(Priority::High),
            "critical" => Ok(Priority::Critical),
            _ => Err(RepositoryError::Database(format!("Invalid priority: {s}"))),
        }
    }

    fn role_str(r: AgentRole) -> &'static str {
        match r {
            AgentRole::Strategoi => "strategoi",
            AgentRole::BusinessAnalyst => "business_analyst",
            AgentRole::ProductManager => "product_manager",
            AgentRole::Architect => "architect",
            AgentRole::Developer => "developer",
            AgentRole::Tester => "tester",
        }
    }

    fn parse_role(s: &str) -> RepositoryResult<AgentRole> {
        match s {
            "strategoi" => Ok(AgentRole::Strategoi),
            "business_analyst" => Ok(AgentRole::BusinessAnalyst),
            "product_manager" => Ok(AgentRole::ProductManager),
            "architect" => Ok(AgentRole::Architect),
            "developer" => Ok(AgentRole::Developer),
            "tester" => Ok(AgentRole::Tester),
            _ => Err(RepositoryError::Database(format!("Invalid role: {s}"))),
        }
    }

    fn kind_str(k: KnowledgeKind) -> &'static str {
        match k {
            KnowledgeKind::Activity => "activity",
            KnowledgeKind::Discovery => "discovery",
            KnowledgeKind::Decision => "decision",
            KnowledgeKind::Blocker => "blocker",
        }
    }

    fn parse_kind(s: &str) -> RepositoryResult<KnowledgeKind> {
        match s {
            "activity" => Ok(KnowledgeKind::Activity),
            "discovery" => Ok(KnowledgeKind::Discovery),
            "decision" => Ok(KnowledgeKind::Decision),
            "blocker" => Ok(KnowledgeKind::Blocker),
            _ => Err(RepositoryError::Database(format!("Invalid kind: {s}"))),
        }
    }

    fn parse_product_status(s: &str) -> RepositoryResult<ProductStatus> {
        match s {
            "concept" => Ok(ProductStatus::Concept),
            "active" => Ok(ProductStatus::Active),
            "maintenance" => Ok(ProductStatus::Maintenance),
            "archived" => Ok(ProductStatus::Archived),
            _ => Err(RepositoryError::Database(format!(
                "Invalid product status: {s}"
            ))),
        }
    }

    fn parse_project_status(s: &str) -> RepositoryResult<ProjectStatus> {
        match s {
            "planning" => Ok(ProjectStatus::Planning),
            "active" => Ok(ProjectStatus::Active),
            "on_hold" => Ok(ProjectStatus::OnHold),
            "completed" => Ok(ProjectStatus::Completed),
            "archived" => Ok(ProjectStatus::Archived),
            _ => Err(RepositoryError::Database(format!(
                "Invalid project status: {s}"
            ))),
        }
    }

    fn parse_plan_status(s: &str) -> RepositoryResult<PlanStatus> {
        match s {
            "draft" => Ok(PlanStatus::Draft),
            "approved" => Ok(PlanStatus::Approved),
            "in_execution" => Ok(PlanStatus::InExecution),
            "paused" => Ok(PlanStatus::Paused),
            "completed" => Ok(PlanStatus::Completed),
            "abandoned" => Ok(PlanStatus::Abandoned),
            _ => Err(RepositoryError::Database(format!(
                "Invalid plan status: {s}"
            ))),
        }
    }

    fn trigger_kind_str(k: crate::TriggerKind) -> &'static str {
        match k {
            crate::TriggerKind::TaskComplete => "task_complete",
            crate::TriggerKind::KnowledgeShare => "knowledge_share",
            crate::TriggerKind::ProjectClose => "project_close",
        }
    }

    fn parse_trigger_kind(s: &str) -> RepositoryResult<crate::TriggerKind> {
        match s {
            "task_complete" => Ok(crate::TriggerKind::TaskComplete),
            "knowledge_share" => Ok(crate::TriggerKind::KnowledgeShare),
            "project_close" => Ok(crate::TriggerKind::ProjectClose),
            _ => Err(RepositoryError::Database(format!(
                "Invalid trigger kind: {s}"
            ))),
        }
    }

    // --- Node property helpers ---

    fn pstr<'a>(n: &'a Node, k: &str) -> RepositoryResult<&'a str> {
        n.get_property(k)
            .and_then(|v| v.as_str())
            .ok_or_else(|| RepositoryError::Database(format!("Missing property '{k}'")))
    }

    fn pint(n: &Node, k: &str) -> RepositoryResult<i64> {
        n.get_property(k)
            .and_then(|v| v.as_int())
            .ok_or_else(|| RepositoryError::Database(format!("Missing property '{k}'")))
    }

    fn pbool(n: &Node, k: &str) -> RepositoryResult<bool> {
        n.get_property(k)
            .and_then(|v| v.as_bool())
            .ok_or_else(|| RepositoryError::Database(format!("Missing property '{k}'")))
    }

    fn opt_str<'a>(n: &'a Node, k: &str) -> Option<&'a str> {
        n.get_property(k)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
    }

    fn opt_vec(n: &Node, k: &str) -> Option<Vec<f32>> {
        n.get_property(k)
            .and_then(|v| v.as_vector())
            .map(|v| v.to_vec())
    }

    fn opt_usize(n: &Node, k: &str) -> Option<usize> {
        n.get_property(k)
            .and_then(|v| v.as_int())
            .map(|v| v as usize)
    }

    // --- Node → Entity converters ---

    fn node_to_session(n: &Node) -> RepositoryResult<Session> {
        Ok(Session {
            id: SessionId::from_uuid(Self::parse_uuid(Self::pstr(n, "id")?)?),
            population_cap: Self::pint(n, "population_cap")? as usize,
            started_at: Self::ts_to_dt(Self::pint(n, "started_at")?),
            agent_id: Self::opt_str(n, "agent_id")
                .and_then(|s| Self::parse_uuid(s).ok())
                .map(AgentId::from_uuid),
        })
    }

    fn node_to_agent(n: &Node) -> RepositoryResult<Agent> {
        Ok(Agent {
            id: AgentId::from_uuid(Self::parse_uuid(Self::pstr(n, "id")?)?),
            role: Self::parse_role(Self::pstr(n, "role")?)?,
            status: Self::parse_agent_status(Self::pstr(n, "status")?)?,
            current_task: Self::opt_str(n, "current_task")
                .and_then(|s| Self::parse_uuid(s).ok())
                .map(TaskId::from_uuid),
            is_strategoi: Self::pbool(n, "is_strategoi")?,
            session_id: SessionId::from_uuid(Self::parse_uuid(Self::pstr(n, "session_id")?)?),
            project_name: Self::opt_str(n, "project_name").map(|s| s.to_string()),
            project_path: Self::opt_str(n, "project_path").map(|s| s.to_string()),
            created_at: Self::ts_to_dt(Self::pint(n, "created_at")?),
        })
    }

    fn node_to_task(n: &Node) -> RepositoryResult<Task> {
        Ok(Task {
            id: TaskId::from_uuid(Self::parse_uuid(Self::pstr(n, "id")?)?),
            title: Self::pstr(n, "title")?.to_string(),
            description: Self::pstr(n, "description")?.to_string(),
            status: Self::parse_task_status(Self::pstr(n, "status")?)?,
            priority: Self::parse_priority(Self::pstr(n, "priority")?)?,
            assigned_to: Self::opt_str(n, "assigned_to")
                .and_then(|s| Self::parse_uuid(s).ok())
                .map(AgentId::from_uuid),
            created_by: Self::opt_str(n, "created_by")
                .and_then(|s| Self::parse_uuid(s).ok())
                .map(AgentId::from_uuid),
            parent_task: Self::opt_str(n, "parent_task")
                .and_then(|s| Self::parse_uuid(s).ok())
                .map(TaskId::from_uuid),
            created_at: Self::ts_to_dt(Self::pint(n, "created_at")?),
            completed_at: n
                .get_property("completed_at")
                .and_then(|v| v.as_int())
                .map(Self::ts_to_dt),
            summary: Self::opt_str(n, "summary").map(String::from),
            session_id: SessionId::from_uuid(Self::parse_uuid(Self::pstr(n, "session_id")?)?),
            embedding: Self::opt_vec(n, "embedding"),
        })
    }

    fn node_to_knowledge(n: &Node) -> RepositoryResult<Knowledge> {
        Ok(Knowledge {
            id: KnowledgeId::from_uuid(Self::parse_uuid(Self::pstr(n, "id")?)?),
            content: Self::pstr(n, "content")?.to_string(),
            kind: Self::parse_kind(Self::pstr(n, "kind")?)?,
            author_id: AgentId::from_uuid(Self::parse_uuid(Self::pstr(n, "author_id")?)?),
            task_id: Self::opt_str(n, "task_id")
                .and_then(|s| Self::parse_uuid(s).ok())
                .map(TaskId::from_uuid),
            embedding: Self::opt_vec(n, "embedding"),
            created_at: Self::ts_to_dt(Self::pint(n, "created_at")?),
            session_id: SessionId::from_uuid(Self::parse_uuid(Self::pstr(n, "session_id")?)?),
        })
    }

    fn node_to_dm(n: &Node) -> RepositoryResult<DirectMessage> {
        Ok(DirectMessage {
            id: DirectMessageId::from_uuid(Self::parse_uuid(Self::pstr(n, "id")?)?),
            from_agent: AgentId::from_uuid(Self::parse_uuid(Self::pstr(n, "from_agent")?)?),
            to_agent: AgentId::from_uuid(Self::parse_uuid(Self::pstr(n, "to_agent")?)?),
            content: Self::pstr(n, "content")?.to_string(),
            task_id: Self::opt_str(n, "task_id")
                .and_then(|s| Self::parse_uuid(s).ok())
                .map(TaskId::from_uuid),
            created_at: Self::ts_to_dt(Self::pint(n, "created_at")?),
            session_id: SessionId::from_uuid(Self::parse_uuid(Self::pstr(n, "session_id")?)?),
        })
    }

    fn node_to_product(n: &Node) -> RepositoryResult<Product> {
        Ok(Product {
            id: ProductId::from_uuid(Self::parse_uuid(Self::pstr(n, "id")?)?),
            name: Self::pstr(n, "name")?.to_string(),
            description: Self::pstr(n, "description")?.to_string(),
            status: Self::parse_product_status(Self::pstr(n, "status")?)?,
            session_id: SessionId::from_uuid(Self::parse_uuid(Self::pstr(n, "session_id")?)?),
            created_at: Self::ts_to_dt(Self::pint(n, "created_at")?),
            embedding: Self::opt_vec(n, "embedding"),
        })
    }

    fn node_to_project(n: &Node) -> RepositoryResult<Project> {
        Ok(Project {
            id: ProjectId::from_uuid(Self::parse_uuid(Self::pstr(n, "id")?)?),
            name: Self::pstr(n, "name")?.to_string(),
            description: Self::pstr(n, "description")?.to_string(),
            status: Self::parse_project_status(Self::pstr(n, "status")?)?,
            product_id: ProductId::from_uuid(Self::parse_uuid(Self::pstr(n, "product_id")?)?),
            session_id: SessionId::from_uuid(Self::parse_uuid(Self::pstr(n, "session_id")?)?),
            created_at: Self::ts_to_dt(Self::pint(n, "created_at")?),
            embedding: Self::opt_vec(n, "embedding"),
        })
    }

    fn node_to_plan(n: &Node) -> RepositoryResult<Plan> {
        Ok(Plan {
            id: PlanId::from_uuid(Self::parse_uuid(Self::pstr(n, "id")?)?),
            name: Self::pstr(n, "name")?.to_string(),
            strategy: Self::pstr(n, "strategy")?.to_string(),
            status: Self::parse_plan_status(Self::pstr(n, "status")?)?,
            project_id: ProjectId::from_uuid(Self::parse_uuid(Self::pstr(n, "project_id")?)?),
            session_id: SessionId::from_uuid(Self::parse_uuid(Self::pstr(n, "session_id")?)?),
            created_at: Self::ts_to_dt(Self::pint(n, "created_at")?),
            embedding: Self::opt_vec(n, "embedding"),
        })
    }

    fn node_to_trajectory(n: &Node) -> RepositoryResult<crate::RawEvent> {
        Ok(crate::RawEvent {
            id: crate::TrajectoryEventId::from_uuid(Self::parse_uuid(Self::pstr(n, "id")?)?),
            session_id: SessionId::from_uuid(Self::parse_uuid(Self::pstr(n, "session_id")?)?),
            trigger_kind: Self::parse_trigger_kind(Self::pstr(n, "trigger_kind")?)?,
            agent_id: AgentId::from_uuid(Self::parse_uuid(Self::pstr(n, "agent_id")?)?),
            task_id: Self::opt_str(n, "task_id")
                .and_then(|s| Self::parse_uuid(s).ok())
                .map(TaskId::from_uuid),
            success: Self::pbool(n, "success")?,
            summary: Self::pstr(n, "summary")?.to_string(),
            knowledge_kind: Self::opt_str(n, "knowledge_kind")
                .and_then(|s| Self::parse_kind(s).ok()),
            project_name: Self::opt_str(n, "project_name").map(String::from),
            tasks_completed: Self::opt_usize(n, "tasks_completed").unwrap_or(0),
            tasks_failed: Self::opt_usize(n, "tasks_failed").unwrap_or(0),
            created_at: Self::ts_to_dt(Self::pint(n, "created_at")?),
        })
    }

    // --- Graph traversal helpers ---

    fn collect_targets(
        &self,
        source: NodeId,
        edge_label: &str,
        convert: fn(&Node) -> RepositoryResult<Agent>,
    ) -> RepositoryResult<Vec<Agent>> {
        self.db_read(|tx| {
            let edge_ids = tx.get_outgoing_edges_with_label(source, edge_label);
            let mut result = Vec::new();
            for eid in edge_ids {
                let edge = tx.get_edge(eid)?;
                let node = tx.get_node(edge.target)?;
                if let Ok(item) = convert(&node) {
                    result.push(item);
                }
            }
            Ok(result)
        })
    }

    fn collect_targets_task(
        &self,
        source: NodeId,
        edge_label: &str,
    ) -> RepositoryResult<Vec<Task>> {
        self.db_read(|tx| {
            let edge_ids = tx.get_outgoing_edges_with_label(source, edge_label);
            let mut result = Vec::new();
            for eid in edge_ids {
                let edge = tx.get_edge(eid)?;
                let node = tx.get_node(edge.target)?;
                if let Ok(item) = Self::node_to_task(&node) {
                    result.push(item);
                }
            }
            Ok(result)
        })
    }

    fn collect_targets_product(
        &self,
        source: NodeId,
        edge_label: &str,
    ) -> RepositoryResult<Vec<Product>> {
        self.db_read(|tx| {
            let edge_ids = tx.get_outgoing_edges_with_label(source, edge_label);
            let mut result = Vec::new();
            for eid in edge_ids {
                let edge = tx.get_edge(eid)?;
                let node = tx.get_node(edge.target)?;
                if let Ok(item) = Self::node_to_product(&node) {
                    result.push(item);
                }
            }
            Ok(result)
        })
    }

    fn collect_targets_project(
        &self,
        source: NodeId,
        edge_label: &str,
    ) -> RepositoryResult<Vec<Project>> {
        self.db_read(|tx| {
            let edge_ids = tx.get_outgoing_edges_with_label(source, edge_label);
            let mut result = Vec::new();
            for eid in edge_ids {
                let edge = tx.get_edge(eid)?;
                let node = tx.get_node(edge.target)?;
                if let Ok(item) = Self::node_to_project(&node) {
                    result.push(item);
                }
            }
            Ok(result)
        })
    }

    fn collect_targets_plan(
        &self,
        source: NodeId,
        edge_label: &str,
    ) -> RepositoryResult<Vec<Plan>> {
        self.db_read(|tx| {
            let edge_ids = tx.get_outgoing_edges_with_label(source, edge_label);
            let mut result = Vec::new();
            for eid in edge_ids {
                let edge = tx.get_edge(eid)?;
                let node = tx.get_node(edge.target)?;
                if let Ok(item) = Self::node_to_plan(&node) {
                    result.push(item);
                }
            }
            Ok(result)
        })
    }

    fn collect_incoming<T>(
        &self,
        target: NodeId,
        edge_label: &str,
        convert: fn(&Node) -> RepositoryResult<T>,
    ) -> RepositoryResult<Vec<T>> {
        self.db_read(|tx| {
            let edge_ids = tx.get_incoming_edges(target);
            let mut result = Vec::new();
            for eid in edge_ids {
                let edge = tx.get_edge(eid)?;
                if !edge.has_label_str(edge_label) {
                    continue;
                }
                let node = tx.get_node(edge.source)?;
                if let Ok(item) = convert(&node) {
                    result.push(item);
                }
            }
            Ok(result)
        })
    }

    fn collect_outgoing<T>(
        &self,
        source: NodeId,
        edge_label: &str,
        convert: fn(&Node) -> RepositoryResult<T>,
    ) -> RepositoryResult<Vec<T>> {
        self.db_read(|tx| {
            let edge_ids = tx.get_outgoing_edges_with_label(source, edge_label);
            let mut result = Vec::new();
            for eid in edge_ids {
                let edge = tx.get_edge(eid)?;
                let node = tx.get_node(edge.target)?;
                if let Ok(item) = convert(&node) {
                    result.push(item);
                }
            }
            Ok(result)
        })
    }

    fn get_node(&self, node_id: NodeId) -> RepositoryResult<Node> {
        self.db_read(|tx| Ok(tx.get_node(node_id)?))
    }

    /// Query cold storage statistics and tiered storage metrics.
    ///
    /// Returns information about AletheiaDB's cold storage backend (cold.redb),
    /// including compression statistics and tiered storage access patterns.
    pub fn query_cold_storage(
        &self,
    ) -> RepositoryResult<(
        Option<ColdStorageStatsData>,
        Option<TieredStorageMetricsData>,
    )> {
        // TODO: Add public API to AletheiaDB to access historical.tiered_storage
        // The historical field is currently private, so we can't access tiered storage directly.
        // For now, return None to indicate cold storage metrics are not available.
        // Future implementation will require AletheiaDB to expose a method like:
        // pub fn tiered_storage_metrics(&self) -> Option<TieredStorageMetrics>
        Ok((None, None))
    }
}

/// Cold storage statistics data.
#[derive(Debug, Clone)]
pub struct ColdStorageStatsData {
    pub node_versions_stored: u64,
    pub edge_versions_stored: u64,
    pub compression_ratio: f64,
    pub bytes_stored_compressed: u64,
    pub bytes_stored_raw: u64,
}

/// Tiered storage metrics data.
#[derive(Debug, Clone)]
pub struct TieredStorageMetricsData {
    pub hot_hits: u64,
    pub warm_hits: u64,
    pub cold_hits: u64,
    pub misses: u64,
    pub hot_ratio: f64,
    pub warm_ratio: f64,
}

#[async_trait]
impl Repository for AletheiaRepository {
    async fn create_session(&self, session: &Session) -> RepositoryResult<()> {
        let id_str = session.id.as_uuid().to_string();
        let started_at = Self::dt_to_ts(session.started_at);

        let node_id = self.db_write(|tx| {
            let props = PropertyMapBuilder::new()
                .insert("id", id_str.as_str())
                .insert("population_cap", session.population_cap as i64)
                .insert("started_at", started_at)
                .build();
            Ok(tx.create_node(LABEL_SESSION, props)?)
        })?;

        self.index_set(&Self::session_key(session.id), node_id)
    }

    async fn get_session(&self, id: SessionId) -> RepositoryResult<Session> {
        let node_id = self.index_get(&Self::session_key(id))?;
        Self::node_to_session(&self.get_node(node_id)?)
    }

    async fn set_session_agent(
        &self,
        session_id: SessionId,
        agent_id: AgentId,
    ) -> RepositoryResult<()> {
        let session_node = self.index_get(&Self::session_key(session_id))?;
        let agent_id_str = agent_id.as_uuid().to_string();

        self.db_write(|tx| {
            Ok(tx.update_node(
                session_node,
                PropertyMapBuilder::new()
                    .insert("agent_id", agent_id_str.as_str())
                    .build(),
            )?)
        })
    }

    async fn create_agent(&self, agent: &Agent) -> RepositoryResult<()> {
        let id_str = agent.id.as_uuid().to_string();
        let sid_str = agent.session_id.as_uuid().to_string();
        let created_at = Self::dt_to_ts(agent.created_at);
        let status = Self::agent_status_str(agent.status);
        let role = Self::role_str(agent.role);
        let session_node = self.index_get(&Self::session_key(agent.session_id))?;

        let agent_node = self.db_write(|tx| {
            let mut props = PropertyMapBuilder::new()
                .insert("id", id_str.as_str())
                .insert("role", role)
                .insert("status", status)
                .insert("is_strategoi", agent.is_strategoi)
                .insert("session_id", sid_str.as_str())
                .insert("created_at", created_at);

            if let Some(tid) = agent.current_task {
                let s = tid.as_uuid().to_string();
                props = props.insert("current_task", s.as_str());
            }

            if let Some(ref name) = agent.project_name {
                props = props.insert("project_name", name.as_str());
            }

            if let Some(ref path) = agent.project_path {
                props = props.insert("project_path", path.as_str());
            }

            let an = tx.create_node(LABEL_AGENT, props.build())?;
            tx.create_edge(
                session_node,
                an,
                EDGE_CONTAINS_AGENT,
                PropertyMapBuilder::new().build(),
            )?;
            Ok(an)
        })?;

        self.index_set(&Self::agent_key(agent.id), agent_node)
    }

    async fn get_agent(&self, id: AgentId) -> RepositoryResult<Agent> {
        let node_id = self.index_get(&Self::agent_key(id))?;
        Self::node_to_agent(&self.get_node(node_id)?)
    }

    async fn update_agent_status(&self, id: AgentId, status: AgentStatus) -> RepositoryResult<()> {
        let node_id = self.index_get(&Self::agent_key(id))?;
        let s = Self::agent_status_str(status);
        self.db_write(|tx| {
            Ok(tx.update_node(
                node_id,
                PropertyMapBuilder::new().insert("status", s).build(),
            )?)
        })
    }

    async fn update_agent_task(&self, id: AgentId, task: Option<TaskId>) -> RepositoryResult<()> {
        let node_id = self.index_get(&Self::agent_key(id))?;
        self.db_write(|tx| {
            let val = task.map_or(String::new(), |t| t.as_uuid().to_string());
            Ok(tx.update_node(
                node_id,
                PropertyMapBuilder::new()
                    .insert("current_task", val.as_str())
                    .build(),
            )?)
        })
    }

    async fn list_agents(&self, session_id: SessionId) -> RepositoryResult<Vec<Agent>> {
        let sn = self.index_get(&Self::session_key(session_id))?;
        self.collect_targets(
            sn,
            EDGE_CONTAINS_AGENT,
            Self::node_to_agent as fn(&Node) -> _,
        )
    }

    async fn list_active_agents(&self, session_id: SessionId) -> RepositoryResult<Vec<Agent>> {
        Ok(self
            .list_agents(session_id)
            .await?
            .into_iter()
            .filter(|a| matches!(a.status, AgentStatus::Starting | AgentStatus::Active))
            .collect())
    }

    async fn count_active_agents(&self, session_id: SessionId) -> RepositoryResult<usize> {
        Ok(self.list_active_agents(session_id).await?.len())
    }

    async fn find_agents_by_role(
        &self,
        session_id: SessionId,
        role: AgentRole,
    ) -> RepositoryResult<Vec<Agent>> {
        Ok(self
            .list_agents(session_id)
            .await?
            .into_iter()
            .filter(|a| a.role == role)
            .collect())
    }

    async fn create_task(&self, task: &Task) -> RepositoryResult<()> {
        let id_str = task.id.as_uuid().to_string();
        let sid_str = task.session_id.as_uuid().to_string();
        let created_at = Self::dt_to_ts(task.created_at);
        let status = Self::task_status_str(task.status);
        let priority = Self::priority_str(task.priority);
        let session_node = self.index_get(&Self::session_key(task.session_id))?;

        let task_node = self.db_write(|tx| {
            let mut props = PropertyMapBuilder::new()
                .insert("id", id_str.as_str())
                .insert("title", task.title.as_str())
                .insert("description", task.description.as_str())
                .insert("status", status)
                .insert("priority", priority)
                .insert("session_id", sid_str.as_str())
                .insert("created_at", created_at);

            if let Some(v) = task.assigned_to {
                let s = v.as_uuid().to_string();
                props = props.insert("assigned_to", s.as_str());
            }
            if let Some(v) = task.created_by {
                let s = v.as_uuid().to_string();
                props = props.insert("created_by", s.as_str());
            }
            if let Some(v) = task.parent_task {
                let s = v.as_uuid().to_string();
                props = props.insert("parent_task", s.as_str());
            }

            // Insert embedding as node property for graph-based semantic features
            if let Some(ref embedding) = task.embedding {
                props = props.insert_vector("embedding", embedding);
            }

            let tn = tx.create_node(LABEL_TASK, props.build())?;
            tx.create_edge(
                session_node,
                tn,
                EDGE_CONTAINS_TASK,
                PropertyMapBuilder::new().build(),
            )?;
            Ok(tn)
        })?;

        self.index_set(&Self::task_key(task.id), task_node)?;

        // Add task to vector index if embedding exists
        if let Some(ref embedding) = task.embedding
            && let Some(ref index) = self.vector_index
        {
            index
                .add(task_node, embedding)
                .map_err(|e| RepositoryError::Database(format!("Vector index add failed: {e}")))?;
        }

        if let Some(parent_id) = task.parent_task
            && let Ok(parent_node) = self.index_get(&Self::task_key(parent_id))
        {
            self.db_write(|tx| {
                Ok(tx.create_edge(
                    task_node,
                    parent_node,
                    EDGE_SUBTASK_OF,
                    PropertyMapBuilder::new().build(),
                )?)
            })?;
        }

        Ok(())
    }

    async fn get_task(&self, id: TaskId) -> RepositoryResult<Task> {
        let node_id = self.index_get(&Self::task_key(id))?;
        Self::node_to_task(&self.get_node(node_id)?)
    }

    async fn update_task_status(
        &self,
        id: TaskId,
        status: TaskStatus,
        summary: Option<&str>,
    ) -> RepositoryResult<()> {
        let node_id = self.index_get(&Self::task_key(id))?;
        let s = Self::task_status_str(status);
        self.db_write(|tx| {
            let mut props = PropertyMapBuilder::new().insert("status", s);
            if let Some(sum) = summary {
                props = props.insert("summary", sum);
            }
            if matches!(status, TaskStatus::Completed | TaskStatus::Failed) {
                props = props.insert("completed_at", Self::dt_to_ts(Utc::now()));
            }
            Ok(tx.update_node(node_id, props.build())?)
        })
    }

    async fn claim_task(&self, task_id: TaskId, agent_id: AgentId) -> RepositoryResult<()> {
        let task_node = self.index_get(&Self::task_key(task_id))?;
        let agent_node = self.index_get(&Self::agent_key(agent_id))?;
        let aid_str = agent_id.as_uuid().to_string();

        // Check for blocking dependencies before allowing claim
        let blocking_tasks = self.get_blocking_tasks(task_id).await?;
        let uncompleted: Vec<_> = blocking_tasks
            .iter()
            .filter(|t| !matches!(t.status, TaskStatus::Completed))
            .collect();

        if !uncompleted.is_empty() {
            let blocker_ids: Vec<String> = uncompleted
                .iter()
                .map(|t| t.id.as_uuid().to_string())
                .collect();
            return Err(RepositoryError::Conflict(format!(
                "task {task_id} is blocked by uncompleted tasks: {}",
                blocker_ids.join(", ")
            )));
        }

        self.db_write(|tx| {
            let node = tx.get_node(task_node)?;
            let status = Self::parse_task_status(Self::pstr(&node, "status")?)?;

            if status != TaskStatus::Pending {
                return Err(RepositoryError::Conflict(format!(
                    "task {task_id} is {status:?}, not Pending"
                )));
            }

            if let Some(assigned_str) = Self::opt_str(&node, "assigned_to") {
                let assigned = AgentId::from_uuid(Self::parse_uuid(assigned_str)?);
                if assigned != agent_id {
                    return Err(RepositoryError::Conflict(format!(
                        "task {task_id} assigned to {assigned}, not {agent_id}"
                    )));
                }
            }

            let props = PropertyMapBuilder::new()
                .insert("status", "claimed")
                .insert("assigned_to", aid_str.as_str())
                .build();
            tx.update_node(task_node, props)?;
            tx.create_edge(
                agent_node,
                task_node,
                EDGE_CLAIMS,
                PropertyMapBuilder::new().build(),
            )?;
            Ok(())
        })
    }

    async fn assign_task(&self, task_id: TaskId, agent_id: AgentId) -> RepositoryResult<()> {
        let task_node = self.index_get(&Self::task_key(task_id))?;
        let aid_str = agent_id.as_uuid().to_string();
        self.db_write(|tx| {
            Ok(tx.update_node(
                task_node,
                PropertyMapBuilder::new()
                    .insert("assigned_to", aid_str.as_str())
                    .build(),
            )?)
        })
    }

    async fn clear_task_assignment(&self, task_id: TaskId) -> RepositoryResult<()> {
        let task_node = self.index_get(&Self::task_key(task_id))?;
        self.db_write(|tx| {
            Ok(tx.update_node(
                task_node,
                PropertyMapBuilder::new().insert("assigned_to", "").build(),
            )?)
        })
    }

    async fn list_tasks(
        &self,
        session_id: SessionId,
        status: Option<TaskStatus>,
    ) -> RepositoryResult<Vec<Task>> {
        let sn = self.index_get(&Self::session_key(session_id))?;
        let all = self.collect_targets_task(sn, EDGE_CONTAINS_TASK)?;
        Ok(match status {
            Some(s) => all.into_iter().filter(|t| t.status == s).collect(),
            None => all,
        })
    }

    async fn get_subtasks(&self, parent_id: TaskId) -> RepositoryResult<Vec<Task>> {
        let parent_node = self.index_get(&Self::task_key(parent_id))?;
        self.collect_incoming(parent_node, EDGE_SUBTASK_OF, Self::node_to_task)
    }

    async fn add_task_dependency(
        &self,
        task_id: TaskId,
        blocked_task_id: TaskId,
    ) -> RepositoryResult<()> {
        let task_node = self.index_get(&Self::task_key(task_id))?;
        let blocked_node = self.index_get(&Self::task_key(blocked_task_id))?;

        self.db_write(|tx| {
            tx.create_edge(
                task_node,
                blocked_node,
                EDGE_BLOCKS,
                PropertyMapBuilder::new().build(),
            )?;
            Ok(())
        })
    }

    async fn remove_task_dependency(
        &self,
        task_id: TaskId,
        blocked_task_id: TaskId,
    ) -> RepositoryResult<()> {
        let task_node = self.index_get(&Self::task_key(task_id))?;
        let blocked_node = self.index_get(&Self::task_key(blocked_task_id))?;

        self.db_write(|tx| {
            let edges = tx.get_outgoing_edges_with_label(task_node, EDGE_BLOCKS);
            for eid in edges {
                let edge = tx.get_edge(eid)?;
                if edge.target == blocked_node {
                    tx.delete_edge(eid)?;
                }
            }
            Ok(())
        })
    }

    async fn get_blocking_tasks(&self, task_id: TaskId) -> RepositoryResult<Vec<Task>> {
        let task_node = self.index_get(&Self::task_key(task_id))?;
        self.collect_incoming(task_node, EDGE_BLOCKS, Self::node_to_task)
    }

    async fn get_blocked_tasks(&self, task_id: TaskId) -> RepositoryResult<Vec<Task>> {
        let task_node = self.index_get(&Self::task_key(task_id))?;
        self.collect_outgoing(task_node, EDGE_BLOCKS, Self::node_to_task)
    }

    async fn get_task_history(&self, task_id: TaskId) -> RepositoryResult<Vec<Task>> {
        let task_node = self.index_get(&Self::task_key(task_id))?;

        // Get the complete version history from AletheiaDB
        let history = self.db.get_node_history(task_node)?;

        // Convert each version to a Task by extracting properties directly
        let mut task_versions = Vec::with_capacity(history.versions.len());

        for version_info in &history.versions {
            // Helper to extract string property
            let get_str = |key: &str| -> Result<&str, RepositoryError> {
                version_info
                    .properties
                    .get(key)
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RepositoryError::Database(format!("Missing property: {key}")))
            };

            // Helper to extract optional string property
            let get_opt_str = |key: &str| -> Option<&str> {
                version_info.properties.get(key).and_then(|v| v.as_str())
            };

            // Helper to extract i64 property
            let get_i64 = |key: &str| -> Result<i64, RepositoryError> {
                version_info
                    .properties
                    .get(key)
                    .and_then(|v| v.as_int())
                    .ok_or_else(|| RepositoryError::Database(format!("Missing property: {key}")))
            };

            let task = Task {
                id: TaskId::from_uuid(Self::parse_uuid(get_str("id")?)?),
                title: get_str("title")?.to_string(),
                description: get_str("description")?.to_string(),
                status: Self::parse_task_status(get_str("status")?)?,
                priority: Self::parse_priority(get_str("priority")?)?,
                assigned_to: get_opt_str("assigned_to")
                    .and_then(|s| Self::parse_uuid(s).ok())
                    .map(AgentId::from_uuid),
                created_by: get_opt_str("created_by")
                    .and_then(|s| Self::parse_uuid(s).ok())
                    .map(AgentId::from_uuid),
                parent_task: get_opt_str("parent_task")
                    .and_then(|s| Self::parse_uuid(s).ok())
                    .map(TaskId::from_uuid),
                created_at: Self::ts_to_dt(get_i64("created_at")?),
                completed_at: version_info
                    .properties
                    .get("completed_at")
                    .and_then(|v| v.as_int())
                    .map(Self::ts_to_dt),
                summary: get_opt_str("summary").map(String::from),
                session_id: SessionId::from_uuid(Self::parse_uuid(get_str("session_id")?)?),
                embedding: None, // Task embeddings not stored in version history
            };

            task_versions.push(task);
        }

        Ok(task_versions)
    }

    async fn search_tasks(
        &self,
        query_embedding: &[f32],
        limit: usize,
        status: Option<TaskStatus>,
    ) -> RepositoryResult<Vec<(Task, f32)>> {
        // Use vector index if configured.
        if let Some(ref index) = self.vector_index {
            let results = index
                .search(query_embedding, limit * 2) // Get more candidates for filtering
                .map_err(|e| RepositoryError::Database(format!("Vector search failed: {e}")))?;

            let mut task_results = Vec::new();
            for (node_id, similarity) in results {
                match self.get_node(node_id).and_then(|n| {
                    // Only convert if it's a Task node (not Knowledge or other entities)
                    if n.has_label_str(LABEL_TASK) {
                        Self::node_to_task(&n)
                    } else {
                        Err(RepositoryError::Database("Not a task node".into()))
                    }
                }) {
                    Ok(task) => {
                        // Apply status filter if provided
                        if let Some(filter_status) = status {
                            if task.status == filter_status {
                                task_results.push((task, similarity));
                            }
                        } else {
                            task_results.push((task, similarity));
                        }

                        // Stop if we've collected enough results
                        if task_results.len() >= limit {
                            break;
                        }
                    }
                    Err(_) => {
                        // Skip non-task nodes silently
                    }
                }
            }
            return Ok(task_results);
        }

        // No vector index configured - return empty.
        Ok(Vec::new())
    }

    async fn get_root_tasks(&self, session_id: SessionId) -> RepositoryResult<Vec<Task>> {
        // Get all tasks in the session
        let all_tasks = self.list_tasks(session_id, None).await?;

        // Filter to tasks with no parent
        let root_tasks: Vec<Task> = all_tasks
            .into_iter()
            .filter(|task| task.parent_task.is_none())
            .collect();

        Ok(root_tasks)
    }

    async fn create_knowledge(&self, knowledge: &Knowledge) -> RepositoryResult<()> {
        let id_str = knowledge.id.as_uuid().to_string();
        let aid_str = knowledge.author_id.as_uuid().to_string();
        let sid_str = knowledge.session_id.as_uuid().to_string();
        let created_at = Self::dt_to_ts(knowledge.created_at);
        let kind = Self::kind_str(knowledge.kind);
        let agent_node = self.index_get(&Self::agent_key(knowledge.author_id))?;

        let kn = self.db_write(|tx| {
            let mut props = PropertyMapBuilder::new()
                .insert("id", id_str.as_str())
                .insert("content", knowledge.content.as_str())
                .insert("kind", kind)
                .insert("author_id", aid_str.as_str())
                .insert("session_id", sid_str.as_str())
                .insert("created_at", created_at);

            if let Some(tid) = knowledge.task_id {
                let s = tid.as_uuid().to_string();
                props = props.insert("task_id", s.as_str());
            }

            // Insert embedding as node property for graph-based semantic features
            if let Some(ref embedding) = knowledge.embedding {
                props = props.insert_vector("embedding", embedding);
            }

            let kn = tx.create_node(LABEL_KNOWLEDGE, props.build())?;
            tx.create_edge(
                agent_node,
                kn,
                EDGE_SHARED,
                PropertyMapBuilder::new().build(),
            )?;
            Ok(kn)
        })?;

        self.index_set(&Self::knowledge_key(knowledge.id), kn)?;

        if let Some(tid) = knowledge.task_id
            && let Ok(tn) = self.index_get(&Self::task_key(tid))
        {
            self.db_write(|tx| {
                Ok(tx.create_edge(kn, tn, EDGE_ABOUT, PropertyMapBuilder::new().build())?)
            })?;
        }

        // Add embedding to vector index if both are present.
        if let Some(ref embedding) = knowledge.embedding
            && let Some(ref index) = self.vector_index
        {
            index
                .add(kn, embedding)
                .map_err(|e| RepositoryError::Database(format!("Vector index add failed: {e}")))?;
        }

        Ok(())
    }

    async fn get_knowledge(&self, id: KnowledgeId) -> RepositoryResult<Knowledge> {
        let node_id = self.index_get(&Self::knowledge_key(id))?;
        Self::node_to_knowledge(&self.get_node(node_id)?)
    }

    async fn get_task_knowledge(&self, task_id: TaskId) -> RepositoryResult<Vec<Knowledge>> {
        let tn = self.index_get(&Self::task_key(task_id))?;
        let mut k = self.collect_incoming(tn, EDGE_ABOUT, Self::node_to_knowledge)?;
        k.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(k)
    }

    async fn get_recent_knowledge(
        &self,
        session_id: SessionId,
        limit: usize,
    ) -> RepositoryResult<Vec<Knowledge>> {
        let sn = self.index_get(&Self::session_key(session_id))?;

        let mut knowledge = self.db_read(|tx| {
            let agent_edges = tx.get_outgoing_edges_with_label(sn, EDGE_CONTAINS_AGENT);
            let mut result = Vec::new();
            for ae_id in agent_edges {
                let ae = tx.get_edge(ae_id)?;
                let shared_edges = tx.get_outgoing_edges_with_label(ae.target, EDGE_SHARED);
                for se_id in shared_edges {
                    let se = tx.get_edge(se_id)?;
                    let node = tx.get_node(se.target)?;
                    if let Ok(k) = Self::node_to_knowledge(&node) {
                        result.push(k);
                    }
                }
            }
            Ok(result)
        })?;

        knowledge.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        knowledge.truncate(limit);
        Ok(knowledge)
    }

    async fn search_knowledge(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> RepositoryResult<Vec<(Knowledge, f32)>> {
        // Use vector index if configured.
        if let Some(ref index) = self.vector_index {
            let results = index
                .search(query_embedding, limit)
                .map_err(|e| RepositoryError::Database(format!("Vector search failed: {e}")))?;

            let mut knowledge_results = Vec::new();
            for (node_id, similarity) in results {
                match self
                    .get_node(node_id)
                    .and_then(|n| Self::node_to_knowledge(&n))
                {
                    Ok(knowledge) => knowledge_results.push((knowledge, similarity)),
                    Err(e) => {
                        tracing::warn!("Failed to convert node {node_id:?} to knowledge: {e}");
                    }
                }
            }
            return Ok(knowledge_results);
        }

        // No vector index configured - return empty.
        Ok(Vec::new())
    }

    async fn create_direct_message(&self, message: &DirectMessage) -> RepositoryResult<()> {
        let id_str = message.id.as_uuid().to_string();
        let from_str = message.from_agent.as_uuid().to_string();
        let to_str = message.to_agent.as_uuid().to_string();
        let sid_str = message.session_id.as_uuid().to_string();
        let created_at = Self::dt_to_ts(message.created_at);
        let from_node = self.index_get(&Self::agent_key(message.from_agent))?;
        let to_node = self.index_get(&Self::agent_key(message.to_agent))?;

        let dm_node = self.db_write(|tx| {
            let mut props = PropertyMapBuilder::new()
                .insert("id", id_str.as_str())
                .insert("from_agent", from_str.as_str())
                .insert("to_agent", to_str.as_str())
                .insert("content", message.content.as_str())
                .insert("session_id", sid_str.as_str())
                .insert("created_at", created_at);

            if let Some(tid) = message.task_id {
                let s = tid.as_uuid().to_string();
                props = props.insert("task_id", s.as_str());
            }

            let dn = tx.create_node(LABEL_DIRECT_MESSAGE, props.build())?;
            tx.create_edge(
                from_node,
                dn,
                EDGE_SENT_DM,
                PropertyMapBuilder::new().build(),
            )?;
            tx.create_edge(dn, to_node, EDGE_DM_TO, PropertyMapBuilder::new().build())?;
            Ok(dn)
        })?;

        self.index_set(&Self::dm_key(message.id), dm_node)?;

        if let Some(tid) = message.task_id
            && let Ok(tn) = self.index_get(&Self::task_key(tid))
        {
            self.db_write(|tx| {
                Ok(tx.create_edge(
                    dm_node,
                    tn,
                    EDGE_DM_THREAD,
                    PropertyMapBuilder::new().build(),
                )?)
            })?;
        }

        Ok(())
    }

    async fn get_direct_messages(
        &self,
        agent_id: AgentId,
        limit: usize,
    ) -> RepositoryResult<Vec<DirectMessage>> {
        let an = self.index_get(&Self::agent_key(agent_id))?;
        let mut msgs = self.collect_incoming(an, EDGE_DM_TO, Self::node_to_dm)?;
        msgs.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        if msgs.len() > limit {
            msgs = msgs.split_off(msgs.len() - limit);
        }
        Ok(msgs)
    }

    async fn get_thread_messages(
        &self,
        task_id: TaskId,
        limit: usize,
    ) -> RepositoryResult<Vec<DirectMessage>> {
        let tn = self.index_get(&Self::task_key(task_id))?;
        let mut msgs = self.collect_incoming(tn, EDGE_DM_THREAD, Self::node_to_dm)?;
        msgs.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        msgs.truncate(limit);
        Ok(msgs)
    }

    // === Product operations ===

    async fn create_product(&self, product: &Product) -> RepositoryResult<()> {
        let id_str = product.id.as_uuid().to_string();
        let status_str = format!("{:?}", product.status).to_lowercase();
        let session_id_str = product.session_id.as_uuid().to_string();
        let created_at = Self::dt_to_ts(product.created_at);
        let session_node = self.index_get(&Self::session_key(product.session_id))?;

        let node_id = self.db_write(|tx| {
            let mut props = PropertyMapBuilder::new()
                .insert("id", id_str.as_str())
                .insert("name", product.name.as_str())
                .insert("description", product.description.as_str())
                .insert("status", status_str.as_str())
                .insert("session_id", session_id_str.as_str())
                .insert("created_at", created_at);

            // Insert embedding as node property for graph-based semantic features
            if let Some(ref embedding) = product.embedding {
                props = props.insert_vector("embedding", embedding);
            }

            let pn = tx.create_node(LABEL_PRODUCT, props.build())?;
            tx.create_edge(
                session_node,
                pn,
                EDGE_CONTAINS_PRODUCT,
                PropertyMapBuilder::new().build(),
            )?;
            Ok(pn)
        })?;

        self.index_set(&format!("Product:{}", product.id.as_uuid()), node_id)
    }

    async fn get_product(&self, id: ProductId) -> RepositoryResult<Product> {
        let key = format!("Product:{}", id.as_uuid());
        let node_id = self.index_get(&key)?;
        Self::node_to_product(&self.get_node(node_id)?)
    }

    async fn list_products(
        &self,
        session_id: SessionId,
        status: Option<ProductStatus>,
    ) -> RepositoryResult<Vec<Product>> {
        let sn = self.index_get(&Self::session_key(session_id))?;
        let all = self.collect_targets_product(sn, EDGE_CONTAINS_PRODUCT)?;
        Ok(match status {
            Some(s) => all.into_iter().filter(|p| p.status == s).collect(),
            None => all,
        })
    }

    // === Project operations ===

    async fn create_project(&self, project: &Project) -> RepositoryResult<()> {
        let id_str = project.id.as_uuid().to_string();
        let status_str = format!("{:?}", project.status).to_lowercase();
        let product_id_str = project.product_id.as_uuid().to_string();
        let session_id_str = project.session_id.as_uuid().to_string();
        let created_at = Self::dt_to_ts(project.created_at);
        let session_node = self.index_get(&Self::session_key(project.session_id))?;

        let node_id = self.db_write(|tx| {
            let mut props = PropertyMapBuilder::new()
                .insert("id", id_str.as_str())
                .insert("name", project.name.as_str())
                .insert("description", project.description.as_str())
                .insert("status", status_str.as_str())
                .insert("product_id", product_id_str.as_str())
                .insert("session_id", session_id_str.as_str())
                .insert("created_at", created_at);

            // Insert embedding as node property for graph-based semantic features
            if let Some(ref embedding) = project.embedding {
                props = props.insert_vector("embedding", embedding);
            }

            let pn = tx.create_node(LABEL_PROJECT, props.build())?;
            tx.create_edge(
                session_node,
                pn,
                EDGE_CONTAINS_PROJECT,
                PropertyMapBuilder::new().build(),
            )?;
            Ok(pn)
        })?;

        self.index_set(&format!("Project:{}", project.id.as_uuid()), node_id)
    }

    async fn get_project(&self, id: ProjectId) -> RepositoryResult<Project> {
        let key = format!("Project:{}", id.as_uuid());
        let node_id = self.index_get(&key)?;
        Self::node_to_project(&self.get_node(node_id)?)
    }

    async fn list_projects(
        &self,
        session_id: SessionId,
        product_id: Option<ProductId>,
        status: Option<ProjectStatus>,
    ) -> RepositoryResult<Vec<Project>> {
        let sn = self.index_get(&Self::session_key(session_id))?;
        let mut all = self.collect_targets_project(sn, EDGE_CONTAINS_PROJECT)?;

        if let Some(pid) = product_id {
            all.retain(|p| p.product_id == pid);
        }
        if let Some(s) = status {
            all.retain(|p| p.status == s);
        }

        Ok(all)
    }

    // === Plan operations ===

    async fn create_plan(&self, plan: &Plan) -> RepositoryResult<()> {
        let id_str = plan.id.as_uuid().to_string();
        let status_str = format!("{:?}", plan.status).to_lowercase();
        let project_id_str = plan.project_id.as_uuid().to_string();
        let session_id_str = plan.session_id.as_uuid().to_string();
        let created_at = Self::dt_to_ts(plan.created_at);
        let session_node = self.index_get(&Self::session_key(plan.session_id))?;

        let node_id = self.db_write(|tx| {
            let mut props = PropertyMapBuilder::new()
                .insert("id", id_str.as_str())
                .insert("name", plan.name.as_str())
                .insert("strategy", plan.strategy.as_str())
                .insert("status", status_str.as_str())
                .insert("project_id", project_id_str.as_str())
                .insert("session_id", session_id_str.as_str())
                .insert("created_at", created_at);

            // Insert embedding as node property for graph-based semantic features
            if let Some(ref embedding) = plan.embedding {
                props = props.insert_vector("embedding", embedding);
            }

            let pn = tx.create_node(LABEL_PLAN, props.build())?;
            tx.create_edge(
                session_node,
                pn,
                EDGE_CONTAINS_PLAN,
                PropertyMapBuilder::new().build(),
            )?;
            Ok(pn)
        })?;

        self.index_set(&format!("Plan:{}", plan.id.as_uuid()), node_id)
    }

    async fn get_plan(&self, id: PlanId) -> RepositoryResult<Plan> {
        let key = format!("Plan:{}", id.as_uuid());
        let node_id = self.index_get(&key)?;
        Self::node_to_plan(&self.get_node(node_id)?)
    }

    async fn list_plans(
        &self,
        session_id: SessionId,
        project_id: Option<ProjectId>,
        status: Option<PlanStatus>,
    ) -> RepositoryResult<Vec<Plan>> {
        let sn = self.index_get(&Self::session_key(session_id))?;
        let mut all = self.collect_targets_plan(sn, EDGE_CONTAINS_PLAN)?;

        if let Some(pid) = project_id {
            all.retain(|p| p.project_id == pid);
        }
        if let Some(s) = status {
            all.retain(|p| p.status == s);
        }

        Ok(all)
    }

    // === Experimental features access ===

    fn get_raw_db(&self) -> RepositoryResult<&aletheiadb::AletheiaDB> {
        Ok(&self.db)
    }

    fn get_node_id_for_task(
        &self,
        task_id: TaskId,
    ) -> RepositoryResult<aletheiadb::core::id::NodeId> {
        self.index_get(&Self::task_key(task_id))
    }

    fn get_node_id_for_knowledge(
        &self,
        knowledge_id: crate::KnowledgeId,
    ) -> RepositoryResult<aletheiadb::core::id::NodeId> {
        self.index_get(&Self::knowledge_key(knowledge_id))
    }

    fn get_node_id_for_agent(
        &self,
        agent_id: AgentId,
    ) -> RepositoryResult<aletheiadb::core::id::NodeId> {
        self.index_get(&Self::agent_key(agent_id))
    }

    // === Trajectory operations ===

    async fn create_trajectory_event(&self, event: &crate::RawEvent) -> RepositoryResult<()> {
        let id_str = event.id.as_uuid().to_string();
        let agent_id_str = event.agent_id.as_uuid().to_string();
        let session_id_str = event.session_id.as_uuid().to_string();
        let created_at = Self::dt_to_ts(event.created_at);
        let trigger_kind = Self::trigger_kind_str(event.trigger_kind);

        // Get node references
        let agent_node = self.index_get(&Self::agent_key(event.agent_id))?;
        let session_node = self.index_get(&Self::session_key(event.session_id))?;

        // Create the trajectory node
        let trajectory_node = self.db_write(|tx| {
            let mut props = PropertyMapBuilder::new()
                .insert("id", id_str.as_str())
                .insert("session_id", session_id_str.as_str())
                .insert("trigger_kind", trigger_kind)
                .insert("agent_id", agent_id_str.as_str())
                .insert("success", event.success)
                .insert("summary", event.summary.as_str())
                .insert("created_at", created_at);

            // Optional fields
            if let Some(task_id) = event.task_id {
                let s = task_id.as_uuid().to_string();
                props = props.insert("task_id", s.as_str());
            }
            if let Some(knowledge_kind) = event.knowledge_kind {
                props = props.insert("knowledge_kind", Self::kind_str(knowledge_kind));
            }
            if let Some(ref project_name) = event.project_name {
                props = props.insert("project_name", project_name.as_str());
            }
            if event.tasks_completed > 0 {
                props = props.insert("tasks_completed", event.tasks_completed as i64);
            }
            if event.tasks_failed > 0 {
                props = props.insert("tasks_failed", event.tasks_failed as i64);
            }

            let tn = tx.create_node(LABEL_TRAJECTORY, props.build())?;

            // Create edges
            tx.create_edge(
                agent_node,
                tn,
                EDGE_AGENT_TRAJECTORY,
                PropertyMapBuilder::new().build(),
            )?;

            tx.create_edge(
                session_node,
                tn,
                EDGE_CONTAINS_TRAJECTORY,
                PropertyMapBuilder::new().build(),
            )?;

            Ok(tn)
        })?;

        // Index the trajectory event
        self.index_set(&Self::trajectory_key(event.id), trajectory_node)?;

        // Create edge to task if task_id exists
        if let Some(task_id) = event.task_id
            && let Ok(task_node) = self.index_get(&Self::task_key(task_id))
        {
            self.db_write(|tx| {
                Ok(tx.create_edge(
                    trajectory_node,
                    task_node,
                    EDGE_TASK_TRAJECTORY,
                    PropertyMapBuilder::new().build(),
                )?)
            })?;
        }

        Ok(())
    }

    async fn get_trajectory_events(
        &self,
        session_id: SessionId,
    ) -> RepositoryResult<Vec<crate::RawEvent>> {
        let session_node = self.index_get(&Self::session_key(session_id))?;
        self.collect_outgoing(
            session_node,
            EDGE_CONTAINS_TRAJECTORY,
            Self::node_to_trajectory,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KnowledgeKind, Priority, Repository};

    fn setup() -> (AletheiaRepository, Session) {
        let db = Arc::new(AletheiaDB::new().expect("Failed to create in-memory DB"));
        let repo = AletheiaRepository::new_anon(db);
        let session = Session::new(8);
        (repo, session)
    }

    async fn setup_with_session() -> (AletheiaRepository, Session) {
        let (repo, session) = setup();
        repo.create_session(&session).await.unwrap();
        (repo, session)
    }

    async fn setup_with_agent() -> (AletheiaRepository, Session, Agent) {
        let (repo, session) = setup_with_session().await;
        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();
        (repo, session, agent)
    }

    #[test]
    fn index_set_and_get_roundtrip_uses_sidecar_cache() {
        let db = Arc::new(AletheiaDB::new().expect("db"));
        let repo = AletheiaRepository::new_anon(db);
        let target = repo
            .db_write(|tx| {
                Ok(tx.create_node(
                    LABEL_SESSION,
                    PropertyMapBuilder::new().insert("id", "target").build(),
                )?)
            })
            .expect("target node");

        repo.index_set("session:test", target)
            .expect("index write should succeed");
        let resolved = repo
            .index_get("session:test")
            .expect("index get should resolve");
        assert_eq!(resolved, target);
    }

    #[test]
    fn index_get_missing_key_returns_not_found() {
        let db = Arc::new(AletheiaDB::new().expect("db"));
        let repo = AletheiaRepository::new_anon(db);

        let err = repo
            .index_get("session:missing")
            .expect_err("missing index key should fail");
        assert!(matches!(err, RepositoryError::NotFound { .. }));
    }

    // === Session tests ===

    #[tokio::test]
    async fn create_and_get_session() {
        let (repo, session) = setup_with_session().await;

        let fetched = repo.get_session(session.id).await.unwrap();
        assert_eq!(fetched.id, session.id);
        assert_eq!(fetched.population_cap, 8);
    }

    #[tokio::test]
    async fn get_session_nonexistent_returns_error() {
        let (repo, _session) = setup();
        let fake_id = SessionId::new();
        let result = repo.get_session(fake_id).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RepositoryError::NotFound { .. }
        ));
    }

    // === Agent tests ===

    #[tokio::test]
    async fn create_and_get_agent() {
        let (repo, session, agent) = setup_with_agent().await;

        let fetched = repo.get_agent(agent.id).await.unwrap();
        assert_eq!(fetched.id, agent.id);
        assert_eq!(fetched.role, AgentRole::Developer);
        assert_eq!(fetched.status, AgentStatus::Active); // Auto-activated on creation
        assert!(!fetched.is_strategoi);
        assert!(fetched.current_task.is_none());
        assert_eq!(fetched.session_id, session.id);
    }

    #[tokio::test]
    async fn update_agent_status() {
        let (repo, _session, agent) = setup_with_agent().await;

        repo.update_agent_status(agent.id, AgentStatus::Active)
            .await
            .unwrap();

        let fetched = repo.get_agent(agent.id).await.unwrap();
        assert_eq!(fetched.status, AgentStatus::Active);
    }

    #[tokio::test]
    async fn update_agent_task() {
        let (repo, session, agent) = setup_with_agent().await;

        let task = Task::new("Test task", "Details", Priority::Medium, session.id);
        repo.create_task(&task).await.unwrap();

        repo.update_agent_task(agent.id, Some(task.id))
            .await
            .unwrap();
        let fetched = repo.get_agent(agent.id).await.unwrap();
        assert_eq!(fetched.current_task, Some(task.id));

        repo.update_agent_task(agent.id, None).await.unwrap();
        let fetched = repo.get_agent(agent.id).await.unwrap();
        assert!(fetched.current_task.is_none());
    }

    #[tokio::test]
    async fn list_agents_multiple() {
        let (repo, session) = setup_with_session().await;

        let a1 = Agent::new(AgentRole::Developer, session.id);
        let a2 = Agent::new(AgentRole::Tester, session.id);
        let a3 = Agent::new(AgentRole::Architect, session.id);
        repo.create_agent(&a1).await.unwrap();
        repo.create_agent(&a2).await.unwrap();
        repo.create_agent(&a3).await.unwrap();

        let agents = repo.list_agents(session.id).await.unwrap();
        assert_eq!(agents.len(), 3);
    }

    #[tokio::test]
    async fn list_active_agents_filters_correctly() {
        let (repo, session) = setup_with_session().await;

        let a1 = Agent::new(AgentRole::Developer, session.id);
        let a2 = Agent::new(AgentRole::Tester, session.id);
        let a3 = Agent::new(AgentRole::Architect, session.id);
        repo.create_agent(&a1).await.unwrap();
        repo.create_agent(&a2).await.unwrap();
        repo.create_agent(&a3).await.unwrap();

        // All auto-activated on creation
        assert_eq!(repo.list_active_agents(session.id).await.unwrap().len(), 3);

        // Kill one agent to test filtering
        repo.update_agent_status(a3.id, AgentStatus::Killed)
            .await
            .unwrap();

        let active = repo.list_active_agents(session.id).await.unwrap();
        assert_eq!(active.len(), 2);
    }

    #[tokio::test]
    async fn count_active_agents() {
        let (repo, session) = setup_with_session().await;

        let a1 = Agent::new(AgentRole::Developer, session.id);
        let a2 = Agent::new(AgentRole::Tester, session.id);
        repo.create_agent(&a1).await.unwrap();
        repo.create_agent(&a2).await.unwrap();

        // Both agents auto-activate on creation
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 2);

        // Kill one agent
        repo.update_agent_status(a1.id, AgentStatus::Killed)
            .await
            .unwrap();
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 1);

        // Kill the other agent
        repo.update_agent_status(a2.id, AgentStatus::Killed)
            .await
            .unwrap();
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn find_agents_by_role() {
        let (repo, session) = setup_with_session().await;

        repo.create_agent(&Agent::new(AgentRole::Developer, session.id))
            .await
            .unwrap();
        repo.create_agent(&Agent::new(AgentRole::Developer, session.id))
            .await
            .unwrap();
        repo.create_agent(&Agent::new(AgentRole::Tester, session.id))
            .await
            .unwrap();

        let devs = repo
            .find_agents_by_role(session.id, AgentRole::Developer)
            .await
            .unwrap();
        assert_eq!(devs.len(), 2);

        let testers = repo
            .find_agents_by_role(session.id, AgentRole::Tester)
            .await
            .unwrap();
        assert_eq!(testers.len(), 1);

        let architects = repo
            .find_agents_by_role(session.id, AgentRole::Architect)
            .await
            .unwrap();
        assert!(architects.is_empty());
    }

    #[tokio::test]
    async fn strategoi_agent_flag() {
        let (repo, session) = setup_with_session().await;

        let strategoi = Agent::new(AgentRole::Strategoi, session.id);
        repo.create_agent(&strategoi).await.unwrap();

        let fetched = repo.get_agent(strategoi.id).await.unwrap();
        assert!(fetched.is_strategoi);
        assert_eq!(fetched.role, AgentRole::Strategoi);
    }

    // === Task tests ===

    #[tokio::test]
    async fn create_and_get_task() {
        let (repo, session) = setup_with_session().await;

        let task = Task::new(
            "Design API",
            "Design the REST API endpoints",
            Priority::High,
            session.id,
        );
        repo.create_task(&task).await.unwrap();

        let fetched = repo.get_task(task.id).await.unwrap();
        assert_eq!(fetched.id, task.id);
        assert_eq!(fetched.title, "Design API");
        assert_eq!(fetched.description, "Design the REST API endpoints");
        assert_eq!(fetched.status, TaskStatus::Pending);
        assert_eq!(fetched.priority, Priority::High);
        assert_eq!(fetched.session_id, session.id);
        assert!(fetched.assigned_to.is_none());
        assert!(fetched.completed_at.is_none());
        assert!(fetched.summary.is_none());
    }

    #[tokio::test]
    async fn create_task_with_parent() {
        let (repo, session) = setup_with_session().await;

        let parent = Task::new("Epic", "Big task", Priority::High, session.id);
        repo.create_task(&parent).await.unwrap();

        let child =
            Task::new("Subtask", "Small task", Priority::Medium, session.id).with_parent(parent.id);
        repo.create_task(&child).await.unwrap();

        let fetched = repo.get_task(child.id).await.unwrap();
        assert_eq!(fetched.parent_task, Some(parent.id));
    }

    #[tokio::test]
    async fn create_task_with_created_by() {
        let (repo, session, agent) = setup_with_agent().await;

        let task = Task::new("Dev task", "Details", Priority::Medium, session.id)
            .with_created_by(agent.id);
        repo.create_task(&task).await.unwrap();

        let fetched = repo.get_task(task.id).await.unwrap();
        assert_eq!(fetched.created_by, Some(agent.id));
    }

    #[tokio::test]
    async fn update_task_status_without_summary() {
        let (repo, session) = setup_with_session().await;

        let task = Task::new("Task", "Details", Priority::Medium, session.id);
        repo.create_task(&task).await.unwrap();

        repo.update_task_status(task.id, TaskStatus::InProgress, None)
            .await
            .unwrap();

        let fetched = repo.get_task(task.id).await.unwrap();
        assert_eq!(fetched.status, TaskStatus::InProgress);
        assert!(fetched.summary.is_none());
        assert!(fetched.completed_at.is_none());
    }

    #[tokio::test]
    async fn update_task_status_completed_with_summary() {
        let (repo, session) = setup_with_session().await;

        let task = Task::new("Task", "Details", Priority::Medium, session.id);
        repo.create_task(&task).await.unwrap();

        repo.update_task_status(
            task.id,
            TaskStatus::Completed,
            Some("All done with 5 endpoints"),
        )
        .await
        .unwrap();

        let fetched = repo.get_task(task.id).await.unwrap();
        assert_eq!(fetched.status, TaskStatus::Completed);
        assert_eq!(
            fetched.summary.as_deref(),
            Some("All done with 5 endpoints")
        );
        assert!(fetched.completed_at.is_some());
    }

    #[tokio::test]
    async fn update_task_status_failed_sets_completed_at() {
        let (repo, session) = setup_with_session().await;

        let task = Task::new("Task", "Details", Priority::Medium, session.id);
        repo.create_task(&task).await.unwrap();

        repo.update_task_status(task.id, TaskStatus::Failed, Some("Build error"))
            .await
            .unwrap();

        let fetched = repo.get_task(task.id).await.unwrap();
        assert_eq!(fetched.status, TaskStatus::Failed);
        assert!(fetched.completed_at.is_some());
    }

    #[tokio::test]
    async fn claim_task_success() {
        let (repo, session, agent) = setup_with_agent().await;

        let task = Task::new("Claimable", "Details", Priority::High, session.id);
        repo.create_task(&task).await.unwrap();

        repo.claim_task(task.id, agent.id).await.unwrap();

        let fetched = repo.get_task(task.id).await.unwrap();
        assert_eq!(fetched.status, TaskStatus::Claimed);
        assert_eq!(fetched.assigned_to, Some(agent.id));
    }

    #[tokio::test]
    async fn claim_task_conflict_already_claimed() {
        let (repo, session) = setup_with_session().await;

        let agent_a = Agent::new(AgentRole::Developer, session.id);
        let agent_b = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent_a).await.unwrap();
        repo.create_agent(&agent_b).await.unwrap();

        let task = Task::new(
            "Contested",
            "Only one can claim",
            Priority::High,
            session.id,
        );
        repo.create_task(&task).await.unwrap();

        // First claim succeeds
        repo.claim_task(task.id, agent_a.id).await.unwrap();

        // Second claim fails (task no longer Pending)
        let result = repo.claim_task(task.id, agent_b.id).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RepositoryError::Conflict(_)));
    }

    #[tokio::test]
    async fn claim_task_pre_assigned_wrong_agent_fails() {
        let (repo, session) = setup_with_session().await;

        let agent_a = Agent::new(AgentRole::Developer, session.id);
        let agent_b = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent_a).await.unwrap();
        repo.create_agent(&agent_b).await.unwrap();

        let task = Task::new("Pre-assigned", "Details", Priority::Medium, session.id);
        repo.create_task(&task).await.unwrap();

        // Assign to agent_a
        repo.assign_task(task.id, agent_a.id).await.unwrap();

        // agent_b tries to claim - fails
        let result = repo.claim_task(task.id, agent_b.id).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RepositoryError::Conflict(_)));

        // agent_a claims - succeeds
        repo.claim_task(task.id, agent_a.id).await.unwrap();
        let fetched = repo.get_task(task.id).await.unwrap();
        assert_eq!(fetched.status, TaskStatus::Claimed);
    }

    #[tokio::test]
    async fn assign_task() {
        let (repo, session, agent) = setup_with_agent().await;

        let task = Task::new("Assigned work", "Details", Priority::Medium, session.id);
        repo.create_task(&task).await.unwrap();

        repo.assign_task(task.id, agent.id).await.unwrap();

        let fetched = repo.get_task(task.id).await.unwrap();
        assert_eq!(fetched.assigned_to, Some(agent.id));
    }

    #[tokio::test]
    async fn list_tasks_all() {
        let (repo, session) = setup_with_session().await;

        let t1 = Task::new("Task 1", "Details", Priority::Medium, session.id);
        let t2 = Task::new("Task 2", "Details", Priority::Low, session.id);
        let t3 = Task::new("Task 3", "Details", Priority::High, session.id);
        repo.create_task(&t1).await.unwrap();
        repo.create_task(&t2).await.unwrap();
        repo.create_task(&t3).await.unwrap();

        let all = repo.list_tasks(session.id, None).await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn list_tasks_filtered_by_status() {
        let (repo, session) = setup_with_session().await;

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        let t1 = Task::new("Pending", "Details", Priority::Medium, session.id);
        let t2 = Task::new("Also pending", "Details", Priority::Low, session.id);
        repo.create_task(&t1).await.unwrap();
        repo.create_task(&t2).await.unwrap();

        // Claim t1
        repo.claim_task(t1.id, agent.id).await.unwrap();

        let pending = repo
            .list_tasks(session.id, Some(TaskStatus::Pending))
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);

        let claimed = repo
            .list_tasks(session.id, Some(TaskStatus::Claimed))
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);
    }

    #[tokio::test]
    async fn get_subtasks() {
        let (repo, session) = setup_with_session().await;

        let parent = Task::new("Epic", "Big task", Priority::High, session.id);
        repo.create_task(&parent).await.unwrap();

        let child1 = Task::new("Sub 1", "First subtask", Priority::Medium, session.id)
            .with_parent(parent.id);
        let child2 = Task::new("Sub 2", "Second subtask", Priority::Medium, session.id)
            .with_parent(parent.id);
        repo.create_task(&child1).await.unwrap();
        repo.create_task(&child2).await.unwrap();

        let subtasks = repo.get_subtasks(parent.id).await.unwrap();
        assert_eq!(subtasks.len(), 2);

        let subtask_ids: Vec<TaskId> = subtasks.iter().map(|t| t.id).collect();
        assert!(subtask_ids.contains(&child1.id));
        assert!(subtask_ids.contains(&child2.id));
    }

    #[tokio::test]
    async fn get_subtasks_empty() {
        let (repo, session) = setup_with_session().await;

        let task = Task::new("Standalone", "No subtasks", Priority::Medium, session.id);
        repo.create_task(&task).await.unwrap();

        let subtasks = repo.get_subtasks(task.id).await.unwrap();
        assert!(subtasks.is_empty());
    }

    // === Knowledge tests ===

    #[tokio::test]
    async fn create_and_get_knowledge() {
        let (repo, session, agent) = setup_with_agent().await;

        let k = Knowledge::new(
            "Found race condition in pool.rs",
            KnowledgeKind::Discovery,
            agent.id,
            session.id,
        );
        repo.create_knowledge(&k).await.unwrap();

        let fetched = repo.get_knowledge(k.id).await.unwrap();
        assert_eq!(fetched.id, k.id);
        assert_eq!(fetched.content, "Found race condition in pool.rs");
        assert_eq!(fetched.kind, KnowledgeKind::Discovery);
        assert_eq!(fetched.author_id, agent.id);
        assert_eq!(fetched.session_id, session.id);
        assert!(fetched.task_id.is_none());
    }

    #[tokio::test]
    async fn create_knowledge_with_task() {
        let (repo, session, agent) = setup_with_agent().await;

        let task = Task::new("Implement auth", "Details", Priority::High, session.id);
        repo.create_task(&task).await.unwrap();

        let k = Knowledge::new("Using JWT", KnowledgeKind::Decision, agent.id, session.id)
            .with_task(task.id);
        repo.create_knowledge(&k).await.unwrap();

        let fetched = repo.get_knowledge(k.id).await.unwrap();
        assert_eq!(fetched.task_id, Some(task.id));
    }

    #[tokio::test]
    async fn get_task_knowledge() {
        let (repo, session, agent) = setup_with_agent().await;

        let task = Task::new("Implement auth", "Details", Priority::High, session.id);
        repo.create_task(&task).await.unwrap();

        let k1 = Knowledge::new("Using JWT", KnowledgeKind::Decision, agent.id, session.id)
            .with_task(task.id);
        let k2 = Knowledge::new(
            "bcrypt for passwords",
            KnowledgeKind::Decision,
            agent.id,
            session.id,
        )
        .with_task(task.id);
        let k3 = Knowledge::new(
            "Unrelated discovery",
            KnowledgeKind::Discovery,
            agent.id,
            session.id,
        );

        repo.create_knowledge(&k1).await.unwrap();
        repo.create_knowledge(&k2).await.unwrap();
        repo.create_knowledge(&k3).await.unwrap();

        let task_knowledge = repo.get_task_knowledge(task.id).await.unwrap();
        assert_eq!(task_knowledge.len(), 2);
    }

    #[tokio::test]
    async fn get_recent_knowledge_ordering_and_limit() {
        let (repo, session, agent) = setup_with_agent().await;

        for i in 0..5 {
            let k = Knowledge::new(
                format!("Knowledge {i}"),
                KnowledgeKind::Activity,
                agent.id,
                session.id,
            );
            repo.create_knowledge(&k).await.unwrap();
        }

        let recent = repo.get_recent_knowledge(session.id, 3).await.unwrap();
        assert_eq!(recent.len(), 3);
        // Should be ordered most recent first
        for window in recent.windows(2) {
            assert!(window[0].created_at >= window[1].created_at);
        }
    }

    #[tokio::test]
    async fn search_knowledge_returns_empty() {
        let (repo, _session, _agent) = setup_with_agent().await;

        let results = repo.search_knowledge(&[0.1, 0.2, 0.3], 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn with_vector_index_enables_semantic_search() {
        let db = Arc::new(AletheiaDB::new().unwrap());
        let repo = AletheiaRepository::new_anon(db)
            .with_vector_index(3)
            .expect("HNSW index creation");

        let session = Session::new(4);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        // Create knowledge with embeddings
        let k1 = Knowledge::new(
            "AletheiaDB is a graph database",
            KnowledgeKind::Discovery,
            agent.id,
            session.id,
        )
        .with_embedding(vec![0.1, 0.9, 0.2]);
        let k2 = Knowledge::new(
            "Rust is a systems language",
            KnowledgeKind::Discovery,
            agent.id,
            session.id,
        )
        .with_embedding(vec![0.8, 0.1, 0.3]);
        let k3 = Knowledge::new(
            "Graphs are useful data structures",
            KnowledgeKind::Discovery,
            agent.id,
            session.id,
        )
        .with_embedding(vec![0.2, 0.8, 0.1]);

        repo.create_knowledge(&k1).await.unwrap();
        repo.create_knowledge(&k2).await.unwrap();
        repo.create_knowledge(&k3).await.unwrap();

        // Search with a query similar to k1
        let query = vec![0.15, 0.85, 0.25];
        let results = repo.search_knowledge(&query, 2).await.unwrap();

        assert_eq!(results.len(), 2);
        // First result should be k1 or k3 (both have high second component)
        assert!(results[0].1 > 0.5); // similarity should be high
    }

    #[tokio::test]
    async fn search_knowledge_without_index_returns_empty() {
        let db = Arc::new(AletheiaDB::new().unwrap());
        let repo = AletheiaRepository::new_anon(db); // No vector index

        let session = Session::new(4);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        let k = Knowledge::new("Test", KnowledgeKind::Activity, agent.id, session.id)
            .with_embedding(vec![0.1, 0.2, 0.3]);
        repo.create_knowledge(&k).await.unwrap();

        // Search without index returns empty
        let results = repo.search_knowledge(&[0.1, 0.2, 0.3], 10).await.unwrap();
        assert!(results.is_empty());
    }

    // === DirectMessage tests ===

    #[tokio::test]
    async fn create_and_get_direct_messages() {
        let (repo, session) = setup_with_session().await;

        let ba = Agent::new(AgentRole::BusinessAnalyst, session.id);
        let pm = Agent::new(AgentRole::ProductManager, session.id);
        repo.create_agent(&ba).await.unwrap();
        repo.create_agent(&pm).await.unwrap();

        let dm1 = DirectMessage::new(ba.id, pm.id, "What are the requirements?", session.id);
        let dm2 = DirectMessage::new(pm.id, ba.id, "Here are the requirements...", session.id);
        repo.create_direct_message(&dm1).await.unwrap();
        repo.create_direct_message(&dm2).await.unwrap();

        // PM received 1 message (from BA)
        let pm_inbox = repo.get_direct_messages(pm.id, 10).await.unwrap();
        assert_eq!(pm_inbox.len(), 1);
        assert_eq!(pm_inbox[0].content, "What are the requirements?");

        // BA received 1 message (from PM)
        let ba_inbox = repo.get_direct_messages(ba.id, 10).await.unwrap();
        assert_eq!(ba_inbox.len(), 1);
        assert_eq!(ba_inbox[0].content, "Here are the requirements...");
    }

    #[tokio::test]
    async fn create_direct_message_with_task_thread() {
        let (repo, session) = setup_with_session().await;

        let ba = Agent::new(AgentRole::BusinessAnalyst, session.id);
        let pm = Agent::new(AgentRole::ProductManager, session.id);
        repo.create_agent(&ba).await.unwrap();
        repo.create_agent(&pm).await.unwrap();

        let task = Task::new(
            "PRD Interview",
            "Gather requirements",
            Priority::High,
            session.id,
        );
        repo.create_task(&task).await.unwrap();

        let dm =
            DirectMessage::new(ba.id, pm.id, "Question about auth?", session.id).with_task(task.id);
        repo.create_direct_message(&dm).await.unwrap();

        let pm_inbox = repo.get_direct_messages(pm.id, 10).await.unwrap();
        assert_eq!(pm_inbox.len(), 1);
        assert_eq!(pm_inbox[0].task_id, Some(task.id));
    }

    #[tokio::test]
    async fn get_thread_messages() {
        let (repo, session) = setup_with_session().await;

        let ba = Agent::new(AgentRole::BusinessAnalyst, session.id);
        let pm = Agent::new(AgentRole::ProductManager, session.id);
        repo.create_agent(&ba).await.unwrap();
        repo.create_agent(&pm).await.unwrap();

        let task = Task::new(
            "PRD Interview",
            "Gather requirements",
            Priority::High,
            session.id,
        );
        repo.create_task(&task).await.unwrap();

        let dm1 = DirectMessage::new(ba.id, pm.id, "Question 1?", session.id).with_task(task.id);
        let dm2 = DirectMessage::new(pm.id, ba.id, "Answer 1.", session.id).with_task(task.id);
        let unrelated = DirectMessage::new(ba.id, pm.id, "Off-topic", session.id);

        repo.create_direct_message(&dm1).await.unwrap();
        repo.create_direct_message(&dm2).await.unwrap();
        repo.create_direct_message(&unrelated).await.unwrap();

        let thread = repo.get_thread_messages(task.id, 10).await.unwrap();
        assert_eq!(thread.len(), 2);
    }

    #[tokio::test]
    async fn get_direct_messages_respects_limit() {
        let (repo, session) = setup_with_session().await;

        let sender = Agent::new(AgentRole::Developer, session.id);
        let receiver = Agent::new(AgentRole::Tester, session.id);
        repo.create_agent(&sender).await.unwrap();
        repo.create_agent(&receiver).await.unwrap();

        for i in 0..5 {
            let dm = DirectMessage::new(sender.id, receiver.id, format!("Message {i}"), session.id);
            repo.create_direct_message(&dm).await.unwrap();
        }

        let limited = repo.get_direct_messages(receiver.id, 2).await.unwrap();
        assert_eq!(limited.len(), 2);
    }

    // === Full lifecycle test ===

    #[tokio::test]
    async fn full_task_lifecycle() {
        let (repo, session, agent) = setup_with_agent().await;

        // Create task
        let task = Task::new("Design API", "REST API design", Priority::High, session.id);
        repo.create_task(&task).await.unwrap();

        // Pending -> Claimed
        repo.claim_task(task.id, agent.id).await.unwrap();
        let t = repo.get_task(task.id).await.unwrap();
        assert_eq!(t.status, TaskStatus::Claimed);
        assert_eq!(t.assigned_to, Some(agent.id));

        // Claimed -> InProgress
        repo.update_task_status(task.id, TaskStatus::InProgress, None)
            .await
            .unwrap();
        let t = repo.get_task(task.id).await.unwrap();
        assert_eq!(t.status, TaskStatus::InProgress);

        // InProgress -> Completed with summary
        repo.update_task_status(
            task.id,
            TaskStatus::Completed,
            Some("API designed with 5 endpoints"),
        )
        .await
        .unwrap();
        let t = repo.get_task(task.id).await.unwrap();
        assert_eq!(t.status, TaskStatus::Completed);
        assert_eq!(t.summary.as_deref(), Some("API designed with 5 endpoints"));
        assert!(t.completed_at.is_some());
    }

    #[tokio::test]
    async fn agent_lifecycle_count_tracking() {
        let (repo, session) = setup_with_session().await;

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        // Auto-activated on creation
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 1);

        // Starting -> still active
        repo.update_agent_status(agent.id, AgentStatus::Starting)
            .await
            .unwrap();
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 1);

        // Active -> still active
        repo.update_agent_status(agent.id, AgentStatus::Active)
            .await
            .unwrap();
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 1);

        // Killed -> no longer active
        repo.update_agent_status(agent.id, AgentStatus::Killed)
            .await
            .unwrap();
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn knowledge_all_kinds() {
        let (repo, session, agent) = setup_with_agent().await;

        let kinds = [
            KnowledgeKind::Activity,
            KnowledgeKind::Discovery,
            KnowledgeKind::Decision,
            KnowledgeKind::Blocker,
        ];

        for kind in kinds {
            let k = Knowledge::new(format!("Knowledge: {kind:?}"), kind, agent.id, session.id);
            repo.create_knowledge(&k).await.unwrap();
            let fetched = repo.get_knowledge(k.id).await.unwrap();
            assert_eq!(fetched.kind, kind);
        }
    }

    #[tokio::test]
    async fn all_agent_roles_roundtrip() {
        let (repo, session) = setup_with_session().await;

        let roles = [
            AgentRole::Strategoi,
            AgentRole::BusinessAnalyst,
            AgentRole::ProductManager,
            AgentRole::Architect,
            AgentRole::Developer,
            AgentRole::Tester,
        ];

        for role in roles {
            let agent = Agent::new(role, session.id);
            repo.create_agent(&agent).await.unwrap();
            let fetched = repo.get_agent(agent.id).await.unwrap();
            assert_eq!(fetched.role, role);
        }
    }

    #[tokio::test]
    async fn all_priorities_roundtrip() {
        let (repo, session) = setup_with_session().await;

        let priorities = [
            Priority::Low,
            Priority::Medium,
            Priority::High,
            Priority::Critical,
        ];

        for priority in priorities {
            let task = Task::new("Task", "Details", priority, session.id);
            repo.create_task(&task).await.unwrap();
            let fetched = repo.get_task(task.id).await.unwrap();
            assert_eq!(fetched.priority, priority);
        }
    }

    #[tokio::test]
    async fn all_task_statuses_roundtrip() {
        let (repo, session) = setup_with_session().await;

        let statuses = [
            TaskStatus::Pending,
            TaskStatus::Claimed,
            TaskStatus::InProgress,
            TaskStatus::Completed,
            TaskStatus::Failed,
        ];

        for status in statuses {
            let task = Task::new("Task", "Details", Priority::Medium, session.id);
            repo.create_task(&task).await.unwrap();
            repo.update_task_status(task.id, status, None)
                .await
                .unwrap();
            let fetched = repo.get_task(task.id).await.unwrap();
            assert_eq!(fetched.status, status);
        }
    }

    #[tokio::test]
    async fn all_agent_statuses_roundtrip() {
        let (repo, session) = setup_with_session().await;

        let statuses = [
            AgentStatus::Pending,
            AgentStatus::Starting,
            AgentStatus::Active,
            AgentStatus::Idle,
            AgentStatus::Finished,
            AgentStatus::Killed,
            AgentStatus::Crashed,
        ];

        for status in statuses {
            let agent = Agent::new(AgentRole::Developer, session.id);
            repo.create_agent(&agent).await.unwrap();
            repo.update_agent_status(agent.id, status).await.unwrap();
            let fetched = repo.get_agent(agent.id).await.unwrap();
            assert_eq!(fetched.status, status);
        }
    }
}
