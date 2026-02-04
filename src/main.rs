//! Harness - Multi-Claude Orchestration System

use std::sync::Arc;

use anyhow::Result;
use harness_mcp::ChatState;
use harness_orchestrator::OrchestratorConfig;
use harness_persistence::{InMemoryRepository, Repository, Session};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("Harness - Multi-Claude Orchestration System");

    // Create repository (will be replaced with GallifreyDB)
    let repository = Arc::new(InMemoryRepository::new());

    // Create session
    let session = Session::new(8);
    repository.create_session(&session).await?;

    // Create chat state
    let config = OrchestratorConfig::default();
    let state = ChatState::new(session, repository, config);

    // Create default channel
    state.create_channel("general", "General discussion").await?;

    tracing::info!(
        session_id = %state.session_id(),
        population_cap = state.population_cap(),
        "Session started"
    );

    // TODO: Start TUI or MCP server based on args
    println!("Harness initialized. Session: {}", state.session_id());
    println!("Population cap: {}", state.population_cap());

    let channels = state.list_channels().await?;
    for channel in channels {
        println!("Channel: {} - {}", channel.id, channel.description);
    }

    Ok(())
}

