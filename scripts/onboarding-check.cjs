const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");
const path = require("node:path");
const { chromium } = require("playwright");

const root = path.join(__dirname, "..");
const appPort = Number(process.env.ONBOARDING_PORT || 1437);
const appUrl = `http://127.0.0.1:${appPort}`;
const viteBin = path.join(path.dirname(require.resolve("vite/package.json", { paths: [root] })), "bin", "vite.js");
const emptySettings = {
  proxy_host: "127.0.0.1", proxy_port: 7877, mcp_proxy_host: "127.0.0.1", mcp_proxy_port: 8100, router_port: 8101,
  health_host: "127.0.0.1", health_port: 8080, profile_name: "", tunnel_id: "",
  mcp_executable: "C:\\Program Files\\DCFC\\runtime\\chatgpt-delegate-edit\\chatgpt-delegate-edit.exe",
  proxy_executable: "C:\\Missing\\mcp-proxy.exe", tunnel_executable: "C:\\Missing\\tunnel-client.exe",
  proxy_config_path: "C:\\Users\\Test\\.delegate-control\\mcp-proxy.toml",
  router_config_path: "C:\\Users\\Test\\.delegate-control\\router-projects.json",
  projects: [], active_project_id: null,
};
const ready = { mcp_available: true, proxy_available: true, tunnel_available: true, credential_configured: true };
const status = {
  overall: "stopped", proxy_ready: false, router_ready: false, mcp_proxy_ready: false, mcp_ready: false, tunnel_ready: false,
  proxy_pid: null, router_pid: null, mcp_pid: null, tunnel_pid: null, credential_configured: true,
  text_editing_available: true, connector_capability_message: "内置 Connector 已就绪。", message: "连接仅在需要时启动。", projects: [],
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

async function installMock(page, readiness = ready) {
  await page.addInitScript(({ empty, readyState, initialStatus }) => {
    let callbackId = 1;
    let currentSettings = JSON.parse(localStorage.getItem("__dcfc_onboarding_settings") || "null") || structuredClone(empty);
    let currentStatus = structuredClone(initialStatus);
    const syncStatus = () => {
      currentStatus.projects = currentSettings.projects.map((project) => ({
        project_id: project.id, name: project.name, output_directory: project.output_directory,
        overall: "stopped", mcp_ready: false, mcp_pid: null, message: "项目未启动。",
      }));
    };
    syncStatus();
    window.__TAURI_INTERNALS__ = {
      callbacks: new Map(),
      transformCallback(callback) { const id = callbackId++; this.callbacks.set(id, callback); return id; },
      unregisterCallback(id) { this.callbacks.delete(id); },
      convertFileSrc(filePath) { return filePath; },
      async invoke(command, args = {}) {
        if (command === "get_status") return structuredClone(currentStatus);
        if (command === "get_runtime_readiness") return structuredClone(readyState);
        if (command === "check_runtime_readiness") {
          const candidate = args.settings || currentSettings;
          const pathsReady = !String(candidate.proxy_executable || "").includes("Missing") && !String(candidate.tunnel_executable || "").includes("Missing");
          return { ...structuredClone(readyState), proxy_available: pathsReady, tunnel_available: pathsReady };
        }
        if (command === "detect_magic_port") return { host: "127.0.0.1", configured_port: 7877, listening: true, detected_port: 7877, detail: "已检测到本机代理监听 127.0.0.1:7877。" };
        if (command === "open_external_url") { window.__openedExternalUrl = args.url; return null; }
        if (command === "get_settings") return structuredClone(currentSettings);
        if (command === "save_runtime_key") { currentStatus.credential_configured = true; return null; }
        if (command === "save_settings") {
          currentSettings = structuredClone(args.settings);
          localStorage.setItem("__dcfc_onboarding_settings", JSON.stringify(currentSettings));
          syncStatus();
          return structuredClone(currentSettings);
        }
        if (command.includes("plugin:event")) return 1;
        return structuredClone(currentStatus);
      },
    };
  }, { empty: emptySettings, readyState: readiness, initialStatus: status });
}

(async () => {
  let server;
  if (!(await serverAvailable())) {
    server = spawn(process.execPath, [viteBin, "--host", "127.0.0.1", "--port", String(appPort), "--strictPort"], {
      cwd: root, windowsHide: true, stdio: "ignore",
    });
    await waitForServer();
  }

  const browser = await chromium.launch({ channel: "msedge", headless: true });
  try {
    const context = await browser.newContext({ viewport: { width: 1180, height: 800 } });
    const page = await context.newPage();
    await installMock(page);
    await page.goto(appUrl, { waitUntil: "commit", timeout: 10000 });

    await page.getByRole("heading", { name: "运行依赖" }).waitFor();
    assert.equal(await page.getByText("DCFC 内置运行组件").count(), 1);
    assert.equal(await page.getByRole("button", { name: "选择MCP Connector" }).count(), 0);

    await page.getByLabel("MCP Proxy路径").fill("C:\\Tools\\mcp-proxy.exe");
    await page.getByLabel("Tunnel Client路径").fill("C:\\Tools\\tunnel-client.exe");

    await page.getByRole("button", { name: "下一步" }).click();
    await page.getByRole("heading", { name: "OpenAI 连接" }).waitFor();
    await page.getByRole("button", { name: /Runtime API Keys 创建页面/ }).click();
    assert.equal(await page.evaluate(() => window.__openedExternalUrl), "https://platform.openai.com/settings/organization/api-keys");
    assert.equal(await page.getByText("如何创建 Runtime API Key").count(), 1);
    await page.getByRole("button", { name: "下一步" }).click();
    await page.getByText("Magic 与 Tunnel").waitFor();
    await page.getByRole("button", { name: /Tunnels 管理页面/ }).click();
    assert.equal(await page.evaluate(() => window.__openedExternalUrl), "https://platform.openai.com/settings/organization/tunnels");
    await page.getByRole("button", { name: "检测当前端口" }).click();
    await page.getByText("已检测到本机代理监听").waitFor();
    await page.getByRole("button", { name: "下一步" }).click();
    await page.getByText("Tunnel ID 不能为空").waitFor();

    await page.getByRole("button", { name: "稍后配置" }).first().click();
    await page.getByText("完成本机配置后再开始连接").waitFor();
    await page.getByRole("button", { name: "继续配置" }).click();
    await page.getByText("Magic 与 Tunnel").waitFor();

    await page.getByLabel("Tunnel ID").fill("tunnel_demo_01");
    await page.getByRole("button", { name: "下一步" }).click();
    await page.getByLabel("项目名称").fill("Demo Project");
    await page.getByLabel("文档输出目录").fill("C:\\Demo\\Docs");
    await page.getByRole("button", { name: "下一步" }).click();
    await page.getByText("本地配置检查").waitFor();
    await page.getByRole("button", { name: "保存本地配置" }).click();
    await page.getByText("本机配置已保存").waitFor();
    assert.equal(await page.locator(".product-name").filter({ hasText: "首页" }).count(), 1);
    assert.equal(await page.locator(".lrow").filter({ hasText: "Demo Project" }).count(), 1);

    await page.reload({ waitUntil: "networkidle" });
    assert.equal(await page.getByText("完成本机配置后再开始连接").count(), 0);
    assert.equal(await page.locator(".lrow").filter({ hasText: "Demo Project" }).count(), 1);
    assert.equal(await page.getByText("首次运行配置").count(), 0);

    await context.close();

    const missingContext = await browser.newContext({ viewport: { width: 1180, height: 800 } });
    const missingPage = await missingContext.newPage();
    await installMock(missingPage, { ...ready, mcp_available: false });
    await missingPage.goto(appUrl, { waitUntil: "commit", timeout: 10000 });
    await missingPage.getByRole("heading", { name: "运行依赖" }).waitFor();
    assert.equal(await missingPage.getByText("未找到；请重新安装 DCFC").count(), 1);
    assert.equal(await missingPage.getByText("需要处理的依赖").count(), 1);
    assert.equal(await missingPage.getByRole("button", { name: "选择MCP Connector" }).count(), 0);
    await missingContext.close();

    const responsiveContext = await browser.newContext({ viewport: { width: 900, height: 800 } });
    const responsivePage = await responsiveContext.newPage();
    await installMock(responsivePage);
    await responsivePage.goto(appUrl, { waitUntil: "commit", timeout: 10000 });
    await responsivePage.getByRole("heading", { name: "运行依赖" }).waitFor();
    const widthCheck = await responsivePage.evaluate(() => ({ viewport: window.innerWidth, scrollWidth: document.documentElement.scrollWidth }));
    assert.ok(widthCheck.scrollWidth <= widthCheck.viewport + 1, `responsive horizontal overflow: ${JSON.stringify(widthCheck)}`);
    await responsiveContext.close();

    console.log(JSON.stringify({ status: "ok", checks: ["wizard-visible", "invalid-input", "cancel-resume", "completion", "restart-bypass", "internal-connector-present", "internal-connector-missing", "responsive-900"] }, null, 2));
  } finally {
    await browser.close();
    if (server && !server.killed) server.kill();
  }
})().catch((error) => { console.error(error); process.exitCode = 1; });
