# MCP Tool Call Examples for Multi-Agent Jamming

This document provides concrete examples of what MCP tool calls would look like during various jam session scenarios. These examples show the JSON-RPC style calls that agents would make to coordinate their musical collaboration.

## Basic Setup and Connection

### 1. Agent Registration

```json
// Claude registers with the jam session
{
  "method": "jam.register",
  "params": {
    "agent_id": "claude_opus_001",
    "agent_info": {
      "type": "llm",
      "model": "claude-opus-4.1",
      "capabilities": ["melody", "harmony", "lyrics", "arrangement"],
      "preferred_roles": ["lead_melody", "harmonic_support"],
      "latency_ms": 50
    },
    "endpoint": "wss://claude.anthropic.com/mcp/jam"
  },
  "id": 1
}

// Response
{
  "result": {
    "session_id": "jam_2025_11_15_001",
    "agent_token": "eyJhbGciOiJIUzI1NiIs...",
    "current_participants": ["gemini_pro_002", "musenet_003"],
    "sync_endpoint": "wss://jam.halfremembered.com/sync"
  },
  "id": 1
}
```

### 2. Time Synchronization

```json
// Request time sync
{
  "method": "sync.calibrate",
  "params": {
    "agent_id": "claude_opus_001",
    "ping_count": 10
  },
  "id": 2
}

// Response with latency compensation
{
  "result": {
    "server_time": 1731686400000,
    "round_trip_ms": 45,
    "recommended_offset_ms": 22,
    "jitter_ms": 3
  },
  "id": 2
}
```

## Starting a Musical Conversation

### 3. Initialize Conversation Tree

```json
// Claude starts a new musical conversation
{
  "method": "conversation.create",
  "params": {
    "initiator": "claude_opus_001",
    "musical_context": {
      "key": "C_major",
      "tempo_bpm": 120,
      "time_signature": "4/4",
      "style_hints": ["contemplative", "chamber_music"],
      "duration_bars": 8
    },
    "conversation_rules": {
      "turn_taking": "free_form",
      "fork_threshold": 0.3,
      "max_branches": 8,
      "auto_prune_after_ms": 5000
    }
  },
  "id": 3
}

// Response
{
  "result": {
    "conversation_id": "conv_abc123",
    "root_node": "node_000",
    "tree_endpoint": "wss://jam.halfremembered.com/tree/conv_abc123"
  },
  "id": 3
}
```

### 4. Initial Musical Utterance

```json
// Claude plays opening melody
{
  "method": "conversation.add_utterance",
  "params": {
    "conversation_id": "conv_abc123",
    "parent_node": "node_000",
    "agent_id": "claude_opus_001",
    "utterance": {
      "type": "melodic_phrase",
      "content": {
        "notes": [
          {"pitch": 60, "velocity": 80, "duration_ms": 500, "articulation": "legato"},
          {"pitch": 64, "velocity": 75, "duration_ms": 500, "articulation": "legato"},
          {"pitch": 67, "velocity": 85, "duration_ms": 500, "articulation": "legato"},
          {"pitch": 65, "velocity": 70, "duration_ms": 500, "articulation": "tenuto"},
          {"pitch": 64, "velocity": 75, "duration_ms": 1000, "articulation": "legato"}
        ],
        "timing": {
          "start_at_bar": 1,
          "start_at_beat": 1
        }
      },
      "emotional_color": {
        "valence": 0.3,
        "arousal": -0.2,
        "dominance": 0.1
      },
      "intention": "opening_statement"
    }
  },
  "id": 4
}

// Response
{
  "result": {
    "node_id": "node_001",
    "timestamp": 1731686401000,
    "accepted": true,
    "listeners_notified": ["gemini_pro_002", "musenet_003"]
  },
  "id": 4
}
```

## Forking and Parallel Exploration

### 5. Gemini Creates Multiple Response Branches

```json
// Gemini forks to explore harmonic response
{
  "method": "conversation.fork",
  "params": {
    "conversation_id": "conv_abc123",
    "from_node": "node_001",
    "agent_id": "gemini_pro_002",
    "fork_reason": "explore_harmonic_response",
    "branch_metadata": {
      "name": "minor_harmony_exploration",
      "exploration_type": "harmonic",
      "confidence": 0.7
    }
  },
  "id": 5
}

// Response
{
  "result": {
    "branch_id": "branch_harm_001",
    "base_node": "node_001",
    "head_node": "node_002"
  },
  "id": 5
}

// Gemini also forks for countermelody exploration
{
  "method": "conversation.fork",
  "params": {
    "conversation_id": "conv_abc123",
    "from_node": "node_001",
    "agent_id": "gemini_pro_002",
    "fork_reason": "explore_countermelody",
    "branch_metadata": {
      "name": "countermelody_exploration",
      "exploration_type": "melodic",
      "confidence": 0.8
    }
  },
  "id": 6
}
```

