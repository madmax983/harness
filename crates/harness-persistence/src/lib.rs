//! Persistence layer for Harness v2 - Hive Mind.

mod aletheia;
mod entities;
mod memory;
mod repository;
mod types;

pub use aletheia::AletheiaRepository;
pub use async_trait::async_trait;
pub use entities::{
    Agent, AgentStatus, DirectMessage, Knowledge, Plan, Product, Project, Session, Task,
};
pub use memory::InMemoryRepository;
pub use repository::{Repository, RepositoryError, RepositoryResult};
pub use types::{
    AgentId, AgentRole, DirectMessageId, KnowledgeId, KnowledgeKind, PlanId, PlanStatus, Priority,
    ProductId, ProductStatus, ProjectId, ProjectStatus, SessionId, TaskId, TaskStatus,
};
