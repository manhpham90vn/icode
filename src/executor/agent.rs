use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{info, warn};
use uuid::Uuid;

use crate::agent_selector;
use crate::config::Config;

const SKILL_RELATIVE_PATH: &str = ".agent/skills/icode/SKILL.md";
const SKILL_CONTENT: &str = include_str!("../../skill/SKILL.md");

#[derive(Serialize, Deserialize, Debug)]
pub struct TaskFile {
    pub id: String,
    pub prompt: String,
    pub work_dir: String,
    pub agent: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ResultFile {
    pub id: String,
    pub status: String, // "success" | "error"
    pub summary: String,
    pub completed_at: Option<String>,
}

/// Run an agent task: write task file, spawn agent, poll result
pub async fn run(
    agent: &str,
    prompt: &str,
    work_dir: &str,
    timeout_secs: u64,
) -> Result<String> {
    let task_id = Uuid::new_v4().to_string();
    let tasks_dir = Config::tasks_dir()?;

    // 1. Write task file
    let task_file_path = tasks_dir.join(format!("{task_id}.json"));
    let task = TaskFile {
        id: task_id.clone(),
        prompt: prompt.to_string(),
        work_dir: work_dir.to_string(),
        agent: agent.to_string(),
        created_at: Utc::now().to_rfc3339(),
    };
    let task_json = serde_json::to_string_pretty(&task)?;
    std::fs::write(&task_file_path, &task_json)?;
    info!(agent, task_id, "Task file written: {}", task_file_path.display());

    // 2. Install SKILL.md into work_dir if not present
    install_skill(work_dir)?;

    // 3. Build agent command
    let skill_path = Path::new(work_dir).join(SKILL_RELATIVE_PATH);
    let skill_path_str = skill_path.to_string_lossy();
    let task_path_str = task_file_path.to_string_lossy();
    let cmd_args = agent_selector::get_agent_command(agent, &task_path_str, &skill_path_str)
        .context(format!("Unknown agent: {agent}"))?;

    // 4. Spawn agent
    info!(agent, "Spawning agent: {:?}", cmd_args);
    let mut child = Command::new(&cmd_args[0])
        .args(&cmd_args[1..])
        .current_dir(work_dir)
        .spawn()
        .with_context(|| format!("Failed to spawn agent: {}", cmd_args[0]))?;

    // 5. Poll for result file
    let result_file_path = PathBuf::from(format!("{}.result.json", task_file_path.display()));
    let poll_result = poll_result_file(&result_file_path, &mut child);

    match timeout(Duration::from_secs(timeout_secs), poll_result).await {
        Ok(Ok(result)) => {
            cleanup_task_files(&task_file_path, &result_file_path);
            match result.status.as_str() {
                "success" => Ok(result.summary),
                "error" => Ok(format!("❌ Error: {}", result.summary)),
                _ => Ok(format!("⚠️ Unknown status: {}", result.summary)),
            }
        }
        Ok(Err(e)) => {
            let _ = child.kill().await;
            cleanup_task_files(&task_file_path, &result_file_path);
            bail!("Agent error: {e}")
        }
        Err(_) => {
            let _ = child.kill().await;
            cleanup_task_files(&task_file_path, &result_file_path);
            bail!("Agent timeout after {timeout_secs}s")
        }
    }
}

/// Install SKILL.md into workspace
fn install_skill(work_dir: &str) -> Result<()> {
    let skill_path = Path::new(work_dir).join(SKILL_RELATIVE_PATH);
    if !skill_path.exists() {
        if let Some(parent) = skill_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&skill_path, SKILL_CONTENT)?;
        info!("Installed SKILL.md at {}", skill_path.display());
    }
    Ok(())
}

/// Poll for result file, also wait for agent process to finish
async fn poll_result_file(
    result_path: &PathBuf,
    child: &mut tokio::process::Child,
) -> Result<ResultFile> {
    loop {
        // Check if result file exists
        if result_path.exists() {
            let content = tokio::fs::read_to_string(&result_path).await?;
            match serde_json::from_str::<ResultFile>(&content) {
                Ok(result) => return Ok(result),
                Err(e) => {
                    warn!("Result file parse error, retrying: {e}");
                }
            }
        }

        // Check if process has exited
        if let Some(status) = child.try_wait()? {
            // Process done, check result one more time
            tokio::time::sleep(Duration::from_millis(500)).await;
            if result_path.exists() {
                let content = tokio::fs::read_to_string(&result_path).await?;
                let result: ResultFile = serde_json::from_str(&content)?;
                return Ok(result);
            }
            bail!(
                "Agent exited with {} but no result file written",
                status.code().unwrap_or(-1)
            );
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Clean up task + result files
fn cleanup_task_files(task_path: &Path, result_path: &Path) {
    let _ = std::fs::remove_file(task_path);
    let _ = std::fs::remove_file(result_path);
}

