//! Archetype Analyzer: Behavioral persona classification for agents.
//!
//! "Nova's Archetype Analyzer" - This module synthesizes the `MomentumTracker`
//! and `SynergyGraph` to classify agents into behavioral personas:
//! `Pillar`, `LoneWolf`, `SocialLoafer`, `Rookie`, `Steady`.

use crate::experimental::momentum::MomentumTracker;
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Behavioral persona of an agent within the hive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Archetype {
    /// High momentum, high synergy (The rockstars who also team-play).
    Pillar,
    /// High momentum, low synergy (Gets things done, but alone).
    LoneWolf,
    /// Low momentum, high synergy (Lots of talk/pairing, little delivery).
    SocialLoafer,
    /// Low momentum, low synergy (Struggling or new).
    Rookie,
    /// Average momentum, average synergy (The dependable core).
    Steady,
}

/// The Archetype Analyzer classifies agents into behavioral personas.
pub struct ArchetypeAnalyzer<'a> {
    momentum: &'a MomentumTracker,
    synergy: &'a SynergyGraph,
}

impl<'a> ArchetypeAnalyzer<'a> {
    /// Create a new ArchetypeAnalyzer.
    pub fn new(momentum: &'a MomentumTracker, synergy: &'a SynergyGraph) -> Self {
        Self { momentum, synergy }
    }

    /// Analyze all known agents and assign them an Archetype.
    pub fn analyze(&self) -> HashMap<AgentId, Archetype> {
        let mut classifications = HashMap::new();
        let mut known_agents = HashSet::new();

        // Collect all agents from momentum tracker
        for agent_id in self.momentum.agents() {
            known_agents.insert(agent_id);
        }

        // Collect all agents from synergy graph
        for edge in self.synergy.edges() {
            known_agents.insert(edge.agent_a);
            known_agents.insert(edge.agent_b);
        }

        // Calculate total synergy per agent
        let mut total_synergy: HashMap<AgentId, f32> = HashMap::new();
        for edge in self.synergy.edges() {
            *total_synergy.entry(edge.agent_a).or_insert(0.0) += edge.weight;
            *total_synergy.entry(edge.agent_b).or_insert(0.0) += edge.weight;
        }

        for agent in known_agents {
            let m = self.momentum.agent_momentum(agent);
            let s = total_synergy.get(&agent).copied().unwrap_or(0.0);

            let archetype = if m >= 2.0 && s >= 2.0 {
                Archetype::Pillar
            } else if m >= 2.0 && s < 2.0 {
                Archetype::LoneWolf
            } else if m < 0.0 && s >= 2.0 {
                Archetype::SocialLoafer
            } else if m < 0.0 && s < 2.0 {
                Archetype::Rookie
            } else {
                Archetype::Steady
            };

            classifications.insert(agent, archetype);
        }

        classifications
    }

    /// Generate a Markdown report of the hive's social dynamics.
    pub fn generate_report(&self) -> String {
        let classifications = self.analyze();

        let mut report = String::from("# Hive Archetype Report\n\n");

        // Group agents by archetype
        let mut by_archetype: HashMap<Archetype, Vec<AgentId>> = HashMap::new();
        for (agent, archetype) in classifications {
            by_archetype.entry(archetype).or_default().push(agent);
        }

        let order = [
            (
                Archetype::Pillar,
                "🏛️ Pillars (High Momentum, High Synergy)",
            ),
            (
                Archetype::LoneWolf,
                "🐺 Lone Wolves (High Momentum, Low Synergy)",
            ),
            (Archetype::Steady, "⚙️ Steady (Average Performers)"),
            (
                Archetype::SocialLoafer,
                "🗣️ Social Loafers (Low Momentum, High Synergy)",
            ),
            (Archetype::Rookie, "🌱 Rookies (Low Momentum, Low Synergy)"),
        ];

        for (arch, title) in order {
            report.push_str(&format!("## {}\n", title));

            if let Some(mut agents) = by_archetype.remove(&arch) {
                // Sort by UUID for deterministic output
                agents.sort_by_key(|a| a.as_uuid().to_string());

                for agent in agents {
                    let m = self.momentum.agent_momentum(agent);

                    let mut s = 0.0;
                    for edge in self.synergy.edges() {
                        if edge.agent_a == agent || edge.agent_b == agent {
                            s += edge.weight;
                        }
                    }

                    report.push_str(&format!(
                        "- Agent `{}` (Momentum: {:.1}, Synergy: {:.1})\n",
                        agent
                            .as_uuid()
                            .to_string()
                            .chars()
                            .take(8)
                            .collect::<String>(),
                        m,
                        s
                    ));
                }
            } else {
                report.push_str("- *None*\n");
            }
            report.push('\n');
        }

        report
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
        serde_json::from_value(json).unwrap()
    }

