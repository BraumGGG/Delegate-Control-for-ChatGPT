![Delegate Control for ChatGPT](docs/images/readme-cover.png)

# Delegate Control for ChatGPT

**让 ChatGPT 的成果进入本机项目目录，并以明确的项目边界持续读取和更新文档。**

Delegate Control for ChatGPT（DCFC）是一款 Windows 桌面控制台，通过 MCP 工具连接 ChatGPT 与用户配置的本机项目目录。它支持报告保存、任务结果管理、UTF-8 文本读写和结构化交接追加，同时管理项目 MCP、Router、MCP Proxy 与安全隧道的运行状态。

本项目是第三方工具，并非 OpenAI 或 ChatGPT 官方产品。它不是另一个模型聊天客户端，也不提供通用远程桌面或任意命令执行能力。

当前源码与候选安装包版本为 **0.1.6**。本机构建与回归检查已通过，但 Direct/Magic 双模式的最终用户真机端到端验收仍待完成；本机就绪和页面截图不能证明外部 Connector 已成功调用。

## 解决什么问题

ChatGPT 可以完成研究、分析、设计和写作，但成果经常停留在聊天窗口中。用户还需要手动复制、选择目录、维护文件版本，并把新的项目资料重新提供给 ChatGPT。

DCFC 将这些操作连接起来：用户在本机配置项目目录，在 ChatGPT 中完成内容工作，再通过 Connector 工具保存成果，或者读取、追加和修改已有文本。例如：

- “把研究报告保存到 ScreenCast 项目，目标 project_id 是 screencast。”
- “读取 DCFC 项目的交接文档，先报告当前文件 SHA-256。”
- “使用刚才读取的 SHA-256，在该文档末尾追加这次评审结论。”

适合开发者、产品设计者、研究人员和需要长期维护项目资料的用户。内容仍由 ChatGPT 生成，DCFC 负责工具执行、文件边界和本机运行管理。

## 主要功能

| 功能 | 当前实现 |
| --- | --- |
| 多项目管理 | 保存名称、稳定 Project Key / project_id、独立输出目录、端口和启用状态；不限制为三个项目，实际承载数量受机器资源影响 |
| 项目级运行控制 | 启动/停止单个项目、启动/停止全部项目；当前选中项目不等于停止其他项目 |
| 共享连接 | 多项目共用一个固定 Router、一个 MCP Proxy 和一条 Tunnel，不需要为每个项目创建 Connector 或 Tunnel ID |
| 报告与任务结果 | 保存、列出、读取 Markdown 报告和带任务标识的成果 |
| 文本编辑 | 读取、追加、精确替换、删除 UTF-8 文本片段；限定在对应项目输出目录内 |
| 结构化交接 | 向固定交接文件追加经过字段、编号和文件哈希校验的条目 |
| 首次配置 | 引导配置运行依赖、Runtime API Key、Tunnel ID、网络模式和首个项目；提供外部创建/管理页面入口 |
| 本机安装诊断 | 检查配置、内置组件、Tunnel Client、选定网络模式、共享链路和每个项目；不自动重置配置或启停进程 |
| 日志与启动诊断 | 按组件/项目查看日志，记录启动阶段、实际路径、配置可读性和 HTTP 状态，归档上一运行会话 |
| 桌面常驻 | 系统托盘、单实例运行、关闭窗口隐藏；界面版本读取真实应用版本信息 |

## 架构与调用链

```text
ChatGPT Connector
        |
   Secure Tunnel（一个）
        |
     MCP Proxy（一个）
        |
  固定 Router MCP（一个）
        | 按 project_id 解析项目注册表
        +--- 项目 A 输出目录
        +--- 项目 B 输出目录
        +--- 项目 C 输出目录
```

DCFC 桌面客户端负责配置、启动、停止和监控这些组件，并为每个在线项目管理独立 MCP 进程、端口和日志。公共 MCP Proxy 只连接固定 Router，不随项目数量增加公共工具命名空间。

