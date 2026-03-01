use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{info, warn};

/// Run a shell command with streaming output and timeout
pub async fn run(
    command: &str,
    work_dir: &str,
    timeout_secs: u64,
    on_output: impl Fn(&str),
) -> Result<String> {
    info!(command, work_dir, "Executing shell command");

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn shell process")?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut output = String::new();
    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let collect_output = async {
        loop {
            tokio::select! {
                line = stdout_reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            output.push_str(&line);
                            output.push('\n');
                            on_output(&output);
                        }
                        Ok(None) => break,
                        Err(e) => {
                            warn!("Error reading stdout: {e}");
                            break;
                        }
                    }
                }
                line = stderr_reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            output.push_str("[stderr] ");
                            output.push_str(&line);
                            output.push('\n');
                            on_output(&output);
                        }
                        Ok(None) => {}
                        Err(e) => {
                            warn!("Error reading stderr: {e}");
                        }
                    }
                }
            }
        }
        // Wait for process to finish
        let status = child.wait().await?;
        if !status.success() {
            output.push_str(&format!("\n[exit code: {}]", status.code().unwrap_or(-1)));
        }
        Ok::<String, anyhow::Error>(output.clone())
    };

    match timeout(Duration::from_secs(timeout_secs), collect_output).await {
        Ok(result) => result,
        Err(_) => {
            // Kill the process on timeout
            let _ = child.kill().await;
            output.push_str(&format!("\n[TIMEOUT after {timeout_secs}s]"));
            Ok(output)
        }
    }
}
