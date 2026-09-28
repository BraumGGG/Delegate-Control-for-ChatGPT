from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

from .text_editing import (
    append_handoff_entry as append_handoff_entry_operation,
    append_text_file as append_text_file_operation,
    delete_text_from_file as delete_text_from_file_operation,
    read_text_file as read_text_file_operation,
    replace_text_in_file as replace_text_in_file_operation,
)


DEFAULT_OUTPUT_DIR = Path("artifacts/chatgpt-delegation")
DEFAULT_MAX_BYTES = 5 * 1024 * 1024
DEFAULT_ALLOWED_ORIGINS = [
    "http://127.0.0.1:*",
    "http://localhost:*",
    "http://[::1]:*",
    "https://chatgpt.com",
    "https://chat.openai.com",
]
PROFILE_ALIASES = {
    "auto": "auto",
    "research": "research_pro",
    "research_pro": "research_pro",
    "think": "thinking_high",
    "thinking_high": "thinking_high",
    "lookup": "instant_web",
    "instant": "instant_web",
    "instant_web": "instant_web",
}
PROFILE_TRIGGERS = {
    "research_pro": ("调研", "深度研究", "社区最佳实践", "研究一下"),
    "thinking_high": ("设计", "构思", "思考", "比较", "撰写", "方案", "PRD", "prd"),
    "instant_web": ("查一下", "找一下", "确认", "最新"),
}
PROFILE_INSTRUCTIONS = {
    "research_pro": (
        "请使用 thinking pro 风格处理：进行深入、系统的联网调研，优先核对社区最佳实践、"
        "论文、官方文档、GitHub 项目和高质量讨论；输出长 Markdown 报告，包含来源链接、"
        "日期、结论、对比表、风险和后续问题。"
    ),
    "thinking_high": (
        "请使用 thinking high 风格处理：充分构思、比较方案、权衡取舍；按需联网核对事实；"
        "输出可执行的 Markdown 方案、PRD、比较稿或写作稿。"
    ),
    "instant_web": (
        "请使用 instant 联网风格处理：快速联网查询，输出短结论、关键证据和来源链接；"
        "只展开必要背景。"
    ),
}


class ConnectorError(ValueError):
    pass


@dataclass(frozen=True)
class SavedReport:
    title: str
    filename: str
    path: str
    bytes: int
    created_at: str


@dataclass(frozen=True)
class DelegationTask:
    task_id: str
    request: str
    profile: str
    kind: str
    target_filename: str
    created_at: str
    task_dir: str
    prompt_path: str
    result_path: str


def utc_timestamp() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def slugify_topic(topic: str) -> str:
    slug = re.sub(r"[^A-Za-z0-9]+", "-", topic.strip().lower()).strip("-")
    digest = hashlib.sha1(topic.encode("utf-8")).hexdigest()[:8]
    if not slug:
        slug = "report"
    return f"{slug[:48].strip('-')}-{digest}"


def digest_text(value: str, length: int = 12) -> str:
    return hashlib.sha1(value.encode("utf-8")).hexdigest()[:length]


def default_report_filename(title: str) -> str:
    day = datetime.now().strftime("%Y%m%d")
    return f"{day}-{slugify_topic(title)}.md"


def normalize_report_filename(filename: str | None, title: str) -> str:
    candidate = filename.strip() if filename else default_report_filename(title)
    candidate = candidate.replace("\\", "/")
    if "/" in candidate:
        raise ConnectorError("filename must not contain path separators")
    if candidate in {"", ".", ".."}:
        raise ConnectorError("filename is empty or invalid")
    if not candidate.endswith(".md"):
        candidate = f"{candidate}.md"
    safe = re.sub(r"[^\w._-]+", "-", candidate, flags=re.UNICODE).strip(".-")
    if not safe:
        safe = default_report_filename(title)
    if not safe.endswith(".md"):
        safe = f"{safe}.md"
    return safe


def resolve_report_path(output_dir: Path, filename: str) -> Path:
    root = output_dir.resolve()
    path = (root / filename).resolve()
    if path.parent != root:
        raise ConnectorError("resolved path escaped the output directory")
    return path


def unique_report_path(path: Path) -> Path:
    if not path.exists():
        return path
    stem = path.stem
    suffix = path.suffix
    for index in range(2, 10_000):
        candidate = path.with_name(f"{stem}-{index}{suffix}")
        if not candidate.exists():
            return candidate
    raise ConnectorError("could not allocate a unique report filename")


