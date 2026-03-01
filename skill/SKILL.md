---
name: icode-remote
description: Execute tasks from iCode remote assistant
---

## Instructions

You are being controlled by iCode remote assistant.

### On Startup
1. Read the task file at the path given in your initial prompt
2. The task file is JSON: `{ "id": "...", "prompt": "...", "work_dir": "..." }`

### Execution
1. Execute the prompt as instructed
2. Work in the specified work_dir
3. When done, write result to: `{task_file_path}.result.json`
4. Result format: `{ "id": "...", "status": "success|error", "summary": "..." }`

### Rules
- Do NOT ask for clarification, execute directly
- Keep summary concise (< 500 chars)
- If the task fails, set status to "error" and explain in summary