**当前 Router 的文件工具根据项目注册表直接操作选定项目目录，并非将每次文件调用转发到对应项目 MCP 进程。** 项目进程生命周期与公共工具执行是两个需要区分的层面。

单项目停止会移除目标项目进程并更新 Router 的在线项目注册表；其他项目仍在线且共享链路正常时，保持 Router/Proxy/Tunnel。最后一个在线项目停止时才释放共享链路。共享链路故障时可能需要重新建立连接，不能将这种恢复等同于正常的单项目停止。

项目属于当前机器上的 DCFC 实例。同一 ChatGPT 账号不会自动同步不同机器的项目；切换机器需要使用对应机器的 Tunnel/Connector 端点。

### 固定工具目录

当前 Router 注册 **12 个固定工具**，项目选择通过可选 `project_id` 参数完成：

| 工具分组 | 工具名称 |
| --- | --- |
| 任务结果 | `save_task_result`、`list_results`、`read_result` |
| Markdown 报告 | `save_markdown_report`、`list_reports`、`read_report` |
| 状态查询 | `connector_status` |
| 文本操作 | `read_text_file`、`append_text_file`、`replace_text_in_file`、`delete_text_from_file` |
| 交接追加 | `append_handoff_entry` |

工具名称与 Schema 不随项目增删变化，避免每个项目产生一套 `project_id__tool_name`。项目改名不会自动改变 Project Key；迁移 key 需要显式操作。

多项目调用建议始终提供稳定 `project_id`。底层允许省略后使用当前项目或唯一可用项目，但这不是自然语言意图识别：请求不明确时应先澄清，不能猜目录。仅增删项目不改变工具清单；Connector 工具本身升级后仍可能需要在 ChatGPT 中重新同步或注册。

## 关键技术

### 分层桌面实现

- **Tauri 2 + Rust**：原生桌面外壳、系统托盘、凭据访问、配置持久化、进程生命周期和诊断。
- **React + TypeScript + Vite**：首页、项目管理、运行日志、连接设置、安装诊断和首次配置向导；通过 Tauri IPC 调用 Rust。
- **Python + FastMCP**：报告、任务结果、文本编辑和固定 Router MCP 工具；使用 HTTP 传输。
- **MCP Proxy + Tunnel Client**：公共 MCP 入口适配和外部连接链路。

### 进程生命周期与就绪检查

Rust ProcessManager 跟踪项目及共享组件的 Child 句柄，将项目级停止与全局停止分开。Windows Job Object 管理所属进程，减少退出后残留进程的风险；状态访问通过 Mutex 串行保护，阻塞启停通过 Tauri blocking task 执行。

启动不只检查“进程创建成功”：还检查文件、端口占用、进程退出、就绪日志、Router TCP 可连接性与 MCP initialize HTTP 状态。对 Proxy 显式传入 `--config`，避免依赖工作目录中的 proxy.toml。

### 文本版本保护与可恢复写入

文本读取返回 SHA-256。一般编辑工具可传入 `expected_sha256`，检测读取后发生的文件变化；结构化交接追加强制要求该参数。替换/删除默认要求目标片段恰好出现一次，避免匹配过多时误改。

修改已有文本前保留 `.dcfc-backups` 备份，然后在目标目录写临时文件、刷新并替换原文件。交接追加使用操作系统文件锁，并校验条目格式与重复编号。这些机制降低冲突和半写入风险，但不是 Git 合并，也不保证消除与所有外部编辑器的竞争。

### 目录与凭据边界

文本工具只接受项目目录内的相对文件路径，拒绝绝对路径、父目录跳转等输入，并检查解析后的路径仍位于项目根目录内；只处理符合大小限制的 UTF-8 文本。

Runtime API Key 保存在 **Windows Credential Manager**，不写入普通 settings，也不向界面提供明文读取接口。后端启动 Tunnel 时读取凭据并注入子进程。这一凭据用于运行连接，不是 DCFC 用来调用模型生成正文的接口。

