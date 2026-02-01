//! MCP Resources - Entry points into the Trustfall graph
//!
//! Resources provide grounding for agents to understand context. They're curated
//! views into session state, not exhaustive listings of all data.
//!
//! Design philosophy: Resources are entry points, not replacements for graph_query.

use rmcp::model::{AnnotateAble, RawResource, RawResourceTemplate, Resource, ResourceContents};
use std::fmt;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

use crate::backend::BackendPool;
use hooteproto::{request::ToolRequest, Payload};

/// Registry of available MCP resources.
pub struct ResourceRegistry {
    backends: Arc<RwLock<BackendPool>>,
}

impl ResourceRegistry {
    pub fn new(backends: Arc<RwLock<BackendPool>>) -> Self {
        Self { backends }
    }

    /// List all available static resources.
    pub fn list_resources() -> Vec<Resource> {
        vec![
            RawResource {
                uri: "holler://session/context".into(),
                name: "Session Context".into(),
                title: Some("Session Context".into()),
                description: Some("Current artifacts, identities, and session state".into()),
                mime_type: Some("application/json".into()),
                size: None,
                icons: None,
                meta: None,
            }
            .no_annotation(),
            RawResource {
                uri: "holler://artifacts/recent".into(),
                name: "Recent Artifacts".into(),
                title: Some("Recent Artifacts".into()),
                description: Some("Last 20 generated artifacts".into()),
                mime_type: Some("application/json".into()),
                size: None,
                icons: None,
                meta: None,
            }
            .no_annotation(),
            RawResource {
                uri: "holler://soundfonts".into(),
                name: "SoundFonts".into(),
                title: Some("Available SoundFonts".into()),
                description: Some("SoundFonts available for MIDI rendering".into()),
                mime_type: Some("application/json".into()),
                size: None,
                icons: None,
                meta: None,
            }
            .no_annotation(),
            RawResource {
                uri: "holler://status".into(),
                name: "System Status".into(),
                title: Some("System Status".into()),
                description: Some("Transport, audio, and MIDI subsystem status".into()),
                mime_type: Some("application/json".into()),
                size: None,
                icons: None,
                meta: None,
            }
            .no_annotation(),
            RawResource {
                uri: "holler://schema".into(),
                name: "Trustfall Schema".into(),
                title: Some("GraphQL Schema".into()),
                description: Some("Trustfall GraphQL schema for graph_query tool".into()),
                mime_type: Some("text/plain".into()),
                size: None,
                icons: None,
                meta: None,
            }
            .no_annotation(),
        ]
    }

    /// List available resource templates (parameterized resources).
    pub fn list_resource_templates() -> Vec<rmcp::model::ResourceTemplate> {
        vec![
            RawResourceTemplate {
                uri_template: "holler://artifact/{id}".into(),
                name: "Artifact".into(),
                title: Some("Single Artifact".into()),
                description: Some(
                    "Artifact details with parent/children refs. Replace {id} with artifact_xxx"
                        .into(),
                ),
                mime_type: Some("application/json".into()),
            }
            .no_annotation(),
            RawResourceTemplate {
                uri_template: "holler://soundfont/{hash}".into(),
                name: "SoundFont Details".into(),
                title: Some("SoundFont Details".into()),
                description: Some(
                    "SoundFont presets and metadata. Replace {hash} with CAS hash".into(),
                ),
                mime_type: Some("application/json".into()),
            }
            .no_annotation(),
        ]
    }

    /// Read a resource by URI.
    pub async fn read(&self, uri: &str) -> Result<String, ResourceError> {
        debug!(uri = %uri, "Reading resource");

        match uri {
            "holler://session/context" => self.read_session_context().await,
            "holler://artifacts/recent" => self.read_recent_artifacts().await,
            "holler://soundfonts" => self.read_soundfonts().await,
            "holler://status" => self.read_status().await,
            "holler://schema" => Ok(self.read_schema()),
            _ if uri.starts_with("holler://artifact/") => {
                let id = uri
                    .strip_prefix("holler://artifact/")
                    .ok_or_else(|| ResourceError::NotFound(uri.to_string()))?;
                self.read_artifact(id).await
            }
            _ if uri.starts_with("holler://soundfont/") => {
                let hash = uri
                    .strip_prefix("holler://soundfont/")
                    .ok_or_else(|| ResourceError::NotFound(uri.to_string()))?;
                self.read_soundfont(hash).await
            }
            _ => Err(ResourceError::NotFound(uri.to_string())),
        }
    }

