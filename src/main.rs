//! Harness v2 - Hive Mind Orchestration System
//!
//! A multi-Claude orchestration system where agents coordinate through
//! a shared knowledge graph (AletheiaDB) using BMAD methodology.

use std::sync::Arc;

use anyhow::Result;
use harness_mcp::{HiveState, start_mcp_server};
use harness_orchestrator::{McpServerConfig, OrchestratorConfig, ProcessManager};
use harness_persistence::{AgentRole, AletheiaRepository, Repository, Session};
use harness_tui::TuiRunner;
use tracing_subscriber::EnvFilter;

/// CLI arguments for Harness.
struct Args {
    /// Number of worker agents to spawn.
    workers: usize,
    /// Whether to disable the TUI dashboard.
    no_tui: bool,
    /// Initial prompt/goal for the Strategoi.
    prompt: Option<String>,
    /// MCP server port.
    port: u16,
    /// Embedding model for semantic search (e.g., "nomic-embed-text").
    embedding_model: Option<String>,
    /// Ollama base URL (default: http://localhost:11434).
    ollama_url: Option<String>,
}

impl Args {
    fn parse() -> Self {
        let mut args = std::env::args().skip(1);
        let mut result = Self {
            workers: 3,
            no_tui: false,
            prompt: None,
            port: 3000,
            embedding_model: None,
            ollama_url: None,
        };

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--workers" | "-w" => {
                    if let Some(n) = args.next().and_then(|s| s.parse().ok()) {
                        result.workers = n;
                    }
                }
                "--no-tui" => result.no_tui = true,
                "-p" | "--prompt" => {
                    result.prompt = args.next();
                }
                "--port" => {
                    if let Some(p) = args.next().and_then(|s| s.parse().ok()) {
                        result.port = p;
                    }
                }
                "--embedding-model" | "-e" => {
                    result.embedding_model = args.next();
                }
                "--ollama-url" => {
                    result.ollama_url = args.next();
                }
                _ => {}
            }
        }

        result
    }
}

/// Get the embedding dimensions for common Ollama models.
fn ollama_model_dimensions(model: &str) -> usize {
    match model {
        "nomic-embed-text" => 768,
        "mxbai-embed-large" => 1024,
        "all-minilm" => 384,
        "snowflake-arctic-embed" => 1024,
        _ => {
            tracing::warn!(
                model = %model,
                "Unknown model, defaulting to 768 dimensions. Use --embedding-dims to override."
            );
            768
        }
    }
}

