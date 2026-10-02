const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");

const root = path.join(__dirname, "..");
const runtimeRoot = path.join(root, "artifacts", "build-mcp-proxy");
const executable = path.join(runtimeRoot, "mcp-proxy.exe");
const expectedSha256 = "A1019FDA27715A984C1127DE90AC4398867984FFD8191C8CC9E4DA61B7D75CBC";
const files = [executable, path.join(runtimeRoot, "LICENSE-APACHE"), path.join(runtimeRoot, "LICENSE-MIT")];

for (const file of files) {
  if (!fs.existsSync(file)) throw new Error(`Managed MCP Proxy file is missing: ${path.relative(root, file)}`);
}
const hash = crypto.createHash("sha256").update(fs.readFileSync(executable)).digest("hex").toUpperCase();
if (hash !== expectedSha256) throw new Error(`Managed MCP Proxy hash mismatch: expected ${expectedSha256}, got ${hash}`);

console.log(JSON.stringify({
  version: "0.4.3",
  executable: path.relative(root, executable),
  executableSha256: hash,
  licenses: ["LICENSE-APACHE", "LICENSE-MIT"],
}, null, 2));
