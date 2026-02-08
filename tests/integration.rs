//! Integration tests for Harness v2 Hive Mind.
//!
//! Tests end-to-end MCP handler workflows using InMemoryRepository.

use std::sync::Arc;

use harness_mcp::{HiveHandler, HiveState};
use harness_persistence::{InMemoryRepository, Repository, Session};

async fn setup_hive() -> (Arc<HiveState<InMemoryRepository>>, HiveHandler<InMemoryRepository>) {
    let repo = Arc::new(InMemoryRepository::new());
    let session = Session::new(8);
    repo.create_session(&session).await.unwrap();

    let state = Arc::new(HiveState::new(session, repo));
    let handler = HiveHandler::new(state.clone());
    (state, handler)
}

/// Test 1: Full coordination flow - register agents, create tasks, claim, share knowledge, complete.
#[tokio::test]
async fn test_full_coordination_flow() {
    let (_state, handler) = setup_hive().await;

    // Register strategoi
    let strategoi_resp = handler
        .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
        .await
        .expect("register strategoi");
    let strategoi_id: String =
        serde_json::from_value(strategoi_resp.get("agent_id").unwrap().clone()).unwrap();

    // Register two workers
    let dev_handler = HiveHandler::new(handler.state_ref().clone());
    let dev_resp = dev_handler
        .call_tool("register_agent", serde_json::json!({"role": "developer"}))
        .await
        .expect("register developer");
    let dev_id: String =
        serde_json::from_value(dev_resp.get("agent_id").unwrap().clone()).unwrap();

    let tester_handler = HiveHandler::new(handler.state_ref().clone());
    let tester_resp = tester_handler
        .call_tool("register_agent", serde_json::json!({"role": "tester"}))
        .await
        .expect("register tester");

    // Strategoi creates a task
    let task_resp = handler
        .call_tool(
            "create_task",
            serde_json::json!({
                "title": "Implement authentication",
                "description": "Add JWT auth to API",
                "priority": "high"
            }),
        )
        .await
        .expect("create task");
    let task_id: String =
        serde_json::from_value(task_resp.get("task_id").unwrap().clone()).unwrap();

    // Developer claims the task
    let claim_resp = dev_handler
        .call_tool("claim_task", serde_json::json!({"task_id": task_id}))
        .await
        .expect("claim task");
    assert!(claim_resp.get("success").unwrap().as_bool().unwrap());

    // Developer shares knowledge
    let knowledge_resp = dev_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Using jsonwebtoken crate for JWT",
                "kind": "decision",
                "task_id": task_id
            }),
        )
        .await
        .expect("share knowledge");
    assert!(knowledge_resp.get("knowledge_id").is_some());

    // Update task to in_progress
    dev_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": task_id,
                "status": "in_progress"
            }),
        )
        .await
        .expect("update status");

    // Complete task
    dev_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": task_id,
                "status": "completed",
                "summary": "JWT auth implemented and tested"
            }),
        )
        .await
        .expect("complete task");

    // Verify hive status
    let status_resp = handler
        .call_tool("get_hive_status", serde_json::json!({}))
        .await
        .expect("get hive status");

    let agents = status_resp.get("agents").unwrap().as_array().unwrap();
    assert_eq!(agents.len(), 3);

    let task_summary = status_resp.get("task_summary").unwrap();
    assert_eq!(task_summary.get("completed").unwrap().as_u64().unwrap(), 1);
    assert_eq!(task_summary.get("pending").unwrap().as_u64().unwrap(), 0);
}

/// Test 2: Concurrent task claims - only one agent should succeed.
#[tokio::test]
async fn test_concurrent_task_claims() {
    let (state, handler) = setup_hive().await;

    // Register strategoi and create task
    handler
        .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
        .await
        .unwrap();

    let task_resp = handler
        .call_tool(
            "create_task",
            serde_json::json!({
                "title": "Contested task",
                "description": "Multiple agents will try to claim this",
                "priority": "critical"
            }),
        )
        .await
        .unwrap();
    let task_id: String =
        serde_json::from_value(task_resp.get("task_id").unwrap().clone()).unwrap();

    // Spawn 5 agents, each trying to claim the task concurrently
    let mut handles = vec![];
    for i in 0..5 {
        let state_clone = state.clone();
        let task_id_clone = task_id.clone();

        let handle = tokio::spawn(async move {
            let agent_handler = HiveHandler::new(state_clone);
            agent_handler
                .call_tool("register_agent", serde_json::json!({"role": "developer"}))
                .await
                .unwrap();

            let claim_result = agent_handler
                .call_tool("claim_task", serde_json::json!({"task_id": task_id_clone}))
                .await
                .unwrap();

            (
                i,
                claim_result.get("success").unwrap().as_bool().unwrap(),
            )
        });
        handles.push(handle);
    }

    let results = futures::future::join_all(handles).await;
    let success_count = results
        .iter()
        .filter(|r| r.as_ref().unwrap().1)
        .count();

    // Exactly 1 should succeed, 4 should fail
    assert_eq!(success_count, 1, "Exactly one agent should claim the task");
}

