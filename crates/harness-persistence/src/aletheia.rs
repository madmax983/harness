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
//! (Agent)──[:SENT_DM]────────────►(DirectMessage)
//! (DirectMessage)─[:DM_TO]───────►(Agent)
//! (DirectMessage)─[:DM_THREAD]───►(Task)
//! ```

use std::sync::Arc;

use aletheiadb::api::transaction::{ReadTransaction, WriteTransaction};
use aletheiadb::core::Node;
use aletheiadb::core::id::NodeId;
use aletheiadb::core::property::PropertyMapBuilder;
use aletheiadb::index::VectorIndex;
use aletheiadb::index::vector::{DistanceMetric, HnswIndex, HnswIndexBuilder};
use aletheiadb::{AletheiaDB, ReadOps, WriteOps};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;

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
const LABEL_INDEX: &str = "HarnessIndex";

const EDGE_CONTAINS_AGENT: &str = "CONTAINS_AGENT";
const EDGE_CONTAINS_TASK: &str = "CONTAINS_TASK";
const EDGE_CONTAINS_PRODUCT: &str = "CONTAINS_PRODUCT";
const EDGE_CONTAINS_PROJECT: &str = "CONTAINS_PROJECT";
const EDGE_CONTAINS_PLAN: &str = "CONTAINS_PLAN";
const EDGE_CLAIMS: &str = "CLAIMS";
const EDGE_SHARED: &str = "SHARED";
const EDGE_ABOUT: &str = "ABOUT";
const EDGE_SUBTASK_OF: &str = "SUBTASK_OF";
const EDGE_SENT_DM: &str = "SENT_DM";
const EDGE_DM_TO: &str = "DM_TO";
const EDGE_DM_THREAD: &str = "DM_THREAD";

const INDEX_KEY_SELF: &str = "_index_node_id";

