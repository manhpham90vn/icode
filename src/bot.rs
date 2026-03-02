use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use teloxide::prelude::*;
use teloxide::types::{MessageEntityKind, MessageId, ParseMode, ReplyParameters};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::agent_selector;
use crate::claim::{self, ClaimTracker, CLAIM_PREFIX};
use crate::config::Config;
use crate::executor;
use crate::formatter;

/// Parsed command from a Telegram message
#[derive(Debug)]
enum ParsedCommand {
    /// @bot_username cd /path — change work dir
    ChangeDir { path: String },
    /// @bot_username exec <command> — shell targeted at this bot
    ShellMention { command: String },
    /// @bot_username ai <prompt> or just text
    AgentQueue { prompt: String },
    /// @bot_username status
    StatusThisBot,
    /// @all status
    StatusAll,
    /// @bot_username cancel
    Cancel,
    /// @bot_username help
    Help,
    /// Claim message from another bot (starts with 🔒)
    ClaimMarker,
    /// Unknown / ignored
    Ignore,
}

/// Extract mention target from message entities.
/// Returns the bot username mentioned (without @) if any.
fn extract_mention(msg: &Message, bot_username: &str) -> Option<String> {
    let entities = msg.entities()?;
    let text = msg.text()?;

    for entity in entities {
        if let MessageEntityKind::Mention = entity.kind {
            let start = entity.offset;
            let end = start + entity.length;

            // Extract mention text (offset/length are in UTF-16 code units, but for ASCII @username it's safe)
            if end <= text.len() {
                let mention = &text[start..end];
                let username = mention.trim_start_matches('@');

                info!(
                    "Found mention: '{}', comparing with bot_username: '{}'",
                    username, bot_username
                );

                if username.eq_ignore_ascii_case(bot_username) {
                    return Some(username.to_string());
                }
            }
        }
    }
    None
}

/// Parse an incoming message text into a command, given bot username
fn parse_command(text: &str, bot_username: &str, msg: &Message) -> ParsedCommand {
    let text = text.trim();

    info!(
        "Parsing command: '{}', bot_username: '{}'",
        text, bot_username
    );

    // Claim marker from other bots
    if text.starts_with(CLAIM_PREFIX) {
        return ParsedCommand::ClaimMarker;
    }

    if text.to_lowercase() == "@all status" {
        return ParsedCommand::StatusAll;
    }

    // @bot_username <command> — mention targeting this bot
    if extract_mention(msg, bot_username).is_some() {
        // Strip the @mention from the text to get the command
        let at = format!("@{bot_username}");
        let command = text
            .replace(&at, "")
            .replace(&at.to_lowercase(), "")
            .trim()
            .to_string();

        if command.is_empty() {
            return ParsedCommand::Ignore;
        }

        info!("After strip mention, command is: '{}'", command);

        // cd /path → change work dir
        if let Some(stripped) = command.strip_prefix("cd ") {
            let path = stripped.trim();
            if !path.is_empty() {
                return ParsedCommand::ChangeDir {
                    path: path.to_string(),
                };
            }
        }

        if let Some(cmd) = command.strip_prefix("exec ") {
            return ParsedCommand::ShellMention {
                command: cmd.trim().to_string(),
            };
        }

        if command == "status" {
            return ParsedCommand::StatusThisBot;
        }

        if command == "cancel" {
            return ParsedCommand::Cancel;
        }

        if command == "help" {
            return ParsedCommand::Help;
        }

        if let Some(prompt) = command.strip_prefix("ai ") {
            return ParsedCommand::AgentQueue {
                prompt: prompt.trim().to_string(),
            };
        }

        // Any other text after mention → treat as AI prompt (default behavior)
        return ParsedCommand::AgentQueue {
            prompt: command,
        };
    }

    // No mention but has text → treat as AI prompt (default behavior)
    if !text.is_empty() {
        return ParsedCommand::AgentQueue {
            prompt: text.to_string(),
        };
    }

    ParsedCommand::Ignore
}

