# Task 01: Conversation Tree Resources

**Goal**: Expose the conversation tree structure so agents can reason about musical history, branch decisions, and emotional trajectories.

## Current State

```rust
// handler.rs - session://tree returns only:
{
    "current_branch": "main",
    "node_count": 42,
    "root": 0
}
```

This tells agents almost nothing about what happened.

## Proposed Resources

### Static Resources

| URI | Description |
|-----|-------------|
| `session://tree/structure` | Full tree with all branches and nodes |
| `session://tree/branches` | List of all branches with metadata |
| `session://tree/heads` | Current head nodes for all branches |

### Resource Templates

| URI Template | Description |
|--------------|-------------|
| `session://branch/{branch_id}` | Single branch with all nodes |
| `session://node/{node_id}` | Single node with parent/children |
| `session://lineage/{node_id}` | Path from root to node |
| `session://emotion/{branch_id}` | Emotional arc over a branch |

## Implementation

### 1. Add new resources to `fn resources()`

```rust
fn resources(&self) -> Vec<Resource> {
    vec![
        // ... existing ...
        Resource::new("session://tree/structure", "tree-structure")
            .with_description("Complete conversation tree with all branches and nodes")
            .with_mime_type("application/json"),
        Resource::new("session://tree/branches", "branches-list")
            .with_description("All branches with state, participants, fork reason")
            .with_mime_type("application/json"),
        Resource::new("session://tree/heads", "branch-heads")
            .with_description("Current head node for each branch")
            .with_mime_type("application/json"),
    ]
}
```

### 2. Add templates to `fn resource_templates()`

```rust
fn resource_templates(&self) -> Vec<ResourceTemplate> {
    vec![
        // ... existing ...
        ResourceTemplate::new("session://branch/{branch_id}", "branch-detail")
            .with_description("All nodes in a specific branch")
            .with_mime_type("application/json"),
        ResourceTemplate::new("session://node/{node_id}", "node-detail")
            .with_description("Single node with relationships")
            .with_mime_type("application/json"),
        ResourceTemplate::new("session://lineage/{node_id}", "node-lineage")
            .with_description("Path from root to this node")
            .with_mime_type("application/json"),
        ResourceTemplate::new("session://emotion/{branch_id}", "emotional-arc")
            .with_description("Emotional trajectory over a branch")
            .with_mime_type("application/json"),
    ]
}
```

### 3. Implement `read_session_resource()` cases

```rust
async fn read_session_resource(&self, path: &str) -> Result<ReadResourceResult, ErrorData> {
    let state = self.server.state.lock()
        .map_err(|e| ErrorData::internal_error(e.to_string()))?;

    match path {
        "tree" => { /* existing */ }

        "tree/structure" => {
            let result = serde_json::json!({
                "root": state.tree.root,
                "branches": state.tree.branches.values()
                    .map(|b| serde_json::json!({
                        "id": b.id,
                        "name": b.name,
                        "state": format!("{:?}", b.state),
                        "head": b.head,
                        "base": b.base,
                        "fork_reason": format!("{:?}", b.fork_reason),
                        "participants": b.participants,
                    }))
                    .collect::<Vec<_>>(),
                "node_count": state.tree.nodes.len(),
                "current_heads": state.tree.current_heads,
            });
            // ... return as ResourceContents
        }

        "tree/branches" => {
            let branches: Vec<_> = state.tree.branches.values()
                .map(|b| serde_json::json!({
                    "id": b.id,
                    "name": b.name,
                    "state": format!("{:?}", b.state),
                    "head": b.head,
                    "base": b.base,
                    "fork_reason": format!("{:?}", b.fork_reason),
                    "participants": b.participants,
                    "created_at": b.created_at,
                }))
                .collect();
            // ...
        }

        _ if path.starts_with("branch/") => {
            let branch_id: BranchId = path[7..].parse()
                .map_err(|_| ErrorData::invalid_params("Invalid branch ID"))?;
            let branch = state.tree.branches.get(&branch_id)
                .ok_or_else(|| ErrorData::invalid_params("Branch not found"))?;

            // Get all nodes in this branch
            let nodes: Vec<_> = state.tree.nodes.values()
                .filter(|n| n.branch_id == branch_id)
                .map(|n| node_to_json(n))
                .collect();
            // ...
        }

        _ if path.starts_with("node/") => {
            let node_id: NodeId = path[5..].parse()
                .map_err(|_| ErrorData::invalid_params("Invalid node ID"))?;
            let node = state.tree.nodes.get(&node_id)
                .ok_or_else(|| ErrorData::invalid_params("Node not found"))?;

            let result = serde_json::json!({
                "id": node.id,
                "parent": node.parent,
                "children": node.children,
                "branch_id": node.branch_id,
                "author": node.author,
                "timestamp": node.timestamp,
                "emotion": emotion_to_json(&node.emotion),
                "event": event_to_summary(&node.event),
                "description": node.description,
            });
            // ...
        }

        _ if path.starts_with("emotion/") => {
            let branch_id: BranchId = path[8..].parse()
                .map_err(|_| ErrorData::invalid_params("Invalid branch ID"))?;

            // Collect emotional vectors along the branch
            let emotions: Vec<_> = state.tree.nodes.values()
                .filter(|n| n.branch_id == branch_id)
                .map(|n| serde_json::json!({
                    "node_id": n.id,
                    "timestamp": n.timestamp,
                    "valence": n.emotion.valence,
                    "arousal": n.emotion.arousal,
                    "agency": n.emotion.agency,
                }))
                .collect();

            // Calculate trajectory stats
            let avg_valence = emotions.iter()
                .filter_map(|e| e["valence"].as_f64())
                .sum::<f64>() / emotions.len() as f64;
            // ... similar for arousal, agency

            let result = serde_json::json!({
                "branch_id": branch_id,
                "points": emotions,
                "summary": {
                    "avg_valence": avg_valence,
                    "avg_arousal": avg_arousal,
                    "avg_agency": avg_agency,
                    "trend": calculate_trend(&emotions),
                }
            });
            // ...
        }

        _ => Err(ErrorData::invalid_params("Unknown session resource"))
    }
}
```