/// Test 3: BMAD workflow - Strategoi assigns to BA, BA shares knowledge, PM reads it.
#[tokio::test]
async fn test_bmad_workflow() {
    let (_state, strategoi_handler) = setup_hive().await;

    // Register strategoi
    strategoi_handler
        .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
        .await
        .unwrap();

    // Register BA and PM
    let ba_handler = HiveHandler::new(strategoi_handler.state_ref().clone());
    let ba_resp = ba_handler
        .call_tool("register_agent", serde_json::json!({"role": "ba"}))
        .await
        .unwrap();
    let ba_id: String = serde_json::from_value(ba_resp.get("agent_id").unwrap().clone()).unwrap();

    let pm_handler = HiveHandler::new(strategoi_handler.state_ref().clone());
    pm_handler
        .call_tool("register_agent", serde_json::json!({"role": "pm"}))
        .await
        .unwrap();

    // Strategoi creates a task
    let task_resp = strategoi_handler
        .call_tool(
            "create_task",
            serde_json::json!({
                "title": "Gather requirements",
                "description": "Interview stakeholders for mobile app",
                "priority": "high"
            }),
        )
        .await
        .unwrap();
    let task_id: String =
        serde_json::from_value(task_resp.get("task_id").unwrap().clone()).unwrap();

    // Strategoi assigns task to BA
    strategoi_handler
        .call_tool(
            "assign_task",
            serde_json::json!({
                "task_id": task_id,
                "agent_id": ba_id
            }),
        )
        .await
        .unwrap();

    // BA shares knowledge
    ba_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Stakeholders need offline mode and dark theme",
                "kind": "discovery",
                "task_id": task_id
            }),
        )
        .await
        .unwrap();

    ba_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Timeline: 3 months for MVP",
                "kind": "decision",
                "task_id": task_id
            }),
        )
        .await
        .unwrap();

    // PM queries hive knowledge (fallback to recent since no embeddings)
    let ask_resp = pm_handler
        .call_tool(
            "ask_hive",
            serde_json::json!({
                "query": "requirements",
                "limit": 5
            }),
        )
        .await
        .unwrap();

    let results = ask_resp.get("results").unwrap().as_array().unwrap();
    assert_eq!(results.len(), 2, "PM should see both knowledge entries");

    // Verify task context
    let context_resp = ba_handler
        .call_tool("get_task_context", serde_json::json!({"task_id": task_id}))
        .await
        .unwrap();

    let knowledge = context_resp.get("knowledge").unwrap().as_array().unwrap();
    assert_eq!(knowledge.len(), 2);
}