def save_markdown_report_file(
    output_dir: Path,
    title: str,
    markdown: str,
    filename: str | None = None,
    overwrite: bool = False,
    max_bytes: int = DEFAULT_MAX_BYTES,
) -> SavedReport:
    if not title.strip():
        raise ConnectorError("title is required")
    if not markdown.strip():
        raise ConnectorError("markdown is required")

    encoded = markdown.encode("utf-8")
    if len(encoded) > max_bytes:
        raise ConnectorError(f"markdown is too large: {len(encoded)} bytes > {max_bytes} bytes")

    output_dir.mkdir(parents=True, exist_ok=True)
    normalized = normalize_report_filename(filename, title)
    path = resolve_report_path(output_dir, normalized)
    if path.exists() and not overwrite:
        path = unique_report_path(path)

    created_at = utc_timestamp()
    body = markdown.strip() + "\n"
    header = (
        "<!--\n"
        "generated_by: chatgpt-delegate MCP save_markdown_report\n"
        f"created_at: {created_at}\n"
        f"title: {title.strip()}\n"
        "auth_path: ChatGPT web/app connector; no Codex execution\n"
        "-->\n\n"
    )
    path.write_text(header + body, encoding="utf-8")
    return SavedReport(
        title=title.strip(),
        filename=path.name,
        path=str(path),
        bytes=path.stat().st_size,
        created_at=created_at,
    )


def list_report_files(output_dir: Path, limit: int = 20) -> dict[str, Any]:
    if limit < 1:
        raise ConnectorError("limit must be >= 1")
    if not output_dir.exists():
        return {"reports": []}
    files = sorted(output_dir.glob("*.md"), key=lambda path: path.stat().st_mtime, reverse=True)
    return {
        "reports": [
            {
                "filename": path.name,
                "path": str(path.resolve()),
                "bytes": path.stat().st_size,
                "modified_at": datetime.fromtimestamp(path.stat().st_mtime, timezone.utc)
                .replace(microsecond=0)
                .isoformat()
                .replace("+00:00", "Z"),
            }
            for path in files[:limit]
        ]
    }


def read_report_file(output_dir: Path, filename: str) -> dict[str, Any]:
    path = resolve_report_path(output_dir, normalize_report_filename(filename, filename))
    if not path.exists():
        raise ConnectorError(f"report not found: {path.name}")
    return {
        "filename": path.name,
        "path": str(path),
        "markdown": path.read_text(encoding="utf-8"),
    }


def normalize_task_id(task_id: str) -> str:
    value = task_id.strip()
    if not value:
        raise ConnectorError("task_id is required")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+", value):
        raise ConnectorError("task_id may only contain letters, numbers, dot, dash, and underscore")
    if value in {".", ".."}:
        raise ConnectorError("task_id is invalid")
    return value


def resolve_task_dir(output_dir: Path, task_id: str) -> Path:
    root = output_dir.resolve()
    path = (root / normalize_task_id(task_id)).resolve()
    if path.parent != root:
        raise ConnectorError("resolved task path escaped the output directory")
    return path


def normalize_kind(kind: str = "auto") -> str:
    normalized = kind.strip().lower().replace("-", "_") if kind else "auto"
    if normalized not in PROFILE_ALIASES:
        allowed = ", ".join(sorted(PROFILE_ALIASES))
        raise ConnectorError(f"invalid kind: {kind}; expected one of {allowed}")
    return normalized


def infer_task_profile(request: str, kind: str = "auto") -> str:
    if not request.strip():
        raise ConnectorError("request is required")
    normalized_kind = normalize_kind(kind)
    explicit = PROFILE_ALIASES[normalized_kind]
    if explicit != "auto":
        return explicit

    for profile in ("research_pro", "thinking_high", "instant_web"):
        if any(trigger in request for trigger in PROFILE_TRIGGERS[profile]):
            return profile
    return "thinking_high"


def default_task_id(request: str, profile: str) -> str:
    return f"{profile}-{digest_text(profile + ':' + request)}"


