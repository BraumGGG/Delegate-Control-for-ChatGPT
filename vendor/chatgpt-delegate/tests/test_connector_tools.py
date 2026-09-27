import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

import chatgpt_delegate.connector as connector


class ConnectorToolsTestCase(unittest.TestCase):
    def _create_server_and_tools(self, root: Path):
        fake_mcp = Mock()
        functions = {}

        def register(*args, **kwargs):
            def decorator(function):
                functions[function.__name__] = function
                return function

            return decorator

        fake_mcp.tool.side_effect = register
        fake_fastmcp = Mock(return_value=fake_mcp)
        fake_annotations = Mock(side_effect=lambda **kwargs: kwargs)
        with patch.dict(
            "sys.modules",
            {
                "fastmcp": SimpleNamespace(FastMCP=fake_fastmcp),
                "mcp.types": SimpleNamespace(ToolAnnotations=fake_annotations),
            },
        ):
            server = connector.create_mcp_server(root)
        return server, functions

    def test_registers_legacy_and_text_editing_tools(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            _, functions = self._create_server_and_tools(Path(temp_dir))

        expected = {
            "save_task_result",
            "list_results",
            "read_result",
            "save_markdown_report",
            "list_reports",
            "read_report",
            "connector_status",
            "read_text_file",
            "append_text_file",
            "replace_text_in_file",
            "delete_text_from_file",
        }
        self.assertTrue(expected.issubset(functions))

    def test_text_tools_edit_existing_file_and_status_advertises_capability(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            path = root / "notes.md"
            path.write_text("第一段\n第二段", encoding="utf-8", newline="")
            _, functions = self._create_server_and_tools(root)

            read = functions["read_text_file"]("notes.md")
            appended = functions["append_text_file"]("notes.md", "第三段", read["sha256"])
            replaced = functions["replace_text_in_file"]("notes.md", "第二段", "已修改")
            deleted = functions["delete_text_from_file"]("notes.md", "第一段")
            status = functions["connector_status"]()

            content = path.read_text(encoding="utf-8")

        self.assertEqual(content, "\n已修改\n第三段")
        self.assertEqual(appended["operation"], "append")
        self.assertEqual(replaced["replaced_occurrences"], 1)
        self.assertEqual(deleted["deleted_occurrences"], 1)
        self.assertTrue(status["text_editing"])
        self.assertIn("replace_text_in_file", status["text_editing_tools"])

    def test_capabilities_command_is_machine_readable(self) -> None:
        with patch("sys.stdout.write") as write:
            result = connector.main(["capabilities", "--json"])

        payload = json.loads("".join(call.args[0] for call in write.call_args_list))
        self.assertEqual(result, 0)
        self.assertEqual(payload["status"], "ok")
        self.assertEqual(payload["connector_version"], "0.2.0-dcfc-router.1")
        self.assertTrue(payload["fixed_router"])
        self.assertTrue(payload["text_editing"])
        self.assertIn("append_text_file", payload["text_editing_tools"])


if __name__ == "__main__":
    unittest.main()