/// Create an Ollama embedding service with the specified model.
fn create_ollama_embedding_service(
    model: &str,
    base_url: Option<&str>,
) -> Result<aletheiadb::embeddings::EmbeddingService> {
    use aletheiadb::embeddings::EmbeddingService;
    use aletheiadb::embeddings::providers::ollama::{OllamaConfig, OllamaProvider};

    let dimensions = ollama_model_dimensions(model);
    let mut config = OllamaConfig::new(model.to_string(), dimensions);
    if let Some(url) = base_url {
        config = config.with_base_url(url.to_string());
    }

    let provider = Arc::new(OllamaProvider::new(config)?);
    Ok(EmbeddingService::new(provider))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    tracing::info!("Harness v2 - Hive Mind Orchestration System");

    // 1. Create repository with cold storage
    // For now, use in-memory DB (AletheiaDB persistence API needs investigation)
    // TODO: Switch to file-based persistence with proper initialization
    let db = Arc::new(aletheiadb::AletheiaDB::new()?);

    let mut repository = AletheiaRepository::new(db);

    // Add vector index if embeddings are enabled
    if let Some(ref model) = args.embedding_model {
        let dimensions = ollama_model_dimensions(model);
        tracing::info!(
            dimensions = dimensions,
            "Enabling vector index for semantic search"
        );
        repository = repository.with_vector_index(dimensions)?;
    }

    let repository = Arc::new(repository);

    tracing::info!(
        db_path = %db_path.display(),
        "AletheiaDB repository initialized with cold storage"
    );

    // 2. Create session
    let session = Session::new(args.workers + 1); // +1 for Strategoi
    repository.create_session(&session).await?;
    let session_id = session.id;

    tracing::info!(
        session_id = %session_id,
        workers = args.workers,
        "Session created"
    );

    // 3. Build session context for agent prompts
    let session_context = match &args.prompt {
        Some(prompt) => format!("Session {session_id}. Goal: {prompt}"),
        None => format!("Session {session_id}. Awaiting instructions from the human operator."),
    };

    // 4. Configure orchestrator with MCP server URL
    let mcp_url = format!("http://localhost:{}/sse", args.port);
    let config = OrchestratorConfig {
        population_cap: args.workers + 1,
        claude_path: "claude".into(),
        mcp_config: McpServerConfig::http_sse(&mcp_url),
    };

    let process_manager = Arc::new(ProcessManager::new(config, repository.clone()));

    // 5. Create HiveState with optional embedding service
    let mut hive_state = HiveState::new(session.clone(), repository.clone());

    if let Some(model) = &args.embedding_model {
        tracing::info!(model = %model, "Configuring Ollama embeddings");

        match create_ollama_embedding_service(model, args.ollama_url.as_deref()) {
            Ok(service) => {
                hive_state = hive_state.with_embedding_service(Arc::new(service));
                tracing::info!("Embedding service enabled for semantic knowledge search");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to create embedding service, continuing without embeddings");
            }
        }
    } else {
        tracing::info!("No embedding model specified, semantic search disabled");
    }

    let hive_state = Arc::new(hive_state);

    // 6. Start MCP server (background task)
    let mcp_state = hive_state.clone();
    let mcp_port = args.port;
    tokio::spawn(async move {
        tracing::info!(port = mcp_port, "Starting MCP server");
        if let Err(e) = start_mcp_server(mcp_state, "127.0.0.1", mcp_port).await {
            tracing::error!(error = %e, "MCP server failed");
        }
    });

    // Give MCP server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // 7. Spawn Strategoi
    let strategoi_id = process_manager
        .spawn_strategoi(session_id, &session_context)
        .await?;
    tracing::info!(agent_id = %strategoi_id, "Strategoi spawned");

    // 8. Spawn worker agents
    let worker_roles = default_worker_roles(args.workers);
    for role in worker_roles {
        let worker_id = process_manager
            .spawn_worker(role, session_id, &session_context)
            .await?;
        tracing::info!(agent_id = %worker_id, role = %role, "Worker spawned");
    }

    // 9. Start TUI or wait
    if args.no_tui {
        tracing::info!("Running in headless mode (no TUI). Press Ctrl+C to stop.");
        tokio::signal::ctrl_c().await?;
    } else {
        let mut tui = TuiRunner::new(repository.clone(), session_id);
        tui.run().await?;
    }

    // 10. Graceful shutdown
    tracing::info!("Shutting down...");

    Ok(())
}

/// Select worker roles based on the number of workers requested.
///
/// With 1 worker: Developer
/// With 2: Developer, Tester
/// With 3: Developer, Tester, Architect
/// With 4+: Developer, Tester, Architect, BA, PM, Developer...
fn default_worker_roles(count: usize) -> Vec<AgentRole> {
    let role_order = [
        AgentRole::Developer,
        AgentRole::Tester,
        AgentRole::Architect,
        AgentRole::BusinessAnalyst,
        AgentRole::ProductManager,
        AgentRole::Developer, // extra devs after all roles filled
    ];

    role_order.iter().copied().cycle().take(count).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roles_for_3_workers() {
        let roles = default_worker_roles(3);
        assert_eq!(roles.len(), 3);
        assert_eq!(roles[0], AgentRole::Developer);
        assert_eq!(roles[1], AgentRole::Tester);
        assert_eq!(roles[2], AgentRole::Architect);
    }

    #[test]
    fn default_roles_for_5_workers() {
        let roles = default_worker_roles(5);
        assert_eq!(roles.len(), 5);
        assert_eq!(roles[0], AgentRole::Developer);
        assert_eq!(roles[1], AgentRole::Tester);
        assert_eq!(roles[2], AgentRole::Architect);
        assert_eq!(roles[3], AgentRole::BusinessAnalyst);
        assert_eq!(roles[4], AgentRole::ProductManager);
    }

    #[test]
    fn default_roles_for_1_worker() {
        let roles = default_worker_roles(1);
        assert_eq!(roles.len(), 1);
        assert_eq!(roles[0], AgentRole::Developer);
    }
}
