//! AletheiaDB repository implementation for Harness v2 - Hive Mind.
//!
//! # Graph Model
//!
//! ```text
//! (Session)─[:CONTAINS_AGENT]──►(Agent)
//! (Session)─[:CONTAINS_TASK]───►(Task)
//! (Agent)──[:CLAIMS]────────────►(Task)
//! (Agent)──[:SHARED]────────────►(Knowledge)
//! (Knowledge)─[:ABOUT]─────────►(Task)
//! (Task)──[:SUBTASK_OF]─────────►(Task)
//! (Agent)──[:SENT_DM]──────────►(DirectMessage)
//! (DirectMessage)─[:DM_TO]─────►(Agent)
//! (DirectMessage)─[:DM_THREAD]─►(Task)
//! ```

use std::sync::Arc;

use aletheiadb::api::transaction::{ReadTransaction, WriteTransaction};
use aletheiadb::core::Node;
use aletheiadb::core::id::NodeId;
use aletheiadb::core::property::PropertyMapBuilder;
use aletheiadb::{AletheiaDB, ReadOps, WriteOps};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;

use crate::{
    Agent, AgentId, AgentRole, AgentStatus, DirectMessage, DirectMessageId, Knowledge, KnowledgeId,
    KnowledgeKind, Priority, Repository, RepositoryError, RepositoryResult, Session, SessionId,
    Task, TaskId, TaskStatus,
};

const LABEL_SESSION: &str = "Session";
const LABEL_AGENT: &str = "Agent";
const LABEL_TASK: &str = "Task";
const LABEL_KNOWLEDGE: &str = "Knowledge";
const LABEL_DIRECT_MESSAGE: &str = "DirectMessage";
const LABEL_INDEX: &str = "HarnessIndex";

const EDGE_CONTAINS_AGENT: &str = "CONTAINS_AGENT";
const EDGE_CONTAINS_TASK: &str = "CONTAINS_TASK";
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
}

impl AletheiaRepository {
    /// Create a new AletheiaRepository wrapping an AletheiaDB instance.
    pub fn new(db: Arc<AletheiaDB>) -> Self {
        Self {
            db,
            index_node_id: RwLock::new(None),
        }
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
        use std::fs;
        use std::path::Path;

        {
            let cache = self.index_node_id.read();
            if let Some(node_id) = *cache {
                return Ok(node_id);
            }
        }

        let index_file = Path::new(".harness-index");

        let node_id = if index_file.exists() {
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
        let pn = self.index_get(&Self::task_key(parent_id))?;
        self.collect_incoming(pn, EDGE_SUBTASK_OF, Self::node_to_task)
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
        _query_embedding: &[f32],
        limit: usize,
    ) -> RepositoryResult<Vec<(Knowledge, f32)>> {
        // TODO: HNSW vector search. For now return empty.
        let _ = limit;
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
}