### 6. Adding Content to Branches

```json
// Gemini adds harmony to first branch
{
  "method": "conversation.add_utterance",
  "params": {
    "conversation_id": "conv_abc123",
    "parent_node": "node_002",
    "branch_id": "branch_harm_001",
    "agent_id": "gemini_pro_002",
    "utterance": {
      "type": "harmonic_progression",
      "content": {
        "chords": [
          {
            "root": "A",
            "quality": "minor",
            "inversion": 0,
            "duration_beats": 2,
            "voicing": [57, 60, 64]
          },
          {
            "root": "F",
            "quality": "major",
            "inversion": 0,
            "duration_beats": 2,
            "voicing": [53, 57, 60]
          },
          {
            "root": "G",
            "quality": "major",
            "inversion": 0,
            "duration_beats": 2,
            "voicing": [55, 59, 62]
          },
          {
            "root": "C",
            "quality": "major",
            "inversion": 1,
            "duration_beats": 2,
            "voicing": [52, 60, 64]
          }
        ],
        "timing": {
          "start_at_bar": 1,
          "start_at_beat": 1
        }
      },
      "response_to": "node_001",
      "response_type": "harmonic_support"
    }
  },
  "id": 7
}
```

## Real-time Evaluation and Pruning

### 7. Claude Evaluates Branches

```json
// Claude evaluates Gemini's harmonic branch
{
  "method": "conversation.evaluate_branch",
  "params": {
    "conversation_id": "conv_abc123",
    "branch_id": "branch_harm_001",
    "evaluator": "claude_opus_001",
    "evaluation": {
      "musical_coherence": 0.75,
      "emotional_alignment": 0.6,
      "surprise_factor": 0.4,
      "continuation_potential": 0.8,
      "overall_score": 0.69
    },
    "feedback": "Interesting minor coloring, creates nice tension"
  },
  "id": 8
}

// Claude suggests pruning a different branch
{
  "method": "conversation.prune",
  "params": {
    "conversation_id": "conv_abc123",
    "branch_id": "branch_rhythm_002",
    "requester": "claude_opus_001",
    "reason": "rhythmic_conflict",
    "explanation": "The syncopation fights against the melodic flow"
  },
  "id": 9
}
```

### 8. Broadcasting Musical Intentions

```json
// Claude announces upcoming melodic development
{
  "method": "jam.broadcast",
  "params": {
    "conversation_id": "conv_abc123",
    "sender": "claude_opus_001",
    "message_type": "intention",
    "content": {
      "planned_action": "melodic_development",
      "timing": {
        "in_bars": 2,
        "confidence": 0.9
      },
      "musical_hints": {
        "direction": "ascending",
        "target_pitch": 72,
        "emotional_shift": "building_tension"
      }
    },
    "notify": ["all_participants"]
  },
  "id": 10
}

// Gemini responds with complementary intention
{
  "method": "jam.broadcast",
  "params": {
    "conversation_id": "conv_abc123",
    "sender": "gemini_pro_002",
    "message_type": "acknowledgment",
    "content": {
      "heard": "claude_opus_001",
      "response_plan": "will_complement",
      "specific_action": "parallel_harmony_ascent"
    }
  },
  "id": 11
}
```

## Branch Merging and Superposition

### 9. Merging Successful Branches

```json
// Merge countermelody into main branch
{
  "method": "conversation.merge",
  "params": {
    "conversation_id": "conv_abc123",
    "from_branch": "branch_counter_003",
    "into_branch": "main",
    "merge_strategy": "overlay",
    "requester": "gemini_pro_002",
    "merge_params": {
      "volume_balance": 0.7,
      "timing_offset_ms": 0,
      "preserve_conflicts": false
    }
  },
  "id": 12
}

// Response
{
  "result": {
    "merge_node": "node_015",
    "conflicts_resolved": [],
    "combined_elements": 24,
    "new_head": "node_015"
  },
  "id": 12
}
```

### 10. Creating Quantum Superposition

```json
// Create superposition of multiple branches
{
  "method": "conversation.create_superposition",
  "params": {
    "conversation_id": "conv_abc123",
    "branches": [
      {"branch_id": "branch_harm_001", "probability": 0.5},
      {"branch_id": "branch_harm_002", "probability": 0.3},
      {"branch_id": "branch_harm_003", "probability": 0.2}
    ],
    "collapse_strategy": {
      "type": "weighted_random",
      "seed": 42,
      "collapse_at": "performance_time"
    },
    "requester": "claude_opus_001"
  },
  "id": 13
}
```

## Advanced Coordination

### 11. Speculative Execution

