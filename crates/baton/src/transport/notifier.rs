//! Resource Notifier
//!
//! Sends resource update notifications to subscribed sessions.

use std::sync::Arc;

use axum::response::sse::Event;
use crate::session::SessionStore;
use crate::types::jsonrpc::JsonRpcMessage;
use crate::types::subscription::ResourceUpdatedNotification;

/// Notifier for sending resource updates to subscribers.
#[derive(Clone)]
pub struct ResourceNotifier {
    sessions: Arc<dyn SessionStore>,
}

impl ResourceNotifier {
    /// Create a new resource notifier.
    pub fn new(sessions: Arc<dyn SessionStore>) -> Self {
        Self { sessions }
    }

    /// Notify all subscribers that a resource was updated.
    pub async fn notify_updated(&self, uri: &str) {
        let _notification_data = ResourceUpdatedNotification {
            uri: uri.to_string(),
        };

        // Unfortunately we don't have a way to iterate all sessions yet
        // This would need to be added to SessionStore trait in the future
        // For now, this is a placeholder that individual sessions can call
        tracing::debug!(uri = %uri, "Resource updated notification ready");
    }

    /// Notify that the resources list changed (new resources available).
    pub async fn notify_list_changed(&self) {
        tracing::debug!("Resources list changed notification ready");
    }

    /// Send notification to a specific session if subscribed.
    pub async fn notify_session(&self, session_id: &str, uri: &str) {
        if let Some(session) = self.sessions.get(session_id) {
            if !session.is_subscribed(uri) {
                return;
            }

            let notification_data = ResourceUpdatedNotification {
                uri: uri.to_string(),
            };

            let notification = JsonRpcMessage::notification(
                "notifications/resources/updated",
                serde_json::to_value(&notification_data).unwrap_or_default(),
            );

            if let Ok(json) = serde_json::to_string(&notification) {
                let event = Event::default().data(json);
                if let Err(e) = session.send_event(event).await {
                    tracing::warn!(
                        session_id = %session_id,
                        uri = %uri,
                        error = %e,
                        "Failed to send resource update notification"
                    );
                }
            }
        }
    }
}
