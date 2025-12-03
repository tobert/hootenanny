//! baton - MCP (Model Context Protocol) Server Library
//!
//! A Rust implementation of the MCP 2025-06-18 specification for building
//! MCP servers with axum.
//!
//! # Features
//!
//! - **Tools**: Expose callable tools to MCP clients
//! - **Resources**: Serve content via URI-based resources
//! - **Prompts**: Provide prompt templates with arguments
//!
//! # Example
//!
//! ```rust,ignore
//! use baton::{Handler, Tool, CallToolResult, Content, Implementation};
//! use async_trait::async_trait;
//!
//! struct MyHandler;
//!
//! #[async_trait]
//! impl Handler for MyHandler {
//!     fn tools(&self) -> Vec<Tool> {
//!         vec![Tool::new("hello", "Say hello")]
//!     }
//!
//!     async fn call_tool(&self, name: &str, _args: serde_json::Value)
//!         -> Result<CallToolResult, baton::ErrorData>
//!     {
//!         Ok(CallToolResult::success(vec![Content::text("Hello!")]))
//!     }
//!
//!     fn server_info(&self) -> Implementation {
//!         Implementation::new("my-server", "0.1.0")
//!     }
//! }
//!
//! // Build router
//! let state = std::sync::Arc::new(baton::McpState::new(
//!     MyHandler,
//!     "my-server",
//!     "0.1.0",
//! ));
//! let router = baton::router(state);
//! ```

pub mod protocol;
pub mod session;
pub mod transport;
pub mod types;

// Re-export commonly used types at crate root
pub use types::content::Content;
pub use types::error::ErrorData;
pub use types::jsonrpc::{JsonRpcMessage, JsonRpcRequest, JsonRpcResponse, RequestId};
pub use types::protocol::{Implementation, ServerCapabilities};
pub use types::prompt::{Prompt, PromptArgument, PromptMessage};
pub use types::resource::{Resource, ResourceContents, ResourceTemplate};
pub use types::tool::{CallToolResult, Tool, ToolAnnotations, ToolSchema};

// Re-export session types
pub use session::{spawn_cleanup_task, InMemorySessionStore, Session, SessionStats, SessionStore};

// Re-export protocol types
pub use protocol::{Handler, Sampler, ToolContext};

// Re-export transport types
pub use transport::{router, streamable_router, dual_router, McpState};
