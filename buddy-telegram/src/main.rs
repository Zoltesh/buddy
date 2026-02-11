use std::path::Path;
use std::sync::Arc;

use teloxide::prelude::*;
use teloxide::types::ParseMode;

use buddy_core::provider::{AnyProvider, ProviderChain};
use buddy_core::state::AppState;

mod adapter;
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

    let token = match std::env::var(&telegram.bot_token_env) {
        Ok(t) if !t.is_empty() => t,
        _ => {
            eprintln!(
                "Error: environment variable '{}' is not set",
                telegram.bot_token_env
            );
            std::process::exit(1);
        }
    };

    let state = Arc::new(
        AppState::new(config, Path::new(DEFAULT_CONFIG_PATH)).unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }),
    );

    let bot = Bot::new(token);

    println!("buddy-telegram started (polling)");

    let handler = Update::filter_message().endpoint(handle_message);

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

async fn handle_message(
    bot: Bot,
    msg: teloxide::types::Message,
    state: Arc<AppState<ProviderChain<AnyProvider>>>,
) -> ResponseResult<()> {
    // Ignore non-text messages.
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
    let response_text =
        match handler::process_message(&state.store, &**provider, chat_id.0, user_text).await {
            Ok(handler::ProcessResult::Response(text)) => text,
            Ok(handler::ProcessResult::Empty) => return Ok(()),
            Err(e) => {
                bot.send_message(chat_id, e.user_message()).await?;
                return Ok(());
            }
        };

    let escaped = adapter::escape_markdown_v2(&response_text);
    for part in adapter::split_message(&escaped) {
        bot.send_message(chat_id, part)
            .parse_mode(ParseMode::MarkdownV2)
            .await?;
    }

    Ok(())
}
