# 多项目 MCP Proxy 设计说明

## 背景与目标

DCFC 当前只有一个全局 `output_directory`，启动时把该目录传给单个 `chatgpt-delegate` MCP。由于上游 MCP 在进程启动时固定输出目录，多个项目不能共用一个实例而保持可靠隔离；为每个项目创建独立 tunnel ID 又会让项目数量增长转化为 OpenAI Tunnel 资源数量增长。

本阶段采用一个共享 Tunnel 和一个现成的 `mcp-proxy` 聚合代理。每个在线项目运行独立的 `chatgpt-delegate` 后端，代理按项目名称命名空间聚合工具，ChatGPT 通过自然语言指定项目，DCFC 负责项目目录授权与进程生命周期。

成功标准：

- 一个 tunnel ID/Connector 可以同时暴露多个项目的 MCP 工具。
- 每个项目有独立输出目录，项目间不能混写。
- 新增项目不需要新增 tunnel ID。
- 用户只需在 DCFC 中为项目配置一次目录；日常可在 ChatGPT 中用自然语言指定项目。
- 现有 Runtime API Key、托盘、单实例、日志和 Job Object 清理行为保持安全边界。

## 现状与约束

- `chatgpt-delegate --output-dir` 只在 MCP 进程启动时生效。
- 上游工具调用没有逐次目录参数，文件名路径也会限制在启动目录根下。
- DCFC 当前只有一组 MCP/Tunnel 子进程和单一 `AppSettings.output_directory`。
- `mcp-proxy` 通过 TOML 配置把多个 HTTP MCP 后端聚合到一个本地 HTTP endpoint，并以 backend name 作为工具命名空间。
- Runtime API Key 继续只通过 Windows Credential Manager 读取，不进入 TOML、settings.json 或日志。
- `mcp-proxy` 可执行文件作为外部依赖配置，不在本仓库提交二进制、密钥或用户机器配置。

## 推荐方案

DCFC 维护一份项目列表和一份由程序生成的 `mcp-proxy` TOML。启动顺序为：校验全局设置和项目唯一性，启动每个启用项目的 `chatgpt-delegate`，生成 proxy 配置，启动一个 `mcp-proxy`，最后启动一个把 MCP URL 指向 proxy endpoint 的 `tunnel-client`。

代理工具命名空间使用项目稳定 ID，例如 `client-a/save_task_result`、`research-b/read_result`。如果多个项目在线但自然语言没有说明项目，ChatGPT 必须先澄清；DCFC 不根据模糊文本猜目录。

配置模型分为全局与项目两层：全局保存 proxy/tunnel/MCP 可执行文件和端口；项目保存稳定 ID、名称、输出目录、MCP 主机/端口和 enabled 状态。旧版顶层 `output_directory` 迁移为一个启用的 `default` 项目，旧版 profile 继续提供 tunnel ID。

校验要求：启用项目的 ID、backend slug、输出目录和 MCP 端口都唯一；输出目录必须是绝对路径；proxy、health 与 MCP 端口不能冲突；proxy 和所有本地服务只绑定 loopback。

## UI 与进程行为

- 设置页提供项目增删改、原生目录选择、启用/停用和单项目/全部启停。
- 总览显示当前选中项目和目录；当前选中只是界面焦点，不会停止其他在线项目。
- 运行中修改项目目录、名称、端口或启用状态时，必须显示需要重启共享 proxy/Tunnel 的提示。
- `ProcessManager` 持有多个 MCP 子进程、一个 proxy、一个 Tunnel 和一个共享 Job Object；停止顺序为 Tunnel、proxy、所有 MCP。
- 某个项目失败时只标记该项目失败，其他项目状态保持真实；整体状态不能把失败项目伪装为在线。
- Runtime API Key 仍只注入 Tunnel 子进程环境变量；proxy TOML 和日志不得包含密钥或文档正文。

## 测试策略

- Rust：配置迁移、重复目录/端口/backend slug 拒绝、proxy TOML 生成、多项目状态聚合、单项目停止不影响其他项目、失败回滚和 Job Object 清理。
- 前端：项目 CRUD、原生目录选择、启停状态、重启提示、项目日志分组、最小窗口无重叠。
- 桌面自动化：冷启动 stopped；两个模拟项目生成两组 MCP 加一组 proxy/Tunnel；重复启动幂等；停止一个项目不影响另一个；退出后全部受管进程和端口清理；Runtime API Key 不出现在 DOM、日志、报告或 Git diff。
- 外部 `mcp-proxy.exe` 只作为可配置依赖验证，不提交二进制；真实 Tunnel 测试继续保持显式 ignored。

## 风险

- 外部 proxy 版本和配置格式需要在启动前执行检查并锁定兼容版本。
- ChatGPT 是否即时刷新动态新增工具取决于 MCP client，因此项目拓扑变更必须明确提示刷新/重启共享连接。
- 项目数量增长主要增加本地 MCP 进程；未启用项目只保存配置，不占用运行资源。
