//! WhatsApp skill approval: pending map and request flow.
//!
//! When a skill requires approval, we send an interactive button message,
//! store a oneshot sender keyed by the sender's phone number (one pending
//! approval per sender at a time), and wait for the button reply or timeout.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, oneshot};

use crate::client::WhatsAppClient;

/// Pending approval requests keyed by sender phone number.
pub type WhatsAppPendingApprovals =
    Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;

/// Create a new empty pending approvals map.
pub fn new_whatsapp_pending_approvals() -> WhatsAppPendingApprovals {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Button reply IDs (must match the `id` values sent in interactive messages).
pub const BUTTON_APPROVE: &str = "approve";
pub const BUTTON_DENY: &str = "deny";

/// Build the approval request body text per task spec.
fn approval_body_text(skill_name: &str, formatted_arguments: &str) -> String {
    format!(
        "\u{1f527} {skill_name} wants to execute:\n{formatted_arguments}\n\nAllow this action?"
    )
}

/// Send an interactive approval message with Approve/Deny buttons, wait for
/// the button reply or timeout. Returns `true` if approved, `false` if denied
/// or timed out.
pub async fn request_approval(
    client: &WhatsAppClient,
    phone: &str,
    pending: &WhatsAppPendingApprovals,
    timeout: Duration,
    skill_name: &str,
    arguments: &str,
) -> bool {
    let formatted_args = serde_json::from_str::<serde_json::Value>(arguments)
        .map(|v| serde_json::to_string_pretty(&v).unwrap_or_else(|_| arguments.to_string()))
        .unwrap_or_else(|_| arguments.to_string());

    let body_text = approval_body_text(skill_name, &formatted_args);
    let buttons = [(BUTTON_APPROVE, "Approve"), (BUTTON_DENY, "Deny")];

    if let Err(e) = client
        .send_interactive_message(phone, &body_text, &buttons)
        .await
    {
        log::error!("Failed to send approval message to {phone}: {e}");
        return false;
    }

    let (tx, rx) = oneshot::channel();
    {
        let mut guard = pending.lock().await;
        guard.insert(phone.to_string(), tx);
    }

    let result = tokio::time::timeout(timeout, rx).await;
    let approved = matches!(&result, Ok(Ok(true)));

    // Clean up the pending entry (may already be removed by the reply handler).
    {
        let mut guard = pending.lock().await;
        guard.remove(phone);
    }

    if result.is_err() {
        // Timeout — notify the user.
        let _ = client
            .send_text_message(phone, "\u{23f0} Timed out (denied)")
            .await;
    }

    approved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn approve_button_reply_resolves_to_true() {
        let pending = new_whatsapp_pending_approvals();
        let (tx, rx) = oneshot::channel();
        {
            let mut guard = pending.lock().await;
            guard.insert("15551234567".to_string(), tx);
        }

        // Simulate button reply (what the webhook handler does).
        let sender = {
            let mut guard = pending.lock().await;
            guard.remove("15551234567")
        };
        assert!(sender.is_some());
        sender.unwrap().send(true).unwrap();

        let result = rx.await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn deny_button_reply_resolves_to_false() {
        let pending = new_whatsapp_pending_approvals();
        let (tx, rx) = oneshot::channel();
        {
            let mut guard = pending.lock().await;
            guard.insert("15551234567".to_string(), tx);
        }

        let sender = {
            let mut guard = pending.lock().await;
            guard.remove("15551234567")
        };
        sender.unwrap().send(false).unwrap();

        let result = rx.await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn timeout_results_in_dropped_receiver() {
        let pending = new_whatsapp_pending_approvals();
        let (tx, rx) = oneshot::channel::<bool>();
        {
            let mut guard = pending.lock().await;
            guard.insert("15551234567".to_string(), tx);
        }

        // Simulate timeout: drop the sender without sending.
        {
            let mut guard = pending.lock().await;
            guard.remove("15551234567"); // drops the Sender
        }

        // The receiver should get a RecvError since sender was dropped.
        let result = rx.await;
        assert!(result.is_err());
    }

    #[test]
    fn approval_body_text_matches_spec_format() {
        let text = approval_body_text("remember", r#"{"key": "value"}"#);
        assert!(text.contains("\u{1f527} remember wants to execute:"));
        assert!(text.contains("Allow this action?"));
    }

    #[test]
    fn button_constants_match_expected_values() {
        assert_eq!(BUTTON_APPROVE, "approve");
        assert_eq!(BUTTON_DENY, "deny");
    }
}
