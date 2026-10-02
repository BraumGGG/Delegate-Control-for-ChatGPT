![Delegate Control for ChatGPT](docs/images/readme-cover.png)

# Delegate Control for ChatGPT

Delegate Control for ChatGPT（DCFC）是一个 Windows 桌面控制客户端，用于管理本机的 ChatGPT Delegate MCP 服务和安全隧道。

> 本项目是第三方工具，并非 OpenAI 或 ChatGPT 官方产品。

## 多项目架构

DCFC 使用一条共享连接承载多个项目：

```text
ChatGPT Connector / Secure Tunnel
                |
        mcp-proxy（一个）
          /      |      \\
 chatgpt-delegate  ...  chatgpt-delegate
   项目 A MCP          项目 B MCP
      |                   |
   目录 A              目录 B
```

每个项目都是独立的 `chatgpt-delegate` 进程，并使用独立 MCP 端口和输出目录；项目之间不会混写。新增项目不需要新增 tunnel ID，也不需要为每个项目维护独立 Connector。对 ChatGPT 暴露的是固定 Router MCP 工具目录，项目通过可选 `project_id` 参数路由。

## 使用方式

1. 打开 DCFC，在“设置 → 项目目录”中新增项目并选择文档输出目录。
2. 为需要同时在线的项目勾选“启用”，保存设置。
3. 点击“启动全部项目”。DCFC 会启动多个项目 MCP、一个 `mcp-proxy` 和一个 `tunnel-client`。
4. 日常在 ChatGPT 中用自然语言明确项目，例如“请在客户 A 项目中保存会议纪要”。Router 始终使用固定裸工具名，多项目时通过可选 `project_id` 参数路由。项目不明确时，ChatGPT 应先澄清，不应猜目录。
5. 修改项目目录、端口、名称或启用状态后，必须重启连接才会生效；界面会明确提示这一点。

### 安装诊断与 ChatGPT 注册

“安装诊断”分别检查本机设置、内置 MCP/Router 运行组件、用户提供的 MCP Proxy
和 Tunnel Client、Magic、共享链路及每个项目。某个项目失败不会被显示成所有项目失败；
检查不会自动重置设置或启动/停止进程。

在“连接设置”中单独保存 **Public URL / 公网地址**，只填写 HTTPS 公网基础地址，
不填写 `/mcp`。此项可在运行中修改，不需要重启，不影响本地项目运行。
安装诊断从该地址派生 ChatGPT 注册用的 MCP Proxy 根路由地址，并分别报告公网地址
是否有响应、该端点是否返回 MCP 初始化响应。未配置 Public URL 只阻止注册准备检查。
在 ChatGPT 中由用户自行新增自定义 MCP 应用、完成授权并核对工具；DCFC 无法强制
ChatGPT 刷新外部工具 Schema。本机探测成功不等于 ChatGPT 已完成端到端调用。

### 编辑已有文本文件

编辑版 Connector 额外提供以下工具：

```text
read_text_file
append_text_file
replace_text_in_file
delete_text_from_file
```

这些工具只接受当前项目输出目录内的相对路径，并只处理 UTF-8 纯文本。替换和删除默认要求目标片段恰好出现一次；读取后可把返回的 `sha256` 作为 `expected_sha256` 传回，避免覆盖 Codex 或用户刚刚修改的文件。每次修改都会先在项目目录的 `.dcfc-backups` 中保留备份，再执行原子替换。

编辑工具和固定 Router 由 `chatgpt-delegate-edit.exe` 提供。升级 Connector 后必须停止并重新启动 DCFC 管理的项目 MCP、Router、Proxy 和 Tunnel；正式迁移完成后只需在 ChatGPT 中重新注册一次 Connector，后续项目增删不再改变工具清单。旧版 Connector 仍能使用原有 7 个报告/结果工具，但不会提供文本编辑工具。

当前选中的项目只是界面焦点，不会停止其他在线项目。也可以在总览或设置中单独启动/停止某个项目；停止一个项目不会停止其他项目，但共享 proxy/tunnel 会按当前项目列表重新加载。

## 功能

