//! Federated learning coordinator for multi-agent synchronization.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use harness_persistence::Repository;
use tokio::sync::RwLock;

use crate::base_lora::BaseLoRA;
use crate::config::BaseLoRAConfig;
use crate::error::{SonaError, SonaResult};
use crate::merit::CollectiveMerit;
use crate::types::{AggregatedWeights, LoRADelta};

/// Configuration for the FederatedCoordinator.
#[derive(Debug, Clone)]
pub struct FederatedConfig {
    /// How often to run aggregation rounds.
    pub round_interval: Duration,
    /// Minimum number of agent contributions to complete a round.
    pub min_participants: usize,
    /// Window within which deltas are considered fresh.
    pub staleness_window: Duration,
    /// Stop iterating when weight drift is below this threshold.
    pub convergence_threshold: f64,
}

/// Status of a federated round.
#[derive(Debug, Clone)]
pub struct RoundStatus {
    /// Number of agents that submitted updates.
    pub participants: usize,
    /// Whether the round has been completed.
    pub is_complete: bool,
}

/// Result of completing a federated round.
#[derive(Debug, Clone)]
pub struct RoundResult {
    /// The round identifier.
    pub round_id: String,
    /// Whether the round completed successfully.
    pub is_complete: bool,
    /// Aggregated weights (if round completed).
    pub aggregated_weights: Option<Vec<f32>>,
    /// Number of participants.
    pub participants: usize,
}

/// Convergence metrics across rounds.
#[derive(Debug, Clone)]
pub struct ConvergenceMetrics {
    /// How many rounds have been completed.
    pub rounds_completed: usize,
    /// How much the weights changed between the last two rounds.
    pub weight_drift: f64,
}

/// Internal state for an active round.
struct Round {
    deltas: Vec<LoRADelta>,
    is_complete: bool,
    result: Option<AggregatedWeights>,
}

/// Orchestrates federated learning rounds across agents.
pub struct FederatedCoordinator<R: Repository> {
    fed_config: FederatedConfig,
    base_config: BaseLoRAConfig,
    _repo: Arc<R>,
    /// Active and completed rounds keyed by round_id.
    rounds: RwLock<HashMap<String, Round>>,
    /// Ordered list of completed round IDs.
    completed_rounds: RwLock<Vec<String>>,
    /// Previous round's aggregated weights (for drift computation).
    previous_weights: RwLock<Option<Vec<f32>>>,
}

