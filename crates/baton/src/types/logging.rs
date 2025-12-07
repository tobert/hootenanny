//! Logging Types
//!
//! Types for structured logging to MCP clients.
//! Per MCP 2025-06-18 schema.

use serde::{Deserialize, Serialize};

/// Log levels (matching syslog severity)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    #[default]
    Info,
    Notice,
    Warning,
    Error,
    Critical,
    Alert,
    Emergency,
}

impl From<tracing::Level> for LogLevel {
    fn from(level: tracing::Level) -> Self {
        match level {
            tracing::Level::TRACE | tracing::Level::DEBUG => LogLevel::Debug,
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warning,
            tracing::Level::ERROR => LogLevel::Error,
        }
    }
}

/// Set log level request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLevelParams {
    pub level: LogLevel,
}

/// Log message notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    /// Severity level
    pub level: LogLevel,

    /// Logger name (e.g., module path)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<String>,

    /// The log message
    #[serde(rename = "data")]
    pub message: serde_json::Value,
}

impl LogMessage {
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            logger: None,
            message: serde_json::Value::String(message.into()),
        }
    }

    pub fn with_logger(mut self, logger: impl Into<String>) -> Self {
        self.logger = Some(logger.into());
        self
    }

    pub fn debug(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Debug, message)
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Info, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Warning, message)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Error, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Error > LogLevel::Warning);
        assert!(LogLevel::Warning > LogLevel::Info);
        assert!(LogLevel::Info > LogLevel::Debug);
    }

    #[test]
    fn test_log_level_serialization() {
        let level = LogLevel::Info;
        let json = serde_json::to_value(&level).unwrap();
        assert_eq!(json, "info");

        let level: LogLevel = serde_json::from_value(json).unwrap();
        assert_eq!(level, LogLevel::Info);
    }

    #[test]
    fn test_log_message_construction() {
        let msg = LogMessage::info("Test message");
        assert_eq!(msg.level, LogLevel::Info);
        assert_eq!(msg.message, serde_json::Value::String("Test message".to_string()));
        assert!(msg.logger.is_none());
    }

    #[test]
    fn test_log_message_with_logger() {
        let msg = LogMessage::info("Test message")
            .with_logger("test::module");

        assert_eq!(msg.logger, Some("test::module".to_string()));
    }

    #[test]
    fn test_log_message_serialization() {
        let msg = LogMessage::info("Test message")
            .with_logger("test::module");

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["level"], "info");
        assert_eq!(json["logger"], "test::module");
        assert_eq!(json["data"], "Test message");
    }

    #[test]
    fn test_tracing_level_conversion() {
        assert_eq!(LogLevel::from(tracing::Level::TRACE), LogLevel::Debug);
        assert_eq!(LogLevel::from(tracing::Level::DEBUG), LogLevel::Debug);
        assert_eq!(LogLevel::from(tracing::Level::INFO), LogLevel::Info);
        assert_eq!(LogLevel::from(tracing::Level::WARN), LogLevel::Warning);
        assert_eq!(LogLevel::from(tracing::Level::ERROR), LogLevel::Error);
    }
}