/// Shared mutable state per-bot instance
struct BotState {
    work_dir: String,
}

/// Run the Telegram bot
pub async fn run() -> Result<()> {
    let config = Config::load()?;
    let pc_name = config.pc_name.clone();
    let available_agents = agent_selector::list_available(&config.agent_priority);
    let start_time = Instant::now();

    let bot = Bot::new(&config.bot_token);

    // Get bot info to retrieve username
    let me = bot.get_me().await?;
    let bot_username = me.username.clone().unwrap_or_else(|| {
        warn!("Bot has no username, mention detection will not work");
        String::from("unknown")
    });

    info!(
        pc_name,
        bot_username,
        agents = ?available_agents,
        work_dir = config.work_dir,
        "Starting iCode bot"
    );
    let claim_tracker = Arc::new(ClaimTracker::new());
    let state = Arc::new(Mutex::new(BotState {
        work_dir: config.work_dir.clone(),
    }));

    // Send online notification
    let agents_str = if available_agents.is_empty() {
        "none".to_string()
    } else {
        available_agents.join(", ")
    };
    let online_msg = format!(
        "🟢 [{pc_name}] online | agents: {agents_str} | work_dir: {}",
        config.work_dir
    );
    let chat_id = ChatId(config.chat_id);
    if let Err(e) = bot.send_message(chat_id, &online_msg).await {
        error!("Failed to send online notification: {e}");
    }

    let handler = Update::filter_message().endpoint(handle_message);

    let config = Arc::new(config);
    let bot_username = Arc::new(bot_username);
    let start_time = Arc::new(start_time);

    Dispatcher::builder(bot.clone(), handler)
        .dependencies(dptree::deps![
            config.clone(),
            bot_username.clone(),
            state.clone(),
            claim_tracker.clone(),
            start_time.clone()
        ])
        .default_handler(|_| async {})
        .error_handler(Arc::new(|err| async move {
            error!("Handler error: {err}");
        }))
        .build()
        .dispatch()
        .await;

    let offline_msg = format!("🔴 [{pc_name}] offline");
    let _ = bot.send_message(chat_id, &offline_msg).await;

    Ok(())
}

