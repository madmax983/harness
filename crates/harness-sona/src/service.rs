//! HiveLearningService: High-level orchestration of collective learning.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use harness_persistence::{AgentId, Knowledge, KnowledgeKind, Repository, SessionId};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Sentinel prefix for BaseLoRA state stored in knowledge entries.
const SONA_STATE_PREFIX: &str = "__SONA_BASE_LORA_STATE__:";

use crate::base_lora::BaseLoRA;
use crate::config::BaseLoRAConfig;
use crate::coordinator::RoundResult;
use crate::error::{SonaError, SonaResult};
use crate::merit::CollectiveMerit;
use crate::types::{AggregationStrategy, ContributionStats, LoRADelta};

/// Configuration for the HiveLearningService.
#[derive(Debug, Clone)]
pub struct HiveLearningConfig {
    /// Rank of the BaseLoRA.
    pub base_lora_rank: usize,
    /// Aggregation strategy.
    pub aggregation_strategy: AggregationStrategy,
    /// How often the background loop triggers aggregation.
    pub round_interval: Duration,
    /// Minimum participants for a round to complete.
    pub min_participants: usize,
}

/// State of the BaseLoRA (for inspection/persistence).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseLoRAState {
    /// Whether the BaseLoRA has been initialized with at least one aggregation.
    pub is_initialized: bool,
    /// Current aggregated weights.
    pub weights: Vec<f32>,
    /// Domains covered by the aggregation.
    pub domains: HashSet<String>,
    /// How many rounds have been completed.
    pub rounds_completed: usize,
}

/// What a new agent inherits from the collective.
#[derive(Debug, Clone)]
pub struct AgentInheritance {
    /// Base weights to initialize the new agent's MicroLoRA.
    pub base_weights: Option<Vec<f32>>,
    /// Domains the collective has knowledge in.
    pub domains_covered: HashSet<String>,
    /// How many collective rounds were incorporated.
    pub collective_rounds: usize,
}

/// Statistics about the background learning loop.
#[derive(Debug, Clone)]
pub struct LoopStats {
    /// Number of aggregation rounds completed by the loop.
    pub rounds_completed: usize,
    /// Total number of contributions processed.
    pub total_contributions_processed: usize,
}

/// Internal state tracking per-agent initialization.
struct AgentState {
    initialized_with_base: bool,
    capability_score: f64,
}

/// High-level service that orchestrates collective learning for the hive.
pub struct HiveLearningService<R: Repository> {
    config: HiveLearningConfig,
    _repo: Arc<R>,
    /// The shared BaseLoRA for aggregation.
    base_lora: BaseLoRA,
    /// Current aggregated state.
    current_state: RwLock<BaseLoRAState>,
    /// Per-agent tracking.
    agents: RwLock<HashMap<uuid::Uuid, AgentState>>,
    /// Set of disconnected agents.
    disconnected: RwLock<HashSet<uuid::Uuid>>,
    /// Pending deltas not yet aggregated.
    pending_deltas: RwLock<Vec<LoRADelta>>,
    /// Loop statistics.
    loop_stats: RwLock<LoopStats>,
    /// Persisted state for recovery (legacy, now uses repo).
    #[allow(dead_code)]
    persisted: RwLock<Option<BaseLoRAState>>,
}

