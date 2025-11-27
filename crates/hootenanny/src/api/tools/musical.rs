use crate::api::schema::{AddNodeRequest, ForkRequest, MergeRequest, PruneRequest, EvaluateRequest, GetContextRequest, BroadcastMessageRequest};
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::conversation::ForkReason;
use crate::domain::{EmotionalVector, Event, AbstractEvent, IntentionEvent, ConcreteEvent};
use rmcp::{ErrorData as McpError, model::{CallToolResult, Content}};
use tracing;

// Note: We are implementing these as methods on EventDualityServer in the new manual dispatch style

impl EventDualityServer {
    #[tracing::instrument(name = "mcp.tool.merge_branches", skip(self, _request))]
    pub async fn merge_branches(
        &self,
        _request: MergeRequest,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tracing::instrument(name = "mcp.tool.prune_branch", skip(self, _request))]
    pub async fn prune_branch(
        &self,
        _request: PruneRequest,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tracing::instrument(name = "mcp.tool.evaluate_branch", skip(self, _request))]
    pub async fn evaluate_branch(
        &self,
        _request: EvaluateRequest,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tracing::instrument(name = "mcp.tool.get_context", skip(self, _request))]
    pub async fn get_context(
        &self,
        _request: GetContextRequest,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tracing::instrument(name = "mcp.tool.subscribe_events", skip(self))]
    pub async fn subscribe_events(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tracing::instrument(name = "mcp.tool.broadcast_message", skip(self, _request))]
    pub async fn broadcast_message(
        &self,
        _request: BroadcastMessageRequest,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "not implemented".to_string(),
        )]))
    }

    #[tracing::instrument(
        name = "mcp.tool.play",
        skip(self, request),
        fields(
            music.note = %request.what,
            music.expression = %request.how,
            emotion.valence = request.valence,
            emotion.arousal = request.arousal,
            emotion.agency = request.agency,
            agent.id = %request.agent_id,
            sound.pitch = tracing::field::Empty,
            sound.velocity = tracing::field::Empty,
            sound.duration_ms = tracing::field::Empty,
        )
    )]
    pub async fn play(
        &self,
        request: AddNodeRequest,
    ) -> Result<CallToolResult, McpError> {
        // Reuse the flattened AddNodeRequest structure for simplicity
        let intention = AbstractEvent::Intention(IntentionEvent {
            what: request.what.clone(),
            how: request.how.clone(),
            emotion: EmotionalVector {
                valence: request.valence,
                arousal: request.arousal,
                agency: request.agency,
            },
        });

        let sound = intention.realize();

        // Record sound output in span
        let span = tracing::Span::current();
        if let ConcreteEvent::Note(note_event) = &sound {
            span.record("sound.pitch", note_event.note.pitch.midi_note_number);
            span.record("sound.velocity", note_event.note.velocity.0);
            if let resonode::Duration::Absolute(duration) = &note_event.duration {
                span.record("sound.duration_ms", duration.0);
            }
        }

        let result_value = serde_json::to_value(&sound)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize sound: {}", e), None))?;

        // Store the sound event in CAS
        let sound_json = serde_json::to_string_pretty(&sound)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize sound for CAS: {}", e), None))?;

        // Use general_purpose encoding
        let sound_hash = self.local_models.store_cas_content(
            sound_json.as_bytes(),
            "application/json"
        ).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Create artifact with musical metadata
        let artifact_id = format!("artifact_{}", &sound_hash[..12]);
        let mut artifact = Artifact::new(
            &artifact_id,
            &request.agent_id,
            serde_json::json!({
                "hash": sound_hash,
                "intention": {
                    "what": request.what,
                    "how": request.how,
                },
                "emotion": {
                    "valence": request.valence,
                    "arousal": request.arousal,
                    "agency": request.agency,
                },
                "description": request.description,
            })
        )
        .with_tags(vec![
            "type:musical_event",
            "phase:realization",
            "tool:play"
        ]);

        // Add variation set info if provided
        if let Some(set_id) = request.variation_set_id {
            let index = self.artifact_store.next_variation_index(&set_id)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            artifact = artifact.with_variation_set(&set_id, index);
        }

        // Add parent if provided
        if let Some(parent_id) = request.parent_id {
            artifact = artifact.with_parent(&parent_id);
        }

        // Add custom tags
        artifact = artifact.with_tags(request.tags);

        // Store artifact
        self.artifact_store.put(artifact.clone())
            .map_err(|e| McpError::internal_error(format!("Failed to store artifact: {}", e), None))?;
        self.artifact_store.flush()
            .map_err(|e| McpError::internal_error(format!("Failed to flush artifact store: {}", e), None))?;

        // Include artifact_id in response
        let response = serde_json::json!({
            "sound": result_value,
            "artifact_id": artifact.id,
            "cas_hash": sound_hash,
        });

        Ok(CallToolResult::success(vec![Content::text(
            response.to_string(),
        )]))
    }

    #[tracing::instrument(
        name = "mcp.tool.add_node",
        skip(self, request),
        fields(
            conversation.branch_id = tracing::field::Empty,
            conversation.node_id = tracing::field::Empty,
            music.note = %request.what,
            music.expression = %request.how,
            emotion.valence = request.valence,
            emotion.arousal = request.arousal,
            emotion.agency = request.agency,
            agent.id = %request.agent_id,
            has_description = request.description.is_some(),
            tree.nodes_before = tracing::field::Empty,
            tree.nodes_after = tracing::field::Empty,
        )
    )]
    pub async fn add_node(
        &self,
        request: AddNodeRequest,
    ) -> Result<CallToolResult, McpError> {
        // Scope the mutex to ensure it's dropped before async operations
        let (node_id, branch_id, total_nodes, intention) = {
            let mut state = self.state.lock().unwrap();

            let branch_id = request.branch_id.clone().unwrap_or_else(|| state.current_branch.clone());

            let nodes_before = state.tree.nodes.len();

            // Record branch resolution
            let span = tracing::Span::current();
            span.record("conversation.branch_id", &*branch_id);
            span.record("tree.nodes_before", nodes_before);

            // Construct AbstractEvent from flattened parameters
            let intention = AbstractEvent::Intention(IntentionEvent {
                what: request.what.clone(),
                how: request.how.clone(),
                emotion: EmotionalVector {
                    valence: request.valence,
                    arousal: request.arousal,
                    agency: request.agency,
                },
            });

            let event = Event::Abstract(intention.clone());

            let node_id = state
                .tree
                .add_node(
                    &branch_id,
                    event,
                    request.agent_id.clone(),
                    EmotionalVector::neutral(), // Use intention's emotion
                    request.description.clone(),
                )
                .map_err(|e| McpError::parse_error(e, None))?;

            // Record node creation
            span.record("conversation.node_id", node_id);
            span.record("tree.nodes_after", state.tree.nodes.len());

            // Save to persistence
            state.save().map_err(|e| McpError::parse_error(e.to_string(), None))?;

            let total_nodes = state.tree.nodes.len();

            // MutexGuard dropped here at end of scope
            (node_id, branch_id, total_nodes, intention)
        };

        // Store the intention in CAS
        let intention_json = serde_json::to_string_pretty(&intention)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize intention: {}", e), None))?;

        let intention_hash = self.local_models.store_cas_content(
            intention_json.as_bytes(),
            "application/json"
        ).await.map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Create artifact with conversation context
        let artifact_id = format!("artifact_{}", &intention_hash[..12]);
        let mut artifact = Artifact::new(
            &artifact_id,
            &request.agent_id,
            serde_json::json!({
                "hash": intention_hash,
                "node_id": node_id,
                "branch_id": branch_id,
                "intention": {
                    "what": request.what,
                    "how": request.how,
                },
                "emotion": {
                    "valence": request.valence,
                    "arousal": request.arousal,
                    "agency": request.agency,
                },
                "description": request.description,
            })
        )
        .with_tags(vec![
            "type:intention",
            "phase:contribution",
            "tool:add_node"
        ]);

        // Add variation set info if provided
        if let Some(set_id) = request.variation_set_id {
            let index = self.artifact_store.next_variation_index(&set_id)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            artifact = artifact.with_variation_set(&set_id, index);
        }

        // Add parent if provided
        if let Some(parent_id) = request.parent_id {
            artifact = artifact.with_parent(&parent_id);
        }

        // Add custom tags
        artifact = artifact.with_tags(request.tags);

        // Store artifact
        self.artifact_store.put(artifact.clone())
            .map_err(|e| McpError::internal_error(format!("Failed to store artifact: {}", e), None))?;
        self.artifact_store.flush()
            .map_err(|e| McpError::internal_error(format!("Failed to flush artifact store: {}", e), None))?;

        let result = serde_json::json!({
            "node_id": node_id,
            "branch_id": branch_id,
            "total_nodes": total_nodes,
            "artifact_id": artifact.id,
            "cas_hash": intention_hash,
        });

        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }

    #[tracing::instrument(
        name = "mcp.tool.fork_branch",
        skip(self, request),
        fields(
            conversation.branch_name = %request.branch_name,
            conversation.from_node = tracing::field::Empty,
            conversation.new_branch_id = tracing::field::Empty,
            fork.reason = %request.reason_description,
            fork.participant_count = request.participants.len(),
            fork.participants = ?request.participants,
            tree.branches_before = tracing::field::Empty,
            tree.branches_after = tracing::field::Empty,
        )
    )]
    pub async fn fork_branch(
        &self,
        request: ForkRequest,
    ) -> Result<CallToolResult, McpError> {
        let mut state = self.state.lock().unwrap();

        let from_node = request.from_node.unwrap_or_else(|| {
            state
                .tree
                .branches
                .get(&state.current_branch)
                .map(|b| b.head)
                .unwrap_or(0)
        });

        let branches_before = state.tree.branches.len();

        // Record fork point resolution
        let span = tracing::Span::current();
        span.record("conversation.from_node", from_node);
        span.record("tree.branches_before", branches_before);

        // Fork the tree in memory
        let branch_id = state
            .tree
            .fork_branch(
                from_node,
                request.branch_name.clone(),
                ForkReason::ExploreAlternative {
                    description: request.reason_description.clone(),
                },
                request.participants.clone(),
            )
            .map_err(|e| McpError::parse_error(e, None))?;

        // Record branch creation
        span.record("conversation.new_branch_id", &*branch_id);
        span.record("tree.branches_after", state.tree.branches.len());

        // Persist the entire updated tree
        state.save().map_err(|e| McpError::parse_error(e.to_string(), None))?;

        let result = serde_json::json!({
            "branch_id": branch_id,
            "total_branches": state.tree.branches.len(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }

    #[tracing::instrument(
        name = "mcp.tool.get_tree_status",
        skip(self),
        fields(
            tree.total_nodes = tracing::field::Empty,
            tree.total_branches = tracing::field::Empty,
            tree.current_branch = tracing::field::Empty,
        )
    )]
    pub async fn get_tree_status(&self) -> Result<CallToolResult, McpError> {
        let state = self.state.lock().unwrap();

        // Record tree statistics
        let span = tracing::Span::current();
        span.record("tree.total_nodes", state.tree.nodes.len());
        span.record("tree.total_branches", state.tree.branches.len());
        span.record("tree.current_branch", &*state.current_branch);

        let result = serde_json::json!({
            "total_nodes": state.tree.nodes.len(),
            "total_branches": state.tree.branches.len(),
            "current_branch": state.current_branch,
        });

        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }
}
