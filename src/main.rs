//! Harness MCP daemon.
//!
//! Runs only the MCP server and persistence layer without spawning orchestrator
//! agents or launching the TUI dashboard.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aletheiadb::config::{HistoricalConfigBuilder, WalConfigBuilder};
use aletheiadb::storage::index_persistence::PersistenceConfig;
use aletheiadb::{AletheiaDB, AletheiaDBConfig};
use anyhow::{Context, Result, anyhow, bail};
use harness_mcp::{HiveState, start_mcp_server};
use harness_orchestrator::{OrchestratorConfig, ProcessManager};
use harness_persistence::{AletheiaRepository, Repository, Session, SessionId};
use harness_sona::SonaConfig;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
struct Args {
    host: String,
    port: u16,
    population_cap: usize,
    session_file: PathBuf,
    embedding_model: Option<String>,
    ollama_url: Option<String>,
    // SONA configuration
    sona_enabled: Option<bool>,
    ewc_lambda: Option<f32>,
    ewc_gamma: Option<f32>,
    lora_rank: Option<usize>,
    learning_interval_secs: Option<u64>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            population_cap: 16,
            session_file: PathBuf::from(".harness-mcp-session"),
            embedding_model: None,
            ollama_url: None,
            sona_enabled: None,
            ewc_lambda: None,
            ewc_gamma: None,
            lora_rank: None,
            learning_interval_secs: None,
        }
    }
}

impl Args {
    fn parse() -> Result<Self> {
        Self::parse_from(std::env::args().skip(1))
    }

    fn parse_from<I>(iter: I) -> Result<Self>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args = Self::default();
        let mut it = iter.into_iter();

        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--host" => {
                    args.host = it
                        .next()
                        .ok_or_else(|| anyhow!("--host requires a value"))?;
                }
                "--port" => {
                    let raw = it
                        .next()
                        .ok_or_else(|| anyhow!("--port requires a value"))?;
                    args.port = raw
                        .parse()
                        .with_context(|| format!("invalid --port value: {raw}"))?;
                }
                "--population-cap" => {
                    let raw = it
                        .next()
                        .ok_or_else(|| anyhow!("--population-cap requires a value"))?;
                    args.population_cap = raw
                        .parse()
                        .with_context(|| format!("invalid --population-cap value: {raw}"))?;
                }
                "--session-file" => {
                    let raw = it
                        .next()
                        .ok_or_else(|| anyhow!("--session-file requires a value"))?;
                    args.session_file = PathBuf::from(raw);
                }
                "--embedding-model" | "-e" => {
                    args.embedding_model = Some(
                        it.next()
                            .ok_or_else(|| anyhow!("--embedding-model requires a value"))?,
                    );
                }
                "--ollama-url" => {
                    args.ollama_url = Some(
                        it.next()
                            .ok_or_else(|| anyhow!("--ollama-url requires a value"))?,
                    );
                }
                "--enable-sona" => {
                    args.sona_enabled = Some(true);
                }
                "--disable-sona" => {
                    args.sona_enabled = Some(false);
                }
                "--ewc-lambda" => {
                    let raw = it
                        .next()
                        .ok_or_else(|| anyhow!("--ewc-lambda requires a value"))?;
                    args.ewc_lambda = Some(
                        raw.parse()
                            .with_context(|| format!("invalid --ewc-lambda value: {raw}"))?,
                    );
                }
                "--ewc-gamma" => {
                    let raw = it
                        .next()
                        .ok_or_else(|| anyhow!("--ewc-gamma requires a value"))?;
                    args.ewc_gamma = Some(
                        raw.parse()
                            .with_context(|| format!("invalid --ewc-gamma value: {raw}"))?,
                    );
                }
                "--lora-rank" => {
                    let raw = it
                        .next()
                        .ok_or_else(|| anyhow!("--lora-rank requires a value"))?;
                    args.lora_rank = Some(
                        raw.parse()
                            .with_context(|| format!("invalid --lora-rank value: {raw}"))?,
                    );
                }
                "--learning-interval" => {
                    let raw = it
                        .next()
                        .ok_or_else(|| anyhow!("--learning-interval requires a value"))?;
                    args.learning_interval_secs =
                        Some(raw.parse().with_context(|| {
                            format!("invalid --learning-interval value: {raw}")
                        })?);
                }
                "-h" | "--help" => {
                    print_usage();
                    std::process::exit(0);
                }
                _ => bail!("unknown argument: {arg}"),
            }
        }

        Ok(args)
    }
}