impl<R: Repository + 'static> FederatedCoordinator<R> {
    /// Create a new federated coordinator.
    pub fn new(fed_config: FederatedConfig, base_config: BaseLoRAConfig, repo: Arc<R>) -> Self {
        Self {
            fed_config,
            base_config,
            _repo: repo,
            rounds: RwLock::new(HashMap::new()),
            completed_rounds: RwLock::new(Vec::new()),
            previous_weights: RwLock::new(None),
        }
    }

    /// Start a new federated round. Returns the round ID.
    pub async fn start_round(&self) -> SonaResult<String> {
        let round_id = uuid::Uuid::new_v4().to_string();
        let round = Round {
            deltas: Vec::new(),
            is_complete: false,
            result: None,
        };
        self.rounds.write().await.insert(round_id.clone(), round);
        Ok(round_id)
    }

    /// Submit an agent's update for a round.
    pub async fn submit_update(&self, round_id: &str, delta: LoRADelta) -> SonaResult<()> {
        // Check staleness
        let age = Utc::now()
            .signed_duration_since(delta.timestamp)
            .to_std()
            .unwrap_or(Duration::from_secs(0));

        if age > self.fed_config.staleness_window {
            return Err(SonaError::StaleDelta);
        }

        let mut rounds = self.rounds.write().await;
        let round = rounds
            .get_mut(round_id)
            .ok_or_else(|| SonaError::UnknownRound(round_id.to_string()))?;

        if round.is_complete {
            return Err(SonaError::RoundAlreadyComplete(round_id.to_string()));
        }

        round.deltas.push(delta);
        Ok(())
    }

    /// Get the status of a round.
    pub async fn round_status(&self, round_id: &str) -> SonaResult<RoundStatus> {
        let rounds = self.rounds.read().await;
        let round = rounds
            .get(round_id)
            .ok_or_else(|| SonaError::UnknownRound(round_id.to_string()))?;

        Ok(RoundStatus {
            participants: round.deltas.len(),
            is_complete: round.is_complete,
        })
    }

    /// Complete a round: aggregate all submitted deltas.
    pub async fn complete_round(&self, round_id: &str) -> SonaResult<RoundResult> {
        let base_lora = BaseLoRA::new(self.base_config.clone());

        // Collect deltas and aggregate
        let aggregated = {
            let mut rounds = self.rounds.write().await;
            let round = rounds
                .get_mut(round_id)
                .ok_or_else(|| SonaError::UnknownRound(round_id.to_string()))?;

            if round.is_complete {
                return Err(SonaError::RoundAlreadyComplete(round_id.to_string()));
            }

            // Feed deltas to BaseLoRA
            for delta in &round.deltas {
                base_lora.contribute(delta.clone()).await?;
            }

            let aggregated = base_lora.aggregate().await?;
            let participants = round.deltas.len();

            round.is_complete = true;
            round.result = Some(aggregated.clone());

            (aggregated, participants)
        };

        // Track completion
        self.completed_rounds
            .write()
            .await
            .push(round_id.to_string());

        // Update previous weights for drift tracking
        *self.previous_weights.write().await = Some(aggregated.0.weights.clone());

        Ok(RoundResult {
            round_id: round_id.to_string(),
            is_complete: true,
            aggregated_weights: Some(aggregated.0.weights),
            participants: aggregated.1,
        })
    }

    /// Get history of completed rounds.
    pub async fn round_history(&self, limit: usize) -> SonaResult<Vec<RoundResult>> {
        let completed = self.completed_rounds.read().await;
        let rounds = self.rounds.read().await;

        let history: Vec<RoundResult> = completed
            .iter()
            .rev()
            .take(limit)
            .filter_map(|id| {
                rounds.get(id).map(|r| RoundResult {
                    round_id: id.clone(),
                    is_complete: r.is_complete,
                    aggregated_weights: r.result.as_ref().map(|a| a.weights.clone()),
                    participants: r.deltas.len(),
                })
            })
            .collect();

        // Reverse to get chronological order
        let mut result: Vec<RoundResult> = history.into_iter().rev().collect();
        result.truncate(limit);
        Ok(result)
    }

    /// Compute the collective merit based on completed rounds.
    pub async fn compute_collective_merit(&self) -> SonaResult<CollectiveMerit> {
        let completed = self.completed_rounds.read().await;
        let rounds = self.rounds.read().await;

        if completed.is_empty() {
            return Ok(CollectiveMerit::compute(0.0, 0.0, 0.0, 0.0));
        }

        // Compute merit from the most recent round's data
        let latest_id = completed.last().unwrap();
        let latest = rounds.get(latest_id).unwrap();

        // Knowledge coverage: based on number of domains
        let domains: std::collections::HashSet<&str> =
            latest.deltas.iter().map(|d| d.domain.as_str()).collect();
        let knowledge_coverage = (domains.len() as f64).clamp(0.1, 1.0);

        // Task success rate: average success rate of participants
        let avg_success = if latest.deltas.is_empty() {
            0.0
        } else {
            latest.deltas.iter().map(|d| d.success_rate).sum::<f64>()
                / latest.deltas.len() as f64
        };

        // Agent participation: fraction of minimum met
        let participation = (latest.deltas.len() as f64 / self.fed_config.min_participants as f64)
            .min(1.0);

        // Knowledge diversity: use the number of rounds as a proxy
        let diversity = (completed.len() as f64 * 0.1).min(1.0);

        Ok(CollectiveMerit::compute(
            knowledge_coverage,
            avg_success,
            participation,
            diversity,
        ))
    }

    /// Get convergence metrics.
    pub async fn convergence_metrics(&self) -> SonaResult<ConvergenceMetrics> {
        let completed = self.completed_rounds.read().await;
        let rounds = self.rounds.read().await;

        let rounds_completed = completed.len();

        // Compute weight drift between last two rounds
        let weight_drift = if completed.len() >= 2 {
            let prev_id = &completed[completed.len() - 2];
            let curr_id = &completed[completed.len() - 1];

            let prev_weights = rounds
                .get(prev_id)
                .and_then(|r| r.result.as_ref())
                .map(|a| &a.weights);
            let curr_weights = rounds
                .get(curr_id)
                .and_then(|r| r.result.as_ref())
                .map(|a| &a.weights);

            match (prev_weights, curr_weights) {
                (Some(prev), Some(curr)) => {
                    let sum_sq: f64 = prev
                        .iter()
                        .zip(curr.iter())
                        .map(|(&p, &c)| ((c - p) as f64).powi(2))
                        .sum();
                    (sum_sq / prev.len() as f64).sqrt()
                }
                _ => 0.0,
            }
        } else {
            0.0
        };

        Ok(ConvergenceMetrics {
            rounds_completed,
            weight_drift,
        })
    }
}
