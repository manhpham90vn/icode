/// Escape MarkdownV2 special characters
/// Characters that must be escaped: _ * [ ] ( ) ~ ` > # + - = | { } . !
pub fn escape_markdown(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '_' | '*' | '[' | ']' | '(' | ')' | '~' | '`' | '>' | '#' | '+' | '-' | '=' | '|'
            | '{' | '}' | '.' | '!' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result
}
const MAX_MSG_LEN: usize = 4096;

/// Format command result for Telegram
pub fn format_result(
    pc_name: &str,
    label: &str,
    command: &str,
    output: &str,
    work_dir: &str,
) -> Vec<String> {
    let header = format!("🖥️ *{pc_name}* — `{label}`\n📁 `{work_dir}`\n⚡ `{command}`");
    let body = if output.is_empty() {
        format!("{header}\n\n_(no output)_")
    } else {
        format!("{header}\n\n```shell\n{}\n```", truncate_output(output))
    };
    split_message(&body, MAX_MSG_LEN)
}

/// Format status info
pub fn format_status(
    pc_name: &str,
    uptime_secs: u64,
    agent_priority: &[String],
    available_agents: &[String],
    work_dir: &str,
) -> String {
    let uptime = format_uptime(uptime_secs);
    let os_info = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let hostname = gethostname::gethostname();
    let hostname = hostname.to_string_lossy();

    let priority_str = if agent_priority.is_empty() {
        "(none)".to_string()
    } else {
        agent_priority.join(", ")
    };

    let available_str = if available_agents.is_empty() {
        "⚠️ none".to_string()
    } else {
        available_agents
            .iter()
            .map(|a| format!("✅ {a}"))
            .collect::<Vec<_>>()
            .join(", ")
    };

    format!(
        "📊 *Status: {pc_name}*\n\
         ├ Hostname: `{hostname}`\n\
         ├ OS: `{os_info}/{arch}`\n\
         ├ Uptime: {uptime}\n\
         ├ Work dir: `{work_dir}`\n\
         ├ Priority: {priority_str}\n\
         └ Available: {available_str}"
    )
}

/// Format help text
pub fn format_help(pc_name: &str, bot_username: &str) -> String {
    format!(
        r#"📖 *Help — {pc_name}*

*AI Agent (default):*
Send directly: `fix bug in main.rs`
Or mention: `@{bot_username} fix bug in main.rs`

*Shell (mention bot):*
`@{bot_username} exec ls \-la` — run shell
`@{bot_username} cd /path` — change work dir

*System:*
`@{bot_username} status` or `@all status`
`@{bot_username} cancel`
`@{bot_username} help`"#
    )
}

/// Format uptime
fn format_uptime(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Truncate output if too long (keep head + tail), safe for Unicode
fn truncate_output(output: &str) -> String {
    let max = MAX_MSG_LEN - 200;
    if output.len() <= max {
        return output.to_string();
    }
    let half = max / 2;
    // Find safe char boundaries
    let head_end = output.floor_char_boundary(half);
    let tail_start = output.floor_char_boundary(output.len() - half);
    let head = &output[..head_end];
    let tail = &output[tail_start..];
    format!("{head}\n\n... (truncated) ...\n\n{tail}")
}

/// Split text into multiple messages respecting max length
pub fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut messages = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        if current.len() + line.len() + 1 > max_len {
            if !current.is_empty() {
                messages.push(current.clone());
                current.clear();
            }
            if line.len() > max_len {
                for chunk in line.as_bytes().chunks(max_len) {
                    messages.push(String::from_utf8_lossy(chunk).to_string());
                }
                continue;
            }
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        messages.push(current);
    }
    messages
}
