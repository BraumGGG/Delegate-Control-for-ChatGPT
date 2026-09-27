const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");
const { chromium } = require("playwright");

const root = path.join(__dirname, "..");
const outputDir = path.join(root, "artifacts", "project-identity-review");
const appUrl = "http://127.0.0.1:1420";
const viteBin = path.join(path.dirname(require.resolve("vite/package.json", { paths: [root] })), "bin", "vite.js");
fs.mkdirSync(outputDir, { recursive: true });

const initialStatus = {
  overall: "stopped", proxy_ready: false, router_ready: false, mcp_proxy_ready: false, mcp_ready: false, tunnel_ready: false,
  proxy_pid: null, router_pid: null, mcp_pid: null, tunnel_pid: null, credential_configured: true, text_editing_available: true,
  connector_capability_message: "当前 MCP 支持项目内文本文件编辑。", message: "连接仅在需要时启动。",
  projects: [
    { project_id: "screencast", name: "ScreenCast", output_directory: "D:\\Projects\\ScreenCast", overall: "stopped", mcp_ready: false, mcp_pid: null, message: "项目未启动。" },
    { project_id: "new-project", name: "Realize", output_directory: "D:\\Projects\\Realize", overall: "stopped", mcp_ready: false, mcp_pid: null, message: "项目未启动。" },
    { project_id: "new-project-2", name: "DCFC", output_directory: "D:\\Projects\\DCFC", overall: "stopped", mcp_ready: false, mcp_pid: null, message: "项目未启动。" },
  ],
};

const initialSettings = {
  proxy_host: "127.0.0.1", proxy_port: 7897, mcp_proxy_host: "127.0.0.1", mcp_proxy_port: 8100, router_port: 8101,
  health_port: 8080, health_host: "127.0.0.1", profile_name: "chatgpt-delegate",
  mcp_executable: "D:\\DCFC\\install\\chatgpt-delegate-edit.exe", proxy_executable: "D:\\DCFC\\install\\mcp-proxy.exe",
  proxy_config_path: "C:\\Users\\Redmi\\AppData\\Roaming\\Delegate Control\\mcp-proxy.toml",
  router_config_path: "C:\\Users\\Redmi\\AppData\\Roaming\\Delegate Control\\router-projects.json",
  tunnel_executable: "D:\\tunnel-client\\install\\tunnel-client.exe",
  projects: [
    { id: "screencast", name: "ScreenCast", output_directory: "D:\\Projects\\ScreenCast", mcp_host: "127.0.0.1", mcp_port: 8002, enabled: true },
    { id: "new-project", name: "Realize", output_directory: "D:\\Projects\\Realize", mcp_host: "127.0.0.1", mcp_port: 8000, enabled: true },
    { id: "new-project-2", name: "DCFC", output_directory: "D:\\Projects\\DCFC", mcp_host: "127.0.0.1", mcp_port: 8001, enabled: true },
  ],
  active_project_id: "new-project-2",
};

async function serverAvailable() {
  try { return (await fetch(appUrl)).ok; } catch { return false; }
}

async function waitForServer() {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    if (await serverAvailable()) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("Vite server did not become ready");
}

async function installMock(page) {
  await page.addInitScript(({ status, settings }) => {
    let callbackId = 1;
    let currentStatus = structuredClone(status);
    let currentSettings = structuredClone(settings);
    const syncStatus = () => {
      currentStatus.projects = currentSettings.projects.map((project) => ({
        project_id: project.id, name: project.name, output_directory: project.output_directory,
        overall: "stopped", mcp_ready: false, mcp_pid: null, message: "项目未启动。",
      }));
    };
    window.__dcfcMock = { getSettings: () => structuredClone(currentSettings) };
    window.__TAURI_INTERNALS__ = {
      callbacks: new Map(),
      transformCallback(callback) { const id = callbackId++; this.callbacks.set(id, callback); return id; },
      unregisterCallback(id) { this.callbacks.delete(id); },
      convertFileSrc(filePath) { return filePath; },
      async invoke(command, args = {}) {
        if (["get_status", "start_delegate", "stop_delegate", "start_all_projects", "stop_all_projects", "start_project", "stop_project"].includes(command)) return structuredClone(currentStatus);
        if (command === "get_settings") return structuredClone(currentSettings);
        if (command === "save_settings") { currentSettings = structuredClone(args.settings); syncStatus(); return structuredClone(currentSettings); }
        if (command === "migrate_project_key") {
          if (args.newProjectId === "blocked") throw new Error("新 Project Key 的日志目录已存在，请先处理目录冲突。");
          currentSettings.projects = currentSettings.projects.map((project) => project.id === args.oldProjectId ? { ...project, id: args.newProjectId } : project);
          if (currentSettings.active_project_id === args.oldProjectId) currentSettings.active_project_id = args.newProjectId;
          syncStatus();
          return structuredClone(currentSettings);
        }
        if (command === "read_logs") return "INFO ready";
        if (command.includes("plugin:event")) return 1;
        return null;
      },
    };
  }, { status: initialStatus, settings: initialSettings });
}

async function openSettings(page) {
  await page.goto(appUrl, { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "项目管理" }).click();
  await page.getByRole("heading", { name: "项目管理" }).waitFor();
}

async function currentProjectIds(page) {
  return page.evaluate(() => window.__dcfcMock.getSettings().projects.map((project) => project.id));
}

