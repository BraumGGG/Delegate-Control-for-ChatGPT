from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .connector import (
    ConnectorError,
    list_report_files,
    list_task_results,
    read_report_file,
    read_task_result,
    save_markdown_report_file,
    save_task_result_file,
)
from .text_editing import (
    append_handoff_entry as append_handoff_entry_operation,
    append_text_file as append_text_file_operation,
    delete_text_from_file as delete_text_from_file_operation,
    read_text_file as read_text_file_operation,
    replace_text_in_file as replace_text_in_file_operation,
)


ROUTER_VERSION = "0.2.0-dcfc-router.1"


@dataclass(frozen=True)
class RouterProject:
    project_id: str
    name: str
    output_directory: Path
    enabled: bool = True


def load_router_projects(config_path: Path) -> tuple[str | None, list[RouterProject]]:
    try:
        payload = json.loads(config_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ConnectorError(f"无法读取 Router 项目注册表：{config_path}：{exc}") from exc
    active = payload.get("active_project_id")
    projects: list[RouterProject] = []
    seen: set[str] = set()
    for raw in payload.get("projects", []):
        project_id = str(raw.get("id", "")).strip()
        name = str(raw.get("name", "")).strip()
        output = str(raw.get("output_directory", "")).strip()
        if not project_id or not name or not output:
            raise ConnectorError("Router 项目注册表包含缺少 id、name 或 output_directory 的项目。")
        if project_id in seen:
            raise ConnectorError(f"Router 项目 key 重复：{project_id}")
        raw_path = Path(output).expanduser()
        if not raw_path.is_absolute():
            raise ConnectorError(f"项目“{project_id}”的输出目录必须是绝对路径。")
        path = raw_path.resolve()
        seen.add(project_id)
        projects.append(RouterProject(project_id, name, path, bool(raw.get("enabled", True))))
    if not projects:
        raise ConnectorError("Router 没有可用项目。")
    return (str(active).strip() if active else None), projects


def create_router_mcp_server(config_path: Path, max_bytes: int):
    try:
        from fastmcp import FastMCP
        from mcp.types import ToolAnnotations
    except ImportError as exc:
        raise ConnectorError(
            "FastMCP is required for the Router MCP server. Install with: "
            'python3 -m pip install -e ".[chatgpt]"'
        ) from exc

    instructions = (
        "Use this connector to save, list, read, and safely edit local project text files. "
        "The public tool catalog is fixed. Use project_id when the user names a project; "
        "when project_id is omitted, DCFC's current project is used. Never guess a project "
        "when the request is ambiguous. The connector never calls Codex, OpenAI API, or "
        "ChatGPT private endpoints."
    )
    mcp = FastMCP(name="delegate-control-router", instructions=instructions)
    local_write = ToolAnnotations(readOnlyHint=False, destructiveHint=False, openWorldHint=False)
    read_only = ToolAnnotations(readOnlyHint=True)

    def resolve_project(project_id: str | None) -> RouterProject:
        active, projects = load_router_projects(config_path)
        enabled = [project for project in projects if project.enabled]
        requested = project_id.strip() if project_id else None
        selected_id = requested or active
        if not selected_id and len(enabled) == 1:
            selected_id = enabled[0].project_id
        if not selected_id:
            raise ConnectorError("当前有多个在线项目，请明确提供 project_id。")
        for project in enabled:
            if project.project_id == selected_id:
                project.output_directory.mkdir(parents=True, exist_ok=True)
                return project
        available = ", ".join(project.project_id for project in enabled) or "无"
        raise ConnectorError(f"找不到可用项目“{selected_id}”。可用项目：{available}")

    @mcp.tool(annotations=local_write)
    def save_task_result(
        task_id: str,
        title: str,
        markdown: str,
        summary: str | None = None,
        overwrite: bool = True,
        project_id: str | None = None,
    ) -> dict[str, Any]:
        """Save the final ChatGPT delegation result for a selected project."""
        project = resolve_project(project_id)
        return save_task_result_file(project.output_directory, task_id, title, markdown, summary, overwrite, max_bytes)

    @mcp.tool(annotations=read_only)
    def list_results(limit: int = 20, project_id: str | None = None) -> dict[str, Any]:
        """List completed ChatGPT delegation task results for a selected project."""
        return list_task_results(resolve_project(project_id).output_directory, limit=limit)

    @mcp.tool(annotations=read_only)
    def read_result(task_id: str, project_id: str | None = None) -> dict[str, Any]:
        """Read a completed ChatGPT delegation task result for a selected project."""
        return read_task_result(resolve_project(project_id).output_directory, task_id, include_markdown=True)

    @mcp.tool(annotations=local_write)
    def save_markdown_report(
        title: str,
        markdown: str,
        filename: str | None = None,
        overwrite: bool = False,
        project_id: str | None = None,
    ) -> dict[str, Any]:
        """Save a Markdown report to a selected project."""
        saved = save_markdown_report_file(resolve_project(project_id).output_directory, title, markdown, filename, overwrite, max_bytes)
        return {"status": "ok", **saved.__dict__}

    @mcp.tool(annotations=read_only)
    def list_reports(limit: int = 20, project_id: str | None = None) -> dict[str, Any]:
        """List Markdown reports for a selected project."""
        return list_report_files(resolve_project(project_id).output_directory, limit=limit)

    @mcp.tool(annotations=read_only)
    def read_report(filename: str, project_id: str | None = None) -> dict[str, Any]:
        """Read a Markdown report from a selected project."""
        return read_report_file(resolve_project(project_id).output_directory, filename)

    @mcp.tool(annotations=read_only)
    def connector_status(project_id: str | None = None) -> dict[str, Any]:
        """Return fixed Router status and the selected project directory."""
        active, projects = load_router_projects(config_path)
        selected = resolve_project(project_id)
        return {
            "status": "ready",
            "router": True,
            "router_version": ROUTER_VERSION,
            "current_project": selected.project_id,
            "output_dir": str(selected.output_directory),
            "projects": [{"project_id": item.project_id, "name": item.name, "enabled": item.enabled} for item in projects],
            "active_project_id": active,
            "text_editing": True,
            "text_editing_tools": [
                "read_text_file",
                "append_text_file",
                "append_handoff_entry",
                "replace_text_in_file",
                "delete_text_from_file",
            ],
            "codex_execution": False,
            "openai_api": False,
            "private_chatgpt_api": False,
        }

    @mcp.tool(annotations=read_only)
    def read_text_file(filename: str, project_id: str | None = None) -> dict[str, Any]:
        """Read a UTF-8 text file from a selected project."""
        return read_text_file_operation(resolve_project(project_id).output_directory, filename, max_bytes=max_bytes)

    @mcp.tool(annotations=local_write)
    def append_text_file(
        filename: str,
        content: str,
        expected_sha256: str | None = None,
        project_id: str | None = None,
    ) -> dict[str, Any]:
        """Append text to a selected project file."""
        return append_text_file_operation(resolve_project(project_id).output_directory, filename, content, expected_sha256, max_bytes)

    @mcp.tool(annotations=local_write)
    def append_handoff_entry(
        entry_id: str,
        author: str,
        entry_type: str,
        status: str,
        priority: str,
        title: str,
        related_entries: list[str],
        summary: str,
        acceptance: str,
        next_action: str,
        expected_sha256: str,
        project_id: str | None = None,
    ) -> dict[str, Any]:
        """Append one validated entry to a selected project's handoff document."""
        return append_handoff_entry_operation(
            resolve_project(project_id).output_directory,
            entry_id,
            author,
            entry_type,
            status,
            priority,
            title,
            related_entries,
            summary,
            acceptance,
            next_action,
            expected_sha256,
            max_bytes,
        )

    @mcp.tool(annotations=local_write)
    def replace_text_in_file(
        filename: str,
        old_text: str,
        new_text: str,
        expected_occurrences: int = 1,
        expected_sha256: str | None = None,
        project_id: str | None = None,
    ) -> dict[str, Any]:
        """Replace an exact text fragment in a selected project file."""
        return replace_text_in_file_operation(resolve_project(project_id).output_directory, filename, old_text, new_text, expected_occurrences, expected_sha256, max_bytes)

    @mcp.tool(annotations=local_write)
    def delete_text_from_file(
        filename: str,
        text: str,
        expected_occurrences: int = 1,
        expected_sha256: str | None = None,
        project_id: str | None = None,
    ) -> dict[str, Any]:
        """Delete an exact text fragment from a selected project file."""
        return delete_text_from_file_operation(resolve_project(project_id).output_directory, filename, text, expected_occurrences, expected_sha256, max_bytes)

    return mcp