/// Test 4: Direct message thread - two agents exchange messages on a task.
#[tokio::test]
async fn test_direct_message_thread() {
    let (_state, handler) = setup_hive().await;

    // Register two agents
    let arch_handler = HiveHandler::new(handler.state_ref().clone());
    let arch_resp = arch_handler
        .call_tool("register_agent", serde_json::json!({"role": "architect"}))
        .await
        .unwrap();
    let arch_id: String =
        serde_json::from_value(arch_resp.get("agent_id").unwrap().clone()).unwrap();

    let dev_handler = HiveHandler::new(handler.state_ref().clone());
    let dev_resp = dev_handler
        .call_tool("register_agent", serde_json::json!({"role": "developer"}))
        .await
        .unwrap();
    let dev_id: String =
        serde_json::from_value(dev_resp.get("agent_id").unwrap().clone()).unwrap();

    // Create a task
    handler
        .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
        .await
        .unwrap();

    let task_resp = handler
        .call_tool(
            "create_task",
            serde_json::json!({
                "title": "Design database schema",
                "description": "Schema for user management",
                "priority": "high"
            }),
        )
        .await
        .unwrap();
    let task_id: String =
        serde_json::from_value(task_resp.get("task_id").unwrap().clone()).unwrap();

    // Architect sends DM to developer
    arch_handler
        .call_tool(
            "send_direct_message",
            serde_json::json!({
                "to_agent": dev_id,
                "content": "Should we use UUID or auto-increment for user IDs?",
                "task_id": task_id
            }),
        )
        .await
        .unwrap();

    // Developer replies
    dev_handler
        .call_tool(
            "send_direct_message",
            serde_json::json!({
                "to_agent": arch_id,
                "content": "UUID for distributed systems, auto-increment for simpler setup",
                "task_id": task_id
            }),
        )
        .await
        .unwrap();

    // Architect sends follow-up
    arch_handler
        .call_tool(
            "send_direct_message",
            serde_json::json!({
                "to_agent": dev_id,
                "content": "Let's go with UUID then",
                "task_id": task_id
            }),
        )
        .await
        .unwrap();

    // Get thread messages
    let thread_resp = arch_handler
        .call_tool(
            "get_thread_messages",
            serde_json::json!({
                "task_id": task_id,
                "limit": 10
            }),
        )
        .await
        .unwrap();

    let messages = thread_resp.get("messages").unwrap().as_array().unwrap();
    assert_eq!(messages.len(), 3);

    // Verify ordering (ascending by time)
    let first_content = messages[0]
        .get("content")
        .unwrap()
        .as_str()
        .unwrap();
    assert!(first_content.contains("UUID or auto-increment"));
}

/// Test 5: Hive status accuracy - verify counts match actual state.
#[tokio::test]
async fn test_hive_status_accuracy() {
    let (_state, handler) = setup_hive().await;

    // Register multiple agents
    handler
        .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
        .await
        .unwrap();

    for role in ["developer", "developer", "tester", "architect"] {
        let agent_handler = HiveHandler::new(handler.state_ref().clone());
        agent_handler
            .call_tool("register_agent", serde_json::json!({"role": role}))
            .await
            .unwrap();
    }

    // Create tasks in various statuses
    let task1 = handler
        .call_tool(
            "create_task",
            serde_json::json!({"title": "Task 1", "description": "Pending task", "priority": "low"}),
        )
        .await
        .unwrap();
    let task1_id: String =
        serde_json::from_value(task1.get("task_id").unwrap().clone()).unwrap();

    let task2 = handler
        .call_tool(
            "create_task",
            serde_json::json!({"title": "Task 2", "description": "In progress", "priority": "medium"}),
        )
        .await
        .unwrap();
    let task2_id: String =
        serde_json::from_value(task2.get("task_id").unwrap().clone()).unwrap();

    let task3 = handler
        .call_tool(
            "create_task",
            serde_json::json!({"title": "Task 3", "description": "Completed", "priority": "high"}),
        )
        .await
        .unwrap();
    let task3_id: String =
        serde_json::from_value(task3.get("task_id").unwrap().clone()).unwrap();

    // Update task statuses
    handler
        .call_tool(
            "update_task_status",
            serde_json::json!({"task_id": task2_id, "status": "in_progress"}),
        )
        .await
        .unwrap();

    handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": task3_id,
                "status": "completed",
                "summary": "Done"
            }),
        )
        .await
        .unwrap();

    // Share knowledge
    handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({"content": "Test knowledge 1", "kind": "activity"}),
        )
        .await
        .unwrap();

    handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({"content": "Test knowledge 2", "kind": "discovery"}),
        )
        .await
        .unwrap();

    // Get hive status
    let status = handler
        .call_tool("get_hive_status", serde_json::json!({}))
        .await
        .unwrap();

    // Verify agent count
    let agents = status.get("agents").unwrap().as_array().unwrap();
    assert_eq!(agents.len(), 5, "Should have 5 agents total");

    // Verify task summary
    let task_summary = status.get("task_summary").unwrap();
    assert_eq!(
        task_summary.get("pending").unwrap().as_u64().unwrap(),
        1,
        "1 pending task"
    );
    assert_eq!(
        task_summary.get("in_progress").unwrap().as_u64().unwrap(),
        1,
        "1 in-progress task"
    );
    assert_eq!(
        task_summary.get("completed").unwrap().as_u64().unwrap(),
        1,
        "1 completed task"
    );

    // Verify recent knowledge
    let recent_knowledge = status.get("recent_knowledge").unwrap().as_array().unwrap();
    assert_eq!(
        recent_knowledge.len(),
        2,
        "Should have 2 recent knowledge entries"
    );
}
