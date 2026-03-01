mod agent_selector;
mod bot;
mod claim;
mod config;
mod executor;
mod formatter;

use clap::Parser;

#[derive(Parser)]
#[command(name = "icode", version, about = "Multi-PC Telegram Assistant")]
enum Cli {
    /// Chạy bot, lắng nghe group chat
    Run,
    /// Interactive setup (token, chat ID, tên PC, agent priority)
    Setup,
    /// Cài SKILL.md vào workspace hiện tại
    Init,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    match Cli::parse() {
        Cli::Run => {
            if let Err(e) = bot::run().await {
                eprintln!("❌ Bot error: {e}");
                std::process::exit(1);
            }
        }
        Cli::Setup => {
            if let Err(e) = config::setup() {
                eprintln!("❌ Setup error: {e}");
                std::process::exit(1);
            }
        }
        Cli::Init => {
            if let Err(e) = init_skill() {
                eprintln!("❌ Init error: {e}");
                std::process::exit(1);
            }
        }
    }
}

/// Install SKILL.md into current workspace
fn init_skill() -> anyhow::Result<()> {
    let skill_dir = std::env::current_dir()?.join(".agent/skills/icode");
    std::fs::create_dir_all(&skill_dir)?;
    let skill_path = skill_dir.join("SKILL.md");
    std::fs::write(&skill_path, include_str!("../skill/SKILL.md"))?;
    println!("✅ Installed SKILL.md at {}", skill_path.display());
    Ok(())
}
