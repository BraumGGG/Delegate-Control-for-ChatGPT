const { chromium } = require("playwright");
const fs = require("fs");
const path = require("path");

const outputDir = path.join(__dirname, "..", "artifacts", "visual-check");
fs.mkdirSync(outputDir, { recursive: true });

const status = {
  overall: "running",
  proxy_ready: true,
  mcp_proxy_ready: true,
  mcp_ready: true,
  tunnel_ready: true,
  proxy_pid: 24680,
  mcp_pid: 38768,
  tunnel_pid: 13388,
  credential_configured: true,
  message: "ChatGPT Delegate 连接已建立。",
  projects: [
    { project_id: "screencast", name: "ScreenCast", output_directory: "C:\\Example\\Projects\\ScreenCast\\docs\\product-design-handoff", overall: "running", mcp_ready: true, mcp_pid: 8002, message: "项目 MCP 已就绪。" },
    { project_id: "realize", name: "Realize", output_directory: "C:\\Example\\Projects\\workflow\\.worktrees\\agent-runtime-v2\\docs\\product-designer-handoff", overall: "stopped", mcp_ready: false, mcp_pid: null, message: "项目未启动。" },
    { project_id: "dcfc", name: "DCFC", output_directory: "C:\\Example\\Projects\\Delegate Control\\docs\\product-designer-handoff", overall: "running", mcp_ready: true, mcp_pid: 8003, message: "项目 MCP 已就绪。" },
  ],
};
const settings = {
  proxy_host: "127.0.0.1",
  proxy_port: 7877,
  mcp_proxy_host: "127.0.0.1",
  mcp_proxy_port: 8100,
  health_port: 8080,
  health_host: "127.0.0.1",
  profile_name: "chatgpt-delegate",
  mcp_executable: "C:\\Example\\.local\\bin\\chatgpt-delegate.exe",
  proxy_executable: "C:\\Example\\.local\\bin\\mcp-proxy.exe",
  proxy_config_path: "C:\\Example\\AppData\\Roaming\\Delegate Control\\mcp-proxy.toml",
  tunnel_executable: "C:\\Example\\.local\\bin\\tunnel-client.exe",
  projects: [
    { id: "screencast", name: "ScreenCast", output_directory: "C:\\Example\\Projects\\ScreenCast\\docs\\product-design-handoff", mcp_host: "127.0.0.1", mcp_port: 8002, enabled: true },
    { id: "realize", name: "Realize", output_directory: "C:\\Example\\Projects\\workflow\\.worktrees\\agent-runtime-v2\\docs\\product-designer-handoff", mcp_host: "127.0.0.1", mcp_port: 8003, enabled: true },
    { id: "dcfc", name: "DCFC", output_directory: "C:\\Example\\Projects\\Delegate Control\\docs\\product-designer-handoff", mcp_host: "127.0.0.1", mcp_port: 8004, enabled: true },
  ],
  active_project_id: "dcfc",
};

(async () => {
  const installedChromium = process.env.PLAYWRIGHT_EXECUTABLE_PATH || "C:\\Users\\Redmi\\AppData\\Local\\ms-playwright\\chromium-1243\\chrome-win64\\chrome.exe";
  const browser = await chromium.launch({
    headless: true,
    executablePath: fs.existsSync(installedChromium) ? installedChromium : undefined,
  });
  const results = [];
  for (const viewport of [{ width: 980, height: 680 }, { width: 760, height: 580 }]) {
    const page = await browser.newPage({ viewport });
    await page.addInitScript(({ status, settings }) => {
      let callbackId = 1;
      window.__TAURI_INTERNALS__ = {
        callbacks: new Map(),
        transformCallback(callback) {
          const id = callbackId++;
          this.callbacks.set(id, callback);
          return id;
        },
        unregisterCallback(id) { this.callbacks.delete(id); },
        convertFileSrc(filePath) { return filePath; },
        async invoke(command) {
          if (["get_status", "start_delegate", "stop_delegate", "start_all_projects", "stop_all_projects", "start_project", "stop_project"].includes(command)) return status;
          if (command === "get_settings" || command === "save_settings") return settings;
          if (command === "read_logs") return "INFO  MCP server ready on 127.0.0.1:8000/mcp\nINFO  Secure tunnel connected\nINFO  Connector status: ready";
          if (command.includes("plugin:event")) return 1;
          return null;
        },
      };
    }, { status, settings });
    await page.goto("http://127.0.0.1:1420", { waitUntil: "networkidle" });
    await page.waitForTimeout(800);
    for (const [view, label] of [["overview", "首页"], ["projects", "项目管理"], ["logs", "运行日志"], ["settings", "连接设置"]]) {
      if (view !== "overview") {
        await page.getByRole("button", { name: label }).click();
        await page.waitForTimeout(180);
      }
      const metrics = await page.evaluate(() => ({
        viewport: { width: innerWidth, height: innerHeight },
        body: { width: document.body.scrollWidth, height: document.body.scrollHeight },
        root: { width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight },
        clippedText: [...document.querySelectorAll("button, strong, h1, h2")]
          .filter((element) => element.scrollWidth > element.clientWidth + 1)
          .map((element) => element.textContent?.trim())
          .filter(Boolean),
      }));
      const name = `${view}-${viewport.width}x${viewport.height}.png`;
      await page.screenshot({ path: path.join(outputDir, name), fullPage: true });
      results.push({ name, metrics });
    }
    await page.close();
  }
  fs.writeFileSync(path.join(outputDir, "metrics.json"), JSON.stringify(results, null, 2));
  console.log(JSON.stringify(results, null, 2));
  await browser.close();
})();