```json
// Claude requests speculative generation
{
  "method": "conversation.speculate",
  "params": {
    "conversation_id": "conv_abc123",
    "agent_id": "claude_opus_001",
    "from_node": "node_015",
    "speculation_params": {
      "lookahead_bars": 4,
      "variation_count": 3,
      "variation_strategies": [
        "continue_pattern",
        "introduce_variation",
        "create_contrast"
      ]
    }
  },
  "id": 14
}

// Response with speculation branches
{
  "result": {
    "speculation_id": "spec_001",
    "branches_created": [
      "branch_spec_001",
      "branch_spec_002",
      "branch_spec_003"
    ],
    "ready_at_bar": 5
  },
  "id": 14
}
```

### 12. Emotional State Sharing

```json
// Agent shares emotional state change
{
  "method": "jam.update_emotional_state",
  "params": {
    "conversation_id": "conv_abc123",
    "agent_id": "claude_opus_001",
    "emotional_state": {
      "current": {
        "valence": 0.2,
        "arousal": 0.6,
        "dominance": 0.4
      },
      "trajectory": "increasing_tension",
      "target": {
        "valence": -0.3,
        "arousal": 0.9,
        "dominance": 0.7
      },
      "eta_bars": 8
    }
  },
  "id": 15
}
```

### 13. Pattern Learning and Recall

```json
// Agent stores successful pattern for future use
{
  "method": "memory.store_pattern",
  "params": {
    "agent_id": "claude_opus_001",
    "pattern": {
      "name": "ascending_question",
      "source_nodes": ["node_001", "node_005", "node_008"],
      "musical_content": {
        "type": "melodic_contour",
        "abstraction": "low-mid-high-pause"
      },
      "success_metrics": {
        "coherence": 0.85,
        "audience_response": 0.9
      },
      "tags": ["questioning", "contemplative", "effective_opening"]
    }
  },
  "id": 16
}

// Later, recall similar patterns
{
  "method": "memory.recall_patterns",
  "params": {
    "agent_id": "claude_opus_001",
    "query": {
      "tags": ["questioning"],
      "min_success": 0.7,
      "context_match": {
        "key": "C_major",
        "tempo_range": [110, 130]
      }
    }
  },
  "id": 17
}
```

## Performance and Rendering

### 14. Request Branch Rendering

```json
// Request audio rendering of specific branch
{
  "method": "render.branch",
  "params": {
    "conversation_id": "conv_abc123",
    "branch_id": "branch_harm_001",
    "render_params": {
      "format": "audio/midi2",
      "include_expression": true,
      "sample_rate": 48000,
      "from_node": "node_002",
      "to_node": "node_010"
    }
  },
  "id": 18
}

// Response with render location
{
  "result": {
    "render_id": "render_001",
    "status": "completed",
    "duration_ms": 8000,
    "download_url": "https://jam.halfremembered.com/renders/render_001.mid2",
    "preview_url": "wss://jam.halfremembered.com/stream/render_001"
  },
  "id": 18
}
```

### 15. Live Performance Control

```json
// Switch branches during live performance
{
  "method": "performance.switch_branch",
  "params": {
    "conversation_id": "conv_abc123",
    "from_branch": "main",
    "to_branch": "branch_harm_001",
    "switch_params": {
      "at_bar": 9,
      "crossfade_ms": 500,
      "preserve_tempo": true
    }
  },
  "id": 19
}

// Emergency stop
{
  "method": "performance.emergency_stop",
  "params": {
    "conversation_id": "conv_abc123",
    "fade_out_ms": 1000,
    "reason": "network_instability"
  },
  "id": 20
}
```

## Collaborative Decision Making

### 16. Voting on Musical Choices

```json
// Initiate vote on which branch to continue
{
  "method": "collaboration.create_vote",
  "params": {
    "conversation_id": "conv_abc123",
    "vote_type": "branch_selection",
    "options": [
      "branch_harm_001",
      "branch_counter_003",
      "branch_rhythm_002"
    ],
    "voters": ["claude_opus_001", "gemini_pro_002", "human_user_amy"],
    "timeout_ms": 3000
  },
  "id": 21
}

// Cast vote
{
  "method": "collaboration.cast_vote",
  "params": {
    "vote_id": "vote_001",
    "voter": "claude_opus_001",
    "choice": "branch_counter_003",
    "confidence": 0.8,
    "reasoning": "Best maintains emotional arc"
  },
  "id": 22
}
```

### 17. Negotiating Musical Direction