/// AletheiaDB-backed repository for production use.
pub struct AletheiaRepository {
    db: Arc<AletheiaDB>,
    index_node_id: RwLock<Option<NodeId>>,
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
            index_node_id: RwLock::new(None),
            skip_file_io: false,
            vector_index: None,
        }
    }

    /// Create a new AletheiaRepository for an anonymous (in-memory) DB.
    /// Skips `.harness-index` file I/O so tests don't interfere with each other.
    pub fn new_anon(db: Arc<AletheiaDB>) -> Self {
        Self {
            db,
            index_node_id: RwLock::new(None),
            skip_file_io: true,
            vector_index: None,
        }
    }

    /// Configure with an HNSW vector index for semantic knowledge search.
    pub fn with_vector_index(mut self, dimensions: usize) -> Result<Self, RepositoryError> {
        let index = HnswIndexBuilder::new(dimensions, DistanceMetric::Cosine)
            .m(16)
            .ef_construction(200)
            .ef_search(64)
            .build()
            .map_err(|e| RepositoryError::Database(format!("HNSW index creation failed: {e}")))?;
        self.vector_index = Some(Arc::new(index));
        Ok(self)
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

    fn get_or_create_index_node(&self) -> RepositoryResult<NodeId> {
        {
            let cache = self.index_node_id.read();
            if let Some(node_id) = *cache {
                return Ok(node_id);
            }
        }

        let node_id = if self.skip_file_io {
            // Anonymous DB: always create a fresh index node, no file persistence.
            self.db_write(|tx| {
                let props = PropertyMapBuilder::new()
                    .insert(INDEX_KEY_SELF, "harness-index")
                    .build();
                Ok(tx.create_node(LABEL_INDEX, props)?)
            })?
        } else {
            use std::fs;
            use std::path::Path;

            let index_file = Path::new(".harness-index");

            if index_file.exists() {
                let content = fs::read_to_string(index_file).map_err(|e| {
                    RepositoryError::Database(format!("Failed to read index file: {e}"))
                })?;
                let id_u64: u64 = content
                    .trim()
                    .parse()
                    .map_err(|e| RepositoryError::Database(format!("Invalid index file: {e}")))?;
                NodeId::new(id_u64)
                    .map_err(|e| RepositoryError::Database(format!("Invalid NodeId: {e}")))?
            } else {
                let node_id = self.db_write(|tx| {
                    let props = PropertyMapBuilder::new()
                        .insert(INDEX_KEY_SELF, "harness-index")
                        .build();
                    Ok(tx.create_node(LABEL_INDEX, props)?)
                })?;
                fs::write(index_file, node_id.as_u64().to_string()).map_err(|e| {
                    RepositoryError::Database(format!("Failed to write index file: {e}"))
                })?;
                node_id
            }
        };

        *self.index_node_id.write() = Some(node_id);
        Ok(node_id)
    }

    fn index_set(&self, key: &str, node_id: NodeId) -> RepositoryResult<()> {
        let index_id = self.get_or_create_index_node()?;
        self.db_write(|tx| {
            let props = PropertyMapBuilder::new()
                .insert(key, node_id.as_u64() as i64)
                .build();
            Ok(tx.update_node(index_id, props)?)
        })
    }

    fn index_get(&self, key: &str) -> RepositoryResult<NodeId> {
        let index_id = self.get_or_create_index_node()?;
        let node = self.db_read(|tx| Ok(tx.get_node(index_id)?))?;
        let node_id_i64 = node
            .get_property(key)
            .and_then(|v| v.as_int())
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "IndexEntry".into(),
                id: key.to_string(),
            })?;
        NodeId::new(node_id_i64 as u64)
            .map_err(|e| RepositoryError::Database(format!("Invalid NodeId: {e}")))
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
            _ => Err(RepositoryError::Database(format!("Invalid product status: {s}"))),
        }
    }

    fn parse_project_status(s: &str) -> RepositoryResult<ProjectStatus> {
        match s {
            "planning" => Ok(ProjectStatus::Planning),
            "active" => Ok(ProjectStatus::Active),
            "on_hold" => Ok(ProjectStatus::OnHold),
            "completed" => Ok(ProjectStatus::Completed),
            "archived" => Ok(ProjectStatus::Archived),
            _ => Err(RepositoryError::Database(format!("Invalid project status: {s}"))),
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
            _ => Err(RepositoryError::Database(format!("Invalid plan status: {s}"))),
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
            embedding: None, // TODO: load from vector index
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

    fn get_node(&self, node_id: NodeId) -> RepositoryResult<Node> {
        self.db_read(|tx| Ok(tx.get_node(node_id)?))
    }
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
            let props = PropertyMapBuilder::new()
                .insert("id", id_str.as_str())
                .insert("name", product.name.as_str())
                .insert("description", product.description.as_str())
                .insert("status", status_str.as_str())
                .insert("session_id", session_id_str.as_str())
                .insert("created_at", created_at)
                .build();
            let pn = tx.create_node(LABEL_PRODUCT, props)?;
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
            let props = PropertyMapBuilder::new()
                .insert("id", id_str.as_str())
                .insert("name", project.name.as_str())
                .insert("description", project.description.as_str())
                .insert("status", status_str.as_str())
                .insert("product_id", product_id_str.as_str())
                .insert("session_id", session_id_str.as_str())
                .insert("created_at", created_at)
                .build();
            let pn = tx.create_node(LABEL_PROJECT, props)?;
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
            all = all.into_iter().filter(|p| p.product_id == pid).collect();
        }
        if let Some(s) = status {
            all = all.into_iter().filter(|p| p.status == s).collect();
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
            let props = PropertyMapBuilder::new()
                .insert("id", id_str.as_str())
                .insert("name", plan.name.as_str())
                .insert("strategy", plan.strategy.as_str())
                .insert("status", status_str.as_str())
                .insert("project_id", project_id_str.as_str())
                .insert("session_id", session_id_str.as_str())
                .insert("created_at", created_at)
                .build();
            let pn = tx.create_node(LABEL_PLAN, props)?;
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
            all = all.into_iter().filter(|p| p.project_id == pid).collect();
        }
        if let Some(s) = status {
            all = all.into_iter().filter(|p| p.status == s).collect();
        }

        Ok(all)
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
        assert_eq!(fetched.status, AgentStatus::Pending);
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

        // All pending initially - none active
        assert_eq!(repo.list_active_agents(session.id).await.unwrap().len(), 0);

        // Set a1 to Starting, a2 to Active
        repo.update_agent_status(a1.id, AgentStatus::Starting)
            .await
            .unwrap();
        repo.update_agent_status(a2.id, AgentStatus::Active)
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

        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 0);

        repo.update_agent_status(a1.id, AgentStatus::Active)
            .await
            .unwrap();
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 1);

        repo.update_agent_status(a1.id, AgentStatus::Killed)
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

        // Initially pending, not counted as active
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 0);

        // Starting -> active
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
