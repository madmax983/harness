//! Archetype Analyzer: Classifies agents into behavioral personas based on momentum and synergy.
//!
//! "Nova's Social Dynamics" - This module combines the `MomentumTracker` and
//! `SynergyGraph` to classify agents into behavioral personas (`Pillar`, `LoneWolf`,
//! `SocialLoafer`, `Rookie`, `Steady`) and can generate a Markdown report of hive
//! social dynamics.

use crate::experimental::momentum::MomentumTracker;
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::AgentId;
use std::collections::{HashMap, HashSet};

/// Behavioral persona of an AI agent based on performance and collaboration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentArchetype {
    /// High momentum, high synergy
    Pillar,
    /// High momentum, low synergy
    LoneWolf,
    /// Low momentum, high synergy
    SocialLoafer,
    /// Low momentum, low synergy
    Rookie,
    /// Medium momentum and/or medium synergy (fallback)
    Steady,
}

/// Analyzes agent behavior and classifies them into archetypes.
pub struct ArchetypeAnalyzer<'a> {
    momentum: &'a MomentumTracker,
    synergy: &'a SynergyGraph,
}

impl<'a> ArchetypeAnalyzer<'a> {
    /// Create a new ArchetypeAnalyzer.
    pub fn new(momentum: &'a MomentumTracker, synergy: &'a SynergyGraph) -> Self {
        Self { momentum, synergy }
    }

    /// Analyze all agents present in momentum or synergy data and classify them.
    pub fn analyze(&self) -> HashMap<AgentId, AgentArchetype> {
        let mut agents = HashSet::new();

        for edge in self.synergy.edges() {
            agents.insert(edge.agent_a);
            agents.insert(edge.agent_b);
        }

        for agent in self.momentum.agents() {
            agents.insert(agent);
        }

        let mut archetypes = HashMap::new();

        for agent in agents {
            let m_score = self.momentum.agent_momentum(agent);

            // Calculate total synergy for this agent
            let mut s_score = 0.0;
            for edge in self.synergy.edges() {
                if edge.agent_a == agent || edge.agent_b == agent {
                    s_score += edge.weight;
                }
            }

            // Classification Thresholds
            let is_high_momentum = m_score >= 2.0;
            let is_low_momentum = m_score < 0.0;
            let is_high_synergy = s_score >= 2.0;
            let is_low_synergy = s_score < 1.0;

            let archetype = if is_high_momentum && is_high_synergy {
                AgentArchetype::Pillar
            } else if is_high_momentum && is_low_synergy {
                AgentArchetype::LoneWolf
            } else if is_low_momentum && is_high_synergy {
                AgentArchetype::SocialLoafer
            } else if is_low_momentum && is_low_synergy {
                AgentArchetype::Rookie
            } else {
                AgentArchetype::Steady
            };

            archetypes.insert(agent, archetype);
        }

        archetypes
    }

