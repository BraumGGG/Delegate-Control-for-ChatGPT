# 新会话交接指令

将下面整段内容原样发送给新的 Codex 会话：

```text
请接管 Windows 项目 Delegate Control for ChatGPT（DCFC）。

项目路径：
D:\claudecode\cchaha\Project\咨询\delegate-control

GitHub：
https://github.com/BraumGGG/Delegate-Control-for-ChatGPT

你的第一步不是立即改代码。请先完整阅读：
1. D:\claudecode\cchaha\Project\咨询\delegate-control\HANDOFF.md
2. D:\claudecode\cchaha\Project\咨询\delegate-control\README.md
3. HANDOFF.md 中“源码导航”列出的关键文件

然后执行并报告：
- git status --short
- git branch --show-current
- git remote -v
- git log -5 --oneline --decorate
- git show --no-patch --decorate baseline-2026-09-22
- npm run build
- cargo test --manifest-path src-tauri/Cargo.toml

重要规则：
- 交接文档是导航，不是绝对真相；以当前 Git 和代码为准，发现不一致要明确指出。
- 不要覆盖、删除或回退任何你不确定来源的用户改动。
- 不要使用 git reset --hard、git checkout -- . 等破坏性命令。
- 不要读取、输出、记录或提交 Runtime API Key。
- 不要提交 settings.json、日志、测试 evidence、artifacts、dist、target 或本机密钥文件。
- 用户不希望反复人工测试。桌面端验证请使用 clienttrail-desktop-testing skill，通过结构化自动化和确定性检查完成。
- 所有沟通使用中文。

当前已完成的产品：
- Tauri 2 + React 19 + TypeScript + Rust 的 Windows 桌面客户端。
- 手动按需启动和停止 chatgpt-delegate MCP 与 tunnel-client。
- 总览、日志、设置、系统托盘、单实例和 Windows Job Object 子进程清理。
- Runtime API Key 使用 Windows Credential Manager。
- 产品名 Delegate Control for ChatGPT，桌面快捷方式名 DCFC。
- Git 回退基线标签 baseline-2026-09-22。

当前尚未实现、也是接下来要完成的核心需求：
- 现在 output_directory 是全局固定值，多个项目会把文档写到同一目录。
- 用户要求每个项目能独立选择任意文档输出目录，项目间不能混写。
- 用户不接受只能在客户端预设一个全局目录。
- ChatGPT 应能向当前项目目录写文档，Codex 也能在该项目目录读取和继续编辑。
- 其他连接、凭据、托盘和进程管理行为不要改。

在实现前，先核实上游 chatgpt-delegate 的 --output-dir 是否只能在进程启动时设置、工具调用能否逐次指定目录、单个 Tunnel/Connector 能否支持多实例。不要把“能快速切换当前项目”和“三个项目同时在线”混为一谈。如果技术约束或用户目标仍有关键歧义，只问一个最必要的问题；其余应通过代码和上游实现自行核实。

完成方案确认后再实施。修改必须包含：配置迁移/兼容、路径校验、清晰的当前项目/目录显示、必要的目录选择 UI、单元测试、构建验证，以及桌面端自动化测试。运行中的目录切换如果必须重启 MCP，需要在 UI 中明确体现，不能静默假装生效。

最终交付时请说明：
1. 修改了哪些文件和行为；
2. 如何选择和切换项目目录；
3. 是否支持多项目同时在线；
4. 执行了哪些测试及结果；
5. 如何回退到 baseline-2026-09-22。
```