    fn make_synergy_event(agent_id: AgentId, task_id: &str) -> TrajectoryEvent {
        let id = TrajectoryEventId::new();
        let task_uuid = uuid::Uuid::parse_str(task_id).unwrap_or_else(|_| uuid::Uuid::new_v4());
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
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn test_archetype_classification() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let pillar = AgentId::new();
        let lone_wolf = AgentId::new();
        let social_loafer = AgentId::new();
        let rookie = AgentId::new();
        let steady = AgentId::new();

        let t1 = uuid::Uuid::new_v4().to_string();
        let t2 = uuid::Uuid::new_v4().to_string();

        let m_events = vec![
            // Pillar: High momentum
            make_momentum_event(pillar, true),
            make_momentum_event(pillar, true),
            // Lone Wolf: High momentum
            make_momentum_event(lone_wolf, true),
            make_momentum_event(lone_wolf, true),
            // Social Loafer: Low momentum
            make_momentum_event(social_loafer, false),
            // Rookie: Low momentum
            make_momentum_event(rookie, false),
            // Steady: average momentum (needs an event to be tracked, momentum = 1.0)
            make_momentum_event(steady, true),
        ];
        momentum.process_events(&m_events);

        let s_events = vec![
            // Pillar and Social Loafer work together a lot (High Synergy)
            make_synergy_event(pillar, &t1),
            make_synergy_event(social_loafer, &t1),
            make_synergy_event(pillar, &t2),
            make_synergy_event(social_loafer, &t2),
            // Lone Wolf, Rookie, Steady have no/low shared tasks
        ];
        synergy.build_from_events(&s_events);

        let analyzer = ArchetypeAnalyzer::new(&momentum, &synergy);
        let class = analyzer.analyze();

        assert_eq!(class.get(&pillar), Some(&Archetype::Pillar));
        assert_eq!(class.get(&lone_wolf), Some(&Archetype::LoneWolf));
        assert_eq!(class.get(&social_loafer), Some(&Archetype::SocialLoafer));
        assert_eq!(class.get(&rookie), Some(&Archetype::Rookie));
        assert_eq!(class.get(&steady), Some(&Archetype::Steady));
    }

    #[test]
    fn test_steady_classification() {
        let mut momentum = MomentumTracker::new();
        let synergy = SynergyGraph::new();

        let steady = AgentId::new();
        let m_events = vec![
            make_momentum_event(steady, true), // Momentum +1 (Steady)
        ];
        momentum.process_events(&m_events);

        let analyzer = ArchetypeAnalyzer::new(&momentum, &synergy);
        let class = analyzer.analyze();
        assert_eq!(class.get(&steady), Some(&Archetype::Steady));
    }

    #[test]
    fn test_report_generation() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let pillar = AgentId::new();
        let lone_wolf = AgentId::new();

        let t1 = uuid::Uuid::new_v4().to_string();

        let m_events = vec![
            make_momentum_event(pillar, true),
            make_momentum_event(pillar, true),
            make_momentum_event(lone_wolf, true),
            make_momentum_event(lone_wolf, true),
        ];
        momentum.process_events(&m_events);

        let s_events = vec![
            make_synergy_event(pillar, &t1),
            make_synergy_event(lone_wolf, &t1),
            make_synergy_event(pillar, &t1),
            make_synergy_event(lone_wolf, &t1),
        ];
        synergy.build_from_events(&s_events);
        // synergy logic adds weight = 1.0 per task instance, but unique tasks. Let's just manually add synergy.
        synergy.add_synergy(pillar, AgentId::new(), 2.0); // Now Pillar has >= 2 synergy
        // Lone wolf has no extra synergy

        let analyzer = ArchetypeAnalyzer::new(&momentum, &synergy);
        let report = analyzer.generate_report();

        assert!(report.contains("# Hive Archetype Report"));
        assert!(report.contains("## 🏛️ Pillars (High Momentum, High Synergy)"));
        assert!(report.contains("## 🐺 Lone Wolves (High Momentum, Low Synergy)"));
        assert!(
            report.contains(
                &pillar
                    .as_uuid()
                    .to_string()
                    .chars()
                    .take(8)
                    .collect::<String>()
            )
        );
        assert!(
            report.contains(
                &lone_wolf
                    .as_uuid()
                    .to_string()
                    .chars()
                    .take(8)
                    .collect::<String>()
            )
        );
    }
}
