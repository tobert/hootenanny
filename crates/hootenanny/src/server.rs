use crate::domain::Intention;
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::Parameters,
    },
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};

#[derive(Debug, Clone)]
pub struct EventDualityServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl EventDualityServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Transform an intention into sound - the core magic of event duality")]
    fn play(&self, Parameters(intention): Parameters<Intention>) -> Result<CallToolResult, McpError> {
        let sound = intention.realize();

        let result = serde_json::json!({
            "pitch": sound.pitch,
            "velocity": sound.velocity,
        });

        Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
    }
}

#[tool_handler]
impl ServerHandler for EventDualityServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Event Duality: Where intentions become sounds. Send an Intention with 'what' (note name) and 'how' (feeling), receive a Sound with pitch and velocity.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