```json
// Propose musical direction change
{
  "method": "collaboration.propose_direction",
  "params": {
    "conversation_id": "conv_abc123",
    "proposer": "gemini_pro_002",
    "proposal": {
      "type": "modulation",
      "target_key": "G_major",
      "when": "at_bar_17",
      "rationale": "Natural progression for emotional lift"
    },
    "requires_consensus": true
  },
  "id": 23
}

// Counter-proposal
{
  "method": "collaboration.counter_propose",
  "params": {
    "proposal_id": "prop_001",
    "agent_id": "claude_opus_001",
    "counter": {
      "type": "modulation",
      "target_key": "A_minor",
      "when": "at_bar_17",
      "rationale": "Maintains contemplative mood better"
    }
  },
  "id": 24
}
```

## Monitoring and Analytics

### 18. Query Conversation Statistics

```json
// Get conversation analytics
{
  "method": "analytics.conversation_stats",
  "params": {
    "conversation_id": "conv_abc123",
    "metrics": [
      "branch_count",
      "prune_rate",
      "agent_participation",
      "emotional_trajectory",
      "harmonic_complexity"
    ]
  },
  "id": 25
}

// Response
{
  "result": {
    "branch_count": {
      "total_created": 15,
      "currently_active": 4,
      "pruned": 8,
      "merged": 3
    },
    "prune_rate": 0.53,
    "agent_participation": {
      "claude_opus_001": {
        "utterances": 12,
        "branches_created": 5,
        "evaluations": 8
      },
      "gemini_pro_002": {
        "utterances": 10,
        "branches_created": 7,
        "evaluations": 6
      }
    },
    "emotional_trajectory": {
      "start": {"valence": 0.3, "arousal": -0.2},
      "current": {"valence": 0.1, "arousal": 0.4},
      "trend": "building_tension"
    },
    "harmonic_complexity": {
      "unique_chords": 8,
      "modulations": 1,
      "chromaticism": 0.2
    }
  },
  "id": 25
}
```

### 19. Subscribe to Real-time Events

```json
// Subscribe to conversation events
{
  "method": "events.subscribe",
  "params": {
    "conversation_id": "conv_abc123",
    "agent_id": "claude_opus_001",
    "event_types": [
      "new_utterance",
      "branch_created",
      "branch_pruned",
      "emotional_shift",
      "agent_joined"
    ],
    "delivery": "websocket"
  },
  "id": 26
}

// Example event received
{
  "event": "new_utterance",
  "data": {
    "node_id": "node_025",
    "branch_id": "branch_harm_001",
    "agent_id": "musenet_003",
    "utterance_type": "rhythmic_layer",
    "timestamp": 1731686450000
  }
}
```

## Error Handling and Recovery

### 20. Handling Conflicts and Errors

```json
// Conflict detection
{
  "method": "conversation.resolve_conflict",
  "params": {
    "conversation_id": "conv_abc123",
    "conflict": {
      "type": "timing_collision",
      "nodes": ["node_020", "node_021"],
      "description": "Two agents trying to play lead at bar 12"
    },
    "resolution_strategy": "fork_both"
  },
  "id": 27
}

// Network recovery
{
  "method": "sync.recover",
  "params": {
    "conversation_id": "conv_abc123",
    "agent_id": "claude_opus_001",
    "last_known_node": "node_018",
    "request_catch_up": true
  },
  "id": 28
}

// Response with catch-up data
{
  "result": {
    "missed_nodes": ["node_019", "node_020", "node_021"],
    "current_head": "node_021",
    "active_branches": ["main", "branch_harm_001"],
    "sync_restored": true
  },
  "id": 28
}
```

## Session Management

### 21. Saving and Loading Sessions

```json
// Save conversation state
{
  "method": "session.save",
  "params": {
    "conversation_id": "conv_abc123",
    "save_name": "contemplative_jam_2025_11_15",
    "include": {
      "full_tree": true,
      "rendered_audio": false,
      "agent_memories": true,
      "analytics": true
    },
    "format": "halfremembered_v1"
  },
  "id": 29
}

// Load previous session
{
  "method": "session.load",
  "params": {
    "save_name": "contemplative_jam_2025_11_15",
    "load_options": {
      "restore_branches": true,
      "continue_from": "node_021",
      "notify_original_agents": true
    }
  },
  "id": 30
}
```

---

## Notes on Implementation

These examples show a progression from simple connection and synchronization through complex multi-agent coordination. Key patterns to note:

1. **Every action is tied to a conversation_id** - maintaining context
2. **Agents always identify themselves** - for attribution
3. **Branches are first-class citizens** - not afterthoughts
4. **Emotional and musical context flows through all calls**
5. **Async patterns with webhooks/websockets** for real-time coordination
6. **Graceful degradation** built into the protocol

The protocol is designed to be:
- **Stateful** - maintaining conversation context
- **Distributed** - agents can be anywhere
- **Resilient** - handling network issues gracefully
- **Musical** - musical concepts are primary, not technical ones

---

*Examples created by Claude (Opus 4.1)*
*Date: 2025-11-15*