//! Test trajectory persistence across recorder instances.

use std::sync::Arc;

use harness_persistence::{
    Agent, AgentRole, AletheiaRepository, KnowledgeKind, LearningTrigger, Repository, Session,
    TrajectoryQuery, TrajectoryRecorder, TriggerKind,
};

#[tokio::test]
async fn test_trajectory_persistence_across_instances() {
    // Setup: create an AletheiaRepository (in-memory, not anonymous)
    let db = Arc::new(aletheiadb::AletheiaDB::new().expect("Failed to create DB"));
    let repo = Arc::new(AletheiaRepository::new_anon(db));
    let session = Session::new(8);
    repo.create_session(&session).await.unwrap();

    // Create an agent (required for trajectory events)
    let agent = Agent::new(AgentRole::Developer, session.id);
    let agent_id = agent.id;
    repo.create_agent(&agent).await.unwrap();

    // Record events with first recorder instance
    {
        let recorder = TrajectoryRecorder::new(repo.clone(), session.id);

        let trigger = LearningTrigger::new(TriggerKind::TaskComplete, agent_id)
            .with_success(true)
            .with_summary("Implemented rate limiting middleware");
        recorder.record(trigger).await.unwrap();

        let trigger = LearningTrigger::new(TriggerKind::KnowledgeShare, agent_id)
            .with_knowledge_kind(KnowledgeKind::Decision)
            .with_summary("Chose token bucket over leaky bucket for bursty traffic");
        recorder.record(trigger).await.unwrap();

        // Flush to ensure persistence
        recorder.flush().await.unwrap();
    }
    // First recorder dropped

    // Create a NEW recorder on the same repo (simulates restart)
    let recorder2 = TrajectoryRecorder::new(repo.clone(), session.id);

    // Restore persisted events
    let restored_count = recorder2.restore().await.unwrap();
    assert_eq!(restored_count, 2, "Should restore 2 events");

    // Events from the first recorder should be visible
    let events = recorder2
        .query(TrajectoryQuery::new().with_agent(agent_id).with_limit(10))
        .await
        .unwrap();

    assert_eq!(
        events.len(),
        2,
        "Both events should survive across recorder instances"
    );

    // Verify content survived
    let summaries: Vec<&str> = events.iter().map(|e| e.summary()).collect();
    assert!(
        summaries.iter().any(|s| s.contains("rate limiting")),
        "Rate limiting event should persist"
    );
    assert!(
        summaries.iter().any(|s| s.contains("token bucket")),
        "Token bucket decision should persist"
    );
}
