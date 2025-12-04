//! Resource Subscription Types
//!
//! Types for resource subscription management.
//! Per MCP 2025-06-18 schema.

use serde::{Deserialize, Serialize};

/// Subscribe request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeParams {
    /// URI of resource to subscribe to
    pub uri: String,
}

/// Unsubscribe request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsubscribeParams {
    /// URI of resource to unsubscribe from
    pub uri: String,
}

/// Resource updated notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUpdatedNotification {
    /// URI of the updated resource
    pub uri: String,
}

/// Resources list changed notification (empty params)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourcesListChangedNotification {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscribe_params_serialization() {
        let params = SubscribeParams {
            uri: "artifacts://recent".to_string(),
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["uri"], "artifacts://recent");
    }

    #[test]
    fn test_resource_updated_notification() {
        let notification = ResourceUpdatedNotification {
            uri: "graph://identities".to_string(),
        };
        let json = serde_json::to_value(&notification).unwrap();
        assert_eq!(json["uri"], "graph://identities");
    }
}
