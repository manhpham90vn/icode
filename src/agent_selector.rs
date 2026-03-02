/// Mapping agent name → binary name in PATH
const AGENT_BINARIES: &[(&str, &str)] = &[("claude", "claude")];

/// Get the binary name for an agent
pub fn get_binary_name(agent: &str) -> Option<&'static str> {
    AGENT_BINARIES
        .iter()
        .find(|(name, _)| *name == agent)
        .map(|(_, binary)| *binary)
}

/// Select the first installed agent from the priority list
pub fn select_agent(priority: &[String]) -> Option<String> {
    for agent in priority {
        if let Some(binary) = get_binary_name(agent) {
            if which::which(binary).is_ok() {
                return Some(agent.clone());
            }
        }
    }
    None
}

/// List all available (installed) agents from the priority list
pub fn list_available(priority: &[String]) -> Vec<String> {
    priority
        .iter()
        .filter(|a| {
            get_binary_name(a)
                .map(|b| which::which(b).is_ok())
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// Get the spawn command for an agent
pub fn get_agent_command(
    agent: &str,
    task_file_path: &str,
    skill_path: &str,
) -> Option<Vec<String>> {
    let binary = get_binary_name(agent)?;
    let prompt = format!("Read SKILL.md at {skill_path}, then execute task at {task_file_path}");
    let args = match agent {
        "claude" => vec![
            binary.to_string(),
            "-p".to_string(),
            "--dangerously-skip-permissions".to_string(),
            prompt,
        ],
        _ => return None,
    };
    Some(args)
}
