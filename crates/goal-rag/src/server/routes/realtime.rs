//! WebSocket real-time API endpoint
//!
//! Provides WebSocket connections for subscribing to database change notifications.
//!
//! ## IMPORTANT: Current Limitations
//!
//! This is a **stub implementation**. While clients can connect and subscribe,
//! actual database change events are NOT delivered because:
//! - No PostgreSQL LISTEN/NOTIFY integration exists yet
//! - No broadcast mechanism to fan out events to subscribers
//!
//! The subscription tracking works, but events will only be delivered once
//! a PostgreSQL notification listener is implemented.
//!
//! ## Security Features
//! - Table whitelist (only allowed tables can be subscribed)
//! - Connection limits (max connections per handler)
//! - Subscription limits (max 50 per connection)
//! - Message size limits (max 8KB per message)
//! - Graceful error handling

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::RwLock;

use crate::server::state::AppState;

/// Maximum subscriptions per WebSocket connection
const MAX_SUBSCRIPTIONS_PER_CONNECTION: usize = 50;

/// Maximum message size in bytes (8KB)
const MAX_MESSAGE_SIZE: usize = 8 * 1024;

/// Global connection counter for monitoring
static ACTIVE_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

/// Maximum concurrent WebSocket connections
const MAX_CONNECTIONS: usize = 1000;

/// WebSocket subscription request
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum WsRequest {
    /// Subscribe to a table's changes
    #[serde(rename = "subscribe")]
    Subscribe {
        table: String,
        /// Required: organization_id for multi-tenancy isolation
        organization_id: String,
        #[serde(default)]
        event: Option<String>, // INSERT, UPDATE, DELETE, or * for all
        #[serde(default)]
        filter: Option<String>, // e.g., "status=eq.done"
    },
    /// Unsubscribe from a table
    #[serde(rename = "unsubscribe")]
    Unsubscribe { table: String },
    /// Ping to keep connection alive
    #[serde(rename = "ping")]
    Ping,
    /// Get connection status
    #[serde(rename = "status")]
    Status,
}

/// WebSocket response/event
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum WsResponse {
    /// Subscription confirmed
    #[serde(rename = "subscribed")]
    Subscribed {
        table: String,
        subscription_id: String,
        /// Note: Events not yet delivered (stub implementation)
        warning: Option<String>,
    },
    /// Unsubscribed
    #[serde(rename = "unsubscribed")]
    Unsubscribed { table: String },
    /// Database change event (will be sent when PostgreSQL NOTIFY is implemented)
    #[serde(rename = "change")]
    Change {
        table: String,
        event: String,
        new_record: Option<serde_json::Value>,
        old_record: Option<serde_json::Value>,
    },
    /// Pong response
    #[serde(rename = "pong")]
    Pong { timestamp: u64 },
    /// Status response
    #[serde(rename = "status")]
    Status {
        connected: bool,
        subscription_count: usize,
        max_subscriptions: usize,
        active_connections: usize,
        implementation_status: String,
    },
    /// Error
    #[serde(rename = "error")]
    Error { message: String, code: String },
    /// Connection established
    #[serde(rename = "connected")]
    Connected {
        message: String,
        max_subscriptions: usize,
        max_message_size: usize,
        warning: String,
    },
}

/// Allowed tables for subscription (security whitelist)
const ALLOWED_TABLES: &[&str] = &[
    "tasks",
    "goals",
    "users",
    "organizations",
    "documents",
    "conversations",
    "chat_messages",
    "task_comments",
    "task_attachments",
    "messages",
    "categories",
    "groups",
    "special_events",
];

/// Validate table name against whitelist
fn validate_table(table: &str) -> Result<(), String> {
    // Reject empty table names
    if table.is_empty() {
        return Err("Table name cannot be empty".to_string());
    }

    // Reject special characters that could be used for injection
    if table.chars().any(|c| !c.is_alphanumeric() && c != '_') {
        return Err("Table name contains invalid characters".to_string());
    }

    if !ALLOWED_TABLES.contains(&table) {
        return Err(format!(
            "Table '{}' is not allowed. Allowed tables: {}",
            table,
            ALLOWED_TABLES.join(", ")
        ));
    }
    Ok(())
}