/// Handle incoming messages
async fn handle_message(
    bot: Bot,
    msg: Message,
    config: Arc<Config>,
    bot_username: Arc<String>,
    state: Arc<Mutex<BotState>>,
    claim_tracker: Arc<ClaimTracker>,
    start_time: Arc<Instant>,
) -> Result<(), teloxide::RequestError> {
    let text = match msg.text() {
        Some(t) => t,
        None => return Ok(()),
    };

    let from_id = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);

    // Track claim markers from other bots (before owner check)
    let parsed = parse_command(text, &bot_username, &msg);
    if matches!(parsed, ParsedCommand::ClaimMarker) {
        if let Some(reply) = msg.reply_to_message() {
            claim_tracker.mark_claimed(reply.id.0).await;
        }
        return Ok(());
    }

    // Only process commands from owners
    if !config.owner_ids.contains(&from_id) {
        return Ok(());
    }

    let chat_id = msg.chat.id;
    let pc_name = &config.pc_name;

    match parsed {
        ParsedCommand::ChangeDir { path } => {
            let current_dir = state.lock().await.work_dir.clone();

            // Resolve path: absolute or relative to current work_dir
            let target_path = if std::path::Path::new(&path).is_absolute() {
                std::path::PathBuf::from(&path)
            } else {
                std::path::Path::new(&current_dir).join(&path)
            };

            // Canonicalize to resolve .. and . and get absolute path
            let new_dir = match target_path.canonicalize() {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(_) => {
                    let _ = bot
                        .send_message(
                            chat_id,
                            format!(
                                "❌ [{pc_name}] Path không tồn tại: {}",
                                target_path.display()
                            ),
                        )
                        .reply_parameters(ReplyParameters::new(msg.id))
                        .await;
                    return Ok(());
                }
            };

            // Update runtime state
            state.lock().await.work_dir = new_dir.clone();

            // Update config file to persist work_dir
            let mut updated_config = (*config).clone();
            updated_config.work_dir = new_dir.clone();
            if let Err(e) = updated_config.save() {
                warn!("Failed to save config with new work_dir: {e}");
            }

            let _ = bot
                .send_message(
                    chat_id,
                    format!("📁 [{pc_name}] đã đổi work_dir sang:\n{}", new_dir),
                )
                .reply_parameters(ReplyParameters::new(msg.id))
                .await;
        }

        ParsedCommand::ShellMention { command } => {
            let work_dir = state.lock().await.work_dir.clone();
            handle_shell(&bot, chat_id, msg.id, &command, &work_dir, &config).await;
        }

        ParsedCommand::AgentQueue { prompt } => {
            let work_dir = state.lock().await.work_dir.clone();
            handle_agent_queue(
                &bot,
                chat_id,
                msg.id,
                &prompt,
                &work_dir,
                &config,
                &claim_tracker,
            )
            .await;
        }

        ParsedCommand::StatusThisBot | ParsedCommand::StatusAll => {
            let work_dir = state.lock().await.work_dir.clone();
            let uptime = start_time.elapsed().as_secs();
            let available = agent_selector::list_available(&config.agent_priority);
            let status_text = formatter::format_status(
                pc_name,
                uptime,
                &config.agent_priority,
                &available,
                &work_dir,
            );
            let _ = bot
                .send_message(chat_id, &status_text)
                .parse_mode(ParseMode::MarkdownV2)
                .reply_parameters(ReplyParameters::new(msg.id))
                .await;
        }

        ParsedCommand::Cancel => {
            let _ = bot
                .send_message(
                    chat_id,
                    format!("⚠️ [{pc_name}] Cancel not yet implemented"),
                )
                .reply_parameters(ReplyParameters::new(msg.id))
                .await;
        }

        ParsedCommand::Help => {
            let help_text = formatter::format_help(pc_name, &bot_username);
            let _ = bot
                .send_message(chat_id, &help_text)
                .parse_mode(ParseMode::MarkdownV2)
                .reply_parameters(ReplyParameters::new(msg.id))
                .await;
        }

        ParsedCommand::ClaimMarker | ParsedCommand::Ignore => {}
    }

    Ok(())
}

/// Handle shell command execution
async fn handle_shell(
    bot: &Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    command: &str,
    work_dir: &str,
    config: &Config,
) {
    let pc_name = &config.pc_name;
    info!(pc_name, command, work_dir, "Executing shell command");

    let running_msg = match bot
        .send_message(
            chat_id,
            format!("⏳ [{pc_name}] `{command}`\n📁 `{work_dir}`"),
        )
        .parse_mode(ParseMode::MarkdownV2)
        .reply_parameters(ReplyParameters::new(msg_id))
        .await
    {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to send running message: {e}");
            return;
        }
    };

    let bot_clone = bot.clone();
    let running_msg_chat = running_msg.chat.id;
    let running_msg_id = running_msg.id;
    let pc_name_owned = pc_name.to_string();
    let command_owned = command.to_string();
    let work_dir_owned = work_dir.to_string();
    let last_update = Arc::new(Mutex::new(Instant::now()));

    let result = executor::shell::run(command, work_dir, config.shell_timeout_secs, |output| {
        let bot = bot_clone.clone();
        let output = output.to_string();
        let pc_name = pc_name_owned.clone();
        let command_owned = command_owned.clone();
        let work_dir = work_dir_owned.clone();
        let last_update = last_update.clone();
        let msg_chat = running_msg_chat;
        let msg_id = running_msg_id;
        tokio::spawn(async move {
            let mut last = last_update.lock().await;
            if last.elapsed().as_secs() < 2 {
                return;
            }
            *last = Instant::now();
            drop(last);

            let parts =
                formatter::format_result(&pc_name, "shell", &command_owned, &output, &work_dir);
            if let Some(text) = parts.first() {
                let _ = bot
                    .edit_message_text(msg_chat, msg_id, text)
                    .parse_mode(ParseMode::MarkdownV2)
                    .await;
            }
        });
    })
    .await;

    match result {
        Ok(output) => {
            let parts = formatter::format_result(pc_name, "shell", command, &output, work_dir);
            for (i, part) in parts.iter().enumerate() {
                if i == 0 {
                    let _ = bot
                        .edit_message_text(running_msg.chat.id, running_msg.id, part)
                        .parse_mode(ParseMode::MarkdownV2)
                        .await;
                } else {
                    let _ = bot
                        .send_message(chat_id, part)
                        .parse_mode(ParseMode::MarkdownV2)
                        .await;
                }
            }
        }
        Err(e) => {
            let error_text = format!("❌ [{pc_name} · shell]\n📁 {work_dir}\nError: {e}");
            let _ = bot
                .edit_message_text(running_msg.chat.id, running_msg.id, &error_text)
                .await;
        }
    }
}

