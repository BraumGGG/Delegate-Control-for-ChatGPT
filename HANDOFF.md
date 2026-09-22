# Delegate Control for ChatGPT 项目交接文档

> 交接日期：2026-09-22  
> 项目简称：DCFC  
> 本地项目：`D:\claudecode\cchaha\Project\咨询\delegate-control`  
> GitHub：`https://github.com/BraumGGG/Delegate-Control-for-ChatGPT`  
> 当前分支：`main`

## 1. 交接结论

当前仓库中的 **Delegate Control for ChatGPT 0.1.0** 已经是可构建的 Windows Tauri 桌面客户端。它负责用一个普通窗口和系统托盘统一启动、停止及观察以下本机链路：

```text
ChatGPT 网页端
    ↕ ChatGPT Connector / Secure Tunnel
tunnel-client
    ↕ http://127.0.0.1:8000/mcp
chatgpt-delegate 本地 MCP Server
    ↕ 文件读写
当前配置的 output_directory
```

当前版本的核心控制功能已经落地，但下一阶段用户确认要做的功能 **尚未实现**：

- 不再让所有项目共用一个固定的 `output_directory`。
- 每个项目应能独立选择自己的文档输出目录。
- 用户应能按实际任务选择任意目录，不要求提前维护一个全局固定目录。
- 除目录选择及相关文档读写能力外，不要改动现有连接、凭据、托盘和进程管理行为。

## 2. Git 状态与回退基线

交接检查时，功能代码对应的提交为：

```text
f38fab6 docs: add README product cover
8585043 feat: publish Delegate Control for ChatGPT
```

已经创建回退标签：

```text
baseline-2026-09-22
```

该标签是“实现多项目独立输出目录之前”的保护基线。继续开发前必须先运行：

```powershell
Set-Location 'D:\claudecode\cchaha\Project\咨询\delegate-control'
git status --short
git log -5 --oneline --decorate
git show --no-patch --decorate baseline-2026-09-22
```

不要删除或移动该标签。需要查看基线代码时可使用：

```powershell
git switch --detach baseline-2026-09-22
```

不要在有用户未提交改动时执行 `git reset --hard`、`git checkout -- .` 或其他破坏性回退命令。若要恢复，应先创建分支或提交当前工作。

## 3. 产品定位与命名

- 正式名称：`Delegate Control for ChatGPT`
- 桌面快捷方式名称：`DCFC`
- 内部包名及可执行文件名：`delegate-control`
- Tauri identifier：`com.delegate.control`
- 当前版本：`0.1.0`
- 项目为第三方工具，不是 OpenAI 或 ChatGPT 官方产品。
- 上游 MCP 项目：`felixlark/chatgpt-delegate`
- 本项目不是上游 MCP 的镜像，而是其独立 Windows 控制客户端。

## 4. 用户已经确认的使用偏好

- 应用默认不自动启动连接，由用户主动控制。
- 用户不接受每次手工打开两个 PowerShell 窗口。
- 应用必须是普通 Windows 窗口程序，并具有系统托盘能力。
- 关闭主窗口应隐藏到托盘，而不是留下不可控后台进程。
- 退出应用时必须清理 MCP、Tunnel 及其子进程。
- Runtime API Key 必须安全保存，不得写入仓库或普通明文配置。
- 用户不希望反复充当人工测试员；桌面端修改后应使用自动化与确定性检查自行验证。
- 新功能必须保持范围克制：目前只解决“多项目独立选择输出目录及文档读写”问题。

## 5. 当前功能

### 5.1 桌面界面

前端位于 `src/`，使用 React 19、TypeScript、Vite 和 Lucide 图标。

界面包含：

- 总览：显示 Clash、MCP、Tunnel 状态与进程 PID。
- 启动、停止、重新连接。
- 日志：分别读取 MCP 与 Tunnel 日志，每 2.5 秒刷新。
- 设置：网络端口、本机程序路径、当前结果目录和 Runtime API Key。
- 状态轮询：每 1.8 秒调用一次后端状态接口。

### 5.2 Windows 桌面行为

后端位于 `src-tauri/src/`，使用 Rust 与 Tauri 2。

- 单实例运行；再次启动时显示并聚焦已有窗口。
- 关闭窗口时隐藏到系统托盘。
- 托盘菜单包含打开、启动连接、停止连接、退出程序。
- 真正退出前会先停止受管进程。
- 使用 Windows Job Object 管理 MCP 和 Tunnel，配置 `KILL_ON_JOB_CLOSE`，防止退出后留下子进程。

### 5.3 连接启动流程

`src-tauri/src/process_manager.rs` 中的顺序如下：

1. 校验设置与端口。
2. 从 Windows Credential Manager 读取 Runtime API Key。
3. 检查 MCP 与 Tunnel 可执行文件是否存在。
4. 检查 Clash HTTP 代理，默认 `127.0.0.1:7897`。
5. 启动 `chatgpt-delegate`：

