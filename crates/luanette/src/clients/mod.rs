//! Client management for upstream MCP servers.

mod manager;

pub use manager::{ClientManager, CachedUpstream, UpstreamConfig};

// Re-export baton client types for convenience
pub use baton::client::{ClientOptions, McpClient, ToolInfo};