fn print_usage() {
    println!(
        "\
Harness MCP daemon

Usage:
  cargo run --bin harness-mcpd -- [options]

Options:
  --host <HOST>               Bind host (default: 127.0.0.1)
  --port <PORT>               MCP server port (default: 3000)
  --population-cap <NUM>      Session population cap for tool metadata (default: 16)
  --session-file <PATH>       Session ID file path (default: .harness-mcp-session)
  --embedding-model, -e <M>   Enable semantic search embeddings using Ollama model M
  --ollama-url <URL>          Ollama base URL (default provider default)

SONA (Self-Optimizing Neural Architecture):
  --enable-sona               Enable SONA adaptive learning (default: disabled)
  --disable-sona              Explicitly disable SONA
  --ewc-lambda <FLOAT>        EWC penalty strength (default: 0.4)
  --ewc-gamma <FLOAT>         EWC online decay factor (default: 0.9)
  --lora-rank <INT>           BaseLoRA rank (default: 8)
  --learning-interval <SECS>  Learning cycle interval in seconds (default: 300)

Environment variables (override CLI flags):
  HARNESS_SONA_ENABLED        1/true to enable, 0/false to disable
  HARNESS_EWC_LAMBDA          EWC lambda value
  HARNESS_EWC_GAMMA           EWC gamma value
  HARNESS_LORA_RANK           LoRA rank value
  HARNESS_LEARNING_INTERVAL   Learning interval in seconds

  -h, --help                  Show this help
"
    );
}

fn ollama_model_dimensions(model: &str) -> usize {
    match model {
        "nomic-embed-text" => 768,
        "mxbai-embed-large" => 1024,
        "all-minilm" => 384,
        "snowflake-arctic-embed" => 1024,
        _ => {
            tracing::warn!(
                model = %model,
                "Unknown model, defaulting to 768 dimensions."
            );
            768
        }
    }
}

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

fn read_session_id(path: &Path) -> Result<Option<SessionId>> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed reading session file: {}", path.display()))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let uuid = Uuid::parse_str(trimmed)
        .with_context(|| format!("invalid session UUID in {}: {trimmed}", path.display()))?;

    Ok(Some(SessionId::from_uuid(uuid)))
}

fn write_session_id(path: &Path, session_id: SessionId) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed creating directory for session file: {}",
                parent.display()
            )
        })?;
    }

    fs::write(path, session_id.as_uuid().to_string())
        .with_context(|| format!("failed writing session file: {}", path.display()))?;
    Ok(())
}

/// Build SONA configuration from CLI args and environment variables.
///
/// Priority: Environment variables > CLI flags > Defaults
fn build_sona_config(args: &Args) -> SonaConfig {
    let mut config = SonaConfig::default();

    // Apply CLI flags first (lower priority)
    if let Some(enabled) = args.sona_enabled {
        config = config.with_enabled(enabled);
    }
    if let Some(lambda) = args.ewc_lambda {
        config = config.with_ewc_lambda(lambda);
    }
    if let Some(gamma) = args.ewc_gamma {
        config = config.with_ewc_gamma(gamma);
    }
    if let Some(rank) = args.lora_rank {
        config = config.with_lora_rank(rank);
    }
    if let Some(interval) = args.learning_interval_secs {
        config = config.with_learning_interval(interval);
    }

    // Apply environment variables last (highest priority - overrides CLI)
    if let Ok(val) = std::env::var("HARNESS_SONA_ENABLED") {
        config.enabled = val == "1" || val.eq_ignore_ascii_case("true");
    }
    if let Ok(val) = std::env::var("HARNESS_EWC_LAMBDA")
        && let Ok(lambda) = val.parse::<f32>()
    {
        config = config.with_ewc_lambda(lambda);
    }
    if let Ok(val) = std::env::var("HARNESS_EWC_GAMMA")
        && let Ok(gamma) = val.parse::<f32>()
    {
        config = config.with_ewc_gamma(gamma);
    }
    if let Ok(val) = std::env::var("HARNESS_LORA_RANK")
        && let Ok(rank) = val.parse::<usize>()
    {
        config = config.with_lora_rank(rank);
    }
    if let Ok(val) = std::env::var("HARNESS_LEARNING_INTERVAL")
        && let Ok(interval) = val.parse::<u64>()
    {
        config = config.with_learning_interval(interval);
    }

    config
}

