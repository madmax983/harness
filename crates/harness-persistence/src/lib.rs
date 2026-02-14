//! Persistence layer for Harness v2 - Hive Mind.

mod aletheia;
mod entities;
mod memory;
pub mod reasoning_bank;
mod repository;
pub mod trajectory;
mod types;

pub use aletheia::AletheiaRepository;
pub use async_trait::async_trait;
pub use entities::{
    Agent, AgentStatus, DirectMessage, Knowledge, Plan, Product, Project, Session, Task,
};
pub use memory::InMemoryRepository;
pub use reasoning_bank::{PatternQuery, PatternStore, ReasoningBank, SimilarPattern, TaskPattern};
pub use repository::{Repository, RepositoryError, RepositoryResult};
pub use trajectory::{
    LearningTrigger, RawEvent, TrajectoryEvent, TrajectoryEventId, TrajectoryQuery,
    TrajectoryRecorder, TrajectoryStep, TriggerKind,
};
pub use types::{
    AgentId, AgentRole, DirectMessageId, KnowledgeId, KnowledgeKind, PatternId, PlanId, PlanStatus,
    Priority, ProductId, ProductStatus, ProjectId, ProjectStatus, SessionId, TaskId, TaskStatus,
};
