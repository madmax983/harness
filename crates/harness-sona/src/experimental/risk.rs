//! Risk Assessor: Evaluates assignment risks and suggests backup pairs.
//!
//! "Nova's Risk Matrix" - This module combines `MomentumTracker`, `SynergyGraph`
//! and `Oracle` predictions to assess the risk of assigning a task to an agent.
//! If the risk is high (e.g. low momentum agent + high complexity task),
//! it identifies a "backup agent" who has high synergy with the primary agent
//! and positive momentum.

use crate::experimental::momentum::MomentumTracker;
use crate::experimental::oracle::Complexity;
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::AgentId;

/// Assessment of the risk level for a task assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    /// Safe assignment (e.g. high momentum, low complexity)
    Safe,
    /// Acceptable risk (e.g. average momentum, average complexity)
    Calculated,
    /// High risk, consider backup (e.g. low momentum, high complexity)
    High,
    /// Critical risk, immediate action required
    Critical,
}

/// The result of a risk assessment.
#[derive(Debug, Clone)]
pub struct RiskAssessment {
    pub level: RiskLevel,
    /// Recommended backup agent, if risk is High or Critical and a suitable backup exists.
    pub backup_agent: Option<AgentId>,
    pub reasoning: String,
}

/// The Risk Assessor evaluates the safety of task assignments.
pub struct RiskAssessor<'a> {
    momentum: &'a MomentumTracker,
    synergy: &'a SynergyGraph,
}

impl<'a> RiskAssessor<'a> {
    pub fn new(momentum: &'a MomentumTracker, synergy: &'a SynergyGraph) -> Self {
        Self { momentum, synergy }
    }

    /// Evaluates the risk of assigning a task of a given complexity to an agent.
    /// Provides a backup agent suggestion if the risk is High or Critical.
    pub fn evaluate_assignment(
        &self,
        agent: AgentId,
        complexity: Complexity,
        available_agents: &[AgentId],
    ) -> RiskAssessment {
        let agent_momentum = self.momentum.agent_momentum(agent);

        let (level, reasoning) = match (complexity, agent_momentum) {
            (Complexity::Low, m) if m >= 0.0 => (
                RiskLevel::Safe,
                "Low complexity and non-negative momentum.".to_string(),
            ),
            (Complexity::Low, m) if m < 0.0 => (
                RiskLevel::Calculated,
                "Low complexity but agent has negative momentum.".to_string(),
            ),
            (Complexity::Medium, m) if m > 2.0 => (
                RiskLevel::Safe,
                "Medium complexity but agent has high momentum.".to_string(),
            ),
            (Complexity::Medium, m) if m >= 0.0 => (
                RiskLevel::Calculated,
                "Medium complexity and stable momentum.".to_string(),
            ),
            (Complexity::Medium, m) if m < 0.0 => (
                RiskLevel::High,
                "Medium complexity and negative momentum.".to_string(),
            ),
            (Complexity::High, m) if m > 2.0 => (
                RiskLevel::Calculated,
                "High complexity but agent is on fire.".to_string(),
            ),
            (Complexity::High, m) if m >= 0.0 => (
                RiskLevel::High,
                "High complexity and average momentum.".to_string(),
            ),
            (Complexity::High, m) if m < 0.0 => (
                RiskLevel::Critical,
                "High complexity and negative momentum.".to_string(),
            ),
            (Complexity::Unknown, m) if m > 0.0 => (
                RiskLevel::Calculated,
                "Unknown complexity, but agent has positive momentum.".to_string(),
            ),
            (Complexity::Unknown, _) => (
                RiskLevel::High,
                "Unknown complexity and low momentum.".to_string(),
            ),
            _ => (RiskLevel::Calculated, "Fallback risk level.".to_string()),
        };

        let mut backup_agent = None;
        if level == RiskLevel::High || level == RiskLevel::Critical {
            backup_agent = self.find_backup_agent(agent, available_agents);
        }

        RiskAssessment {
            level,
            backup_agent,
            reasoning,
        }
    }

