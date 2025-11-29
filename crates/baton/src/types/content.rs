//! Content Types
//!
//! Types for content blocks in MCP messages.
//! Per MCP 2025-06-18 schema lines 428-675.

use serde::{Deserialize, Serialize};

use super::resource::ResourceContents;
use super::Annotations;

/// Content block in a message or tool result.
/// Per MCP 2025-06-18 schema lines 428-446.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    /// Text content.
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Annotations>,
    },

    /// Base64-encoded image.
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Annotations>,
    },

    /// Base64-encoded audio.
    Audio {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Annotations>,
    },

    /// Link to a resource.
    /// Per MCP 2025-06-18 schema lines 1826-1874.
    #[serde(rename = "resource_link")]
    ResourceLink {
        uri: String,
        name: String,
        #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        size: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Annotations>,
    },

    /// Embedded resource content.
    /// Per MCP 2025-06-18 schema lines 643-675.
    Resource {
        resource: ResourceContents,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Annotations>,
    },
}

impl Content {
    /// Create text content.
    pub fn text(text: impl Into<String>) -> Self {
        Content::Text {
            text: text.into(),
            annotations: None,
        }
    }

    /// Create text content with annotations.
    pub fn text_with_annotations(text: impl Into<String>, annotations: Annotations) -> Self {
        Content::Text {
            text: text.into(),
            annotations: Some(annotations),
        }
    }

    /// Create image content from base64 data.
    pub fn image(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Content::Image {
            data: data.into(),
            mime_type: mime_type.into(),
            annotations: None,
        }
    }

    /// Create audio content from base64 data.
    pub fn audio(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Content::Audio {
            data: data.into(),
            mime_type: mime_type.into(),
            annotations: None,
        }
    }

    /// Create a resource link.
    pub fn resource_link(uri: impl Into<String>, name: impl Into<String>) -> Self {
        Content::ResourceLink {
            uri: uri.into(),
            name: name.into(),
            mime_type: None,
            title: None,
            description: None,
            size: None,
            annotations: None,
        }
    }

    /// Create a resource link with mime type.
    pub fn resource_link_with_mime(
        uri: impl Into<String>,
        name: impl Into<String>,
        mime_type: impl Into<String>,
    ) -> Self {
        Content::ResourceLink {
            uri: uri.into(),
            name: name.into(),
            mime_type: Some(mime_type.into()),
            title: None,
            description: None,
            size: None,
            annotations: None,
        }
    }

    /// Create embedded resource content.
    pub fn resource(contents: ResourceContents) -> Self {
        Content::Resource {
            resource: contents,
            annotations: None,
        }
    }

    /// Check if this is text content.
    pub fn is_text(&self) -> bool {
        matches!(self, Content::Text { .. })
    }

    /// Get the text if this is text content.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Content::Text { text, .. } => Some(text),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_content() {
        let content = Content::text("Hello, World!");

        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "Hello, World!");
        assert!(json.get("annotations").is_none());
    }

    #[test]
    fn test_image_content() {
        let content = Content::image("base64data...", "image/png");

        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json["type"], "image");
        assert_eq!(json["data"], "base64data...");
        assert_eq!(json["mimeType"], "image/png");
    }

    #[test]
    fn test_audio_content() {
        let content = Content::audio("base64audio...", "audio/wav");

        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json["type"], "audio");
        assert_eq!(json["data"], "base64audio...");
        assert_eq!(json["mimeType"], "audio/wav");
    }

    #[test]
    fn test_resource_link() {
        let content = Content::resource_link_with_mime(
            "cas://sha256:abc123",
            "audio.wav",
            "audio/wav",
        );

        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json["type"], "resource_link");
        assert_eq!(json["uri"], "cas://sha256:abc123");
        assert_eq!(json["name"], "audio.wav");
        assert_eq!(json["mimeType"], "audio/wav");
    }

    #[test]
    fn test_content_roundtrip() {
        let original = Content::text("Test message");
        let json = serde_json::to_string(&original).unwrap();
        let parsed: Content = serde_json::from_str(&json).unwrap();

        assert!(parsed.is_text());
        assert_eq!(parsed.as_text(), Some("Test message"));
    }

    #[test]
    fn test_text_with_annotations() {
        let annotations = Annotations {
            priority: Some(0.8),
            ..Default::default()
        };
        let content = Content::text_with_annotations("Important!", annotations);

        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["annotations"]["priority"], 0.8);
    }
}
