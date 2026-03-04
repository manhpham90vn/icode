use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub bot_token: String,
    pub chat_id: i64,
    pub owner_ids: Vec<i64>,
    pub pc_name: String,
    pub work_dir: String,
    pub agent_priority: Vec<String>,
    #[serde(default = "default_shell_timeout")]
    pub shell_timeout_secs: u64,
    #[serde(default = "default_agent_timeout")]
    pub agent_timeout_secs: u64,
    #[serde(default = "default_claim_delay")]
    pub claim_delay_max_ms: u64,
}

fn default_shell_timeout() -> u64 {
    300
}
fn default_agent_timeout() -> u64 {
    600
}
fn default_claim_delay() -> u64 {
    1000
}

impl Config {
    /// Platform-specific config path
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Cannot determine config directory")?;
        Ok(config_dir.join("icode").join("config.json"))
    }

    /// Load config from disk
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Cannot read config at {}", path.display()))?;
        let config: Config =
            serde_json::from_str(&content).context("Invalid config JSON format")?;
        Ok(config)
    }

    /// Save config to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Tasks directory: ~/.icode/tasks/
    pub fn tasks_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Cannot determine home directory")?;
        let dir = home.join(".icode").join("tasks");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

/// Interactive setup — prompts user for config values
pub fn setup() -> Result<()> {
    println!("🔧 iCode Setup\n");

    let bot_token = prompt("Telegram Bot Token")?;
    let chat_id: i64 = prompt("Group Chat ID (negative, e.g., -100123456789)")?.parse()?;
    let owner_id: i64 = prompt("Owner Telegram User ID")?.parse()?;
    let pc_name = prompt(&format!(
        "PC name [{}]",
        gethostname::gethostname().to_string_lossy()
    ))?;
    let pc_name = if pc_name.is_empty() {
        gethostname::gethostname().to_string_lossy().to_string()
    } else {
        pc_name
    };
    let work_dir = prompt(&format!(
        "Default work dir [{}]",
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .display()
    ))?;
    let work_dir = if work_dir.is_empty() {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .to_string_lossy()
            .to_string()
    } else {
        work_dir
    };
    let agents_input = prompt("Agent priority (comma-separated, e.g., claude)")?;
    let agent_priority: Vec<String> = agents_input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let config = Config {
        bot_token,
        chat_id,
        owner_ids: vec![owner_id],
        pc_name: pc_name.clone(),
        work_dir,
        agent_priority,
        shell_timeout_secs: default_shell_timeout(),
        agent_timeout_secs: default_agent_timeout(),
        claim_delay_max_ms: default_claim_delay(),
    };

    config.save()?;
    println!("\n✅ Config saved to {}", Config::config_path()?.display());
    println!("   PC name: {pc_name}");
    println!("   Agents: {:?}", config.agent_priority);

    // Auto-install SKILL.md to the selected work_dir
    let skill_dir = std::path::PathBuf::from(&config.work_dir).join(".agent/skills/icode");
    if let Err(e) = std::fs::create_dir_all(&skill_dir) {
        println!("⚠️ Warning: Failed to create skill directory: {e}");
    } else {
        let skill_path = skill_dir.join("SKILL.md");
        match std::fs::write(&skill_path, include_str!("../skill/SKILL.md")) {
            Ok(_) => println!("✅ Installed SKILL.md at {}", skill_path.display()),
            Err(e) => println!("⚠️ Warning: Failed to write SKILL.md: {e}"),
        }
    }

    Ok(())
}

fn prompt(label: &str) -> Result<String> {
    print!("{label}: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}
