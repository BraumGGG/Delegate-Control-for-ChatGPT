# ChatGPT Delegate Connector

目标：让 Codex 只做编排，让 ChatGPT Web/App 执行调研、思考、撰写、PRD、比较和联网查询，并通过本机 MCP connector 把 Markdown 结果写回项目文件系统。

关键边界：

- ChatGPT 是执行方，Codex 不生成正文。
- MCP 不能主动通知 ChatGPT；Codex 仍需要用官方 Chrome 控制插件把 prompt 发到 ChatGPT Web。
- 本机 connector 只保存、列出、读取结果；不调用 Codex、OpenAI API、ChatGPT 私有接口，也不读取 session token。
- 默认结果落到文件，不把全文回填 Codex，除非用户再要求。

## 数据流

```text
Codex 用户请求
  -> chatgpt-delegate prepare 生成 task.json/prompt.md
  -> Codex 启动 MCP server + HTTPS tunnel
  -> Codex 用官方 Chrome 插件把 prompt.md 发到 ChatGPT Web
  -> ChatGPT 研究/思考/撰写/联网查询
  -> ChatGPT 调用 MCP save_task_result
  -> 本机写入 artifacts/chatgpt-delegation/<task_id>/result.md
  -> Codex 轮询 status，只返回文件路径和状态
```

## 安装与启动

```bash
python3 -m pip install -e ".[chatgpt,test]"
```

启动本机 MCP server：

```bash
chatgpt-delegate serve --host 127.0.0.1 --port 8000
```

ChatGPT Developer Mode connector 需要公网 HTTPS URL。开发期可用 ngrok、Cloudflare Tunnel 或 OpenAI Secure MCP Tunnel，把公网地址指到本机 `/mcp`。

通过公网域名连接时，必须显式加入 allowed host，避免 MCP SDK 的 DNS rebinding 保护拒绝请求：

```bash
chatgpt-delegate serve \
  --host 127.0.0.1 \
  --port 8000 \
  --public-host your-tunnel.example.com
```

ChatGPT connector URL：

```text
https://your-tunnel.example.com/mcp
```

实际配置时使用无尾斜杠 `/mcp`；`/mcp/` 可能触发 307 重定向，部分 MCP client 会跟随到错误的本机 HTTPS 地址。

新建或刚连接的 app 首次使用时，建议在 ChatGPT composer 里输入 `@` 并选择对应 app（例如 `ChatGPT Delegate Connector`），再粘贴 `prompt.md`。这样可以避免模型在普通新对话里先卡在工具发现阶段。

如果使用随机临时 tunnel，ChatGPT 侧 app 的 URL 可能需要随 tunnel 重建。当前 ChatGPT Apps UI 不一定提供修改已创建 dev app URL 的入口；更稳的做法是使用固定公网域名，或在 tunnel host 变化后新建一个 dev app，再删除旧的临时 app。

## 委托任务 CLI

准备任务和 ChatGPT prompt：

```bash
chatgpt-delegate prepare "调研一下 MCP 社区最佳实践" --kind auto
```

`--kind` 可选：

| kind | profile | 用途 |
| --- | --- | --- |
| `auto` | 自动推断 | 默认；按中文触发词路由 |
| `research` | `research_pro` | 调研、深度研究、社区最佳实践 |
| `think` | `thinking_high` | 设计、构思、思考、比较、撰写、方案、PRD |
| `lookup` | `instant_web` | 查一下、找一下、确认、最新 |

检查任务：

```bash
chatgpt-delegate status <task-id>
```

列出完成结果：

```bash
chatgpt-delegate list
```

读取结果元数据或正文：

```bash
chatgpt-delegate read <task-id> --metadata-only
chatgpt-delegate read <task-id> --markdown
```

## MCP 工具

`serve` 暴露这些工具：

| 工具 | 类型 | 作用 |
| --- | --- | --- |
| `save_task_result` | 写 | 保存 ChatGPT 委托任务结果到 `result.md/result.json` |
| `list_results` | 读 | 列出已完成任务 |
| `read_result` | 读 | 读取指定任务结果 |
| `connector_status` | 读 | 返回 connector 状态，明确 `codex_execution=false` |
| `save_markdown_report` | 写 | 兼容旧研究报告 prompt |
| `list_reports` | 读 | 兼容旧报告列表 |
| `read_report` | 读 | 兼容旧报告读取 |

写文件约束：

- 任务 id 只允许字母、数字、点、短横线、下划线。
- 结果固定写入对应任务目录下的 `result.md` 和 `result.json`。
- 解析后的路径必须停留在输出目录内。
- 默认最大 Markdown 大小为 5MB。
- `save_task_result` 默认覆盖同一任务的旧结果。

## 旧报告命令

旧入口 `chatgpt-research-connector` 仍可用，指向同一实现。旧的单文件报告导入/读取保留为：

```bash
chatgpt-research-connector prompt "100M上下文大模型"
chatgpt-research-connector import --title "100M上下文大模型" --input report.md --filename 100m-context-models.md
chatgpt-research-connector reports-list
chatgpt-research-connector report-read 100m-context-models.md
```

## Deep Research 限制

普通 ChatGPT 对话在 Developer Mode 下可以调用自定义 MCP 工具，适合 `save_task_result` 自动写回。

ChatGPT Deep Research 产品模式通常更偏向资料源连接器，不保证能调用自定义写入工具。因此本工具默认走普通 ChatGPT 对话，并在 prompt 中要求 thinking pro/high/instant 风格；不依赖 UI 模式必须精确切换。

## 不实现的路线

| 方法 | 结论 |
| --- | --- |
| 逆向 ChatGPT Web 私有接口 | 不实现；凭据和条款风险高 |
| 读取 session cookie/token | 不实现 |
| OpenAI API / Codex exec 代写正文 | 不符合节省 Codex token 的目标 |
| 纯 MCP 后台推送 ChatGPT 执行 | MCP 不具备主动通知 ChatGPT 的能力 |

## Sources

- [OpenAI Apps SDK: connect from ChatGPT](https://developers.openai.com/apps-sdk/deploy/connect-chatgpt)
- [OpenAI Apps SDK: MCP server quickstart](https://developers.openai.com/apps-sdk/build/mcp-server/)
- [OpenAI Platform: MCP and connectors](https://developers.openai.com/api/docs/mcp)
- [OpenAI Platform: Deep research guide](https://developers.openai.com/api/docs/guides/deep-research)
- [OpenAI Terms of Use](https://openai.com/policies/row-terms-of-use/)
- [FastMCP ChatGPT integration](https://gofastmcp.com/integrations/chatgpt)
- [FastMCP server transport guide](https://gofastmcp.com/deployment/running-server)
- [modelcontextprotocol/python-sdk](https://github.com/modelcontextprotocol/python-sdk)
