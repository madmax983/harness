//! Harness v2 - Hive Mind Orchestration System
//!
//! A multi-Claude orchestration system where agents coordinate through
//! a shared knowledge graph (AletheiaDB) using BMAD methodology.

use std::sync::Arc;

use anyhow::Result;
use harness_orchestrator::{McpServerConfig, OrchestratorConfig, ProcessManager};
use harness_persistence::{AgentRole, InMemoryRepository, Repository, Session};
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
}

impl Args {
    fn parse() -> Self {
        let mut args = std::env::args().skip(1);
        let mut result = Self {
            workers: 3,
            no_tui: false,
            prompt: None,
            port: 3000,
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
                _ => {}
            }
        }

        result
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    tracing::info!("Harness v2 - Hive Mind Orchestration System");

    // 1. Create repository
    // TODO: Switch to AletheiaRepository for production
    let repository = Arc::new(InMemoryRepository::new());

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

    // 5. TODO: Start MCP server (background task)
    // The MCP server is started when harness-mcp HiveHandler is integrated.
    // For now, log that it should be started.
    tracing::info!(
        port = args.port,
        "MCP server should start on port {}",
        args.port
    );

    // 6. Spawn Strategoi
    let strategoi_id = process_manager
        .spawn_strategoi(session_id, &session_context)
        .await?;
    tracing::info!(agent_id = %strategoi_id, "Strategoi spawned");

    // 7. Spawn worker agents
    let worker_roles = default_worker_roles(args.workers);
    for role in worker_roles {
        let worker_id = process_manager
            .spawn_worker(role, session_id, &session_context)
            .await?;
        tracing::info!(agent_id = %worker_id, role = %role, "Worker spawned");
    }

    // 8. Start TUI or wait
    if args.no_tui {
        tracing::info!("Running in headless mode (no TUI). Press Ctrl+C to stop.");
        tokio::signal::ctrl_c().await?;
    } else {
        // TODO: Start TUI dashboard when harness-tui TuiRunner is integrated
        // let mut tui = harness_tui::TuiRunner::new(repository.clone(), session_id);
        // tui.run().await?;
        tracing::info!("TUI not yet integrated. Running in headless mode.");
        tokio::signal::ctrl_c().await?;
    }

    // 9. Graceful shutdown
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
