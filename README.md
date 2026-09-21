# Delegate Control for ChatGPT

Delegate Control for ChatGPT（DCFC）是一个 Windows 桌面控制客户端，用于管理本机的 ChatGPT Delegate MCP 服务和安全隧道。

> 本项目是第三方工具，并非 OpenAI 或 ChatGPT 官方产品。

## 功能

- 按需启动和停止本地 MCP 与 Tunnel 进程
- 显示 Clash、MCP 和 Tunnel 的运行状态与进程信息
- 防止重复启动同一组受管进程
- 退出时清理完整子进程树和监听端口
- 将 Runtime API Key 保存在 Windows Credential Manager
- 关闭主窗口后隐藏到系统托盘
- 支持单实例运行

## 技术栈

- Tauri 2
- React 19
- TypeScript
- Rust
- Vite

## 环境要求

- Windows 10/11 x64
- Node.js 与 npm
- Rust stable 工具链
- 已安装并配置 `chatgpt-delegate` 与 `tunnel-client`
- 可用的本地 Clash HTTP 代理（默认 `127.0.0.1:7897`）

## 本地开发

```powershell
npm install
npm run tauri -- dev
```

## 构建安装包

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri -- build
```

构建完成后，可在以下目录找到安装包：

```text
src-tauri/target/release/bundle/nsis/
src-tauri/target/release/bundle/msi/
```

## 安全说明

- Runtime API Key 不写入项目配置文件，也不应提交到 Git。
- 凭据通过当前 Windows 用户的 Credential Manager 保存。
- Release 构建默认不启用桌面测试插件或测试端口。
- 提交前请确认 `.env`、日志、测试 evidence 和本机设置文件未被纳入版本控制。

## 上游组件

本客户端是面向 `chatgpt-delegate` 的独立控制界面，不是该上游仓库本身。项目内部仍保留 `delegate-control.exe` 作为可执行文件名，以维持升级和配置兼容性。