/// Handle agent task with queue/claim
async fn handle_agent_queue(
    bot: &Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    prompt: &str,
    work_dir: &str,
    config: &Config,
    claim_tracker: &Arc<ClaimTracker>,
) {
    let pc_name = &config.pc_name;

    let agent = match agent_selector::select_agent(&config.agent_priority) {
        Some(a) => a,
        None => return, // No agent available — silently skip
    };

    let claim_msg = match claim::try_claim(
        bot,
        chat_id,
        msg_id,
        pc_name,
        &agent,
        config.claim_delay_max_ms,
        claim_tracker,
    )
    .await
    {
        Ok(Some(msg)) => msg,
        Ok(None) => return,
        Err(e) => {
            warn!("Claim error: {e}");
            return;
        }
    };

    handle_agent(
        bot,
        chat_id,
        msg_id,
        prompt,
        work_dir,
        config,
        Some(&claim_msg),
    )
    .await;
}

/// Handle agent execution
async fn handle_agent(
    bot: &Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    prompt: &str,
    work_dir: &str,
    config: &Config,
    claim_msg: Option<&Message>,
) {
    let pc_name = &config.pc_name;
    let agent = match agent_selector::select_agent(&config.agent_priority) {
        Some(a) => a,
        None => {
            let _ = bot
                .send_message(chat_id, format!("⚠️ [{pc_name}] No agent available"))
                .reply_parameters(ReplyParameters::new(msg_id))
                .await;
            return;
        }
    };

    info!(pc_name, agent, prompt, work_dir, "Executing agent task");

    let status_msg = if let Some(cm) = claim_msg {
        cm.clone()
    } else {
        match bot
            .send_message(
                chat_id,
                format!("🔒 [{pc_name}] đang xử lý ({agent})...\n📁 {work_dir}"),
            )
            .reply_parameters(ReplyParameters::new(msg_id))
            .await
        {
            Ok(m) => m,
            Err(e) => {
                error!("Failed to send status message: {e}");
                return;
            }
        }
    };

    match executor::agent::run(&agent, prompt, work_dir, config.agent_timeout_secs).await {
        Ok(summary) => {
            let escaped_summary = formatter::escape_markdown(&summary);
            let escaped_work_dir = formatter::escape_markdown(work_dir);
            let result_text = format!("✅ [{pc_name} · {agent}]\n📁 {escaped_work_dir}\n{escaped_summary}");
            let _ = claim::update_claim(bot, &status_msg, &result_text).await;
        }
        Err(e) => {
            let error_str = format!("{e}");
            let escaped_error = formatter::escape_markdown(&error_str);
            let escaped_work_dir = formatter::escape_markdown(work_dir);
            let error_text = format!("❌ [{pc_name} · {agent}]\n📁 {escaped_work_dir}\nError: {escaped_error}");
            let _ = claim::update_claim(bot, &status_msg, &error_text).await;
        }
    }
}