(async () => {
  let server;
  if (!(await serverAvailable())) {
    server = spawn(process.execPath, [viteBin, "--host", "127.0.0.1", "--port", "1420", "--strictPort"], {
      cwd: root, windowsHide: true, stdio: "ignore",
    });
    await waitForServer();
  }

  const browser = await chromium.launch({ channel: "msedge", headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1180, height: 800 } });
    await installMock(page);
    await openSettings(page);

    assert.equal(await page.getByText("建议清理旧 Project Key").count(), 2);
    assert.equal(await page.getByLabel("new-project 迁移为 realize").count(), 1);
    assert.equal(await page.getByLabel("new-project-2 迁移为 dcfc").count(), 1);
    assert.equal(await page.locator(".project-editor-row").filter({ hasText: "ScreenCast" }).getByText("建议清理旧 Project Key").count(), 0);
    await page.screenshot({ path: path.join(outputDir, "01-legacy-guidance-1180x800.png"), fullPage: true });

    await page.getByRole("button", { name: "稍后处理 Realize" }).click();
    assert.equal(await page.getByRole("button", { name: "迁移 Realize 的 Project Key" }).count(), 0);
    assert.deepEqual(await currentProjectIds(page), ["screencast", "new-project", "new-project-2"]);

    await page.reload({ waitUntil: "networkidle" });
    await page.getByRole("button", { name: "项目管理" }).click();
    await page.getByRole("button", { name: "迁移 Realize 的 Project Key" }).click();
    await page.getByText("迁移旧 Project Key").waitFor();
    await page.screenshot({ path: path.join(outputDir, "02-realize-confirmation-1180x800.png"), fullPage: true });
    await page.getByRole("button", { name: "取消" }).click();
    assert.deepEqual(await currentProjectIds(page), ["screencast", "new-project", "new-project-2"]);

    await page.getByRole("button", { name: "迁移 Realize 的 Project Key" }).click();
    await page.getByLabel("新的 Project Key").fill("screencast");
    await page.getByRole("alert").waitFor();
    assert.match(await page.getByRole("alert").innerText(), /已存在/);
    assert.equal(await page.getByRole("button", { name: "确认迁移" }).isDisabled(), true);

    await page.getByLabel("新的 Project Key").fill("blocked");
    await page.getByRole("button", { name: "确认迁移" }).click();
    await page.getByText(/日志目录已存在/).waitFor();
    assert.deepEqual(await currentProjectIds(page), ["screencast", "new-project", "new-project-2"]);

    await page.getByRole("button", { name: "取消" }).click();
    const screenCastRow = page.locator(".project-editor-row").filter({ hasText: "ScreenCast" });
    await screenCastRow.getByLabel("项目名称").fill("ScreenCast Renamed");
    assert.equal(await screenCastRow.locator(".project-key-display code").innerText(), "screencast");

    await page.reload({ waitUntil: "networkidle" });
    await page.getByRole("button", { name: "项目管理" }).click();
    await page.getByRole("button", { name: "迁移 Realize 的 Project Key" }).click();
    await page.getByRole("button", { name: "确认迁移" }).click();
    await page.getByText(/new-project 迁移为 realize/).waitFor();
    assert.deepEqual(await currentProjectIds(page), ["screencast", "realize", "new-project-2"]);

    await page.getByRole("button", { name: "迁移 DCFC 的 Project Key" }).click();
    await page.getByRole("button", { name: "确认迁移" }).click();
    await page.getByText(/new-project-2 迁移为 dcfc/).waitFor();
    assert.deepEqual(await currentProjectIds(page), ["screencast", "realize", "dcfc"]);
    assert.equal(await page.getByText("建议清理旧 Project Key").count(), 0);
    await page.locator(".view").evaluate((element) => { element.scrollTop = 0; });
    await page.screenshot({ path: path.join(outputDir, "03-canonical-keys-1180x800.png"), fullPage: true });
    await page.close();

    const narrow = await browser.newPage({ viewport: { width: 900, height: 700 } });
    await installMock(narrow);
    await openSettings(narrow);
    await narrow.getByRole("button", { name: "迁移 DCFC 的 Project Key" }).click();
    await narrow.getByLabel("新的 Project Key").fill("screencast");
    assert.equal(await narrow.getByRole("button", { name: "确认迁移" }).isDisabled(), true);
    const overflow = await narrow.evaluate(() => ({
      body: document.body.scrollWidth > document.body.clientWidth + 1,
      clipped: [...document.querySelectorAll("button, strong")].filter((element) => element.scrollWidth > element.clientWidth + 1).map((element) => element.textContent?.trim()).filter(Boolean),
    }));
    assert.equal(overflow.body, false);
    assert.deepEqual(overflow.clipped, []);
    await narrow.screenshot({ path: path.join(outputDir, "04-validation-900x700.png"), fullPage: true });

    await narrow.getByRole("button", { name: "取消" }).click();
    await narrow.getByRole("button", { name: "新增项目" }).click();
    const draftRow = narrow.locator(".project-editor-row").last();
    await draftRow.getByLabel("项目名称").fill("Atlas");
    assert.equal(await draftRow.getByLabel("Project Key").inputValue(), "atlas");
    await draftRow.getByLabel("Project Key").fill("Bad Key");
    await narrow.getByRole("button", { name: "保存设置" }).click();
    await narrow.getByText(/Project Key 无效/).waitFor();
    await draftRow.scrollIntoViewIfNeeded();
    await narrow.screenshot({ path: path.join(outputDir, "05-new-project-validation-900x700.png") });
    await narrow.close();

    console.log(JSON.stringify({ status: "ok", screenshots: fs.readdirSync(outputDir).sort() }, null, 2));
  } finally {
    await browser.close();
    if (server && !server.killed) server.kill();
  }
})().catch((error) => { console.error(error); process.exitCode = 1; });
