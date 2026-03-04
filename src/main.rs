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
    /// Run bot, listen to group chat
    Run,
    /// Interactive setup (token, chat ID, PC name, agent priority)
    Setup,
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
    }
}