### Direct / Magic 网络模式

| 模式 | 行为 |
| --- | --- |
| Direct | 不检查 Magic 端口，不向 Tunnel 注入 Magic HTTP 代理参数；仍需要有效的实际出网条件 |
| Magic | 使用配置的本机 HTTP/混合代理地址和端口，启动前检查监听；不是扫描所有本机端口 |

MCP Proxy 与本机 Router 通信保留 `NO_PROXY/no_proxy` 的 localhost、127.0.0.1、::1 绕行，避免 loopback 请求误走系统代理。Magic 端口可连接不代表公网、Tunnel 或 ChatGPT 调用成功。

旧 settings 缺少网络模式字段时按 Magic 兼容加载。运行期间改变网络模式后，应停止连接再按新模式启动。

### 受管运行时与诊断

Connector 使用 PyInstaller onedir 打包；MCP Proxy 固定为 v0.4.3，随安装包及许可证交付。正式构建检查运行资源完整性与 Proxy SHA-256，启动时从 Tauri resource_dir 解析安装资源路径，不依赖当前工作目录。缺失/不可读文件由向导或 Doctor 提示；哈希检查属于发布构建流程，不应误解为每次应用启动都会完整校验 SHA-256。

启动诊断区分配置生成、文件可读性、Router TCP、Router MCP HTTP 和 Proxy backend 初始化等阶段。每次新的完整启动创建日志 session，上一会话进入 archive；单项目追加启动可能继续使用当前会话。

Public URL 只作为外部 Connector 注册地址信息，不是本机启动条件，不会自动创建公网映射。Setup Doctor 不再执行公网 DNS/HTTPS 或 ChatGPT 注册探测；实际外部连接需独立验证。

## 使用流程

1. 安装 DCFC，按首次配置向导检查内置运行组件并选择实际 Tunnel Client。
2. 配置当前组织的 Runtime API Key 与 Tunnel ID；不要把密钥发到聊天、截图或日志。
3. 根据实际网络选择 Direct 或 Magic；Magic 填写本机代理的实际监听端口。
4. 创建项目，确认稳定 project_id、绝对输出目录、独立端口和启用状态，保存设置。
5. 启动目标项目或全部项目，在本机诊断与日志中确认各组件状态。
6. 按实际 Tunnel 提供的外部地址在 ChatGPT 中完成 Connector 注册/授权并核对工具。
7. 明确指定项目进行状态查询、报告保存或文本编辑，检查文件实际落盘位置。

项目目录、端口和启用配置变化后应重启对应运行连接；界面选中状态不能替代路由参数。Public URL 可单独保存，不要求为此重启本地项目。

### 安装版运行依赖