def build_delegate_prompt(task: dict[str, Any]) -> str:
    profile = task["profile"]
    return f"""请使用当前 ChatGPT Web/App 账号能力完成下面任务，并在完成后调用已连接的 ChatGPT Delegate Connector 的 `save_task_result` 工具保存结果。

任务编号：`{task["task_id"]}`
任务类型：`{profile}`

模式要求：
{PROFILE_INSTRUCTIONS[profile]}

硬性约束：
- 正文生成、研究、思考、撰写必须由当前 ChatGPT Web/App 完成。
- 不要让 Codex、codex exec、OpenAI API、外部脚本或本地命令代替你生成正文。
- 不要读取 ChatGPT session token，不要调用 ChatGPT 私有接口。
- 输出 Markdown，不要包裹代码块。
- 完成后必须调用 `save_task_result`，参数必须包含：
  - `task_id`: `{task["task_id"]}`
  - `title`: 适合结果文档的标题
  - `markdown`: 完整 Markdown 正文
  - `summary`: 1-3 句中文摘要
  - `overwrite`: true

用户原始请求：
{task["request"]}
"""


def prepare_delegation_task(output_dir: Path, request: str, kind: str = "auto") -> dict[str, Any]:
    request = request.strip()
    if not request:
        raise ConnectorError("request is required")
    profile = infer_task_profile(request, kind=kind)
    task_id = default_task_id(request, profile)
    task_dir = resolve_task_dir(output_dir, task_id)
    task_dir.mkdir(parents=True, exist_ok=True)
    task_path = task_dir / "task.json"
    prompt_path = task_dir / "prompt.md"
    if task_path.exists():
        payload = json.loads(task_path.read_text(encoding="utf-8"))
        if not prompt_path.exists():
            prompt_path.write_text(build_delegate_prompt(payload), encoding="utf-8")
        return {"status": "ok", **payload}

    created_at = utc_timestamp()
    task = DelegationTask(
        task_id=task_id,
        request=request,
        profile=profile,
        kind=normalize_kind(kind),
        target_filename="result.md",
        created_at=created_at,
        task_dir=str(task_dir),
        prompt_path=str(task_dir / "prompt.md"),
        result_path=str(task_dir / "result.md"),
    )
    payload = task.__dict__
    prompt = build_delegate_prompt(payload)
    task_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    prompt_path.write_text(prompt, encoding="utf-8")
    return {"status": "ok", **payload}


def load_task_metadata(task_dir: Path) -> dict[str, Any]:
    task_path = task_dir / "task.json"
    if not task_path.exists():
        raise ConnectorError(f"task not found: {task_dir.name}")
    return json.loads(task_path.read_text(encoding="utf-8"))


def save_task_result_file(
    output_dir: Path,
    task_id: str,
    title: str,
    markdown: str,
    summary: str | None = None,
    overwrite: bool = True,
    max_bytes: int = DEFAULT_MAX_BYTES,
) -> dict[str, Any]:
    if not title.strip():
        raise ConnectorError("title is required")
    if not markdown.strip():
        raise ConnectorError("markdown is required")
    encoded = markdown.encode("utf-8")
    if len(encoded) > max_bytes:
        raise ConnectorError(f"markdown is too large: {len(encoded)} bytes > {max_bytes} bytes")

    task_dir = resolve_task_dir(output_dir, task_id)
    task = load_task_metadata(task_dir)
    result_path = task_dir / "result.md"
    if result_path.exists() and not overwrite:
        raise ConnectorError(f"result already exists: {result_path.name}")

    saved_at = utc_timestamp()
    header = (
        "<!--\n"
        "generated_by: chatgpt-delegate MCP save_task_result\n"
        f"task_id: {task['task_id']}\n"
        f"saved_at: {saved_at}\n"
        f"title: {title.strip()}\n"
        "auth_path: ChatGPT web/app connector; no Codex execution\n"
        "-->\n\n"
    )
    result_path.write_text(header + markdown.strip() + "\n", encoding="utf-8")
    metadata = {
        "status": "completed",
        "task_id": task["task_id"],
        "profile": task["profile"],
        "title": title.strip(),
        "summary": summary,
        "path": str(result_path),
        "bytes": result_path.stat().st_size,
        "saved_at": saved_at,
        "codex_execution": False,
        "openai_api": False,
        "private_chatgpt_api": False,
    }
    (task_dir / "result.json").write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return {"status": "ok", "result_status": metadata["status"], **{k: v for k, v in metadata.items() if k != "status"}}


