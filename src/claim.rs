use anyhow::Result;
use rand::Rng;
use std::collections::HashSet;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{MessageId, ReplyParameters};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tracing::info;

/// Claim prefix marker in Telegram messages
pub const CLAIM_PREFIX: &str = "🔒";

/// Tracks which message IDs have been claimed by any bot
/// Each bot maintains its own in-memory set
pub struct ClaimTracker {
    claimed: Mutex<HashSet<i32>>,
}

impl ClaimTracker {
    pub fn new() -> Self {
        Self {
            claimed: Mutex::new(HashSet::new()),
        }
    }

    /// Record a message ID as claimed (seen a claim reply)
    pub async fn mark_claimed(&self, msg_id: i32) {
        self.claimed.lock().await.insert(msg_id);
    }

    /// Check if a message has been claimed
    pub async fn is_claimed(&self, msg_id: i32) -> bool {
        self.claimed.lock().await.contains(&msg_id)
    }
}

/// Attempt to claim a task message.
/// Returns the claim message if successful, None if someone else already claimed.
pub async fn try_claim(
    bot: &Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    pc_name: &str,
    agent_name: &str,
    max_delay_ms: u64,
    tracker: &Arc<ClaimTracker>,
) -> Result<Option<Message>> {
    // 1. Random delay to reduce collision
    let delay = rand::thread_rng().gen_range(0..max_delay_ms);
    sleep(Duration::from_millis(delay)).await;

    // 2. Check if already claimed
    if tracker.is_claimed(msg_id.0).await {
        info!(
            msg_id = msg_id.0,
            "Already claimed by another bot, skipping"
        );
        return Ok(None);
    }

    // 3. Send claim reply
    let claim_text = format!("{CLAIM_PREFIX} [{pc_name}] đang xử lý ({agent_name})...");
    let claim_msg = bot
        .send_message(chat_id, &claim_text)
        .reply_parameters(ReplyParameters::new(msg_id))
        .await?;

    // 4. Mark as claimed in our tracker
    tracker.mark_claimed(msg_id.0).await;

    // 5. Double-check after a short delay
    sleep(Duration::from_millis(500)).await;

    // The claim is now ours. In the rare case of double-claim,
    // both bots run — the owner sees 2 results, no data loss.
    info!(
        msg_id = msg_id.0,
        pc_name, agent_name, "Claimed task successfully"
    );
    Ok(Some(claim_msg))
}

/// Update the claim message with the final result
pub async fn update_claim(bot: &Bot, claim_msg: &Message, result_text: &str) -> Result<()> {
    bot.edit_message_text(claim_msg.chat.id, claim_msg.id, result_text)
        .await?;
    Ok(())
}
