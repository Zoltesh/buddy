use std::path::Path;
use std::sync::Arc;

use teloxide::prelude::*;
use teloxide::types::ParseMode;

use buddy_core::provider::{AnyProvider, ProviderChain};
use buddy_core::state::AppState;

mod adapter;
mod approval;
mod handler;

const DEFAULT_CONFIG_PATH: &str = "buddy.toml";

#[tokio::main]
async fn main() {
    env_logger::init();

    let config =
        buddy_core::config::Config::from_file(Path::new(DEFAULT_CONFIG_PATH)).unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            std::process::exit(1);
        });

    let telegram = &config.interfaces.telegram;
    if !telegram.enabled {
        println!("Telegram interface is not enabled in config.");
        return;
    }

    let token = telegram.resolve_bot_token().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let state = Arc::new(
        AppState::new(config, Path::new(DEFAULT_CONFIG_PATH)).unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }),
    );

    let pending_approvals = approval::new_telegram_pending_approvals();

    let bot = Bot::new(token);

    println!("buddy-telegram started (polling)");

    let message_handler = Update::filter_message().endpoint(handle_message);
    let callback_handler = Update::filter_callback_query().endpoint(handle_callback);
    let handler = dptree::entry()
        .branch(message_handler)
        .branch(callback_handler);

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state, pending_approvals])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

async fn handle_message(
    bot: Bot,
    msg: teloxide::types::Message,
    state: Arc<AppState<ProviderChain<AnyProvider>>>,
    pending: Arc<approval::TelegramPendingApprovals>,
) -> ResponseResult<()> {
    let user_text = match msg.text() {
        Some(t) => t,
        None => return Ok(()),
    };

    let chat_id = msg.chat.id;
    let sender = msg
        .from
        .as_ref()
        .map(|u| u.first_name.as_str())
        .unwrap_or("unknown");
    log::info!("[chat {}] {}: {}", chat_id, sender, user_text);

    let provider = state.provider.load();
    let registry = state.registry.load();
    let approval_overrides = state.approval_overrides.load();
    let approval_ctx = handler::TelegramApprovalContext {
        bot: &bot,
        chat_id,
        pending: &pending,
        timeout: state.approval_timeout,
    };

    let result = handler::process_message(
        &state.store,
        &**provider,
        &registry,
        &approval_overrides,
        &state.conversation_approvals,
        chat_id.0,
        user_text,
        Some(approval_ctx),
    )
    .await;

    let (final_text, tool_results) = match result {
        Ok(handler::ProcessResult::Response {
            final_text,
            tool_results,
        }) => (final_text, tool_results),
        Ok(handler::ProcessResult::Empty) => return Ok(()),
        Err(e) => {
            bot.send_message(chat_id, e.user_message()).await?;
            return Ok(());
        }
    };

    for part in tool_results {
        for chunk in adapter::split_message(&part) {
            bot.send_message(chat_id, chunk).await?;
        }
    }

    let escaped = adapter::escape_markdown_v2(&final_text);
    for part in adapter::split_message(&escaped) {
        bot.send_message(chat_id, part)
            .parse_mode(ParseMode::MarkdownV2)
            .await?;
    }

    Ok(())
}

async fn handle_callback(
    bot: Bot,
    q: CallbackQuery,
    pending: Arc<approval::TelegramPendingApprovals>,
) -> ResponseResult<()> {
    let data = match &q.data {
        Some(d) => d.as_str(),
        None => return Ok(()),
    };
    let (approved, status_text) = match data {
        approval::CALLBACK_APPROVE => (true, "✅ Approved"),
        approval::CALLBACK_DENY => (false, "❌ Denied"),
        _ => return Ok(()),
    };

    let query_id = q.id.clone();
    let message_id_i32 = q
        .message
        .as_ref()
        .map(|m| m.id().0)
        .unwrap_or(0);
    let regular_msg = q.regular_message().map(|m| (m.chat.id, m.id));
    let sender = {
        let mut guard = pending.lock().await;
        guard.remove(&message_id_i32).map(|(_, tx)| tx)
    };

    if let Some(tx) = sender {
        let _ = tx.send(approved);
    }

    bot.answer_callback_query(query_id).await?;

    if let Some((chat_id, message_id)) = regular_msg {
        let _ = bot
            .edit_message_text(chat_id, message_id, status_text)
            .await;
    }

    Ok(())
}