/// Get current timestamp in milliseconds
fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// WebSocket handler - upgrades HTTP to WebSocket
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(_state): State<AppState>,
) -> Response {
    // Check connection limit
    let current = ACTIVE_CONNECTIONS.load(Ordering::Relaxed);
    if current >= MAX_CONNECTIONS {
        tracing::warn!(
            current_connections = current,
            max_connections = MAX_CONNECTIONS,
            "WebSocket connection rejected: limit reached"
        );
        // Return 503 Service Unavailable
        return Response::builder()
            .status(503)
            .body(axum::body::Body::from("Too many WebSocket connections"))
            .unwrap_or_else(|_| {
                Response::new(axum::body::Body::from("Too many WebSocket connections"))
            });
    }

    ws.on_upgrade(handle_socket)
}

/// Handle WebSocket connection
async fn handle_socket(socket: WebSocket) {
    // Increment connection counter
    ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
    let connection_id = uuid::Uuid::new_v4();

    tracing::info!(
        connection_id = %connection_id,
        active_connections = ACTIVE_CONNECTIONS.load(Ordering::Relaxed),
        "WebSocket connection established"
    );

    let (mut sender, mut receiver) = socket.split();

    // Track subscriptions for this connection
    let subscriptions: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(HashSet::new()));

    // Send connected message with warnings about stub implementation
    let connected = WsResponse::Connected {
        message: "WebSocket connection established".to_string(),
        max_subscriptions: MAX_SUBSCRIPTIONS_PER_CONNECTION,
        max_message_size: MAX_MESSAGE_SIZE,
        warning: "NOTE: This is a stub implementation. Subscriptions are tracked but database change events are not yet delivered. PostgreSQL NOTIFY integration is pending.".to_string(),
    };

    let connected_json = serde_json::to_string(&connected)
        .unwrap_or_else(|_| r#"{"type":"error","message":"Serialization failed"}"#.to_string());

    if sender.send(Message::Text(connected_json)).await.is_err() {
        ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
        return;
    }

    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Check message size limit
                if text.len() > MAX_MESSAGE_SIZE {
                    let error = WsResponse::Error {
                        message: format!(
                            "Message too large ({} bytes). Maximum allowed: {} bytes",
                            text.len(),
                            MAX_MESSAGE_SIZE
                        ),
                        code: "MESSAGE_TOO_LARGE".to_string(),
                    };
                    let json = serde_json::to_string(&error)
                        .unwrap_or_else(|_| r#"{"type":"error","message":"Message too large"}"#.to_string());
                    if sender.send(Message::Text(json)).await.is_err() {
                        break;
                    }
                    continue;
                }

                let response = handle_message(&text, &subscriptions).await;
                let json = serde_json::to_string(&response)
                    .unwrap_or_else(|e| {
                        format!(r#"{{"type":"error","message":"Serialization failed: {}","code":"INTERNAL_ERROR"}}"#, e)
                    });

                if sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
            Ok(Message::Ping(data)) => {
                if sender.send(Message::Pong(data)).await.is_err() {
                    break;
                }
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                tracing::debug!(
                    connection_id = %connection_id,
                    error = %e,
                    "WebSocket error"
                );
                break;
            }
            _ => {}
        }
    }

    // Cleanup
    ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
    let sub_count = subscriptions.read().await.len();

    tracing::info!(
        connection_id = %connection_id,
        subscriptions_cleared = sub_count,
        active_connections = ACTIVE_CONNECTIONS.load(Ordering::Relaxed),
        "WebSocket connection closed"
    );
}

