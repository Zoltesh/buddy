//! Telegram skill approval: pending map and request flow.
//!
//! When a skill requires approval, we send a message with Approve/Deny buttons,
//! store a oneshot sender keyed by message_id, and wait for the callback or timeout.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};
use tokio::sync::{Mutex, oneshot};

/// Pending approval requests keyed by Telegram message_id.
/// Value is (chat_id, sender): we need chat_id to edit the message on timeout.
pub type TelegramPendingApprovals =
    Arc<Mutex<HashMap<i32, (ChatId, oneshot::Sender<bool>)>>>;

/// Create a new empty pending approvals map.
pub fn new_telegram_pending_approvals() -> TelegramPendingApprovals {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Callback data for inline buttons (max 64 bytes). We use short literals.
pub const CALLBACK_APPROVE: &str = "approve";
pub const CALLBACK_DENY: &str = "deny";

/// Build the approval request message text (HTML) per task spec.
fn approval_message_text(skill_name: &str, formatted_arguments: &str) -> String {
    let name = html_escape(skill_name);
    let args = html_escape(formatted_arguments);
    format!("🔧 <b>{name}</b> wants to execute:\n<pre>{args}</pre>\nAllow this action?")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Send an approval request message with Approve/Deny buttons, wait for callback or timeout.
/// Returns true if approved, false if denied or timed out. Edits the message on timeout.
pub async fn request_approval(
    bot: &teloxide::Bot,
    chat_id: ChatId,
    pending: &TelegramPendingApprovals,
    timeout: Duration,
    skill_name: &str,
    arguments: &str,
) -> bool {
    let formatted_args = serde_json::from_str::<serde_json::Value>(arguments)
        .map(|v| serde_json::to_string_pretty(&v).unwrap_or_else(|_| arguments.to_string()))
        .unwrap_or_else(|_| arguments.to_string());
    let text = approval_message_text(skill_name, &formatted_args);
    let keyboard = InlineKeyboardMarkup::new([
        vec![
            InlineKeyboardButton::callback("✅ Approve", CALLBACK_APPROVE),
            InlineKeyboardButton::callback("❌ Deny", CALLBACK_DENY),
        ],
    ]);
    let msg = match bot
        .send_message(chat_id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await
    {
        Ok(m) => m,
        Err(e) => {
            log::error!("Failed to send approval message: {e}");
            return false;
        }
    };
    let message_id_i32 = msg.id.0;
    let (tx, rx) = oneshot::channel();
    {
        let mut guard = pending.lock().await;
        guard.insert(message_id_i32, (chat_id, tx));
    }
    let result = tokio::time::timeout(timeout, rx).await;
    let approved = match &result {
        Ok(Ok(true)) => true,
        _ => false,
    };
    {
        let mut guard = pending.lock().await;
        guard.remove(&message_id_i32);
    }
    if result.is_err() {
        let _ = bot
            .edit_message_text(chat_id, msg.id, "⏰ Timed out (denied)")
            .await;
    } else if approved {
        let _ = bot
            .edit_message_text(chat_id, msg.id, "✅ Approved")
            .await;
    }
    approved
}