### 4. Helper functions

```rust
fn node_to_json(node: &ConversationNode) -> serde_json::Value {
    serde_json::json!({
        "id": node.id,
        "parent": node.parent,
        "children": node.children,
        "branch_id": node.branch_id,
        "author": format!("{:?}", node.author),
        "timestamp": node.timestamp,
        "emotion": {
            "valence": node.emotion.valence,
            "arousal": node.emotion.arousal,
            "agency": node.emotion.agency,
        },
        "event_type": event_type_name(&node.event),
        "description": node.description,
    })
}

fn event_type_name(event: &Event) -> &'static str {
    match event {
        Event::Abstract(a) => match a {
            AbstractEvent::Prompt(_) => "prompt",
            AbstractEvent::Constraint(_) => "constraint",
            AbstractEvent::Orchestration(_) => "orchestration",
            AbstractEvent::Intention(_) => "intention",
        },
        Event::Concrete(c) => match c {
            ConcreteEvent::Note(_) => "note",
            ConcreteEvent::Chord(_) => "chord",
            ConcreteEvent::Control(_) => "control",
            ConcreteEvent::Pattern(_) => "pattern",
            ConcreteEvent::MidiClip(_) => "midi_clip",
        },
    }
}

fn emotion_to_json(e: &EmotionalVector) -> serde_json::Value {
    serde_json::json!({
        "valence": e.valence,
        "arousal": e.arousal,
        "agency": e.agency,
        "mood": describe_mood(e),
    })
}

fn describe_mood(e: &EmotionalVector) -> &'static str {
    // Simple mood quadrant
    match (e.valence > 0.0, e.arousal > 0.5) {
        (true, true) => "excited/joyful",
        (true, false) => "calm/content",
        (false, true) => "tense/anxious",
        (false, false) => "melancholic/peaceful",
    }
}
```

## Example Responses

### `session://tree/structure`
```json
{
  "root": 0,
  "branches": [
    {"id": 0, "name": "main", "state": "Active", "head": 15, "base": 0, "fork_reason": "Initial"},
    {"id": 1, "name": "ambient-exploration", "state": "Active", "head": 8, "base": 5, "fork_reason": "ExploreAlternative"}
  ],
  "node_count": 23,
  "current_heads": {"0": 15, "1": 8}
}
```

### `session://emotion/0`
```json
{
  "branch_id": 0,
  "points": [
    {"node_id": 0, "timestamp": 1234567890, "valence": 0.0, "arousal": 0.3, "agency": 0.5},
    {"node_id": 1, "timestamp": 1234567900, "valence": 0.2, "arousal": 0.4, "agency": 0.3},
    {"node_id": 5, "timestamp": 1234568000, "valence": -0.3, "arousal": 0.6, "agency": -0.2}
  ],
  "summary": {
    "avg_valence": -0.03,
    "avg_arousal": 0.43,
    "avg_agency": 0.2,
    "trend": "darkening"
  }
}
```

## Success Criteria

- [ ] All 4 resource templates implemented
- [ ] Emotional arc calculation works
- [ ] JSON output is agent-readable
- [ ] Tests verify resource responses
