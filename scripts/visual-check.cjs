const { chromium } = require("playwright");
const fs = require("fs");
const path = require("path");

const outputDir = path.join(__dirname, "..", "artifacts", "visual-check");
fs.mkdirSync(outputDir, { recursive: true });

const status = {
  overall: "running",
  proxy_ready: true,
  mcp_ready: true,
  tunnel_ready: true,
  mcp_pid: 38768,
  tunnel_pid: 13388,
  credential_configured: true,
  message: "ChatGPT Delegate 连接已建立。",
};
const settings = {
  proxy_host: "127.0.0.1",
  proxy_port: 7897,
  mcp_host: "127.0.0.1",
  mcp_port: 8000,
  health_port: 8080,
  profile_name: "chatgpt-delegate",
  mcp_executable: "C:\\Users\\Redmi\\.local\\bin\\chatgpt-delegate.exe",
  tunnel_executable: "D:\\tunnel-client\\install\\tunnel-client.exe",
  output_directory: "D:\\claudecode\\cchaha\\Project\\咨询\\artifacts\\chatgpt-delegation",
};

(async () => {
  const browser = await chromium.launch({ headless: true });
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
          if (command === "get_status" || command === "start_delegate" || command === "stop_delegate") return status;
          if (command === "get_settings" || command === "save_settings") return settings;
          if (command === "read_logs") return "INFO  MCP server ready on 127.0.0.1:8000/mcp\nINFO  Secure tunnel connected\nINFO  Connector status: ready";
          if (command.includes("plugin:event")) return 1;
          return null;
        },
      };
    }, { status, settings });
    await page.goto("http://127.0.0.1:1420", { waitUntil: "networkidle" });
    await page.waitForTimeout(800);
    const metrics = await page.evaluate(() => ({
      viewport: { width: innerWidth, height: innerHeight },
      body: { width: document.body.scrollWidth, height: document.body.scrollHeight },
      root: { width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight },
      powerButton: (() => {
        const element = document.querySelector(".power-button");
        if (!element) return null;
        const rect = element.getBoundingClientRect();
        return { width: rect.width, height: rect.height, left: rect.left, top: rect.top };
      })(),
      clippedText: [...document.querySelectorAll("button, strong, h1, h2")]
        .filter((element) => element.scrollWidth > element.clientWidth + 1)
        .map((element) => element.textContent?.trim())
        .filter(Boolean),
    }));
    const name = `overview-${viewport.width}x${viewport.height}.png`;
    await page.screenshot({ path: path.join(outputDir, name), fullPage: true });
    results.push({ name, metrics });
    await page.close();
  }
  fs.writeFileSync(path.join(outputDir, "metrics.json"), JSON.stringify(results, null, 2));
  console.log(JSON.stringify(results, null, 2));
  await browser.close();
})();
