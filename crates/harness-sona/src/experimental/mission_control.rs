//! Mission Control: Mashing up Oracle and DreamTeamBuilder.
//!
//! "Nova's Mission Control" - This module uses the Oracle to determine task complexity
//! and the optimal team size, then unleashes the DreamTeamBuilder to select the best
//! available agents based on their momentum and synergy.

use crate::experimental::dream_team::DreamTeamBuilder;
use crate::experimental::momentum::MomentumTracker;
use crate::experimental::oracle::{Complexity, Oracle};
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::{AgentId, ReasoningBank};

/// Mission Control orchestrates team formation by evaluating tasks and available agents.
pub struct MissionControl;

impl MissionControl {
    pub fn new() -> Self {
        Self
    }

    /// Plans a mission by determining the required team size using the Oracle,
    /// then forming the team using the DreamTeamBuilder.
    pub async fn plan_mission(
        &self,
        oracle: &Oracle,
        bank: &ReasoningBank,
        momentum: &MomentumTracker,
        synergy: &SynergyGraph,
        candidates: &[AgentId],
        task_description: &str,
    ) -> Result<Vec<AgentId>, anyhow::Error> {
        let prediction = oracle.predict(bank, task_description).await?;

        let team_size = match prediction.estimated_complexity {
            Complexity::Low => 1,
            Complexity::Medium => 2,
            Complexity::High => 3,
            Complexity::Unknown => 3,
        };

        let builder = DreamTeamBuilder::new(momentum, synergy);
        let team = builder.assemble_team(candidates, team_size);

        Ok(team)
    }
}

impl Default for MissionControl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_persistence::{
        PatternStore, SessionId, TrajectoryEvent, TrajectoryEventId, TriggerKind,
    };
    use std::sync::Arc;

    // Helper to bypass privacy rules for testing.
    fn make_event_for_momentum(agent_id: AgentId, success: bool) -> TrajectoryEvent {
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

    #[tokio::test]
    async fn test_mission_control_plan_mission() {
        let store = Arc::new(PatternStore::new());
        let session_id = SessionId::new();
        let bank = ReasoningBank::new(store, session_id);

        let oracle = Oracle::new();
        let mut momentum = MomentumTracker::new();
        let synergy = SynergyGraph::new();

        let a1 = AgentId::new();
        let a2 = AgentId::new();

        let m_events = vec![
            make_event_for_momentum(a1, true),
            make_event_for_momentum(a2, false),
        ];
        momentum.process_events(&m_events);

        let mc = MissionControl::new();
        let candidates = vec![a1, a2];

        let result = mc
            .plan_mission(
                &oracle,
                &bank,
                &momentum,
                &synergy,
                &candidates,
                "Test task",
            )
            .await;

        assert!(result.is_ok(), "plan_mission should return Ok");
    }
}
