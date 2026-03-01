# iCode

A multi-PC Telegram assistant that turns a group chat into a shared coding workspace. Each PC runs its own bot instance — when a task is sent, one PC claims it and executes it using the best available AI agent.

## How It Works

```
         ┌────────────────────────────────┐
         │     Telegram Group Chat        │
         │                                │
         │   Owner sends: "fix login bug" │
         └──┬──────────────┬──────────────┘
            │              │
         ┌──▼──┐        ┌──▼──┐
         │ PC1 │        │ PC2 │
         │codex│        │claude│
         └──┬──┘        └──┬──┘
            │              │
   PC1 claims first ───────┘ PC2 skips
            │
   Spawns codex → executes → replies result
```

Multiple PCs join the same group chat, each with its own bot token. When the owner sends a task:

1. All bots receive the message
2. A **claim race** runs (random delay 0–1s to avoid collisions)
3. The first bot to claim selects its highest-priority **installed** agent
4. The agent executes the task via a SKILL file protocol
5. The result is posted back to the group

If a PC is offline, another PC picks up the task automatically.

## Agent Priority

Each PC configures a priority list of AI agents. The bot picks the first one that's actually installed:

```
PC1: ["codex", "claude", "antigravity"]  →  uses codex (installed)
PC2: ["claude", "antigravity"]           →  uses claude (installed)
```

Supported agents: `codex`, `claude`, `antigravity`

## Commands

### Agent Tasks (auto claim)

| Message | Action |
|---------|--------|
| `fix bug in main.rs` | One PC claims and runs with best available agent |

### Shell Commands (mention bot)

| Message | Action |
|---------|--------|
| `@bot ls -la` | That specific bot runs the shell command |
| `@bot cd /path` | Changes the bot's working directory (persisted) |

### System Commands

| Message | Action |
|---------|--------|
| `/status@pc1` | PC1 replies with status info |
| `/status *` | All PCs reply |
| `/cancel@pc1` | Cancel running task on PC1 |
| `/help` | All bots reply with help |

## Project Structure

```
icode/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point
│   ├── config.rs            # Config management
│   ├── bot.rs               # Telegram bot + message dispatch
│   ├── claim.rs             # Queue/claim race logic
│   ├── agent_selector.rs    # Agent priority + install check
│   ├── executor/
│   │   ├── mod.rs
│   │   ├── shell.rs         # Shell command runner
│   │   └── agent.rs         # Agent spawn + SKILL workflow
│   └── formatter.rs         # Telegram output formatting
└── skill/
    └── SKILL.md             # Instructions for agents
```

## Setup

1. Create a bot via `@BotFather` and get the token
2. Create a group chat and add the bot
3. Disable Group Privacy: `@BotFather` → `/setprivacy` → Disable
4. Run the setup wizard:

```bash
icode setup
```

## Usage

```bash
icode run       # Start listening to the group chat
icode setup     # Interactive configuration
icode init      # Install SKILL.md into current workspace
```

## Configuration

Stored at `~/.config/icode/config.json` (Linux):

```json
{
  "bot_token": "123456:ABC...",
  "chat_id": -100123456789,
  "owner_ids": [123456789],
  "pc_name": "desktop-home",
  "work_dir": "/home/user/projects",
  "agent_priority": ["codex", "claude", "antigravity"],
  "shell_timeout_secs": 300,
  "agent_timeout_secs": 600,
  "claim_delay_max_ms": 1000
}
```

| Field | Description |
|-------|-------------|
| `bot_token` | Telegram bot token (unique per PC) |
| `chat_id` | Shared group chat ID |
| `owner_ids` | Telegram user IDs authorized to send commands |
| `pc_name` | Display name shown in replies |
| `work_dir` | Default working directory for tasks |
| `agent_priority` | Ordered list of preferred agents |
| `shell_timeout_secs` | Shell command timeout (default: 300) |
| `agent_timeout_secs` | Agent task timeout (default: 600) |
| `claim_delay_max_ms` | Max random delay for claim race (default: 1000) |

## SKILL Protocol

When an agent is spawned, it follows a SKILL file protocol:

1. Read the task file (`~/.icode/tasks/{uuid}.json`)
2. Execute the prompt in the specified `work_dir`
3. Write the result to `{task_file}.result.json`

Task file:
```json
{
  "id": "uuid",
  "prompt": "fix bug in main.rs",
  "work_dir": "/home/user/project",
  "agent": "codex",
  "created_at": "2026-03-02T10:30:00Z"
}
```

Result file:
```json
{
  "id": "uuid",
  "status": "success",
  "summary": "Fixed null pointer in line 42",
  "completed_at": "2026-03-02T10:35:00Z"
}
```

## Build

```bash
cargo build && cargo clippy
```
