# 多项目验证说明

## 已执行

- `npm run build`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `git diff --check`
- `scripts/multi-project-check.cjs`
- `client-test doctor --project "D:\\claudecode\\cchaha\\Project\\咨询\\delegate-control" --json`
- Connector pytest/unittest：工具注册、路径越权、精确匹配、SHA 冲突和原子写入
- MCP 协议级发现：单项目和多项目都确认固定 11 个裸工具名；项目通过可选 `project_id` 参数路由，不再生成 `<project_id>__` 工具名

Rust 单元测试覆盖配置迁移、目录/端口/稳定 key 校验、Router 注册表与固定 Proxy TOML 生成、停用项目拒绝启动和空闲进程状态。真实 Tunnel 测试继续使用 `#[ignore]`，因为它需要本机安装依赖和 Runtime API Key。

## 桌面自动化状态

当前正式项目尚未接入可运行的 Windows UIA、WebDriver 或 WebView CDP 测试入口。`client-test doctor` 能识别 Tauri 2、Node、npm、Rust 和 cargo-test capability，但报告 embedded WebDriver、Windows UIA 和 multi-instance isolation 未配置。

因此本次不伪造桌面回归通过：完整冷启动、项目 A/B 同时在线、单项目停止保留另一项目、托盘退出清理和最小窗口布局测试状态为 `not configured/blocked`。新增 clienttrail setup 需要单独的隔离环境确认，不应读取真实 Runtime API Key。

本次实现验证时未找到 ClientTrail checkout 或 `client-test` CLI；也未在正式项目执行 setup。CUA 原生桌面控制同样因当前环境的 API Key 认证方式不受支持而未执行。因此桌面窗口交互结论保持 `not configured/blocked`，不把构建和协议级测试冒充为桌面回归通过。

## 手工验收脚本（依赖外部程序）

1. 在 DCFC 中新增两个项目并为它们选择不同目录和 MCP 端口。
2. 勾选两个项目并保存，点击“启动全部项目”。
3. 确认总览显示两个项目在线，并且只有一个 MCP Proxy 与一个 Tunnel PID。
4. 停止项目 A，确认项目 B 仍在线；再停止全部项目，确认所有受管进程退出。
5. 在 ChatGPT 中通过自然语言明确项目；工具调用使用可选 `project_id`，确认文档落入对应目录。

## 已有文件编辑验证

编辑版 Connector 的确定性测试包括：

1. 读取 UTF-8 文件并返回 SHA-256；
2. 追加文本并避免重复换行；
3. 唯一精确替换和删除；
4. 零匹配、多匹配、越权路径、非法 UTF-8 和过大文件拒绝；
5. `expected_sha256` 冲突时原文件不变；
6. 修改前备份写入 `.dcfc-backups`，不进入报告列表。
