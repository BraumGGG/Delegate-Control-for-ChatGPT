# Delegate Control for ChatGPT 命名设计

## 背景与目标

当前客户端显示名为 `Delegate Control`，与上游 GitHub 项目 `chatgpt-delegate` 的关系不够清楚。产品正式显示名调整为 `Delegate Control for ChatGPT`，使用户能直接理解它是配合 ChatGPT Delegate 使用的 Windows 控制端。

## 命名规则

- 正式产品名：`Delegate Control for ChatGPT`
- 桌面快捷方式名：`DCFC`
- 窗口标题、托盘提示、安装程序产品名及卸载列表名称统一使用正式产品名。
- 可执行文件及内部包名保持 `delegate-control`，避免无必要地破坏安装、配置和进程管理兼容性。
- 应用标识符保持 `com.delegate.control`，避免升级安装被识别为另一款应用。

## 实施范围

- 修改 Tauri 产品信息、窗口标题、发布者和安装包描述。
- 修改系统托盘提示文本。
- 更新桌面快捷方式，使其名称为 `DCFC` 且指向新构建安装的程序。
- 重新执行前端构建、Rust 检查和安装包构建。

## 不变范围

- 不修改 MCP、Tunnel、Runtime API Key 或进程生命周期逻辑。
- 不修改 Windows Credential Manager 中的凭据。
- 不修改应用数据目录和设置文件格式。

## 验收标准

- 窗口和托盘显示 `Delegate Control for ChatGPT`。
- 安装程序及 Windows 卸载列表显示正式产品名。
- 桌面快捷方式显示为 `DCFC`。
- Release 构建成功，现有 Rust 测试通过。
