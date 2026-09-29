const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");

const root = path.resolve(__dirname, "..");
const runtimeRoot = path.join(root, "artifacts", "build-chatgpt-delegate", "chatgpt-delegate-edit");
const executable = path.join(runtimeRoot, "chatgpt-delegate-edit.exe");
const config = JSON.parse(fs.readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"));

if (!fs.existsSync(executable)) {
  throw new Error(`缺少正式 onedir Connector：${executable}。先运行 npm run release:build。`);
}

const files = [];
let bytes = 0;
function walk(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) walk(target);
    else {
      files.push(target);
      bytes += fs.statSync(target).size;
    }
  }
}
walk(runtimeRoot);

const resourceMapping = config.bundle?.resources ?? {};
const mappedDestination = resourceMapping["../artifacts/build-chatgpt-delegate/chatgpt-delegate-edit"];
if (mappedDestination !== "runtime/chatgpt-delegate-edit") {
  throw new Error("Tauri 资源映射不是 runtime/chatgpt-delegate-edit。");
}
if (files.length < 500 || bytes < 50 * 1024 * 1024) {
  throw new Error(`Connector onedir 资源不完整：${files.length} 个文件，${bytes} 字节。`);
}

const hash = crypto.createHash("sha256").update(fs.readFileSync(executable)).digest("hex").toUpperCase();
console.log(JSON.stringify({
  source: path.relative(root, runtimeRoot),
  executable: path.relative(root, executable),
  files: files.length,
  bytes,
  executableSha256: hash,
  destination: mappedDestination,
}, null, 2));
