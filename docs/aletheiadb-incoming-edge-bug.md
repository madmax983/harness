# AletheiaDB Bug: `get_incoming_edges` misses edges when multiple edges target the same node

## Summary

When multiple edges from different source nodes target the same destination node, `get_incoming_edges(target)` may not return all of them. This was discovered while implementing a graph-backed repository for the Harness project, where subtasks link to their parent via `SUBTASK_OF` edges.

## Observed Behavior

Creating 2+ edges that all point to the same target node, then calling `get_incoming_edges(target)` returns fewer edges than expected. Specifically, when child1 and child2 both have a `SUBTASK_OF` edge pointing at `parent`, only one edge is returned.

The `get_outgoing_edges` API does NOT have this problem — outgoing edges from the same source are tracked correctly.

## Reproduction

This standalone test should reproduce the issue. Place it in the AletheiaDB test suite (e.g., `tests/incoming_edge_bug.rs`):

```rust
use aletheiadb::{AletheiaDB, ReadOps, WriteOps};
use aletheiadb::core::property::PropertyMapBuilder;

#[test]
fn incoming_edges_multiple_sources_same_target() {
    let db = AletheiaDB::new();
    let empty_props = || PropertyMapBuilder::new().build();

    // Create 3 nodes: parent, child1, child2
    let parent = db.write(|tx| Ok::<_, aletheiadb::Error>(tx.create_node("Task", empty_props())?)).unwrap();
    let child1 = db.write(|tx| Ok::<_, aletheiadb::Error>(tx.create_node("Task", empty_props())?)).unwrap();
    let child2 = db.write(|tx| Ok::<_, aletheiadb::Error>(tx.create_node("Task", empty_props())?)).unwrap();

    // Create 2 edges pointing AT the same target (parent)
    // child1 --SUBTASK_OF--> parent
    // child2 --SUBTASK_OF--> parent
    let edge1 = db.write(|tx| {
        Ok::<_, aletheiadb::Error>(tx.create_edge(child1, parent, "SUBTASK_OF", empty_props())?)
    }).unwrap();

    let edge2 = db.write(|tx| {
        Ok::<_, aletheiadb::Error>(tx.create_edge(child2, parent, "SUBTASK_OF", empty_props())?)
    }).unwrap();

    // Verify outgoing edges work (these pass)
    let child1_out = db.read(|tx| Ok::<_, aletheiadb::Error>(tx.get_outgoing_edges(child1))).unwrap();
    assert_eq!(child1_out.len(), 1, "child1 should have 1 outgoing edge");

    let child2_out = db.read(|tx| Ok::<_, aletheiadb::Error>(tx.get_outgoing_edges(child2))).unwrap();
    assert_eq!(child2_out.len(), 1, "child2 should have 1 outgoing edge");

    // BUG: This should return 2 edges but may only return 1
    let parent_in = db.read(|tx| Ok::<_, aletheiadb::Error>(tx.get_incoming_edges(parent))).unwrap();

    assert!(parent_in.contains(&edge1), "should contain edge from child1");
    assert!(parent_in.contains(&edge2), "should contain edge from child2");
    assert_eq!(
        parent_in.len(), 2,
        "parent should have 2 incoming edges, but got {}. Edge IDs: {:?}",
        parent_in.len(), parent_in
    );
}

/// Variant: all edges created in a single transaction (may or may not trigger the same bug)
#[test]
fn incoming_edges_multiple_sources_single_transaction() {
    let db = AletheiaDB::new();
    let empty_props = || PropertyMapBuilder::new().build();

    let (parent, child1, child2, edge1, edge2) = db.write(|tx| {
        let parent = tx.create_node("Task", empty_props())?;
        let child1 = tx.create_node("Task", empty_props())?;
        let child2 = tx.create_node("Task", empty_props())?;
        let e1 = tx.create_edge(child1, parent, "SUBTASK_OF", empty_props())?;
        let e2 = tx.create_edge(child2, parent, "SUBTASK_OF", empty_props())?;
        Ok::<_, aletheiadb::Error>((parent, child1, child2, e1, e2))
    }).unwrap();

    let parent_in = db.read(|tx| Ok::<_, aletheiadb::Error>(tx.get_incoming_edges(parent))).unwrap();
    assert_eq!(parent_in.len(), 2, "parent should have 2 incoming SUBTASK_OF edges");
}

/// Variant: 3+ edges to the same target, different labels
#[test]
fn incoming_edges_fan_in_pattern() {
    let db = AletheiaDB::new();
    let empty_props = || PropertyMapBuilder::new().build();

    let target = db.write(|tx| Ok::<_, aletheiadb::Error>(tx.create_node("Hub", empty_props())?)).unwrap();

    let mut edge_ids = Vec::new();
    for i in 0..5 {
        let source = db.write(|tx| Ok::<_, aletheiadb::Error>(tx.create_node("Spoke", empty_props())?)).unwrap();
        let eid = db.write(|tx| {
            Ok::<_, aletheiadb::Error>(tx.create_edge(source, target, "CONNECTS_TO", empty_props())?)
        }).unwrap();
        edge_ids.push(eid);
    }

    let incoming = db.read(|tx| Ok::<_, aletheiadb::Error>(tx.get_incoming_edges(target))).unwrap();
    assert_eq!(
        incoming.len(), 5,
        "hub should have 5 incoming edges, got {}. IDs: {:?}",
        incoming.len(), incoming
    );
}
```

## Where to Look

The incoming adjacency index is implemented in `src/index/incremental_adjacency.rs`. The data structure looks correct — `DashMap<NodeId, SmallVec<[AdjacencyEntry; 8]>>` is multi-value. Possible causes:

1. **`DashMap::entry().or_default().push()` race with separate write transactions** — each `db.write()` call is a separate transaction. If the DashMap entry is being read/written concurrently by the compaction scheduler or other internal machinery, the `push` might be lost.

2. **Background compaction draining delta between writes** — the `CompactionScheduler` may compact between the two `db.write()` calls, draining the first delta entry into a frozen CSR and leaving only the second entry in delta.

3. **`WriteTransaction::create_edge`** — check how edges are buffered in the write transaction and how they're flushed to the `CurrentIndexes` on commit. The edge insert might go through a path that overwrites rather than appends.

## Workaround Used in Harness

Instead of graph traversal via `get_incoming_edges`, we list all tasks in the session and filter by their `parent_task` property:

```rust
async fn get_subtasks(&self, parent_id: TaskId) -> RepositoryResult<Vec<Task>> {
    let parent = self.get_task(parent_id).await?;
    let all_tasks = self.list_tasks(parent.session_id, None).await?;
    Ok(all_tasks
        .into_iter()
        .filter(|t| t.parent_task == Some(parent_id))
        .collect())
}
```

This is O(n) instead of O(k) but correct.

## Environment

- AletheiaDB: local build from `../../../gallifreydb`
- Rust edition 2024
- `AletheiaDB::new()` (in-memory mode)
- Windows 11
