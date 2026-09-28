import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import chatgpt_delegate.connector as connector


class ChatGPTResearchConnectorTestCase(unittest.TestCase):
    def test_slugify_keeps_stable_digest_when_topic_has_no_ascii(self) -> None:
        slug = connector.slugify_topic("100M上下文大模型")
        self.assertTrue(slug.startswith("100m-"))
        self.assertRegex(slug, r"-[0-9a-f]{8}$")

    def test_save_report_sanitizes_filename_and_writes_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            saved = connector.save_markdown_report_file(
                output_dir=Path(temp_dir),
                title="100M上下文大模型",
                markdown="# 标题\n\n正文",
                filename="100M 上下文.md",
            )
            path = Path(saved.path)
            content = path.read_text(encoding="utf-8")

        self.assertEqual(saved.filename, "100M-上下文.md")
        self.assertIn("generated_by: chatgpt-delegate MCP save_markdown_report", content)
        self.assertIn("auth_path: ChatGPT web/app connector; no Codex execution", content)
        self.assertIn("# 标题", content)

    def test_save_report_rejects_path_escape(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            with self.assertRaises(connector.ConnectorError):
                connector.save_markdown_report_file(
                    output_dir=Path(temp_dir),
                    title="bad",
                    markdown="# bad",
                    filename="../bad.md",
                )

    def test_save_report_allocates_unique_name_without_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            first = connector.save_markdown_report_file(root, "Title", "# one", filename="same.md")
            second = connector.save_markdown_report_file(root, "Title", "# two", filename="same.md")

        self.assertEqual(first.filename, "same.md")
        self.assertEqual(second.filename, "same-2.md")

    def test_prompt_command_writes_chatgpt_web_handoff_prompt(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            output = Path(temp_dir) / "handoff.md"
            rc = connector.main(["prompt", "100M上下文大模型", "--output", str(output)])
            content = output.read_text(encoding="utf-8")

        self.assertEqual(rc, 0)
        self.assertIn("当前 ChatGPT Web/App", content)
        self.assertIn("save_markdown_report", content)
        self.assertIn("不要让 Codex", content)

    def test_delegate_profile_routing_from_chinese_request(self) -> None:
        self.assertEqual(connector.infer_task_profile("调研一下长上下文社区最佳实践"), "research_pro")
        self.assertEqual(connector.infer_task_profile("设计一个 AI PRD 方案"), "thinking_high")
        self.assertEqual(connector.infer_task_profile("查一下今天最新进展"), "instant_web")
        self.assertEqual(connector.infer_task_profile("随便帮我想想", kind="think"), "thinking_high")

    def test_prepare_command_writes_task_prompt_and_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            rc = connector.main(
                [
                    "--output-dir",
                    str(root),
                    "prepare",
                    "调研一下 MCP 社区最佳实践",
                    "--kind",
                    "auto",
                ]
            )
            tasks = list(root.iterdir())
            task_dir = tasks[0]
            task = json.loads((task_dir / "task.json").read_text(encoding="utf-8"))
            prompt = (task_dir / "prompt.md").read_text(encoding="utf-8")

        self.assertEqual(rc, 0)
        self.assertEqual(len(tasks), 1)
        self.assertEqual(task["profile"], "research_pro")
        self.assertEqual(task["request"], "调研一下 MCP 社区最佳实践")
        self.assertEqual(task["target_filename"], "result.md")
        self.assertIn(task["task_id"], prompt)
        self.assertIn("save_task_result", prompt)
        self.assertIn("thinking pro", prompt)
        self.assertIn("不要让 Codex", prompt)

    def test_prepare_is_idempotent_for_same_request(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            first = connector.prepare_delegation_task(root, "调研一下 MCP 社区最佳实践")
            second = connector.prepare_delegation_task(root, "调研一下 MCP 社区最佳实践")

        self.assertEqual(first["task_id"], second["task_id"])
        self.assertEqual(first["created_at"], second["created_at"])
        self.assertEqual(first["prompt_path"], second["prompt_path"])

    def test_save_task_result_writes_result_markdown_and_json(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            prepared = connector.prepare_delegation_task(root, "设计一个 PRD")
            saved = connector.save_task_result_file(
                output_dir=root,
                task_id=prepared["task_id"],
                title="PRD",
                markdown="# PRD\n\n正文",
                summary="PRD 摘要",
            )
            result_md = Path(saved["path"])
            result_json = result_md.with_name("result.json")
            content = result_md.read_text(encoding="utf-8")
            metadata = json.loads(result_json.read_text(encoding="utf-8"))

        self.assertEqual(saved["status"], "ok")
        self.assertIn("generated_by: chatgpt-delegate MCP save_task_result", content)
        self.assertIn("task_id:", content)
        self.assertIn("# PRD", content)
        self.assertEqual(metadata["summary"], "PRD 摘要")
        self.assertFalse(metadata["codex_execution"])
        self.assertFalse(metadata["openai_api"])

    def test_save_task_result_rejects_escape_empty_large_and_no_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            prepared = connector.prepare_delegation_task(root, "设计一个 PRD")
            task_id = prepared["task_id"]
            connector.save_task_result_file(root, task_id, "PRD", "# one")

            with self.assertRaises(connector.ConnectorError):
                connector.save_task_result_file(root, "../bad", "Bad", "# bad")
            with self.assertRaises(connector.ConnectorError):
                connector.save_task_result_file(root, task_id, "PRD", "   ")
            with self.assertRaises(connector.ConnectorError):
                connector.save_task_result_file(root, task_id, "PRD", "abcdef", max_bytes=5)
            with self.assertRaises(connector.ConnectorError):
                connector.save_task_result_file(root, task_id, "PRD", "# two", overwrite=False)

    def test_delegate_status_list_and_read_commands(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            prepared = connector.prepare_delegation_task(root, "查一下 MCP 最新进展")
            task_id = prepared["task_id"]
            pending = connector.get_task_status(root, task_id)
            connector.save_task_result_file(root, task_id, "Lookup", "# Lookup")
            completed = connector.get_task_status(root, task_id)
            listed = connector.list_task_results(root)
            read = connector.read_task_result(root, task_id, include_markdown=True)

            status_rc = connector.main(["--output-dir", str(root), "status", task_id])
            list_rc = connector.main(["--output-dir", str(root), "list"])
            read_rc = connector.main(["--output-dir", str(root), "read", task_id, "--metadata-only"])

        self.assertEqual(pending["status"], "pending")
        self.assertEqual(completed["status"], "completed")
        self.assertEqual(listed["results"][0]["task_id"], task_id)
        self.assertIn("# Lookup", read["markdown"])
        self.assertEqual(status_rc, 0)
        self.assertEqual(list_rc, 0)
        self.assertEqual(read_rc, 0)

    def test_import_command_saves_chatgpt_markdown_file(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            source = root / "source.md"
            source.write_text("# Report\n\nBody", encoding="utf-8")
            rc = connector.main(
                [
                    "--output-dir",
                    str(root / "reports"),
                    "import",
                    "--title",
                    "Imported",
                    "--input",
                    str(source),
                    "--filename",
                    "imported.md",
                ]
            )
            report = root / "reports" / "imported.md"
            exists = report.exists()
            content = report.read_text(encoding="utf-8")

        self.assertEqual(rc, 0)
        self.assertTrue(exists)
        self.assertIn("# Report", content)

    def test_list_and_read_reports(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            connector.save_markdown_report_file(root, "Title", "# one", filename="one.md")
            listed = connector.list_report_files(root)
            read = connector.read_report_file(root, "one.md")

        self.assertEqual(listed["reports"][0]["filename"], "one.md")
        self.assertEqual(read["filename"], "one.md")
        self.assertIn("# one", read["markdown"])

    def test_allowed_hosts_include_explicit_public_host(self) -> None:
        allowed_hosts = connector.build_allowed_hosts(
            ["https://impregnable-ambagiously-kenley.ngrok-free.dev"]
        )

        self.assertIn("127.0.0.1:*", allowed_hosts)
        self.assertIn("impregnable-ambagiously-kenley.ngrok-free.dev", allowed_hosts)
        self.assertIn("impregnable-ambagiously-kenley.ngrok-free.dev:*", allowed_hosts)

    def test_create_mcp_server_registers_no_codex_tools(self) -> None:
        fake_mcp = Mock()
        fake_mcp.tool.side_effect = lambda *args, **kwargs: (lambda func: func)
        fake_fastmcp = Mock(return_value=fake_mcp)
        fake_annotations = Mock(side_effect=lambda **kwargs: kwargs)

        with (
            patch.dict(
                "sys.modules",
                {
                    "fastmcp": Mock(FastMCP=fake_fastmcp),
                    "mcp.types": Mock(ToolAnnotations=fake_annotations),
                },
            ),
            tempfile.TemporaryDirectory() as temp_dir,
        ):
            server = connector.create_mcp_server(Path(temp_dir))

        self.assertIs(server, fake_mcp)
        instructions = fake_fastmcp.call_args.kwargs["instructions"]
        self.assertIn("never calls Codex", instructions)
        self.assertIn("save_task_result", instructions)
        self.assertGreaterEqual(fake_mcp.tool.call_count, 7)
        tool_kwargs = [call.kwargs for call in fake_mcp.tool.call_args_list]
        self.assertIn(
            {
                "annotations": {
                    "readOnlyHint": False,
                    "destructiveHint": False,
                    "idempotentHint": True,
                    "openWorldHint": False,
                }
            },
            tool_kwargs,
        )
        self.assertIn(
            {
                "annotations": {
                    "readOnlyHint": False,
                    "destructiveHint": False,
                    "idempotentHint": False,
                    "openWorldHint": False,
                }
            },
            tool_kwargs,
        )

    def test_serve_command_handles_keyboard_interrupt_cleanly(self) -> None:
        fake_server = Mock()
        fake_server.run.side_effect = KeyboardInterrupt

        with patch("chatgpt_delegate.connector.create_mcp_server", return_value=fake_server):
            rc = connector.main(["serve", "--port", "8765"])

        self.assertEqual(rc, 130)

    def test_missing_import_file_returns_structured_error(self) -> None:
        with patch("sys.stderr.write") as write:
            rc = connector.main(["import", "--title", "Missing", "--input", "/no/such/file"])

        rendered = "".join(call.args[0] for call in write.call_args_list)
        payload = json.loads(rendered)
        self.assertEqual(rc, 1)
        self.assertEqual(payload["status"], "error")
        self.assertIn("No such file", payload["detail"])