    /// Generate a Markdown report of the hive's social dynamics.
    pub fn generate_markdown_report(&self) -> String {
        let archetypes = self.analyze();

        // Group agents by archetype
        let mut by_archetype: HashMap<AgentArchetype, Vec<AgentId>> = HashMap::new();
        for (agent, archetype) in archetypes {
            by_archetype.entry(archetype).or_default().push(agent);
        }

        // We must sort AgentIds for deterministic output since AgentId does not implement Ord
        for agents in by_archetype.values_mut() {
            agents.sort_by_key(|a| a.as_uuid().to_string());
        }

        let mut report = String::from("# Hive Social Dynamics Report\n\n");

        let order = [
            (
                AgentArchetype::Pillar,
                "🏛️ Pillars (High Momentum, High Synergy)",
            ),
            (
                AgentArchetype::LoneWolf,
                "🐺 Lone Wolves (High Momentum, Low Synergy)",
            ),
            (
                AgentArchetype::SocialLoafer,
                "🛋️ Social Loafers (Low Momentum, High Synergy)",
            ),
            (
                AgentArchetype::Rookie,
                "🌱 Rookies (Low Momentum, Low Synergy)",
            ),
            (
                AgentArchetype::Steady,
                "⚖️ Steady (Medium Momentum/Synergy)",
            ),
        ];

        for (archetype, title) in order {
            report.push_str(&format!("## {}\n", title));
            if let Some(agents) = by_archetype.get(&archetype) {
                if agents.is_empty() {
                    report.push_str("*None*\n\n");
                } else {
                    for agent in agents {
                        let m_score = self.momentum.agent_momentum(*agent);

                        // Calculate synergy for report
                        let mut s_score = 0.0;
                        for edge in self.synergy.edges() {
                            if edge.agent_a == *agent || edge.agent_b == *agent {
                                s_score += edge.weight;
                            }
                        }

                        report.push_str(&format!(
                            "- `{}` (Momentum: {:.1}, Synergy: {:.1})\n",
                            agent.as_uuid(),
                            m_score,
                            s_score
                        ));
                    }
                    report.push('\n');
                }
            } else {
                report.push_str("*None*\n\n");
            }
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_persistence::{TrajectoryEvent, TrajectoryEventId, TriggerKind};

    // Helper to create mock events for testing
    fn make_event(agent_id: AgentId, success: bool, task_id: Option<String>) -> TrajectoryEvent {
        let id = TrajectoryEventId::new();
        let task_uuid =
            task_id.map(|t| uuid::Uuid::parse_str(&t).unwrap_or_else(|_| uuid::Uuid::new_v4()));

        let json = serde_json::json!({
            "id": id,
            "trigger_kind": TriggerKind::TaskComplete,
            "agent_id": agent_id,
            "task_id": task_uuid,
            "success": success,
            "summary": "did work",
            "steps": [],
            "created_at": Utc::now()
        });

        serde_json::from_value(json).expect("Failed to create mock event")
    }

    #[test]
    fn test_archetype_classification() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let a_pillar = AgentId::new();
        let a_lonewolf = AgentId::new();
        let a_loafer = AgentId::new();
        let a_rookie = AgentId::new();
        let a_steady = AgentId::new();

        let t1 = uuid::Uuid::new_v4().to_string();
        let t2 = uuid::Uuid::new_v4().to_string();
        let t3 = uuid::Uuid::new_v4().to_string();

        // Pillar: 3 wins (momentum = 3.0), shares 2 tasks
        let mut events = vec![
            make_event(a_pillar, true, Some(t1.clone())),
            make_event(a_pillar, true, Some(t2.clone())),
            make_event(a_pillar, true, None),
        ];

        // LoneWolf: 3 wins (momentum = 3.0), no shared tasks
        events.extend(vec![
            make_event(a_lonewolf, true, None),
            make_event(a_lonewolf, true, None),
            make_event(a_lonewolf, true, None),
        ]);

        // SocialLoafer: 1 loss (momentum = -1.0), shares 2 tasks
        events.extend(vec![
            make_event(a_loafer, false, Some(t1.clone())),
            make_event(a_loafer, false, Some(t2.clone())),
            make_event(a_loafer, true, None), // momentum now -1
        ]);

        // Rookie: 2 losses (momentum = -2.0), no shared tasks
        events.extend(vec![
            make_event(a_rookie, false, None),
            make_event(a_rookie, false, None),
        ]);

        // Steady: 1 win, 1 loss (momentum = 0.0), shares 1 task
        events.extend(vec![
            make_event(a_steady, true, Some(t3.clone())),
            make_event(a_steady, false, None),
        ]);

        // Need another agent for steady to share task with, let's use pillar
        events.push(make_event(a_pillar, true, Some(t3.clone()))); // Pillar momentum now 4.0

        momentum.process_events(&events);
        synergy.build_from_events(&events);

        let analyzer = ArchetypeAnalyzer::new(&momentum, &synergy);
        let archetypes = analyzer.analyze();

        assert_eq!(archetypes.get(&a_pillar), Some(&AgentArchetype::Pillar));
        assert_eq!(archetypes.get(&a_lonewolf), Some(&AgentArchetype::LoneWolf));
        assert_eq!(
            archetypes.get(&a_loafer),
            Some(&AgentArchetype::SocialLoafer)
        );
        assert_eq!(archetypes.get(&a_rookie), Some(&AgentArchetype::Rookie));
        assert_eq!(archetypes.get(&a_steady), Some(&AgentArchetype::Steady));
    }

    #[test]
    fn test_markdown_report_generation() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let a1 = AgentId::new();

        let events = vec![make_event(a1, true, None), make_event(a1, true, None)];

        momentum.process_events(&events);
        synergy.build_from_events(&events);

        let analyzer = ArchetypeAnalyzer::new(&momentum, &synergy);
        let report = analyzer.generate_markdown_report();

        assert!(report.contains("# Hive Social Dynamics Report"));
        assert!(report.contains("Lone Wolves"));
        assert!(report.contains(&a1.as_uuid().to_string()));
    }
}