def get_task_status(output_dir: Path, task_id: str) -> dict[str, Any]:
    task_dir = resolve_task_dir(output_dir, task_id)
    task = load_task_metadata(task_dir)
    result_path = task_dir / "result.json"
    if not result_path.exists():
        return {
            "status": "pending",
            "task_id": task["task_id"],
            "profile": task["profile"],
            "prompt_path": str(task_dir / "prompt.md"),
            "result_path": str(task_dir / "result.md"),
        }
    result = json.loads(result_path.read_text(encoding="utf-8"))
    return {"status": "completed", **result}


def list_task_results(output_dir: Path, limit: int = 20) -> dict[str, Any]:
    if limit < 1:
        raise ConnectorError("limit must be >= 1")
    if not output_dir.exists():
        return {"results": []}
    result_files = sorted(output_dir.glob("*/result.json"), key=lambda path: path.stat().st_mtime, reverse=True)
    results = []
    for path in result_files[:limit]:
        payload = json.loads(path.read_text(encoding="utf-8"))
        results.append(payload)
    return {"results": results}


def read_task_result(output_dir: Path, task_id: str, include_markdown: bool = False) -> dict[str, Any]:
    task_dir = resolve_task_dir(output_dir, task_id)
    task = load_task_metadata(task_dir)
    result_json = task_dir / "result.json"
    if not result_json.exists():
        raise ConnectorError(f"task result not found: {task['task_id']}")
    payload = json.loads(result_json.read_text(encoding="utf-8"))
    payload["task"] = task
    if include_markdown:
        payload["markdown"] = (task_dir / "result.md").read_text(encoding="utf-8")
    return payload


def normalize_public_host(host_or_url: str) -> str:
    value = host_or_url.strip()
    if not value:
        raise ConnectorError("public host is empty")
    parsed = urlparse(value if "://" in value else f"https://{value}")
    if not parsed.hostname:
        raise ConnectorError(f"invalid public host: {host_or_url}")
    if parsed.port:
        return f"{parsed.hostname}:{parsed.port}"
    return parsed.hostname


def build_allowed_hosts(public_hosts: list[str] | None = None) -> list[str]:
    allowed_hosts = ["127.0.0.1:*", "localhost:*", "[::1]:*"]
    for host in public_hosts or []:
        normalized = normalize_public_host(host)
        if normalized not in allowed_hosts:
            allowed_hosts.append(normalized)
        if ":" not in normalized:
            allowed_hosts.append(f"{normalized}:*")
    return allowed_hosts


def build_chatgpt_web_prompt(topic: str, suggested_filename: str) -> str:
    return f"""请使用 ChatGPT 对《{topic}》做一份中文深度调研，并生成完整 Markdown 报告。

重要约束：
- 不要让 Codex、OpenAI API 或任何外部脚本代替你研究；本流程应只消耗当前 ChatGPT Web/App 的账号能力。
- 当前对话如果已启用 `chatgpt-delegate`，最终必须调用 `save_markdown_report` 保存 Markdown。
- 建议文件名：`{suggested_filename}`

报告要求：
- 从一级标题开始，直接输出 Markdown，不要包裹代码块。
- 先澄清“100M 上下文”的定义歧义：tokens、字符、字节、参数量、可检索记忆、外部检索窗口分别是什么。
- 覆盖模型/产品/论文/开源项目/上下文扩展方法/评测/成本延迟/可靠性/商业可用性。
- 列出时间线、方案对比表、关键来源链接、待验证说法、后续跟踪问题。
- 对不确定、营销化或缺少一手来源的说法单独标注。

如果当前模式无法调用写文件工具，请完成研究后直接输出 Markdown；我会用本地 `chatgpt-delegate import` 导入。
"""