    /// Finds the best backup agent based on synergy and momentum.
    fn find_backup_agent(&self, primary: AgentId, available_agents: &[AgentId]) -> Option<AgentId> {
        let mut best_backup = None;
        let mut best_score = f32::NEG_INFINITY;

        for &candidate in available_agents {
            if candidate == primary {
                continue;
            }

            // We want an agent with positive momentum and high synergy
            let candidate_momentum = self.momentum.agent_momentum(candidate);
            if candidate_momentum <= 0.0 {
                continue; // Backup should not be struggling themselves
            }

            let synergy_weight = self.get_synergy(primary, candidate);
            if synergy_weight > 0.0 {
                let score = candidate_momentum + synergy_weight * 2.0; // Emphasize synergy
                if score > best_score {
                    best_score = score;
                    best_backup = Some(candidate);
                }
            }
        }

        best_backup
    }

    /// Get the synergy weight between two agents from the synergy graph.
    fn get_synergy(&self, agent_a: AgentId, agent_b: AgentId) -> f32 {
        for edge in self.synergy.edges() {
            if (edge.agent_a == agent_a && edge.agent_b == agent_b)
                || (edge.agent_a == agent_b && edge.agent_b == agent_a)
            {
                return edge.weight;
            }
        }
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_persistence::{TrajectoryEvent, TrajectoryEventId, TriggerKind};

    fn make_momentum_event(agent_id: AgentId, success: bool) -> TrajectoryEvent {
        let id = TrajectoryEventId::new();
        let json = serde_json::json!({
            "id": id,
            "trigger_kind": TriggerKind::TaskComplete,
            "agent_id": agent_id,
            "task_id": null,
            "success": success,
            "summary": "did work",
            "steps": [],
            "created_at": Utc::now()
        });
        serde_json::from_value(json).expect("Failed to create mock event")
    }

    fn make_synergy_event(agent_id: AgentId, task_id: Option<String>) -> TrajectoryEvent {
        let id = TrajectoryEventId::new();
        let task_uuid =
            task_id.map(|t| uuid::Uuid::parse_str(&t).unwrap_or_else(|_| uuid::Uuid::new_v4()));

        let json = serde_json::json!({
            "id": id,
            "trigger_kind": TriggerKind::TaskComplete,
            "agent_id": agent_id,
            "task_id": task_uuid,
            "success": true,
            "summary": "did work",
            "steps": [],
            "created_at": Utc::now()
        });
        serde_json::from_value(json).expect("Failed to create mock event")
    }

    #[test]
    fn test_risk_evaluation() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let a_pro = AgentId::new();
        let a_struggling = AgentId::new();
        let a_backup = AgentId::new();

        // Setup Momentum
        // pro: +3
        // backup: +2
        // struggling: -2
        let m_events = vec![
            make_momentum_event(a_pro, true),
            make_momentum_event(a_pro, true),
            make_momentum_event(a_pro, true),
            make_momentum_event(a_backup, true),
            make_momentum_event(a_backup, true),
            make_momentum_event(a_struggling, false),
            make_momentum_event(a_struggling, false),
        ];
        momentum.process_events(&m_events);

        // Setup Synergy
        // struggling and backup have worked together on a task
        let t1 = uuid::Uuid::new_v4().to_string();
        let s_events = vec![
            make_synergy_event(a_struggling, Some(t1.clone())),
            make_synergy_event(a_backup, Some(t1.clone())),
        ];
        synergy.build_from_events(&s_events);

        let assessor = RiskAssessor::new(&momentum, &synergy);
        let available = vec![a_pro, a_struggling, a_backup];

        // 1. Pro + Low Complexity = Safe
        let safe_eval = assessor.evaluate_assignment(a_pro, Complexity::Low, &available);
        assert_eq!(safe_eval.level, RiskLevel::Safe);
        assert!(safe_eval.backup_agent.is_none());

        // 2. Struggling + High Complexity = High/Critical Risk
        let risky_eval = assessor.evaluate_assignment(a_struggling, Complexity::High, &available);
        assert!(risky_eval.level == RiskLevel::High || risky_eval.level == RiskLevel::Critical);

        // 3. Backup agent should be suggested because of synergy
        assert_eq!(risky_eval.backup_agent, Some(a_backup));
    }
}
