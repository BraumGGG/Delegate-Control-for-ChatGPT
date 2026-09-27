const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const required = [
  ["src-tauri/src/config.rs", "projects"],
  ["src-tauri/src/process_manager.rs", "start_all"],
  ["src-tauri/src/proxy_config.rs", "hot_reload"],
  ["src-tauri/src/proxy_config.rs", "build_router_registry"],
  ["vendor/chatgpt-delegate/src/chatgpt_delegate/router.py", "create_router_mcp_server"],
  ["vendor/chatgpt-delegate/src/chatgpt_delegate/connector.py", "append_text_file"],
  ["vendor/chatgpt-delegate/src/chatgpt_delegate/text_editing.py", "expected_sha256"],
  ["src/App.tsx", "chooseDirectory"],
  ["src/types.ts", "active_project_id"],
];

for (const [relative, marker] of required) {
  const file = path.join(root, relative);
  const content = fs.readFileSync(file, "utf8");
  if (!content.includes(marker)) {
    throw new Error(`${relative} 缺少预期标记：${marker}`);
  }
}

const managerSource = fs.readFileSync(path.join(root, "src-tauri/src/process_manager.rs"), "utf8");
const stopProjectStart = managerSource.indexOf("pub fn stop_project");
const stopProjectEnd = managerSource.indexOf("pub fn read_logs", stopProjectStart);
const stopProjectBody = managerSource.slice(stopProjectStart, stopProjectEnd);
if (stopProjectBody.includes("self.stop_internal()")) {
  throw new Error("stop_project 不得通过 stop_internal 全局终止其他项目");
}
const refreshStart = managerSource.indexOf("fn refresh_process_state");
const refreshEnd = managerSource.indexOf("fn stop_internal", refreshStart);
const refreshBody = managerSource.slice(refreshStart, refreshEnd);
if (refreshBody.includes("self.stop_internal()")) {
  throw new Error("共享进程异常时不得通过 stop_internal 全局终止项目 MCP");
}
if (!managerSource.includes("fn fail_preserving_projects")) {
  throw new Error("缺少保留项目进程的失败路径");
}

const forbidden = /(?:sk-[A-Za-z0-9_-]{20,}|CONTROL_PLANE_API_KEY\s*[:=]\s*[^$<\n`]{20,})/i;
for (const relative of ["README.md", "src-tauri/src/proxy_config.rs"]) {
  const content = fs.readFileSync(path.join(root, relative), "utf8");
  if (forbidden.test(content)) {
    throw new Error(`${relative} 可能包含 Runtime API Key 内容`);
  }
}

console.log("multi-project contract checks passed");
