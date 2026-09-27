const { chromium } = require("playwright");
const fs = require("fs");
const path = require("path");

const outputDir = path.join(__dirname, "..", "artifacts", "visual-check");
fs.mkdirSync(outputDir, { recursive: true });

const status = {
  overall: "stopped",
  proxy_ready: true,
  router_ready: false,
  mcp_proxy_ready: true,
  mcp_ready: false,
  tunnel_ready: false,
  proxy_pid: null,
  router_pid: null,
  mcp_pid: null,
  tunnel_pid: null,
  credential_configured: true,
  text_editing_available: true,
  connector_capability_message: "当前 MCP 支持项目内文本文件编辑。",
  message: "连接仅在需要时启动。",
  projects: [
    { project_id: "screencast", name: "ScreenCast", output_directory: "D:\\Projects\\ScreenCast\\docs\\product-design-handoff", overall: "stopped", mcp_ready: false, mcp_pid: null, message: "项目未启动。" },
    { project_id: "new-project", name: "Realize", output_directory: "D:\\Projects\\workflow\\docs\\product-designer-handoff", overall: "stopped", mcp_ready: false, mcp_pid: null, message: "项目未启动。" },
    { project_id: "new-project-2", name: "DCFC", output_directory: "D:\\Projects\\Delegate Control for ChatGPT\\docs\\product-designer-handoff", overall: "stopped", mcp_ready: false, mcp_pid: null, message: "项目未启动。" },
  ],
};
const settings = {
  proxy_host: "127.0.0.1",
  proxy_port: 7897,
  mcp_proxy_host: "127.0.0.1",
  mcp_proxy_port: 8100,
  router_port: 8101,
  health_port: 8080,
  health_host: "127.0.0.1",
  profile_name: "chatgpt-delegate",
  mcp_executable: "C:\\Users\\Redmi\\.local\\bin\\chatgpt-delegate.exe",
  proxy_executable: "C:\\Users\\Redmi\\.local\\bin\\mcp-proxy.exe",
  proxy_config_path: "C:\\Users\\Redmi\\AppData\\Roaming\\Delegate Control\\mcp-proxy.toml",
  router_config_path: "C:\\Users\\Redmi\\AppData\\Roaming\\Delegate Control\\router-projects.json",
  tunnel_executable: "D:\\tunnel-client\\install\\tunnel-client.exe",
  projects: [
    { id: "screencast", name: "ScreenCast", output_directory: "D:\\Projects\\ScreenCast\\docs\\product-design-handoff", mcp_host: "127.0.0.1", mcp_port: 8002, enabled: true },
    { id: "new-project", name: "Realize", output_directory: "D:\\Projects\\workflow\\docs\\product-designer-handoff", mcp_host: "127.0.0.1", mcp_port: 8000, enabled: true },
    { id: "new-project-2", name: "DCFC", output_directory: "D:\\Projects\\Delegate Control for ChatGPT\\docs\\product-designer-handoff", mcp_host: "127.0.0.1", mcp_port: 8001, enabled: true },
  ],
  active_project_id: "new-project-2",
};

const canonicalStatus = structuredClone(status);
canonicalStatus.projects = canonicalStatus.projects.map((project) => ({
  ...project,
  project_id: project.project_id === "new-project" ? "realize" : project.project_id === "new-project-2" ? "dcfc" : project.project_id,
}));
const canonicalSettings = structuredClone(settings);
canonicalSettings.projects = canonicalSettings.projects.map((project) => ({
  ...project,
  id: project.id === "new-project" ? "realize" : project.id === "new-project-2" ? "dcfc" : project.id,
}));
canonicalSettings.active_project_id = "dcfc";

(async () => {
  const browser = await chromium.launch({ channel: "msedge", headless: true });
  const results = [];
  for (const viewport of [{ width: 1180, height: 800 }, { width: 900, height: 700 }]) {
    for (const view of ["dashboard", "logs", "settings", "migration"]) {
      const page = await browser.newPage({ viewport });
      const fixture = view === "migration" ? { status, settings } : { status: canonicalStatus, settings: canonicalSettings };
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
          if (command === "get_settings" || command === "save_settings" || command === "migrate_project_key") return settings;
          if (command === "read_logs") return "INFO  MCP server ready on 127.0.0.1:8000/mcp\nINFO  Secure tunnel connected\nINFO  Connector status: ready";
          if (command.includes("plugin:event")) return 1;
          return null;
        },
      };
      }, fixture);
      await page.goto("http://127.0.0.1:1420", { waitUntil: "networkidle" });
      await page.waitForTimeout(800);
      if (view === "logs") {
        await page.getByRole("button", { name: "运行日志", exact: true }).click();
      } else if (view === "settings" || view === "migration") {
        await page.getByRole("button", { name: view === "migration" ? "项目管理" : "设置", exact: true }).click();
        await page.waitForTimeout(200);
        if (view === "migration") await page.getByRole("button", { name: "迁移 Realize 的 Project Key" }).click();
      } else {
        await page.getByRole("button", { name: "首页", exact: true }).click();
      }
      await page.waitForTimeout(200);
      const metrics = await page.evaluate(() => ({
        viewport: { width: innerWidth, height: innerHeight },
        body: { width: document.body.scrollWidth, height: document.body.scrollHeight },
        root: { width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight },
        migrationPanel: (() => {
          const element = document.querySelector(".project-key-migration");
          if (!element) return null;
          const rect = element.getBoundingClientRect();
          return { width: rect.width, height: rect.height, left: rect.left, top: rect.top };
        })(),
        legacyGuides: document.querySelectorAll(".legacy-key-guide").length,
        clippedText: [...document.querySelectorAll("button, strong, h1, h2")]
          .filter((element) => element.scrollWidth > element.clientWidth + 1)
          .map((element) => element.textContent?.trim())
          .filter(Boolean),
      }));
      if (metrics.body.width !== viewport.width || metrics.root.width !== viewport.width) {
        throw new Error(`horizontal overflow at ${view} ${viewport.width}: ${JSON.stringify(metrics)}`);
      }
      if (metrics.clippedText.length) throw new Error(`clipped text at ${view} ${viewport.width}: ${metrics.clippedText.join(", ")}`);
      const name = `${view}-${viewport.width}x${viewport.height}.png`;
      await page.screenshot({ path: path.join(outputDir, name), fullPage: true });
      results.push({ name, metrics });
      await page.close();
    }
  }
  fs.writeFileSync(path.join(outputDir, "metrics.json"), JSON.stringify(results, null, 2));
  console.log(JSON.stringify(results, null, 2));
  await browser.close();
})();
