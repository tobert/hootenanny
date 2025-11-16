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

    #[tool(description = "Transform an intention into sound - the Alchemical transmutation of emotion to music")]
    fn play(
        &self,
        Parameters(intention): Parameters<Intention>,
    ) -> Result<CallToolResult, McpError> {
        let sound = intention.realize();

        let result = serde_json::json!({
            "pitch": sound.pitch,
            "velocity": sound.velocity,
            "duration_ms": sound.duration_ms,
            "emotion": {
                "valence": sound.emotion.valence,
                "arousal": sound.emotion.arousal,
                "agency": sound.emotion.agency,
            }
        });

        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }
}

#[tool_handler]
impl ServerHandler for EventDualityServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Event Duality MCP Server - Musical Alchemy in Action\n\n\
                Transform abstract intentions into concrete sounds using the Alchemical Codex.\n\n\
                The 'play' tool accepts:\n\
                - what: Note name (C, D, E, F, G, A, B)\n\
                - how: Feeling word (softly, normally, boldly, questioning)\n\
                - emotion: EmotionalVector with valence (-1 to 1), arousal (0 to 1), agency (-1 to 1)\n\n\
                Returns Sound with:\n\
                - pitch: MIDI note number\n\
                - velocity: MIDI velocity (mapped from emotion)\n\
                - duration_ms: Note duration (mapped from arousal)\n\
                - emotion: The emotional vector that birthed this sound"
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
