---
name: chatgpt-delegate
description: Use when Codex should ask ChatGPT Web/App to research, think, compare, write, design, draft PRDs, look up current information, or save ChatGPT-generated Markdown while Codex only orchestrates and does not generate the substantive content. Trigger on requests like "let ChatGPT research...", "让 ChatGPT 去...", "请 ChatGPT 调研...", "用 ChatGPT 想一下...", "查一下最新...", or explicit token-saving ChatGPT delegation.
---

# ChatGPT Delegate

Use this skill to delegate substantive work to ChatGPT Web/App through a local MCP connector. Codex prepares the task, opens ChatGPT with the official Chrome tool, and waits for a file result. ChatGPT performs the research, thinking, writing, or lookup and calls the MCP tool to save Markdown locally.

## Hard Boundaries

- Keep Codex as the orchestrator only. Do not write the delegated report, PRD, comparison, or lookup answer in Codex.
- Do not use `codex exec --search`, OpenAI API, private ChatGPT APIs, browser session tokens, cookies, reverse-engineered endpoints, or hidden ChatGPT traffic.
- Do not paste the full result into the final response by default. Return status, profile, byte count, and a clickable `result.md` path.
- Read `result.md` only when the user explicitly asks Codex to read, summarize, or edit the saved output.
- Treat temporary no-auth tunnels as short-lived. Stop the tunnel and server after the run unless the user asks to keep them available.

## Required Repo

Run connector commands from the repo checkout that contains `chatgpt-delegate`:

```bash
cd /Users/longbiao/Projects/chatgpt-delegate
```

If that path is unavailable, locate the checkout containing `src/chatgpt_delegate/connector.py` and `docs/connector.md`.

## Profile Routing

Use `--kind auto` unless the user explicitly requests a mode.

| User intent | CLI kind | Prompt profile |
| --- | --- | --- |
| 调研, 深度研究, 社区最佳实践, 研究一下 | `research` | `research_pro` |
| 设计, 构思, 思考, 比较, 撰写, 方案, PRD | `think` | `thinking_high` |
| 查一下, 找一下, 确认, 最新 | `lookup` | `instant_web` |

## Workflow

1. Prepare the delegation task:

   ```bash
   chatgpt-delegate prepare "<user request>" --kind auto
   ```

   Keep the returned `task_id`, `profile`, and `prompt_path`.

2. Ensure the local MCP server is reachable through HTTPS:

   ```bash
   chatgpt-delegate serve --host 127.0.0.1 --port 8000 --public-host <public-host>
   ```

   The ChatGPT connector URL is `https://<public-host>/mcp` with no trailing slash.

3. Use the official Chrome control tool to open ChatGPT Web. In the composer, type `@`, select the ChatGPT Delegate Connector app, then send the full `prompt.md` content.

4. Poll status without reading the Markdown body:

   ```bash
   chatgpt-delegate status <task_id>
   ```

5. When completed, return only the result metadata and file link:

   ```bash
   chatgpt-delegate read <task_id> --metadata-only
   ```

6. Stop any temporary no-auth tunnel and local server unless the user asked to keep them running.

## ChatGPT App Notes

- Newly connected apps are more reliable when explicitly selected with `@ChatGPT Delegate Connector` before sending the prompt.
- If ChatGPT stalls at `Looking for available tools`, start a fresh ChatGPT conversation, select the connector with `@`, and resend the same `prompt.md`.
- If a random tunnel host changes and the ChatGPT Developer app does not expose URL editing, create a fresh dev app for the new `https://<public-host>/mcp` URL and delete the stale temporary app when safe.
- Prefer a fixed public host for repeat use.

## Expected MCP Write

The prompt tells ChatGPT to call:

```text
save_task_result(task_id, title, markdown, summary=None, overwrite=True)
```

Successful output lands under:

```text
artifacts/chatgpt-delegation/<task_id>/result.md
artifacts/chatgpt-delegation/<task_id>/result.json
```

`connector_status()` must report `codex_execution=false`, `openai_api=false`, and `private_chatgpt_api=false`.

## Failure Handling

- If Chrome automation is unavailable, report the blocker and leave the prepared `prompt.md` path for manual ChatGPT submission.
- If `chatgpt-delegate serve` cannot bind port 8000, check for an existing connector process before starting another one.
- If ChatGPT produces prose but does not call the tool, resend a short follow-up in ChatGPT asking it to call `save_task_result` for the existing `task_id`.
- If `status` remains pending after ChatGPT claims completion, inspect ChatGPT for tool-call errors before reading or regenerating content in Codex.
- If the user asks for a direct answer instead of a saved file, clarify that using Codex to generate the answer defeats the token-saving delegation goal.