- 多个项目同时在线，共享一个 MCP Proxy 和一个 Tunnel
- 每个项目独立选择任意绝对文档输出目录
- 项目级 MCP 进程、端口、日志和状态
- 按需启动、停止、重新连接及单项目启停
- Runtime API Key 保存在 Windows Credential Manager，不写入配置或日志
- Windows Job Object 清理 MCP、Proxy、Tunnel 及其子进程
- 系统托盘、单实例运行和关闭窗口隐藏

## 运行依赖

- Windows 10/11 x64
- 一个本地 Magic HTTP 代理；在“连接设置”中填写其实际监听端口
- `mcp-proxy.exe`：DCFC 安装包内置并固定为 `joshrotenberg/mcp-proxy` v0.4.3；安装完整性会校验 SHA-256，用户无需单独下载或复制
- `tunnel-client.exe`：从 `openai/tunnel-client` 官方 Release 单独获取；Gate A 验证版本为 v0.0.14
- `chatgpt-delegate-edit.exe`：DCFC 安装包的内置运行组件；如果安装目录缺少该文件，首次运行会提示安装完整性异常，而不会要求普通用户手工选择 Connector 路径

Gate D1 起，安装包会把 MCP Proxy 与许可证文件放入安装资源目录，并在启动时解析为受管路径；缺失或损坏时 Setup Doctor/首次运行向导会明确提示重新安装。Tunnel Client 仍由用户提供，首次运行向导会让用户选择其实际路径。内置 Connector 由 DCFC 安装完整性负责，不属于普通用户的运行依赖配置。

默认程序路径：

```text
%USERPROFILE%\.local\bin\chatgpt-delegate.exe
%USERPROFILE%\.local\bin\tunnel-client.exe
```

如需使用受信任的自定义 `mcp-proxy.exe`，可在“设置 → 本机程序”中明确选择；默认安装路径不会从 PATH 或其他无关目录回退。启动前 DCFC 会检查所有运行程序是否存在。

DCFC 会在启动前检查内置 Connector 是否存在，并显示文件编辑能力状态；如果安装目录缺少内置组件，应重新安装 DCFC。用户无需在设置中选择 Connector 路径。

## 本地开发与构建

以下 Node.js、npm、Rust stable 和 Python 要求仅适用于从源码构建，
不是目标安装版的最终用户要求。

```powershell
npm install
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri -- dev
npm run release:build
```

`npm run release:build` 会先生成固定版本的 `chatgpt-delegate-edit` PyInstaller
onedir runtime，再执行 Tauri 正式构建。安装包会把完整 runtime 放在应用资源目录的
`runtime/chatgpt-delegate-edit/` 下；应用启动后从 Tauri `resource_dir()` 解析该路径，
不依赖当前工作目录。普通开发构建不需要把 runtime 提交到 Git，发布构建脚本会在缺失时
按 `scripts/build-chatgpt-delegate.ps1` 生成它。

安装包输出在：

```text
src-tauri/target/release/bundle/nsis/
src-tauri/target/release/bundle/msi/
```

## 配置与日志

新版本的非敏感设置、proxy/Router 配置和日志保存在 `%USERPROFILE%\.delegate-control\`；
历史 `%APPDATA%\Delegate Control\` 设置由兼容加载路径迁移并保留备份。这些本机文件不应提交到 Git。

旧版本顶层 `output_directory` 会自动迁移为名为“默认项目”的项目配置。Runtime API Key 仍只通过 Windows Credential Manager 注入 Tunnel 子进程。

## 安全说明

- 不读取、输出、记录或提交 Runtime API Key。
- 不提交 `settings.json`、日志、测试 evidence、artifacts、`dist`、`target` 或本机密钥文件。
- 项目目录必须是绝对路径，并在 UI 中明确显示；目录不存在时，启动项目会创建它。
- 共享 proxy 只绑定 loopback 地址，不暴露到局域网。

## 回退基线

多项目改造前的保护标签为 `baseline-2026-09-22`。在确认当前改动已保存后，可基于该标签创建回退分支：

```powershell
git switch -c rollback-baseline-2026-09-22 baseline-2026-09-22
```

不要对含有用户改动的工作区执行破坏性 reset 或 checkout。