```text
chatgpt-delegate.exe --output-dir <output_directory> serve --host 127.0.0.1 --port 8000
```

6. 最多等待 15 秒，直到 MCP 端口可连接。
7. 启动 `tunnel-client`：

```text
tunnel-client.exe run --profile chatgpt-delegate \
  --control-plane.http-proxy http://127.0.0.1:7897 \
  --open-web-ui=false \
  --log.file <本机日志路径>
```

8. 通过环境变量 `CONTROL_PLANE_API_KEY` 向 Tunnel 进程传递密钥。
9. 最多等待 30 秒，检查 `http://127.0.0.1:8080/readyz`。
10. 两个进程都就绪后，状态变为 `running`。

### 5.4 凭据与本机配置

Runtime API Key：

- 由 `src-tauri/src/credentials.rs` 管理。
- 使用 Windows Credential Manager。
- service：`DelegateControl`
- account：`RuntimeApiKey`
- 不得在日志、文档、截图、测试 evidence 或 Git 中记录真实密钥。

非敏感设置保存在：

```text
%APPDATA%\Delegate Control\settings.json
```

日志保存在：

```text
%APPDATA%\Delegate Control\logs\
```

主要日志文件：

```text
mcp.stdout.log
mcp.stderr.log
tunnel.stdout.log
tunnel.stderr.log
tunnel.log
```

## 6. 当前默认环境

代码中的默认值位于 `src-tauri/src/config.rs`：

```text
Clash 代理：127.0.0.1:7897
MCP：127.0.0.1:8000
Tunnel 健康检查：127.0.0.1:8080/readyz
Tunnel profile：chatgpt-delegate
MCP 程序：%USERPROFILE%\.local\bin\chatgpt-delegate.exe
Tunnel 程序：D:\tunnel-client\install\tunnel-client.exe
当前默认输出目录：D:\claudecode\cchaha\Project\咨询\artifacts\chatgpt-delegation
```

注意：`settings.json` 中的已保存值会覆盖代码默认值。排障时应同时检查代码默认值和本机设置文件，但不得提交本机设置文件。

## 7. 源码导航

| 文件 | 职责 |
| --- | --- |
| `src/App.tsx` | 主界面、状态轮询、启动停止、设置和日志交互 |
| `src/api.ts` | 前端到 Tauri command 的调用封装 |
| `src/types.ts` | 前端状态与设置类型 |
| `src/styles.css` | 界面样式 |
| `src-tauri/src/lib.rs` | 应用状态、单实例、托盘、窗口关闭行为 |
| `src-tauri/src/commands.rs` | 暴露给前端的 Tauri commands |
| `src-tauri/src/config.rs` | 设置结构、默认值、校验与持久化 |
| `src-tauri/src/credentials.rs` | Runtime API Key 的安全存取 |
| `src-tauri/src/process_manager.rs` | MCP/Tunnel 进程、健康检查、日志和清理 |
| `src-tauri/tauri.conf.json` | 窗口、产品、安装包和 CSP 配置 |
| `scripts/Migrate-LegacyCredential.ps1` | 旧凭据到 Credential Manager 的一次性迁移脚本 |
| `scripts/visual-check.cjs` | 界面视觉检查辅助脚本 |
| `docs/images/readme-cover.png` | GitHub README 封面 |

## 8. 下一阶段需求：多项目独立输出目录

### 8.1 当前问题

当前 `AppSettings` 只有一个全局字段：

```rust
pub output_directory: String
```

启动 MCP 时，这个值一次性传给 `chatgpt-delegate --output-dir`。因此一旦连接启动，ChatGPT 写出的所有文档都会进入同一目录。三个项目同时存在、路径各不相同时，当前模型无法满足需求。

设置页面目前也只显示结果目录，没有目录选择按钮；保存设置会把目录作为全局设置持久化。这正是要解决的问题。

### 8.2 已确认的验收目标

- 项目 A、B、C 可分别使用各自的输出目录，互不混写。
- 目录由用户按项目或当前任务选择，可以是任意有效路径。
- 不要求用户先把所有目录登记成一个全局固定值。
- ChatGPT 应能把文档写入选定项目目录；Codex 随后也能在同一项目目录读取和继续编辑这些文件。
- 切换项目不能静默把文档写进上一个项目。
- UI 必须清楚显示当前生效的项目/目录。
- 连接运行期间若不支持热切换，应明确要求停止后切换并重新连接，不能假装已生效。
- 路径不存在时应给出可理解错误，或在用户确认的合理位置创建目录。
- 不改 Runtime API Key 存储方式，不降低安全边界。
- 不破坏单实例、托盘、日志、Job Object 清理和现有端口检查。

### 8.3 实现前必须先确认的技术约束