    /// Delegate to graph_context tool for session context.
    async fn read_session_context(&self) -> Result<String, ResourceError> {
        let request = ToolRequest::GraphContext(hooteproto::request::GraphContextRequest {
            limit: Some(20),
            tag: None,
            creator: None,
            vibe_search: None,
            within_minutes: Some(60),
            include_annotations: true,
            include_metadata: false,
        });

        self.execute_tool(request).await
    }

    /// Query recent artifacts via Trustfall.
    async fn read_recent_artifacts(&self) -> Result<String, ResourceError> {
        let request = ToolRequest::GraphQuery(hooteproto::request::GraphQueryRequest {
            query: r#"{ Artifact { id @output creator @output tags @output } }"#.into(),
            limit: Some(20),
            variables: None,
        });

        self.execute_tool(request).await
    }

    /// List available soundfonts via artifact query.
    async fn read_soundfonts(&self) -> Result<String, ResourceError> {
        let request = ToolRequest::ArtifactList(hooteproto::request::ArtifactListRequest {
            tag: Some("type:soundfont".into()),
            creator: None,
            limit: Some(50),
        });

        self.execute_tool(request).await
    }

    /// Get system status.
    async fn read_status(&self) -> Result<String, ResourceError> {
        let request = ToolRequest::GardenStatus;
        self.execute_tool(request).await
    }

    /// Return the Trustfall GraphQL schema.
    fn read_schema(&self) -> String {
        // Embedded schema from audio-graph-mcp
        include_str!("../../audio-graph-mcp/src/schema.graphql").to_string()
    }

    /// Read a single artifact by ID with lineage info.
    async fn read_artifact(&self, id: &str) -> Result<String, ResourceError> {
        let request = ToolRequest::ArtifactGet(hooteproto::request::ArtifactGetRequest {
            id: id.to_string(),
        });

        self.execute_tool(request).await
    }

    /// Read soundfont details by hash.
    async fn read_soundfont(&self, hash: &str) -> Result<String, ResourceError> {
        let request =
            ToolRequest::SoundfontInspect(hooteproto::request::SoundfontInspectRequest {
                soundfont_hash: hash.to_string(),
                include_drum_map: false,
            });

        self.execute_tool(request).await
    }

    /// Execute a tool request against the backend.
    async fn execute_tool(&self, request: ToolRequest) -> Result<String, ResourceError> {
        let name = request.name();
        let backends = self.backends.read().await;
        let backend = backends
            .route_tool(name)
            .ok_or_else(|| ResourceError::BackendUnavailable(name.to_string()))?;

        let payload = Payload::ToolRequest(request);

        match backend.request(payload).await {
            Ok(Payload::TypedResponse(envelope)) => {
                let json = envelope.to_json();
                serde_json::to_string_pretty(&json)
                    .map_err(|e| ResourceError::Internal(format!("JSON serialization: {}", e)))
            }
            Ok(Payload::Error {
                code,
                message,
                details,
            }) => Err(ResourceError::ToolError {
                code,
                message,
                details,
            }),
            Ok(other) => Err(ResourceError::Internal(format!(
                "Unexpected response: {:?}",
                other
            ))),
            Err(e) => Err(ResourceError::BackendError(e.to_string())),
        }
    }
}

/// Resource reading errors.
#[derive(Debug)]
pub enum ResourceError {
    NotFound(String),
    BackendUnavailable(String),
    BackendError(String),
    ToolError {
        code: String,
        message: String,
        details: Option<serde_json::Value>,
    },
    Internal(String),
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceError::NotFound(uri) => write!(f, "Resource not found: {}", uri),
            ResourceError::BackendUnavailable(tool) => {
                write!(f, "Backend unavailable for tool: {}", tool)
            }
            ResourceError::BackendError(msg) => write!(f, "Backend error: {}", msg),
            ResourceError::ToolError { code, message, .. } => {
                write!(f, "Tool error ({}): {}", code, message)
            }
            ResourceError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for ResourceError {}

/// Create ResourceContents for a text response.
pub fn text_resource_contents(text: String, uri: &str) -> ResourceContents {
    ResourceContents::text(text, uri.to_string())
}
