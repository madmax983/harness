//! Synergy Graph: Analyzes collaborative relationships between agents based on shared activities.
//!
//! "Nova's Social Graph" - This module takes a sequence of TrajectoryEvents and builds a
//! weighted graph of agent collaborations. It enables visualizing how often agents work
//! together on the same tasks or projects, forming a dynamic "synergy score".

use harness_persistence::{AgentId, TrajectoryEvent};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// An edge in the Synergy Graph representing the collaboration strength between two agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SynergyEdge {
    pub agent_a: AgentId,
    pub agent_b: AgentId,
    pub weight: f32,
}

/// The Synergy Graph maps out collaborative interactions across the hive.
#[derive(Debug, Default)]
pub struct SynergyGraph {
    /// Maps an unordered pair of agents (min_id, max_id) to their synergy score.
    edges: HashMap<(AgentId, AgentId), f32>,
}

impl SynergyGraph {
    /// Create a new, empty Synergy Graph.
    pub fn new() -> Self {
        Self {
            edges: HashMap::new(),
        }
    }

    /// Build the graph from a sequence of events.
    ///
    /// The current heuristic rewards agents that interact with the same Task or Project.
    pub fn build_from_events(&mut self, events: &[TrajectoryEvent]) {
        // Group events by task
        let mut task_participants: HashMap<String, HashSet<AgentId>> = HashMap::new();

        for event in events {
            if let Some(task_id) = event.task_id() {
                task_participants
                    .entry(task_id.as_uuid().to_string())
                    .or_default()
                    .insert(event.agent_id());
            }
        }

        // Increase synergy weight for every shared task
        for participants in task_participants.values() {
            let agents: Vec<_> = participants.iter().copied().collect();
            for i in 0..agents.len() {
                for j in (i + 1)..agents.len() {
                    self.add_synergy(agents[i], agents[j], 1.0);
                }
            }
        }
    }

    /// Increment the synergy score between two agents.
    pub fn add_synergy(&mut self, agent_a: AgentId, agent_b: AgentId, amount: f32) {
        if agent_a == agent_b {
            return; // No self-synergy
        }
        let key = if agent_a.as_uuid() < agent_b.as_uuid() {
            (agent_a, agent_b)
        } else {
            (agent_b, agent_a)
        };
        *self.edges.entry(key).or_insert(0.0) += amount;
    }

    /// Retrieve all non-zero synergy edges.
    pub fn edges(&self) -> Vec<SynergyEdge> {
        self.edges
            .iter()
            .map(|(&(agent_a, agent_b), &weight)| SynergyEdge {
                agent_a,
                agent_b,
                weight,
            })
            .collect()
    }

    /// Export the synergy graph to Graphviz DOT format.
    pub fn export_dot(&self) -> String {
        let mut dot = String::from("graph Synergy {\n");
        for edge in self.edges() {
            dot.push_str(&format!(
                "    \"{}\" -- \"{}\" [weight=\"{:.1}\"];\n",
                edge.agent_a.as_uuid(),
                edge.agent_b.as_uuid(),
                edge.weight
            ));
        }
        dot.push_str("}\n");
        dot
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_persistence::{TrajectoryEventId, TriggerKind};

    // Helper to bypass privacy rules for testing.
    fn make_event(agent_id: AgentId, task_id: Option<String>) -> TrajectoryEvent {
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
    fn test_synergy_graph_build() {
        let mut graph = SynergyGraph::new();

        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let a3 = AgentId::new();

        // Task 1: a1 and a2
        let t1 = uuid::Uuid::new_v4().to_string();
        // Task 2: a2 and a3
        let t2 = uuid::Uuid::new_v4().to_string();

        let events = vec![
            make_event(a1, Some(t1.clone())),
            make_event(a2, Some(t1.clone())),
            make_event(a2, Some(t2.clone())),
            make_event(a3, Some(t2.clone())),
            make_event(a3, None), // Ignored
        ];

        graph.build_from_events(&events);

        let edges = graph.edges();
        assert_eq!(edges.len(), 2);

        let dot = graph.export_dot();
        assert!(dot.contains("graph Synergy {"));
        assert!(dot.contains(&a1.as_uuid().to_string()));
        assert!(dot.contains(&a2.as_uuid().to_string()));
        assert!(dot.contains(&a3.as_uuid().to_string()));
    }
}
