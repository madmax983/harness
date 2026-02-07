//! Persistence layer for Harness using AletheiaDB.

mod entities;
mod gallifrey;
mod memory;
mod repository;
mod types;

pub use entities::{Agent, AgentStatus, Channel, Message, Session};
pub use gallifrey::AletheiaRepository;
pub use memory::InMemoryRepository;
pub use repository::{Repository, RepositoryError, RepositoryResult};
pub use types::{AgentId, ChannelId, MessageId, SessionId, TypeError};
