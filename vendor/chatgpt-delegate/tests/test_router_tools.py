import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

import chatgpt_delegate.router as router


class RouterToolsTestCase(unittest.TestCase):
    def _create_server_and_tools(self, registry: Path):
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
            server = router.create_router_mcp_server(registry, max_bytes=1024 * 1024)
        return server, functions

    def test_router_registers_fixed_catalog_without_namespaces(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            registry = root / "projects.json"
            registry.write_text(
                json.dumps(
                    {
                        "active_project_id": "alpha",
                        "projects": [
                            {"id": "alpha", "name": "Alpha", "output_directory": str(root / "alpha"), "enabled": True},
                            {"id": "beta", "name": "Beta", "output_directory": str(root / "beta"), "enabled": True},
                        ],
                    }
                ),
                encoding="utf-8",
            )
            _, functions = self._create_server_and_tools(registry)

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
            "append_handoff_entry",
            "replace_text_in_file",
            "delete_text_from_file",
        }
        self.assertEqual(set(functions), expected)
        self.assertFalse(any("__" in name for name in functions))

    def test_router_routes_text_edits_to_explicit_project(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            alpha = root / "alpha"
            beta = root / "beta"
            alpha.mkdir()
            beta.mkdir()
            (alpha / "notes.md").write_text("alpha", encoding="utf-8")
            (beta / "notes.md").write_text("beta", encoding="utf-8")
            (alpha / "PRODUCT_DESIGNER_DEVELOPER_HANDOFF.md").write_text("# Alpha\n", encoding="utf-8")
            (beta / "PRODUCT_DESIGNER_DEVELOPER_HANDOFF.md").write_text("# Beta\n", encoding="utf-8")
            registry = root / "projects.json"
            registry.write_text(
                json.dumps(
                    {
                        "active_project_id": "alpha",
                        "projects": [
                            {"id": "alpha", "name": "Alpha", "output_directory": str(alpha), "enabled": True},
                            {"id": "beta", "name": "Beta", "output_directory": str(beta), "enabled": True},
                        ],
                    }
                ),
                encoding="utf-8",
            )
            _, functions = self._create_server_and_tools(registry)
            functions["append_text_file"]("notes.md", "-edited", project_id="beta")
            before = functions["read_text_file"]("PRODUCT_DESIGNER_DEVELOPER_HANDOFF.md", project_id="beta")
            functions["append_handoff_entry"](
                "PDH-20260928-017",
                "DEV",
                "REVIEW",
                "DONE",
                "P1",
                "Router structured append",
                [],
                "Router resolves the explicit project before appending.",
                "Only the beta handoff is changed.",
                "Request review.",
                before["sha256"],
                project_id="beta",
            )
            status = functions["connector_status"]("beta")
            unknown = functions["read_text_file"]

            self.assertEqual((beta / "notes.md").read_text(encoding="utf-8"), "beta\n-edited")
            self.assertEqual((alpha / "notes.md").read_text(encoding="utf-8"), "alpha")
            self.assertIn(
                "PDH-20260928-017",
                (beta / "PRODUCT_DESIGNER_DEVELOPER_HANDOFF.md").read_text(encoding="utf-8"),
            )
            self.assertNotIn(
                "PDH-20260928-017",
                (alpha / "PRODUCT_DESIGNER_DEVELOPER_HANDOFF.md").read_text(encoding="utf-8"),
            )
            self.assertEqual(status["current_project"], "beta")
            with self.assertRaises(ValueError):
                unknown("notes.md", project_id="missing")


if __name__ == "__main__":
    unittest.main()