/// Handle incoming WebSocket message
async fn handle_message(
    text: &str,
    subscriptions: &Arc<RwLock<HashSet<String>>>,
) -> WsResponse {
    // Parse request
    let request: WsRequest = match serde_json::from_str(text) {
        Ok(req) => req,
        Err(e) => {
            return WsResponse::Error {
                message: format!("Invalid JSON: {}", e),
                code: "INVALID_JSON".to_string(),
            };
        }
    };

    match request {
        WsRequest::Subscribe { table, organization_id, event, filter } => {
            // Validate organization_id (required for multi-tenancy)
            if organization_id.is_empty() || organization_id == "_default" {
                return WsResponse::Error {
                    message: "organization_id is required and cannot be empty or '_default'".to_string(),
                    code: "INVALID_ORGANIZATION_ID".to_string(),
                };
            }
            // Reject org_ids with suspicious characters
            if organization_id.chars().any(|c| !c.is_alphanumeric() && c != '-' && c != '_') {
                return WsResponse::Error {
                    message: "organization_id contains invalid characters".to_string(),
                    code: "INVALID_ORGANIZATION_ID".to_string(),
                };
            }

            // Validate table
            if let Err(e) = validate_table(&table) {
                return WsResponse::Error {
                    message: e,
                    code: "INVALID_TABLE".to_string(),
                };
            }

            // Check subscription limit
            {
                let subs = subscriptions.read().await;
                if subs.len() >= MAX_SUBSCRIPTIONS_PER_CONNECTION {
                    return WsResponse::Error {
                        message: format!(
                            "Subscription limit reached ({}/{})",
                            subs.len(),
                            MAX_SUBSCRIPTIONS_PER_CONNECTION
                        ),
                        code: "SUBSCRIPTION_LIMIT".to_string(),
                    };
                }
            }

            // Create subscription ID scoped to organization
            let event_str = event.as_deref().unwrap_or("*");
            let filter_str = filter.as_deref().unwrap_or("");
            let sub_id = format!("{}:{}:{}:{}", organization_id, table, event_str, filter_str);

            // Add to subscriptions
            {
                let mut subs = subscriptions.write().await;
                subs.insert(sub_id.clone());
            }

            // Log subscription (without sensitive filter data)
            tracing::info!(
                table = %table,
                organization_id = %organization_id,
                event = %event_str,
                has_filter = filter.is_some(),
                "Client subscribed to table"
            );

            WsResponse::Subscribed {
                table,
                subscription_id: sub_id,
                warning: Some("Events not yet delivered - PostgreSQL NOTIFY pending".to_string()),
            }
        }

        WsRequest::Unsubscribe { table } => {
            // Remove all subscriptions for this table
            let removed_count = {
                let mut subs = subscriptions.write().await;
                let before = subs.len();
                subs.retain(|s| !s.starts_with(&format!("{}:", table)));
                before - subs.len()
            };

            tracing::info!(
                table = %table,
                removed_count = removed_count,
                "Client unsubscribed from table"
            );

            WsResponse::Unsubscribed { table }
        }

        WsRequest::Ping => WsResponse::Pong {
            timestamp: current_timestamp_ms(),
        },

        WsRequest::Status => {
            let sub_count = subscriptions.read().await.len();
            WsResponse::Status {
                connected: true,
                subscription_count: sub_count,
                max_subscriptions: MAX_SUBSCRIPTIONS_PER_CONNECTION,
                active_connections: ACTIVE_CONNECTIONS.load(Ordering::Relaxed),
                implementation_status: "stub - PostgreSQL NOTIFY integration pending".to_string(),
            }
        }
    }
}

/// Create a change event (for future use when PostgreSQL NOTIFY is implemented)
/// This function will be called by the NOTIFY listener to broadcast changes
#[allow(dead_code)]
pub fn create_change_event(
    table: &str,
    event: &str,
    new_record: Option<serde_json::Value>,
    old_record: Option<serde_json::Value>,
) -> WsResponse {
    WsResponse::Change {
        table: table.to_string(),
        event: event.to_string(),
        new_record,
        old_record,
    }
}

/// Get current active connection count (for monitoring)
pub fn get_active_connections() -> usize {
    ACTIVE_CONNECTIONS.load(Ordering::Relaxed)
}
