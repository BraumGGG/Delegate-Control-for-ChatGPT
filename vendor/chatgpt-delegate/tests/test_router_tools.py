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
            status = functions["connector_status"]("beta")
            unknown = functions["read_text_file"]

            self.assertEqual((beta / "notes.md").read_text(encoding="utf-8"), "beta\n-edited")
            self.assertEqual((alpha / "notes.md").read_text(encoding="utf-8"), "alpha")
            self.assertEqual(status["current_project"], "beta")
            with self.assertRaises(ValueError):
                unknown("notes.md", project_id="missing")

    def test_single_project_side_effect_may_omit_project_id(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            project = root / "only"
            project.mkdir()
            notes = project / "notes.md"
            notes.write_text("start", encoding="utf-8")
            registry = root / "projects.json"
            registry.write_text(
                json.dumps(
                    {
                        "active_project_id": None,
                        "projects": [
                            {
                                "id": "only",
                                "name": "Only Project",
                                "output_directory": str(project),
                                "enabled": True,
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            _, functions = self._create_server_and_tools(registry)

            functions["append_text_file"]("notes.md", "continued")

            self.assertEqual(notes.read_text(encoding="utf-8"), "start\ncontinued")

    def test_multi_project_side_effect_requires_explicit_project_without_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            screencast = root / "screencast"
            dcfc = root / "dcfc"
            screencast.mkdir()
            dcfc.mkdir()
            screencast_notes = screencast / "notes.md"
            dcfc_notes = dcfc / "notes.md"
            screencast_notes.write_text("screen", encoding="utf-8")
            dcfc_notes.write_text("control", encoding="utf-8")
            registry = root / "projects.json"
            registry.write_text(
                json.dumps(
                    {
                        "active_project_id": "dcfc",
                        "projects": [
                            {
                                "id": "screencast",
                                "name": "ScreenCast",
                                "output_directory": str(screencast),
                                "enabled": True,
                            },
                            {
                                "id": "dcfc",
                                "name": "DCFC",
                                "output_directory": str(dcfc),
                                "enabled": True,
                            },
                        ],
                    }
                ),
                encoding="utf-8",
            )
            _, functions = self._create_server_and_tools(registry)

            with self.assertRaisesRegex(
                ValueError,
                r"ScreenCast \(screencast\).*DCFC \(dcfc\)",
            ):
                functions["append_text_file"]("notes.md", "must-not-write")

            self.assertEqual(screencast_notes.read_text(encoding="utf-8"), "screen")
            self.assertEqual(dcfc_notes.read_text(encoding="utf-8"), "control")

    def test_all_multi_project_side_effect_tools_require_explicit_project(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            alpha = root / "alpha"
            beta = root / "beta"
            alpha.mkdir()
            beta.mkdir()
            (alpha / "notes.md").write_text("alpha", encoding="utf-8")
            (beta / "notes.md").write_text("beta", encoding="utf-8")
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
            calls = [
                ("save_task_result", ("task-1", "Title", "# body"), {}),
                ("save_markdown_report", ("Title", "# body"), {"filename": "report.md"}),
                ("append_text_file", ("notes.md", "tail"), {}),
                ("replace_text_in_file", ("notes.md", "alpha", "new"), {}),
                ("delete_text_from_file", ("notes.md", "alpha"), {}),
            ]

            for tool_name, args, kwargs in calls:
                with self.subTest(tool_name=tool_name), self.assertRaisesRegex(
                    ValueError,
                    "explicit project_id",
                ):
                    functions[tool_name](*args, **kwargs)

            self.assertEqual((alpha / "notes.md").read_text(encoding="utf-8"), "alpha")
            self.assertEqual((beta / "notes.md").read_text(encoding="utf-8"), "beta")
            self.assertFalse((alpha / "reports").exists())
            self.assertFalse((alpha / "task-1").exists())

    def test_read_only_tool_keeps_active_project_fallback(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            alpha = root / "alpha"
            beta = root / "beta"
            alpha.mkdir()
            beta.mkdir()
            (alpha / "notes.md").write_text("alpha", encoding="utf-8")
            (beta / "notes.md").write_text("beta", encoding="utf-8")
            registry = root / "projects.json"
            registry.write_text(
                json.dumps(
                    {
                        "active_project_id": "beta",
                        "projects": [
                            {"id": "alpha", "name": "Alpha", "output_directory": str(alpha), "enabled": True},
                            {"id": "beta", "name": "Beta", "output_directory": str(beta), "enabled": True},
                        ],
                    }
                ),
                encoding="utf-8",
            )
            _, functions = self._create_server_and_tools(registry)

            result = functions["read_text_file"]("notes.md")

            self.assertEqual(result["content"], "beta")


if __name__ == "__main__":
    unittest.main()