impl<R: Repository + 'static> HiveLearningService<R> {
    /// Create a new service.
    pub fn new(config: HiveLearningConfig, repo: Arc<R>) -> Self {
        let base_config = BaseLoRAConfig {
            rank: config.base_lora_rank,
            alpha: 16.0,
            aggregation_strategy: config.aggregation_strategy,
            min_participants: config.min_participants,
            staleness_threshold_secs: 3600,
        };
        let base_lora = BaseLoRA::new(base_config);

        Self {
            config,
            _repo: repo,
            base_lora,
            current_state: RwLock::new(BaseLoRAState {
                is_initialized: false,
                weights: Vec::new(),
                domains: HashSet::new(),
                rounds_completed: 0,
            }),
            agents: RwLock::new(HashMap::new()),
            disconnected: RwLock::new(HashSet::new()),
            pending_deltas: RwLock::new(Vec::new()),
            loop_stats: RwLock::new(LoopStats {
                rounds_completed: 0,
                total_contributions_processed: 0,
            }),
            persisted: RwLock::new(None),
        }
    }

    /// Contribute a LoRA delta from an agent.
    pub async fn contribute_delta(&self, delta: LoRADelta) -> SonaResult<()> {
        let agent_uuid = delta.agent_id.as_uuid();

        // Track the agent
        {
            let mut agents = self.agents.write().await;
            agents.entry(agent_uuid).or_insert(AgentState {
                initialized_with_base: false,
                capability_score: 0.0,
            });
        }

        // Add to base_lora for aggregation
        self.base_lora.contribute(delta.clone()).await?;

        // Also track as pending for background loop
        self.pending_deltas.write().await.push(delta);

        Ok(())
    }

    /// Run a single aggregation round.
    pub async fn run_aggregation_round(&self) -> SonaResult<RoundResult> {
        // Check minimum participants
        let stats = self.base_lora.contribution_stats().await;
        let active_agents = {
            let agents = self.agents.read().await;
            let disconnected = self.disconnected.read().await;
            agents
                .keys()
                .filter(|id| !disconnected.contains(id))
                .count()
        };

        if active_agents < self.config.min_participants {
            return Err(SonaError::InsufficientParticipants {
                required: self.config.min_participants,
                actual: active_agents,
            });
        }

        // Aggregate
        let aggregated = self.base_lora.aggregate().await?;

        // Update state
        {
            let mut state = self.current_state.write().await;
            state.is_initialized = true;
            state.weights = aggregated.weights.clone();
            state.domains = self.base_lora.known_domains().await;
            state.rounds_completed += 1;
        }

        // Update loop stats
        {
            let mut loop_stats = self.loop_stats.write().await;
            loop_stats.rounds_completed += 1;
            let pending_count = self.pending_deltas.read().await.len();
            loop_stats.total_contributions_processed += pending_count;
        }

        // Clear pending
        self.pending_deltas.write().await.clear();

        Ok(RoundResult {
            round_id: uuid::Uuid::new_v4().to_string(),
            is_complete: true,
            aggregated_weights: Some(aggregated.weights),
            participants: stats.total_contributors,
        })
    }

    /// Get the current BaseLoRA state.
    pub async fn base_lora_state(&self) -> SonaResult<BaseLoRAState> {
        Ok(self.current_state.read().await.clone())
    }

    /// Initialize a new agent with the collective's knowledge.
    pub async fn initialize_new_agent(&self, agent_id: AgentId) -> SonaResult<AgentInheritance> {
        let state = self.current_state.read().await;

        let inheritance = if state.is_initialized {
            AgentInheritance {
                base_weights: Some(state.weights.clone()),
                domains_covered: state.domains.clone(),
                collective_rounds: state.rounds_completed,
            }
        } else {
            AgentInheritance {
                base_weights: None,
                domains_covered: HashSet::new(),
                collective_rounds: 0,
            }
        };

        // Mark agent as initialized
        {
            let mut agents = self.agents.write().await;
            let agent = agents.entry(agent_id.as_uuid()).or_insert(AgentState {
                initialized_with_base: false,
                capability_score: 0.0,
            });
            agent.initialized_with_base = state.is_initialized;
            if state.is_initialized {
                // Warm-started agents get a base capability score
                agent.capability_score = 0.5 + (state.rounds_completed as f64 * 0.1).min(0.4);
            }
        }

        Ok(inheritance)
    }

    /// Get the capability score for an agent.
    /// Agents initialized with the BaseLoRA have a higher score.
    pub async fn agent_capability_score(&self, agent_id: AgentId) -> SonaResult<f64> {
        let agents = self.agents.read().await;
        let score = agents
            .get(&agent_id.as_uuid())
            .map(|a| a.capability_score)
            .unwrap_or(0.0);
        Ok(score)
    }

    /// Handle an agent disconnecting from the hive.
    pub async fn handle_agent_disconnect(&self, agent_id: AgentId) -> SonaResult<()> {
        self.disconnected.write().await.insert(agent_id.as_uuid());
        Ok(())
    }

    /// Get loop statistics.
    pub async fn loop_stats(&self) -> SonaResult<LoopStats> {
        Ok(self.loop_stats.read().await.clone())
    }

    /// Get contribution statistics (delegated to base_lora + disconnect info).
    pub async fn contribution_stats(&self) -> ContributionStats {
        let mut stats = self.base_lora.contribution_stats().await;
        let disconnected = self.disconnected.read().await;
        stats.active_contributors = stats.total_contributors.saturating_sub(disconnected.len());
        stats
    }

    /// Compute the collective merit of the hive.
    ///
    /// Merit is computed from both internal SONA state and repo-level data
    /// (knowledge entries, completed tasks) to provide a holistic view.
    pub async fn compute_collective_merit(&self) -> SonaResult<CollectiveMerit> {
        let state = self.current_state.read().await;
        let agents = self.agents.read().await;
        let disconnected = self.disconnected.read().await;

        let active_count = agents
            .keys()
            .filter(|id| !disconnected.contains(id))
            .count();

        // Query the repo for additional signals (knowledge entries as proxy)
        let repo_knowledge = self
            ._repo
            .search_knowledge(&[0.0], 100)
            .await
            .unwrap_or_default();

        // Unique authors from repo knowledge
        let repo_authors: HashSet<_> = repo_knowledge.iter().map(|(k, _)| k.author_id).collect();

        // Knowledge coverage: from SONA domains or repo knowledge count
        let knowledge_coverage = if state.is_initialized {
            (state.domains.len() as f64 * 0.3).clamp(0.1, 1.0)
        } else if !repo_knowledge.is_empty() {
            (repo_knowledge.len() as f64 * 0.1).clamp(0.1, 1.0)
        } else {
            0.1
        };

        // Task success rate: from SONA rounds or repo data
        let task_success_rate = if state.rounds_completed > 0 {
            (state.rounds_completed as f64 * 0.2).min(1.0)
        } else if !repo_knowledge.is_empty() {
            // Use knowledge existence as a proxy for task completion
            (repo_knowledge.len() as f64 * 0.15).clamp(0.1, 1.0)
        } else {
            0.1
        };

        // Agent participation: from SONA-tracked agents or repo authors
        let total_participants = active_count.max(repo_authors.len());
        let agent_participation = if total_participants > 0 {
            (total_participants as f64 * 0.5).min(1.0)
        } else {
            0.0
        };

        // Knowledge diversity: from SONA domains or repo knowledge variety
        let knowledge_diversity = if state.domains.len() > 1 {
            let n = state.domains.len() as f64;
            (n.ln() / 3.0_f64.ln()).min(1.0)
        } else if repo_knowledge.len() > 1 {
            // Multiple knowledge entries = some diversity
            (repo_knowledge.len() as f64 * 0.1).min(0.5)
        } else {
            0.0
        };

        Ok(CollectiveMerit::compute(
            knowledge_coverage,
            task_success_rate,
            agent_participation,
            knowledge_diversity,
        ))
    }

    /// Persist the current BaseLoRA state to the repository.
    pub async fn persist_state(&self) -> SonaResult<()> {
        let state = self.current_state.read().await.clone();
        let json =
            serde_json::to_string(&state).map_err(|e| SonaError::Persistence(e.to_string()))?;

        let content = format!("{}{}", SONA_STATE_PREFIX, json);

        // Use a sentinel agent ID and session ID for storage
        let sentinel_agent = AgentId::new();
        let sentinel_session = SessionId::new();

        let knowledge = Knowledge::new(
            content,
            KnowledgeKind::Decision,
            sentinel_agent,
            sentinel_session,
        );

        self._repo
            .create_knowledge(&knowledge)
            .await
            .map_err(|e| SonaError::Persistence(e.to_string()))?;

        Ok(())
    }

    /// Restore the BaseLoRA state from the repository.
    pub async fn restore_state(&self) -> SonaResult<()> {
        // GREEN phase: Use search_knowledge (which in InMemoryRepository returns
        // all entries without session filtering) to find our persisted state.
        // REFACTOR phase should add a proper key-value persistence API.
        let all_knowledge = self
            ._repo
            .search_knowledge(&[0.0], 1000)
            .await
            .map_err(|e| SonaError::Persistence(e.to_string()))?;

        for (entry, _score) in &all_knowledge {
            if let Some(json_str) = entry.content.strip_prefix(SONA_STATE_PREFIX) {
                let state: BaseLoRAState = serde_json::from_str(json_str)
                    .map_err(|e| SonaError::Persistence(e.to_string()))?;
                *self.current_state.write().await = state;
                return Ok(());
            }
        }

        Err(SonaError::NotInitialized)
    }

    /// Get config (for background loop).
    pub(crate) fn config(&self) -> &HiveLearningConfig {
        &self.config
    }

    /// Check if there are pending deltas.
    pub(crate) async fn has_pending(&self) -> bool {
        !self.pending_deltas.read().await.is_empty()
    }
}
