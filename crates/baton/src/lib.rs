//! baton - MCP (Model Context Protocol) Library for Rust
//!
//! A Rust implementation of the MCP 2025-06-18 specification for building
//! MCP servers and clients with axum.
//!
//! # Features
//!
//! - **Server**: Build MCP servers with the `Handler` trait
//! - **Client**: Connect to MCP servers (enable `client` feature)
//! - **Tools**: Expose callable tools to MCP clients
//! - **Resources**: Serve content via URI-based resources
//! - **Prompts**: Provide prompt templates with arguments
//!
//! # Server Example
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
//!
//! # Client Example (requires `client` feature)
//!
//! ```rust,ignore
//! use baton::client::McpClient;
//!
//! let client = McpClient::new("http://localhost:8080/mcp");
//! client.initialize().await?;
//! let tools = client.list_tools().await?;
//! let result = client.call_tool("my_tool", json!({"key": "value"})).await?;
//! ```

pub mod protocol;
pub mod schema_helpers;
pub mod session;
pub mod transport;
pub mod types;

#[cfg(feature = "client")]
pub mod client;

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

// Re-export schema helpers
pub use schema_helpers::schema_for;