async fn ensure_session<R: Repository + 'static>(
    repository: &Arc<R>,
    session_file: &Path,
    population_cap: usize,
) -> Result<Session> {
    if let Some(session_id) = read_session_id(session_file)? {
        match repository.get_session(session_id).await {
            Ok(session) => {
                tracing::info!(
                    session_id = %session.id,
                    session_file = %session_file.display(),
                    "Loaded existing session"
                );
                return Ok(session);
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    session_id = %session_id,
                    "Session in file not found, creating a new one"
                );
            }
        }
    }

    let session = Session::new(population_cap);
    repository
        .create_session(&session)
        .await
        .context("failed creating session")?;
    write_session_id(session_file, session.id)?;

    tracing::info!(
        session_id = %session.id,
        session_file = %session_file.display(),
        "Created and persisted new session"
    );

    Ok(session)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse()?;

    let db_path = std::env::current_dir()?.join(".harness-data");
    let config = AletheiaDBConfig::builder()
        .wal(WalConfigBuilder::new().wal_dir(db_path.join("wal")).build())
        .persistence(PersistenceConfig {
            enabled: true,
            data_dir: db_path.join("indexes"),
            load_on_startup: true,
            ..Default::default()
        })
        .historical(
            HistoricalConfigBuilder::new()
                .enable_cold_storage(true)
                .cold_storage_path(db_path.join("cold.redb"))
                .migration_age_threshold(std::time::Duration::from_secs(3600))
                .max_hot_versions(1000)
                .build(),
        )
        .build();

    let db = Arc::new(AletheiaDB::with_unified_config(config)?);
    let mut repository = AletheiaRepository::new(db);

    if let Some(model) = args.embedding_model.as_deref() {
        repository = repository.with_vector_index(ollama_model_dimensions(model))?;
    }

    let repository = Arc::new(repository);
    let session = ensure_session(&repository, &args.session_file, args.population_cap).await?;

    // Initialize ProcessManager
    let cli_path = if cfg!(windows) {
        std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        "sh".to_string()
    };

    let orchestrator_config = OrchestratorConfig {
        population_cap: args.population_cap,
        agent_cli_path: cli_path,
        ..Default::default()
    };

    let process_manager = Arc::new(ProcessManager::new(orchestrator_config, repository.clone()));

    // Build SONA configuration
    let sona_config = build_sona_config(&args);

    let mut hive_state = HiveState::with_sona_config(
        session.clone(),
        repository,
        process_manager,
        sona_config.clone(),
    );

    if let Some(model) = args.embedding_model.as_deref() {
        let svc = create_ollama_embedding_service(model, args.ollama_url.as_deref())?;
        hive_state = hive_state.with_embedding_service(Arc::new(svc));
        tracing::info!(model = %model, "Embedding service enabled");
    }

    // Initialize SONA engine if enabled
    if sona_config.enabled {
        tracing::info!(
            ewc_lambda = sona_config.ewc.lambda,
            ewc_gamma = sona_config.ewc.gamma,
            lora_rank = sona_config.base_lora.rank,
            learning_interval = sona_config.learning_loop.interval_secs,
            "SONA adaptive learning enabled"
        );
    }

    tracing::info!(
        host = %args.host,
        port = args.port,
        session_id = %session.id,
        "Starting standalone harness MCP daemon"
    );

    start_mcp_server(Arc::new(hive_state), &args.host, args.port)
        .await
        .map_err(|e| anyhow!("MCP server exited with error: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_parse_defaults() {
        let parsed = Args::parse_from(Vec::<String>::new()).expect("default parse");
        assert_eq!(parsed.host, "127.0.0.1");
        assert_eq!(parsed.port, 3000);
        assert_eq!(parsed.population_cap, 16);
        assert_eq!(parsed.session_file, PathBuf::from(".harness-mcp-session"));
        assert!(parsed.embedding_model.is_none());
        assert!(parsed.ollama_url.is_none());
    }

    #[test]
    fn args_parse_overrides() {
        let parsed = Args::parse_from(vec![
            "--host".into(),
            "0.0.0.0".into(),
            "--port".into(),
            "4001".into(),
            "--population-cap".into(),
            "24".into(),
            "--session-file".into(),
            "tmp/session-id.txt".into(),
            "--embedding-model".into(),
            "nomic-embed-text".into(),
            "--ollama-url".into(),
            "http://localhost:11434".into(),
        ])
        .expect("override parse");

        assert_eq!(parsed.host, "0.0.0.0");
        assert_eq!(parsed.port, 4001);
        assert_eq!(parsed.population_cap, 24);
        assert_eq!(parsed.session_file, PathBuf::from("tmp/session-id.txt"));
        assert_eq!(parsed.embedding_model.as_deref(), Some("nomic-embed-text"));
        assert_eq!(parsed.ollama_url.as_deref(), Some("http://localhost:11434"));
    }

    #[test]
    fn args_parse_rejects_unknown_flag() {
        let err = Args::parse_from(vec!["--wat".into()]).expect_err("must fail");
        assert!(err.to_string().contains("unknown argument"));
    }

    #[test]
    fn args_parse_rejects_invalid_port() {
        let err = Args::parse_from(vec!["--port".into(), "abc".into()]).expect_err("must fail");
        assert!(err.to_string().contains("invalid --port value"));
    }

    #[test]
    fn session_file_roundtrip() {
        let temp_dir = std::env::temp_dir().join(format!("harness-mcpd-{}", Uuid::new_v4()));
        let session_file = temp_dir.join("session.txt");
        let session_id = SessionId::new();

        write_session_id(&session_file, session_id).expect("write");
        let loaded = read_session_id(&session_file).expect("read");

        assert_eq!(loaded, Some(session_id));

        let _ = fs::remove_file(&session_file);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
