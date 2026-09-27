# 固定工具目录 Router MCP 设计说明

## 背景与目标

当前 DCFC 在多项目模式下把项目 ID 拼接到 MCP 工具名中。项目新增、删除、迁移或 ID 重建都会改变 ChatGPT 看到的工具 Schema，导致旧 Connector 会话继续调用不存在的 namespace 工具。

本设计将 ChatGPT 可见的 MCP 工具目录固定下来，项目选择改为工具参数 `project_id`。DCFC 仍然为每个项目运行独立的 `chatgpt-delegate` 后端、独立端口和独立输出目录；Router 只负责安全路由，不合并项目文件。

成功标准：

- ChatGPT 只需注册一次固定工具目录。
- 项目数量、项目名称、项目目录和项目内部端口变化不会改变工具名。
- 单个 Tunnel 支持多个项目同时在线。
- 现有 7 个工具的参数和返回行为保持兼容，仅增加可选 `project_id`。
- 不同项目不能混写文件，未知项目不能回退到猜测目录。

## 现状与约束

- DCFC 当前用 `mcp-proxy` 聚合项目后端。
- 单项目使用裸工具名，多项目使用 `<project_id>__<tool>`，导致 Schema 动态变化。
- 新项目 ID 当前由时间戳生成，不适合作为公共 API 标识。
- Runtime API Key 必须继续使用 Windows Credential Manager，不进入 Router 配置、日志或 Git。
- Tunnel 数量保持一个；不为项目创建独立 Tunnel ID。
- 项目后端继续使用已有路径校验、SHA 冲突保护、备份和原子写入逻辑。

## 方案对比

### 方案一：继续动态 namespace

- 优点：改动最少。
- 缺点：项目拓扑变化会持续触发 ChatGPT 工具缓存不一致；需要长期维护兼容别名。
- 结论：不采用。

### 方案二：固定 Router MCP

- 优点：工具目录稳定；项目增删不需要重新注册工具；显式项目参数便于审计和隔离；一个 Tunnel 支持多个项目。
- 缺点：需要维护一个本地路由 MCP；需要对请求参数和后端错误做一层转发。
- 结论：采用。

### 方案三：固定单项目工具并维护旧 namespace 别名

- 优点：可短期兼容旧 ChatGPT Schema。
- 缺点：别名会随项目历史积累，旧 ID 可能错误映射到新目录，无法成为长期公共接口。
- 结论：不作为正式方案；迁移期间不自动启用隐式别名。

## 推荐方案

对外只暴露固定的 11 个工具：

```text
connector_status
list_reports
read_report
save_markdown_report
list_results
read_result
save_task_result
read_text_file
append_text_file
replace_text_in_file
delete_text_from_file
```

所有工具新增可选 `project_id`：

- 未提供时使用 DCFC 当前项目；
- 提供时按稳定项目 key 严格路由；
- 项目不存在返回可用项目 key，不猜测目录；
- 多项目在线时，ChatGPT 应从用户请求中识别项目，否则先澄清。

## 详细设计

### 架构

```text
ChatGPT Connector
        |
        | 固定工具目录
        v
DCFC Router MCP :8100
        |
        +--> chatgpt-delegate 项目 A :8001
        +--> chatgpt-delegate 项目 B :8002
        +--> chatgpt-delegate 项目 C :8003
```

Router 使用 DCFC 管理的项目注册表，不接受任意主机、端口或文件系统路径。每个项目后端仍由 Job Object 管理，Router 停止时不改变现有进程清理责任。

### 稳定项目标识

项目配置新增稳定的用户可读 key（例如 `screencast`、`client-a`）。显示名称可以修改，但 key 保存后不可自动重写。旧时间戳 ID 迁移为稳定 key，并保留内部迁移映射仅用于读取旧配置，不再生成公共工具 namespace。

### 工具参数

`project_id` 为可选字符串。旧调用不带该字段时使用 DCFC 当前项目，以保持旧 7 个工具兼容；多项目调用应显式传入项目 key。`connector_status` 返回当前项目、可用项目 key、输出目录和文本编辑能力，但不返回 Runtime API Key。

### 路由与错误

- Router 根据 `project_id` 查找启用且在线的项目后端。
- 后端未启动、项目禁用或项目不存在时返回明确的 MCP 工具错误。
- Router 不根据目录名、显示名模糊匹配项目。
- 后端工具错误原样保留安全错误信息，不暴露密钥、绝对内部日志路径或进程命令行。
- 路径安全仍由项目 Connector 执行；Router 只传递相对文件名和工具参数。

### 迁移与兼容

- 当前项目 `project-1790084958708` 迁移为稳定 key `screencast`，目录不变。
- 现有 7 个工具保持名称、旧必填参数和返回字段；新增 `project_id` 为可选字段。
- 旧的 `default`、`project_1790100571834` 不再作为公共 namespace。
- 迁移完成后需要重新注册一次 ChatGPT Connector；之后项目增删不要求重新注册。
- 不自动把历史 namespace 映射到新目录，避免静默写错项目。

### 异常与边界

- 没有启用项目：DCFC 不启动 Router，并显示可理解错误。
- 没有当前项目但存在多个在线项目：不带 `project_id` 的写操作拒绝并要求明确项目。
- 项目 key 重复、包含不安全字符或迁移冲突：保存设置时拒绝。
- Router 与项目后端版本不兼容：启动前执行能力握手，失败时不宣称连接已就绪。

## 测试策略

- Router 固定工具名和 Schema 测试。
- 单项目无 `project_id` 默认路由测试。
- 多项目显式 `project_id` 路由测试。
- 未知、禁用、离线项目错误测试。
- 项目增删和目录迁移不改变 `tools/list` 名称测试。
- 报告工具和 4 个文本编辑工具的目录隔离测试。
- 路径越权、SHA 冲突、原子写入和备份测试。
- Rust 单元测试、Connector 测试、前端构建和 Tauri 构建。
- 使用 `clienttrail-desktop-testing` 完成桌面端启动、状态显示、项目路由和停止清理自动化验证。

## 风险与回退

- Router 是新增运行时组件，启动失败时必须阻止 Tunnel 启动并在 UI 显示原因。
- 迁移期间保留旧项目配置读取能力；不删除用户目录或文档。
- 回退时可恢复到 `baseline-2026-09-22`，但回退版本不具备固定 Router 能力。
- 不提交 `settings.json`、日志、运行时密钥、`dist` 或 `target`。
