use async_trait::async_trait;
use baton::{
    CallToolResult, ErrorData, Handler, Implementation, Resource, ResourceTemplate, Tool,
    ToolContext,
};
use serde_json::Value;

use super::handler::HootHandler;
use llm_mcp_bridge::AgentChatHandler;

/// Composite handler that combines HootHandler and AgentChatHandler.
/// All tools and resources from both handlers are exposed through a single MCP endpoint.
pub struct CompositeHandler {
    hoot: HootHandler,
    agent: AgentChatHandler,
}

impl CompositeHandler {
    pub fn new(hoot: HootHandler, agent: AgentChatHandler) -> Self {
        Self { hoot, agent }
    }
}

#[async_trait]
impl Handler for CompositeHandler {
    fn tools(&self) -> Vec<Tool> {
        let mut tools = self.hoot.tools();
        tools.extend(self.agent.tools());
        tools
    }

    async fn call_tool(&self, name: &str, arguments: Value) -> Result<CallToolResult, ErrorData> {
        if name.starts_with("agent_chat_") {
            self.agent.call_tool(name, arguments).await
        } else {
            self.hoot.call_tool(name, arguments).await
        }
    }

    async fn call_tool_with_context(
        &self,
        name: &str,
        arguments: Value,
        context: ToolContext,
    ) -> Result<CallToolResult, ErrorData> {
        if name.starts_with("agent_chat_") {
            self.agent.call_tool_with_context(name, arguments, context).await
        } else {
            self.hoot.call_tool_with_context(name, arguments, context).await
        }
    }

    fn server_info(&self) -> Implementation {
        Implementation::new("hootenanny", env!("CARGO_PKG_VERSION"))
    }

    fn resources(&self) -> Vec<Resource> {
        let mut resources = self.hoot.resources();
        resources.extend(self.agent.resources());
        resources
    }

    fn resource_templates(&self) -> Vec<ResourceTemplate> {
        let mut templates = self.hoot.resource_templates();
        templates.extend(self.agent.resource_templates());
        templates
    }

    async fn read_resource(
        &self,
        uri: &str,
    ) -> Result<baton::types::resource::ReadResourceResult, ErrorData> {
        if uri.starts_with("conversations://") || uri.starts_with("sessions://") {
            self.agent.read_resource(uri).await
        } else {
            self.hoot.read_resource(uri).await
        }
    }

    fn instructions(&self) -> Option<String> {
        Some(
            r#"Hootenanny is an ensemble performance space for LLM agents and humans to create music together.

## Agent Chat Tools
Use agent_chat_* tools to spawn and manage LLM sub-agents:
- agent_chat_new: Create a session with a backend (e.g., "deepseek")
- agent_chat_send: Send a message (async, returns immediately)
- agent_chat_poll: Poll for response chunks and status
- agent_chat_backends: List available LLM backends

## Music Tools
Use orpheus_* tools to generate MIDI:
- orpheus_generate: Generate new MIDI from scratch
- orpheus_continue: Continue existing MIDI
- midi_to_wav: Render MIDI to audio with a SoundFont

## Storage
- cas_store: Store content by hash
- cas_inspect: Retrieve content by hash
- upload_file: Upload a file from disk"#
                .to_string(),
        )
    }
}