- Windows 10/11 x64。
- 支持所需外部 Connector 功能的 ChatGPT 使用环境及有效的 Tunnel 配置；具体外部功能可用性由平台决定。
- `chatgpt-delegate-edit.exe` 与 `mcp-proxy.exe`：安装包内置，普通用户不需要自行安装或选择其路径；缺失时重新安装可信安装包。
- `tunnel-client.exe`：由用户单独提供，首次向导选择实际文件。来源为 [openai/tunnel-client](https://github.com/openai/tunnel-client)；此前验证版本为 v0.0.14，不等于承诺兼容任意后续版本。
- 本机 Magic HTTP/混合代理：仅 Magic 模式需要，Direct 不依赖其端口。

安装版用户无需准备源码构建用的 Node.js、Rust、Python。不要直接双击 MCP Proxy exe 后把“reading proxy.toml”当成受管启动故障；DCFC 会生成配置并显式传入路径。

## 本地开发与构建

从源码构建需要 Node.js/npm、Rust 工具链、Windows Tauri 构建环境及 Connector 打包所需的 Python 环境。具体依赖以仓库 package.json、Cargo.toml 和构建脚本为准。

```powershell
npm install
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run check:multi-project
npm run check:managed-proxy
npm run check:onboarding
npm run check:setup-doctor
npm run tauri -- dev
```

`npm run check:version-network` 需要预先运行本机 Vite 页面，默认地址为 http://127.0.0.1:1420，也可通过 `DCFC_UI_CHECK_URL` 指定；模拟 IPC 检查不等于真实客户端或 Tunnel 验收。

正式打包运行 `npm run release:build`。脚本在 Connector 资源缺失时构建 onedir runtime，准备固定版本 Proxy、核对完整性，再执行 Tauri build。后续每次重新生成候选安装包应递增版本号，不复用已交付版本。

安装包输出目录：

```text
src-tauri/target/release/bundle/nsis/
src-tauri/target/release/bundle/msi/
```

主要源码入口：[桌面状态](src-tauri/src/lib.rs)、[进程管理](src-tauri/src/process_manager.rs)、[配置](src-tauri/src/config.rs)、[本机诊断](src-tauri/src/setup_doctor.rs)、[Router](vendor/chatgpt-delegate/src/chatgpt_delegate/router.py)、[文本编辑](vendor/chatgpt-delegate/src/chatgpt_delegate/text_editing.py)。

## 配置与日志

非敏感设置、Proxy/Router 配置和日志保存在 `%USERPROFILE%\.delegate-control\`；历史 `%APPDATA%\Delegate Control\` 为兼容来源，迁移/恢复保留相关备份。

配置加载区分真实持久化项目、运行状态和 UI 展示；缺失或损坏配置不能简单通过注入并保存默认项目掩盖。旧单项目 output_directory 配置仍有兼容迁移规则，不等于每次启动都应新建默认项目。当前恢复逻辑会在有效配置候选中按项目数量等信息选择，排障应保留现场，而非清空设置重建项目。

日志按项目 MCP、Router、Proxy、Tunnel 和启动诊断分类。提交问题时说明版本、实际日志目录、session 和操作时间，检查是否误用 archive 中的旧日志，并脱敏路径、项目内容和凭据。

## 安全与限制

- ChatGPT 会接收工具读取的内容；本地文件并不因此成为“永不离开本机”的数据。仅选择允许通过 Connector 处理的项目目录。
- 文件工具不执行任意系统命令、不调用模型 API 生成正文，也不通过 ChatGPT 私有 API 操作账号。
- 当前文本工具不直接编辑 DOCX/XLSX 等二进制格式，不自动同步跨机器项目。
- 目录边界是应用级校验，不是操作系统级沙箱；版本保护、备份和文件锁不能消除所有并发与环境风险。
- 不保证每次调用通过外部平台安全检查；本机就绪不代表外部注册、Tunnel 或端到端调用成功。
- 不提交 settings、日志、测试证据、artifacts、dist、target、密钥或本机备份。安全问题按 [安全政策](SECURITY.md)报告，不公开敏感内容。
- 关闭窗口仅隐藏至托盘；需要彻底退出时先停止连接，再从托盘选择退出。

## 许可证与回退

项目使用 [MIT License](LICENSE)。第三方组件保留各自许可证，见 [第三方声明](THIRD_PARTY_NOTICES.md)。

0.1.2 是此前用户接受的冻结回退基线，0.1.6 仍为候选行为。回退前备份当前配置和日志，使用保留的原安装包并核对原校验值；基线记录不等于已证明当前机器仍持有可用旧包，不要重打旧版本冒充原验收包。

多项目改造前的源码保护标签为 `baseline-2026-09-22`。仅在当前工作已保存后，才基于该标签创建回退分支：

```powershell
git switch -c rollback-baseline-2026-09-22 baseline-2026-09-22
```

不要对包含用户改动的工作区执行破坏性 reset 或 checkout，也不要删除 settings、重建项目或恢复默认配置来掩盖故障。
