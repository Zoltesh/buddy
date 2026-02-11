use std::path::Path;

use teloxide::prelude::*;

mod adapter;

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

    let bot = Bot::new(token);

    println!("buddy-telegram started (polling)");

    let handler = Update::filter_message().endpoint(handle_message);

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

async fn handle_message(msg: Message) -> ResponseResult<()> {
    if let Some(text) = msg.text() {
        let sender = msg
            .from
            .as_ref()
            .map(|u| u.first_name.as_str())
            .unwrap_or("unknown");
        log::info!("[chat {}] {}: {}", msg.chat.id, sender, text);
    }
    Ok(())
}
