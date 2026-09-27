# 多项目显式路由与 Project Key 设计说明

## 背景与目标

DCFC 已使用固定 11 工具 Router、单 Tunnel 和多个项目目录。当前多项目写操作允许省略 `project_id` 并回退到 UI 活动项目，可能把 ScreenCast 请求路由到 DCFC。项目创建又会生成 `new-project`、`new-project-2` 等稳定 key，用户修改显示名称后 key 不变，但 UI 没有充分解释两者区别。

本次实现两个已接受决策：

1. 多项目模式下，所有持久化写操作必须显式提供 `project_id`。
2. 明确区分 Project Name 与 Project Key，并提供停机状态下的显式 key 迁移。

## 现状与约束

- 固定 11 工具名称、单 Tunnel、单 Proxy 和多项目目录隔离不能改变。
- 单项目模式继续允许省略 `project_id`。
- 绝对路径、UNC、父目录逃逸、符号链接逃逸、UTF-8、大小、SHA-256、备份和原子写入保护保持不变。
- Runtime API Key 仍只存储在 Windows Credential Manager。
- 当前 `settings.json` 同时保存 `ProjectConfig.id`、`ProjectConfig.name` 和 `active_project_id`。
- Router 注册表由设置在连接启动时重新生成。
- ChatGPT 历史提示和外部文档中的旧 `project_id` 不受 DCFC 控制，无法自动迁移。

## 方案对比

### 方案一：继续使用活动项目回退

- 优点：兼容所有旧调用。
- 缺点：多项目写操作可能静默写错目录，不能满足 PDH-004。

### 方案二：多项目写入强制显式 key，迁移仅允许停机执行（采用）

- 优点：写入授权明确；单项目兼容；迁移期间没有运行中的旧 Router 或进程映射；容易实现失败回滚。
- 缺点：旧的多项目无 `project_id` 写调用会收到明确错误；迁移前必须停止连接。

### 方案三：运行中热迁移 Project Key

- 优点：少一次人工停止操作。
- 缺点：项目进程表、日志目录、Router 注册表和 ChatGPT 调用可能同时存在新旧 key，不满足原子性要求。

## 推荐方案

采用方案二。Router 将读操作与写操作的项目解析分开：读操作可保留活动项目便利回退；写操作在两个及以上项目在线且没有 `project_id` 时，在任何文件变更之前拒绝调用，并返回 `显示名称 (project_id)` 列表。

Project Key 创建和迁移由桌面端管理。新项目在首次保存前可编辑 key，并从英文/数字项目名生成可读建议；已保存项目通过独立迁移交互改 key。迁移要求所有连接停止，校验新 key 和冲突，更新设置与活动项目引用，并迁移项目日志目录；失败时恢复原 key 和日志目录。

## 详细设计

### Router 写入保护

`router.py` 增加带操作性质的项目解析函数。以下工具标记为 side-effecting：

- `save_task_result`
- `save_markdown_report`
- `append_text_file`
- `replace_text_in_file`
- `delete_text_from_file`

多项目且缺少 `project_id` 时返回可纠正错误，不调用任何文件函数。错误只包含项目名称和 key。日志只记录工具名、项目 key、相对文件名和错误类别，不记录正文、替换内容、密钥或绝对目录。

### 新项目身份

前端继续使用现有 `ProjectConfig`，另维护当前会话中新建且尚未成功保存的项目 key 集合。创建项目时生成不冲突的临时 key；编辑 Project Name 时，仅对尚未提交且 key 仍等于自动建议的项目同步生成新的建议。用户可在首次保存前直接修改 key。

UI 明确显示：

- `项目名称`：人类可读，可随时修改；
- `Project Key / project_id`：Router 和 ChatGPT 使用的稳定标识。

### 显式迁移

新增 Tauri command：

```rust
migrate_project_key(old_project_id: String, new_project_id: String) -> Result<AppSettings, String>
```

迁移流程：

1. 确认没有受管项目、Router、Proxy 或 Tunnel 进程运行。
2. 校验旧项目存在、新 key 格式合法且未被占用。
3. 构造完整的新设置并执行现有设置校验。
4. 预检旧日志目录与新日志目录，拒绝覆盖已有新目录。
5. 如旧日志目录存在，先重命名到新 key。
6. 原子保存更新后的 `settings.json`，同步 `active_project_id`。
7. 保存失败则把日志目录恢复为旧 key，并保持内存设置不变。
8. 下次启动根据新设置重新生成 Router 注册表。

不尝试重写 ChatGPT 历史消息、用户文档或其他外部系统中的旧 key。

### 异常与边界处理

- 运行中迁移：拒绝并提示先停止全部连接。
- key 格式错误或重复：拒绝，不修改设置和日志。
- 新日志目录已存在：拒绝，防止合并不相关历史。
- 日志重命名失败：拒绝，设置不变。
- 设置保存失败：回滚日志目录；若回滚也失败，返回包含恢复路径的明确错误，但不记录敏感内容。
- 单项目写入缺少 key：仍自动选择唯一项目。
- 多项目读取缺少 key：保持当前活动项目回退。

## 测试策略

- Python Router 单元测试覆盖单项目省略 key、多项目写入拒绝、显式项目写入、未知 key、无文件变更和错误项目列表。
- Rust 单元测试覆盖 key 迁移设置变换、活动项目更新、重复/非法 key、日志目录冲突和停机要求。
- 前端构建覆盖新字段、迁移交互和 API 类型。
- 完整执行 `npm run build`、Rust 测试、Connector 测试和 `npm run check:multi-project`。
- 手工静态检查 Git diff，确认没有交接文档、设置、日志、密钥或测试 evidence 被提交。

## 风险与待确认项

- 历史 ChatGPT 调用中的旧 key 不能自动迁移，必须在 UI 明确提示。
- 中文项目名无法在不增加拼音依赖的情况下生成有意义的 ASCII key；此时建议 `project` 并要求用户确认或修改。
- 当前 Router 直接使用注册表中的项目目录执行文件操作；本次不重构为后端 MCP 转发，以保持变更范围克制。
