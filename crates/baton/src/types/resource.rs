//! Resource Types
//!
//! Types for MCP resource definitions and contents.
//! Per MCP 2025-06-18 schema lines 1759-1957.

use serde::{Deserialize, Serialize};

use super::Annotations;

/// A resource that the server can provide.
/// Per MCP 2025-06-18 schema lines 1759-1802.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    /// URI identifying the resource.
    pub uri: String,

    /// Programmatic name.
    pub name: String,

    /// Human-readable title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Description for the LLM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// MIME type of the resource.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,

    /// Size in bytes (before base64 encoding).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,

    /// Optional annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

impl Resource {
    /// Create a new resource.
    pub fn new(uri: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            title: None,
            description: None,
            mime_type: None,
            size: None,
            annotations: None,
        }
    }

    /// Set the human-readable title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the MIME type.
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    /// Set the size.
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }
}

/// A resource template using URI template syntax (RFC 6570).
/// Per MCP 2025-06-18 schema lines 1899-1938.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTemplate {
    /// URI template (RFC 6570).
    pub uri_template: String,

    /// Programmatic name.
    pub name: String,

    /// Human-readable title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Description for the LLM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// MIME type for resources matching this template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,

    /// Optional annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

impl ResourceTemplate {
    /// Create a new resource template.
    pub fn new(uri_template: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri_template: uri_template.into(),
            name: name.into(),
            title: None,
            description: None,
            mime_type: None,
            annotations: None,
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the MIME type.
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }
}

/// Contents of a resource.
/// Per MCP 2025-06-18 schema lines 2326-2352 (text) and 77-104 (blob).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResourceContents {
    /// Text resource contents.
    Text(TextResourceContents),
    /// Binary resource contents (base64 encoded).
    Blob(BlobResourceContents),
}

impl ResourceContents {
    /// Create text resource contents.
    pub fn text(uri: impl Into<String>, text: impl Into<String>) -> Self {
        ResourceContents::Text(TextResourceContents {
            uri: uri.into(),
            text: text.into(),
            mime_type: None,
        })
    }

    /// Create text resource contents with MIME type.
    pub fn text_with_mime(
        uri: impl Into<String>,
        text: impl Into<String>,
        mime_type: impl Into<String>,
    ) -> Self {
        ResourceContents::Text(TextResourceContents {
            uri: uri.into(),
            text: text.into(),
            mime_type: Some(mime_type.into()),
        })
    }

    /// Create blob resource contents.
    pub fn blob(uri: impl Into<String>, blob: impl Into<String>) -> Self {
        ResourceContents::Blob(BlobResourceContents {
            uri: uri.into(),
            blob: blob.into(),
            mime_type: None,
        })
    }

    /// Create blob resource contents with MIME type.
    pub fn blob_with_mime(
        uri: impl Into<String>,
        blob: impl Into<String>,
        mime_type: impl Into<String>,
    ) -> Self {
        ResourceContents::Blob(BlobResourceContents {
            uri: uri.into(),
            blob: blob.into(),
            mime_type: Some(mime_type.into()),
        })
    }
}

/// Text resource contents.
/// Per MCP 2025-06-18 schema lines 2326-2352.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextResourceContents {
    /// URI of the resource.
    pub uri: String,

    /// Text content.
    pub text: String,

    /// MIME type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Binary resource contents (base64 encoded).
/// Per MCP 2025-06-18 schema lines 77-104.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobResourceContents {
    /// URI of the resource.
    pub uri: String,

    /// Base64-encoded binary data.
    pub blob: String,

    /// MIME type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Parameters for resources/read request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceParams {
    /// URI of the resource to read.
    pub uri: String,
}

/// Result of resources/read request.
/// Per MCP 2025-06-18 schema lines 1697-1723.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceResult {
    /// Resource contents.
    pub contents: Vec<ResourceContents>,
}

impl ReadResourceResult {
    /// Create a result with a single content item.
    pub fn single(contents: ResourceContents) -> Self {
        Self {
            contents: vec![contents],
        }
    }

    /// Create a result with multiple content items.
    pub fn multiple(contents: Vec<ResourceContents>) -> Self {
        Self { contents }
    }
}

/// Result of resources/list request.
/// Per MCP 2025-06-18 schema lines 1165-1188.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResourcesResult {
    /// Available resources.
    pub resources: Vec<Resource>,

    /// Pagination cursor for next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl ListResourcesResult {
    /// Create a result with all resources (no pagination).
    pub fn all(resources: Vec<Resource>) -> Self {
        Self {
            resources,
            next_cursor: None,
        }
    }
}

/// Result of resources/templates/list request.
/// Per MCP 2025-06-18 schema lines 1119-1142.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResourceTemplatesResult {
    /// Available resource templates.
    pub resource_templates: Vec<ResourceTemplate>,

    /// Pagination cursor for next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl ListResourceTemplatesResult {
    /// Create a result with all templates (no pagination).
    pub fn all(templates: Vec<ResourceTemplate>) -> Self {
        Self {
            resource_templates: templates,
            next_cursor: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_creation() {
        let resource = Resource::new("cas://sha256:abc123", "audio.wav")
            .with_title("Audio File")
            .with_mime_type("audio/wav")
            .with_size(12345);

        let json = serde_json::to_value(&resource).unwrap();
        assert_eq!(json["uri"], "cas://sha256:abc123");
        assert_eq!(json["name"], "audio.wav");
        assert_eq!(json["title"], "Audio File");
        assert_eq!(json["mimeType"], "audio/wav");
        assert_eq!(json["size"], 12345);
    }

    #[test]
    fn test_resource_template() {
        let template = ResourceTemplate::new("cas://{hash}", "cas-resource")
            .with_description("Content-addressable storage resources");

        let json = serde_json::to_value(&template).unwrap();
        assert_eq!(json["uriTemplate"], "cas://{hash}");
        assert_eq!(json["name"], "cas-resource");
    }

    #[test]
    fn test_text_resource_contents() {
        let contents = ResourceContents::text_with_mime(
            "file:///example.txt",
            "Hello, World!",
            "text/plain",
        );

        let json = serde_json::to_value(&contents).unwrap();
        assert_eq!(json["uri"], "file:///example.txt");
        assert_eq!(json["text"], "Hello, World!");
        assert_eq!(json["mimeType"], "text/plain");
    }

    #[test]
    fn test_blob_resource_contents() {
        let contents = ResourceContents::blob_with_mime(
            "cas://sha256:abc123",
            "SGVsbG8h", // "Hello!" in base64
            "application/octet-stream",
        );

        let json = serde_json::to_value(&contents).unwrap();
        assert_eq!(json["uri"], "cas://sha256:abc123");
        assert_eq!(json["blob"], "SGVsbG8h");
        assert_eq!(json["mimeType"], "application/octet-stream");
    }

    #[test]
    fn test_list_resources_result() {
        let result = ListResourcesResult::all(vec![
            Resource::new("file:///a.txt", "a.txt"),
            Resource::new("file:///b.txt", "b.txt"),
        ]);

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["resources"].as_array().unwrap().len(), 2);
        assert!(json.get("nextCursor").is_none());
    }
}
