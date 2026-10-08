const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");
const { chromium } = require("playwright");

const root = path.join(__dirname, "..");
const appUrl = "http://127.0.0.1:1420";
const viteBin = path.join(path.dirname(require.resolve("vite/package.json", { paths: [root] })), "bin", "vite.js");

async function serverAvailable() {
  try { return (await fetch(appUrl)).ok; } catch { return false; }
}
async function waitForServer() {
  for (let i = 0; i < 80; i += 1) {
    if (await serverAvailable()) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("Vite server did not become ready");
}

async function installMock(page) {
  await page.addInitScript(() => {
    const projects = Array.from({ length: 12 }, (_, index) => ({
      id: `project-${index}`, name: `Project ${index}`, output_directory: `C:\\projects\\${index}`,
      mcp_host: "127.0.0.1", mcp_port: 8000 + index, enabled: true,
    }));
    const settings = {
      network_mode: "magic",
      proxy_host: "127.0.0.1", proxy_port: 7877, mcp_proxy_host: "127.0.0.1",
      mcp_proxy_port: 8100, router_port: 8101, health_host: "127.0.0.1", health_port: 8080,
      profile_name: "", tunnel_id: "tunnel-a", public_base_url: "https://dcfc.example.com",
      mcp_executable: "C:\\DCFC\\runtime\\chatgpt-delegate-edit.exe",
      proxy_executable: "C:\\DCFC\\runtime\\mcp-proxy\\mcp-proxy.exe", proxy_config_path: "C:\\DCFC\\proxy.toml",
      router_config_path: "C:\\DCFC\\router.json", tunnel_executable: "C:\\tools\\tunnel-client.exe",
      projects, active_project_id: "project-0",
    };
    let callbackId = 1;
    const status = {
      overall: "running", proxy_ready: true, router_ready: true, mcp_proxy_ready: true,
      mcp_ready: false, tunnel_ready: true, proxy_pid: 1, router_pid: 2, mcp_pid: 3,
      tunnel_pid: 4, credential_configured: true, text_editing_available: true,
      connector_capability_message: "已就绪", message: "连接运行中",
      projects: projects.map((project, index) => ({
        project_id: project.id, name: project.name, output_directory: project.output_directory,
        overall: index === 3 ? "failed" : "running", mcp_ready: index !== 3,
        mcp_pid: index === 3 ? null : 10 + index, message: index === 3 ? "项目失败" : "运行中",
      })),
    };
    const row = (id, label, scope, state, detail, projectId = null) => ({
      id, label, scope, project_id: projectId, state, detail, action: "检查对应日志和配置。",
    });
    const report = () => ({
      local_ready: true, registration_ready: false,
      registration_endpoint: settings.public_base_url ? `${settings.public_base_url}/` : null,
      checks: [
        row("configuration", "本地项目配置", "shared", "ready", "配置有效"),
        row("managed_runtime", "DCFC 本地运行组件", "shared", "ready", "完整"),
        row("dependencies", "外部运行依赖", "shared", "ready", "可用"),
        row("magic", "Magic 本机代理", "shared", "ready", "就绪"),
        row("tunnel_configuration", "Tunnel 配置", "shared", "ready", "已配置"),
        row("router", "共享 Router 与 MCP Proxy", "shared", "ready", "就绪"),
        ...projects.map((project, index) => row(
          `project:${project.id}`, project.name, "project", index === 3 ? "action" : "ready",
          index === 3 ? "该项目 MCP 失败" : "项目 MCP 已就绪", project.id,
        )),
      ],
    });
    window.__dcfcTestCalls = [];
    window.__doctorFail = false;
    window.__TAURI_INTERNALS__ = {
      callbacks: new Map(),
      transformCallback(callback) { const id = callbackId++; this.callbacks.set(id, callback); return id; },
      unregisterCallback(id) { this.callbacks.delete(id); },
      convertFileSrc(filePath) { return filePath; },
      async invoke(command, args = {}) {
        window.__dcfcTestCalls.push(command);
        if (command === "get_settings") return structuredClone(settings);
        if (command === "get_status") return structuredClone(status);
        if (command === "run_setup_doctor") {
          if (window.__doctorFail) throw new Error("诊断服务暂不可用");
          return structuredClone(report());
        }
        if (command === "get_runtime_readiness") return { mcp_available: true, proxy_available: true, tunnel_available: true, credential_configured: true };
        if (command === "save_public_base_url") {
          settings.public_base_url = args.publicBaseUrl.trim().replace(/\/$/, "");
          return settings.public_base_url;
        }
        if (command.includes("plugin:event")) return 1;
        throw new Error(`Unexpected IPC: ${command}`);
      },
    };
  });
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
    for (const width of [1180, 900]) {
      const context = await browser.newContext({ viewport: { width, height: 760 } });
      const page = await context.newPage();
      await installMock(page);
      await page.goto(appUrl, { waitUntil: "networkidle" });
      await page.getByRole("button", { name: "安装诊断" }).click();
      await page.getByRole("heading", { name: "安装诊断" }).waitFor();
      assert.equal(await page.locator(".doctor-section").filter({ hasText: "项目运行" }).locator(".doctor-row").count(), 12);
      assert.equal(await page.getByText("该项目 MCP 失败").count(), 1);
      assert.equal(await page.getByText("本机连接已就绪").count(), 1);
      assert.equal(await page.locator(".doctor-row").filter({ hasText: "尚未填写" }).count(), 0);
      await page.getByText("https://dcfc.example.com/").waitFor();
      await page.getByText("仅供外部 Connector 使用").scrollIntoViewIfNeeded();
      assert.match(await page.locator(".doctor-registration").innerText(), /不会影响本机启动/);
      assert.equal(await page.locator(".doctor-row").filter({ hasText: "公网" }).count(), 0);
      const evidence = path.join(root, "artifacts", "gate-c");
      fs.mkdirSync(evidence, { recursive: true });
      await page.screenshot({ path: path.join(evidence, `setup-doctor-${width}.png`), fullPage: true });
      await page.evaluate(() => { window.__doctorFail = true; });
      await page.getByRole("button", { name: "重新检查" }).click();
      await page.getByRole("alert").filter({ hasText: "诊断服务暂不可用" }).waitFor();
      await page.evaluate(() => { window.__doctorFail = false; });
      await page.getByRole("button", { name: "重新检查" }).click();
      await page.getByRole("alert").filter({ hasText: "诊断服务暂不可用" }).waitFor({ state: "hidden" });
      const calls = await page.evaluate(() => window.__dcfcTestCalls);
      assert.equal(calls.filter((command) => command === "save_public_base_url").length, 0);
      assert.equal(calls.filter((command) => command.includes("stop") || command.includes("start")).length, 0);
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
      assert.equal(overflow, false, `${width}px horizontal overflow`);
      await context.close();
    }
    console.log("setup-doctor UI checks passed at 1180px and 900px");
  } finally {
    await browser.close();
    if (server && !server.killed) server.kill();
  }
})().catch((error) => { console.error(error); process.exitCode = 1; });