不要直接假设只改一个输入框即可。接手者必须先确认上游 `chatgpt-delegate` 的真实行为：

1. `--output-dir` 是否只在 MCP 进程启动时读取。
2. MCP 工具调用是否支持逐次传入目标目录。
3. 上游是否对路径做根目录限制或安全校验。
4. 同一个 Connector/Tunnel 是否只能指向一个 MCP 实例。
5. 用户是否要求三个项目真正同时在线，还是允许“当前激活一个项目，停止后快速切换”。

优先维持最小改动和安全边界。若上游不支持运行时切换，较稳妥的第一阶段是：让客户端保存项目配置列表，用户选择当前项目后再启动/重启该项目的 MCP；不要绕过上游的目录约束。若用户明确要求多个项目同时在线，则需要进一步设计多实例端口、多个 Tunnel profile/Connector 或修改上游 MCP 协议，不能仅靠前端完成。

### 8.4 预计涉及文件

可能涉及但不限于：

```text
src/types.ts
src/api.ts
src/App.tsx
src/styles.css
src-tauri/src/config.rs
src-tauri/src/commands.rs
src-tauri/src/lib.rs
src-tauri/src/process_manager.rs
src-tauri/Cargo.toml
src-tauri/capabilities/default.json
```

若加入原生目录选择器，优先使用官方 Tauri dialog plugin，并同步配置 Rust/JS 依赖和 capability；不要通过让用户手输路径来代替正常目录选择体验。

## 9. 构建与验证

安装依赖：

```powershell
Set-Location 'D:\claudecode\cchaha\Project\咨询\delegate-control'
npm install
```

基础验证：

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

开发运行：

```powershell
npm run tauri -- dev
```

构建安装包：

```powershell
npm run tauri -- build
```

安装包输出位置：

```text
src-tauri\target\release\bundle\nsis\
src-tauri\target\release\bundle\msi\
```

桌面客户端修改后，应遵循 `clienttrail-desktop-testing` skill 做确定性自动化测试。至少验证：

- 冷启动显示 `stopped`，不会自动建立连接。
- 启动连接只产生一组受管 MCP/Tunnel 进程。
- 重复点击启动不会重复创建进程。
- 停止、托盘退出和异常关闭后，端口及子进程被清理。
- Runtime API Key 不出现在 UI DOM、日志、测试报告和 Git diff 中。
- 目录选择、保存、重启后恢复、项目切换和写入隔离均正确。
- 窗口最小尺寸下文本和控件无重叠。

当前 Rust 中有一个被忽略的真实环境测试：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml live_start_is_idempotent_and_stop_cleans_up -- --ignored
```

它依赖已安装的 Delegate 工具、Clash 和有效 Runtime API Key，执行前应确认不会打断用户正在进行的真实连接。

## 10. 安全边界

- 不读取、打印或提交真实 Runtime API Key。
- 不把 `%APPDATA%\Delegate Control\settings.json` 提交到 Git。
- 不提交 `.env`、日志、测试 evidence、`.client-test/`、`artifacts/`、`dist/` 或 `src-tauri/target/`。
- `output_directory` 最终会授予 ChatGPT Delegate 文件读写范围；目录选择必须明确可见，避免默认指向过大的父目录、用户主目录或磁盘根目录。
- 修改上游 `chatgpt-delegate` 会扩大维护面。只有在客户端无法满足逐项目隔离时才考虑 fork，并应固定上游版本、记录差异和增加路径安全测试。
- 不允许通过关闭路径校验来实现“任意目录”。正确做法是由用户明确选择目录，再把该目录作为本次连接的授权边界。

## 11. 新会话接管顺序

1. 完整阅读本文件和 `README.md`。
2. 运行 Git 状态、分支、远端和基线检查，不要假定工作区干净。
3. 阅读第 7 节列出的关键源码，核对文档与当前代码是否一致。
4. 运行 `npm run build` 与 Rust 单元测试，建立修改前验证基线。
5. 先检查上游 `chatgpt-delegate` 对 `--output-dir` 的实现，再决定客户端项目配置还是上游改造。
6. 在改代码前向用户用简短方案说明：目录选择发生在哪里、何时生效、是否支持同时运行多个项目。
7. 实施时保持现有安全、托盘和进程管理逻辑不变。
8. 完成后自行执行构建、单元测试和桌面自动化测试，不把初级验证工作转交给用户。
9. 提交前检查 `git diff` 是否包含密钥、日志、本机路径配置或测试 evidence。

## 12. 当前未解决事项

- 多项目独立输出目录尚未实现。
- 当前没有项目配置列表，也没有原生目录选择器。
- 当前 MCP 仍使用单实例、单端口、单一 `--output-dir`。
- 是否支持多个项目同时在线，必须结合上游和 Tunnel/Connector 约束进一步验证。
- 本交接文档本身不代表新功能完成，只用于让下一会话准确接管。

