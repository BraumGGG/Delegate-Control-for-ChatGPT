# DCFC ChatGPT Delegate Connector

这是 DCFC 使用的 `chatgpt-delegate` 兼容源码副本，基于上游仓库
`https://github.com/felixlark/chatgpt-delegate` 的固定 commit：

```text
43f05ecab412fc40edd9ce9e00e4681bbd9139e7
```

DCFC 增量增加了 4 个受项目 `--output-dir` 约束的 UTF-8 文本工具：

- `read_text_file`
- `append_text_file`
- `replace_text_in_file`
- `delete_text_from_file`

旧报告和任务结果工具保持兼容。文本编辑只接受项目目录内的相对路径，替换和删除默认要求唯一精确匹配，并支持 `expected_sha256` 防止覆盖并发修改。

此外提供专用的 `append_handoff_entry` 工具。它只追加固定文件
`PRODUCT_DESIGNER_DEVELOPER_HANDOFF.md`，要求传入 `PDH-YYYYMMDD-NNN` Entry ID、
合法的作者/类型/状态/优先级、结构化正文和当前文件 `expected_sha256`。工具在项目目录
`.dcfc-backups` 下加锁，拒绝重复 Entry ID 或旧 SHA，并返回新 SHA、备份路径和 Entry ID。
固定 Router 版本通过 `project_id` 将该工具路由到指定项目；上述通用文本工具仍然保留。

使用 `scripts/build-chatgpt-delegate.ps1` 构建 Windows 独立可执行文件。生成的 exe 可以单独回退到旧版，不需要修改 Tunnel ID。