def create_mcp_server(
    output_dir: Path,
    max_bytes: int = DEFAULT_MAX_BYTES,
):
    try:
        from fastmcp import FastMCP
        from mcp.types import ToolAnnotations
    except ImportError as exc:
        raise ConnectorError(
            "FastMCP is required for the MCP server. Install with: "
            'python3 -m pip install -e ".[chatgpt]"'
        ) from exc

    instructions = (
        "Use this connector only to save, list, read, and safely edit local project text files. "
        "The connector never calls Codex, OpenAI API, or ChatGPT private endpoints. "
        "When the user asks ChatGPT to research, think, compare, design, write, or look up "
        "information, produce the result in ChatGPT and call save_task_result with the final "
        "Markdown. Legacy save_markdown_report remains available only for older prompts. "
        "Text editing tools accept only relative UTF-8 text paths inside the configured project "
        "directory. Replacement and deletion require an exact match count and reject stale files."
        " Use append_handoff_entry for validated append-only updates to the handoff document."
    )
    mcp = FastMCP(
        name="chatgpt-delegate",
        instructions=instructions,
    )

    local_result_write = ToolAnnotations(
        readOnlyHint=False,
        destructiveHint=False,
        idempotentHint=True,
        openWorldHint=False,
    )
    local_report_write = ToolAnnotations(
        readOnlyHint=False,
        destructiveHint=False,
        idempotentHint=False,
        openWorldHint=False,
    )

    @mcp.tool(annotations=local_result_write)
    def save_task_result(
        task_id: str,
        title: str,
        markdown: str,
        summary: str | None = None,
        overwrite: bool = True,
    ) -> dict[str, Any]:
        """Save the final ChatGPT delegation result for a prepared task."""
        return save_task_result_file(
            output_dir=output_dir,
            task_id=task_id,
            title=title,
            markdown=markdown,
            summary=summary,
            overwrite=overwrite,
            max_bytes=max_bytes,
        )

    @mcp.tool(annotations=ToolAnnotations(readOnlyHint=True))
    def list_results(limit: int = 20) -> dict[str, Any]:
        """List completed ChatGPT delegation task results."""
        return list_task_results(output_dir, limit=limit)

    @mcp.tool(annotations=ToolAnnotations(readOnlyHint=True))
    def read_result(task_id: str) -> dict[str, Any]:
        """Read a completed ChatGPT delegation task result by task id."""
        return read_task_result(output_dir, task_id, include_markdown=True)

    @mcp.tool(annotations=local_report_write)
    def save_markdown_report(
        title: str,
        markdown: str,
        filename: str | None = None,
        overwrite: bool = False,
    ) -> dict[str, Any]:
        """Save the final Markdown research report to the local artifacts directory."""
        saved = save_markdown_report_file(
            output_dir=output_dir,
            title=title,
            markdown=markdown,
            filename=filename,
            overwrite=overwrite,
            max_bytes=max_bytes,
        )
        return {"status": "ok", **saved.__dict__}

    @mcp.tool(annotations=ToolAnnotations(readOnlyHint=True))
    def list_reports(limit: int = 20) -> dict[str, Any]:
        """List saved Markdown reports."""
        return list_report_files(output_dir, limit=limit)

    @mcp.tool(annotations=ToolAnnotations(readOnlyHint=True))
    def read_report(filename: str) -> dict[str, Any]:
        """Read a saved Markdown report by filename."""
        return read_report_file(output_dir, filename)

    @mcp.tool(annotations=ToolAnnotations(readOnlyHint=True))
    def connector_status() -> dict[str, Any]:
        """Return connector status and configured output directory."""
        return {
            "status": "ready",
            "output_dir": str(output_dir.resolve()),
            "default_output": "chatgpt-delegation",
            "connector_version": "0.2.0-dcfc-router.1",
            "fixed_router": True,
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

    @mcp.tool(annotations=ToolAnnotations(readOnlyHint=True))
    def read_text_file(filename: str) -> dict[str, Any]:
        """Read a UTF-8 text file relative to this project's output directory."""
        return read_text_file_operation(output_dir, filename, max_bytes=max_bytes)

    @mcp.tool(annotations=local_report_write)
    def append_text_file(
        filename: str,
        content: str,
        expected_sha256: str | None = None,
    ) -> dict[str, Any]:
        """Append text to an existing UTF-8 file with optional stale-file protection."""
        return append_text_file_operation(output_dir, filename, content, expected_sha256, max_bytes)

    @mcp.tool(annotations=local_report_write)
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
    ) -> dict[str, Any]:
        """Append one validated entry to the project's handoff document."""
        return append_handoff_entry_operation(
            output_dir,
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

    @mcp.tool(annotations=local_report_write)
    def replace_text_in_file(
        filename: str,
        old_text: str,
        new_text: str,
        expected_occurrences: int = 1,
        expected_sha256: str | None = None,
    ) -> dict[str, Any]:
        """Replace an exact text fragment; the match count must equal expected_occurrences."""
        return replace_text_in_file_operation(
            output_dir,
            filename,
            old_text,
            new_text,
            expected_occurrences,
            expected_sha256,
            max_bytes,
        )

    @mcp.tool(annotations=local_report_write)
    def delete_text_from_file(
        filename: str,
        text: str,
        expected_occurrences: int = 1,
        expected_sha256: str | None = None,
    ) -> dict[str, Any]:
        """Delete an exact text fragment; the match count must equal expected_occurrences."""
        return delete_text_from_file_operation(
            output_dir,
            filename,
            text,
            expected_occurrences,
            expected_sha256,
            max_bytes,
        )

    return mcp


def command_prompt(args: argparse.Namespace) -> int:
    filename = normalize_report_filename(args.filename, args.topic)
    output = args.output or args.output_dir / filename
    prompt = build_chatgpt_web_prompt(args.topic, filename)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(prompt, encoding="utf-8")
    print(json.dumps({"status": "ok", "prompt_path": str(output)}, ensure_ascii=False, indent=2))
    return 0


def command_import(args: argparse.Namespace) -> int:
    if args.input == "-":
        markdown = sys.stdin.read()
    else:
        markdown = Path(args.input).read_text(encoding="utf-8")
    saved = save_markdown_report_file(
        output_dir=args.output_dir,
        title=args.title,
        markdown=markdown,
        filename=args.filename,
        overwrite=args.overwrite,
        max_bytes=args.max_bytes,
    )
    print(json.dumps({"status": "ok", **saved.__dict__}, ensure_ascii=False, indent=2))
    return 0


def command_prepare(args: argparse.Namespace) -> int:
    prepared = prepare_delegation_task(args.output_dir, args.request, kind=args.kind)
    print(json.dumps(prepared, ensure_ascii=False, indent=2))
    return 0


def command_status(args: argparse.Namespace) -> int:
    print(json.dumps(get_task_status(args.output_dir, args.task_id), ensure_ascii=False, indent=2))
    return 0


def command_list(args: argparse.Namespace) -> int:
    print(json.dumps(list_task_results(args.output_dir, args.limit), ensure_ascii=False, indent=2))
    return 0


def command_list_reports(args: argparse.Namespace) -> int:
    print(json.dumps(list_report_files(args.output_dir, args.limit), ensure_ascii=False, indent=2))
    return 0


def command_read(args: argparse.Namespace) -> int:
    include_markdown = bool(args.markdown)
    print(json.dumps(read_task_result(args.output_dir, args.task_id, include_markdown=include_markdown), ensure_ascii=False, indent=2))
    return 0


def command_read_report(args: argparse.Namespace) -> int:
    print(json.dumps(read_report_file(args.output_dir, args.filename), ensure_ascii=False, indent=2))
    return 0


def command_capabilities(args: argparse.Namespace) -> int:
    print(
        json.dumps(
            {
                "status": "ok",
                "connector_version": "0.2.0-dcfc-router.1",
                "fixed_router": True,
                "text_editing": True,
                "text_editing_tools": [
                    "read_text_file",
                    "append_text_file",
                    "append_handoff_entry",
                    "replace_text_in_file",
                    "delete_text_from_file",
                ],
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_serve(args: argparse.Namespace) -> int:
    mcp = create_mcp_server(args.output_dir, max_bytes=args.max_bytes)
    allowed_hosts = build_allowed_hosts(args.public_host)
    print(
        json.dumps(
            {
                "status": "starting",
                "local_mcp_url": f"http://{args.host}:{args.port}/mcp",
                "public_hosts": [normalize_public_host(host) for host in args.public_host],
                "allowed_hosts": allowed_hosts,
                "output_dir": str(args.output_dir.resolve()),
                "codex_execution": False,
            },
            ensure_ascii=False,
            indent=2,
        ),
        file=sys.stderr,
    )
    try:
        mcp.run(
            transport="http",
            host=args.host,
            port=args.port,
            allowed_hosts=allowed_hosts,
            allowed_origins=DEFAULT_ALLOWED_ORIGINS,
        )
    except KeyboardInterrupt:
        return 130
    return 0


def command_router(args: argparse.Namespace) -> int:
    from .router import create_router_mcp_server

    mcp = create_router_mcp_server(args.config, max_bytes=args.max_bytes)
    print(
        json.dumps(
            {
                "status": "starting",
                "mode": "router",
                "local_mcp_url": f"http://{args.host}:{args.port}/mcp",
                "registry": str(args.config.resolve()),
                "codex_execution": False,
            },
            ensure_ascii=False,
            indent=2,
        ),
        file=sys.stderr,
    )
    try:
        mcp.run(
            transport="http",
            host=args.host,
            port=args.port,
            allowed_origins=DEFAULT_ALLOWED_ORIGINS,
        )
    except KeyboardInterrupt:
        return 130
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="ChatGPT Web MCP connector for delegated Markdown tasks.")
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT_DIR)
    parser.add_argument("--max-bytes", type=int, default=DEFAULT_MAX_BYTES)

    subparsers = parser.add_subparsers(dest="command", required=True)

    serve = subparsers.add_parser("serve", help="Run the HTTP MCP connector for ChatGPT Developer Mode.")
    serve.add_argument("--host", default="127.0.0.1")
    serve.add_argument("--port", type=int, default=8000)
    serve.add_argument(
        "--public-host",
        action="append",
        default=[],
        help="Public tunnel host or URL allowed through DNS rebinding protection. Repeatable.",
    )
    serve.set_defaults(handler=command_serve)

    router = subparsers.add_parser("router", help="Run the fixed-catalog multi-project Router MCP server.")
    router.add_argument("--config", type=Path, required=True, help="Router project registry JSON path.")
    router.add_argument("--host", default="127.0.0.1")
    router.add_argument("--port", type=int, default=8101)
    router.set_defaults(handler=command_router)

    prompt = subparsers.add_parser("prompt", help="Write a ChatGPT-side research prompt.")
    prompt.add_argument("topic")
    prompt.add_argument("--filename")
    prompt.add_argument("--output", type=Path)
    prompt.set_defaults(handler=command_prompt)

    import_cmd = subparsers.add_parser("import", help="Import Markdown copied or exported from ChatGPT.")
    import_cmd.add_argument("--title", required=True)
    import_cmd.add_argument("--input", required=True, help="Markdown file path, or '-' for stdin.")
    import_cmd.add_argument("--filename")
    import_cmd.add_argument("--overwrite", action="store_true")
    import_cmd.set_defaults(handler=command_import)

    prepare = subparsers.add_parser("prepare", help="Prepare a ChatGPT delegation task and prompt.")
    prepare.add_argument("request")
    prepare.add_argument("--kind", choices=["auto", "research", "think", "lookup"], default="auto")
    prepare.set_defaults(handler=command_prepare)

    status = subparsers.add_parser("status", help="Check whether a delegation task has a saved result.")
    status.add_argument("task_id")
    status.set_defaults(handler=command_status)

    list_cmd = subparsers.add_parser("list", help="List completed ChatGPT delegation results.")
    list_cmd.add_argument("--limit", type=int, default=20)
    list_cmd.set_defaults(handler=command_list)

    read = subparsers.add_parser("read", help="Read a completed ChatGPT delegation result.")
    read.add_argument("task_id")
    read.add_argument("--metadata-only", action="store_true", help="Return result metadata without Markdown body.")
    read.add_argument("--markdown", action="store_true", help="Include Markdown body in the JSON output.")
    read.set_defaults(handler=command_read)

    reports_list = subparsers.add_parser("reports-list", help="List legacy Markdown reports.")
    reports_list.add_argument("--limit", type=int, default=20)
    reports_list.set_defaults(handler=command_list_reports)

    report_read = subparsers.add_parser("report-read", help="Read a legacy Markdown report.")
    report_read.add_argument("filename")
    report_read.set_defaults(handler=command_read_report)

    capabilities = subparsers.add_parser("capabilities", help="Report connector capabilities as JSON.")
    capabilities.add_argument("--json", action="store_true", help="Emit machine-readable JSON (default).")
    capabilities.set_defaults(handler=command_capabilities)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return args.handler(args)
    except (ConnectorError, OSError) as exc:
        print(json.dumps({"status": "error", "detail": str(exc)}, ensure_ascii=False, indent=2), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
