from __future__ import annotations

import hashlib
import os
import re
import shutil
import tempfile
from datetime import datetime, timezone
from pathlib import Path, PureWindowsPath
from typing import Any


class TextEditingError(ValueError):
    """A safe, user-facing error raised by text editing operations."""


def _utc_timestamp() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def _relative_filename(filename: str) -> str:
    value = filename.strip()
    if not value:
        raise TextEditingError("filename is required")
    if "\x00" in value:
        raise TextEditingError("filename contains an invalid null byte")
    windows_path = PureWindowsPath(value)
    path = Path(value.replace("\\", "/"))
    if windows_path.is_absolute() or path.is_absolute() or windows_path.drive:
        raise TextEditingError("filename must be relative to the project directory")
    parts = value.replace("\\", "/").split("/")
    if any(part in {"", ".", ".."} for part in parts):
        raise TextEditingError("filename must not contain empty, dot, or parent path segments")
    if any("\x00" in part for part in parts):
        raise TextEditingError("filename contains an invalid null byte")
    return "/".join(parts)


def resolve_text_path(output_dir: Path, filename: str) -> Path:
    root = output_dir.expanduser().resolve()
    if not root.exists() or not root.is_dir():
        raise TextEditingError("project output directory does not exist")
    relative = _relative_filename(filename)
    candidate = (root / relative).resolve()
    try:
        candidate.relative_to(root)
    except ValueError as exc:
        raise TextEditingError("resolved path escaped the project directory") from exc
    if candidate == root or candidate.is_dir():
        raise TextEditingError("filename must point to a text file")
    return candidate


def _read_file(path: Path, max_bytes: int) -> tuple[str, bytes]:
    try:
        raw = path.read_bytes()
    except FileNotFoundError as exc:
        raise TextEditingError(f"file not found: {path.name}") from exc
    except OSError as exc:
        raise TextEditingError(f"could not read file: {exc}") from exc
    if len(raw) > max_bytes:
        raise TextEditingError(f"file is too large: {len(raw)} bytes > {max_bytes} bytes")
    try:
        return raw.decode("utf-8"), raw
    except UnicodeDecodeError as exc:
        raise TextEditingError("file is not valid UTF-8 text") from exc


def _sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def _metadata(path: Path, raw: bytes, content: str) -> dict[str, Any]:
    return {
        "filename": str(path.name),
        "path": str(path),
        "bytes": len(raw),
        "sha256": _sha256(raw),
        "modified_at": datetime.fromtimestamp(path.stat().st_mtime, timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z"),
        "content": content,
    }


def read_text_file(output_dir: Path, filename: str, max_bytes: int) -> dict[str, Any]:
    path = resolve_text_path(output_dir, filename)
    content, raw = _read_file(path, max_bytes)
    return _metadata(path, raw, content)


def _check_expected_sha256(raw: bytes, expected_sha256: str | None) -> None:
    if expected_sha256 is None:
        return
    expected = expected_sha256.strip().lower()
    if not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise TextEditingError("expected_sha256 must be a 64-character hexadecimal SHA-256")
    actual = _sha256(raw)
    if actual != expected:
        raise TextEditingError("file changed since it was read; expected_sha256 does not match")


def _backup_path(root: Path, path: Path) -> Path:
    backup_dir = root / ".dcfc-backups"
    backup_dir.mkdir(parents=True, exist_ok=True)
    safe_name = re.sub(r"[^A-Za-z0-9._-]+", "_", str(path.relative_to(root)).replace("\\", "_"))
    return backup_dir / f"{safe_name}.{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ%f')}.bak"


def _atomic_write(root: Path, path: Path, content: str, max_bytes: int, expected_raw: bytes) -> dict[str, Any]:
    encoded = content.encode("utf-8")
    if len(encoded) > max_bytes:
        raise TextEditingError(f"updated file is too large: {len(encoded)} bytes > {max_bytes} bytes")
    if not path.exists():
        raise TextEditingError(f"file not found: {path.name}")
    try:
        current_raw = path.read_bytes()
    except OSError as exc:
        raise TextEditingError(f"could not recheck file before writing: {exc}") from exc
    if current_raw != expected_raw:
        raise TextEditingError("file changed while preparing the edit; refusing to overwrite it")
    backup = _backup_path(root, path)
    try:
        shutil.copyfile(path, backup)
        with tempfile.NamedTemporaryFile(
            mode="wb",
            prefix=f".{path.name}.",
            suffix=".tmp",
            dir=path.parent,
            delete=False,
        ) as temporary:
            temporary.write(encoded)
            temporary.flush()
            os.fsync(temporary.fileno())
            temporary_path = Path(temporary.name)
        os.replace(temporary_path, path)
    except OSError as exc:
        try:
            if "temporary_path" in locals() and temporary_path.exists():
                temporary_path.unlink()
        except OSError:
            pass
        raise TextEditingError(f"could not safely write file: {exc}") from exc
    new_raw = path.read_bytes()
    return {
        "filename": path.name,
        "path": str(path),
        "bytes": len(new_raw),
        "sha256": _sha256(new_raw),
        "backup_path": str(backup),
        "modified_at": datetime.fromtimestamp(path.stat().st_mtime, timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z"),
    }


def _validate_occurrences(content: str, needle: str, expected_occurrences: int) -> int:
    if not needle:
        raise TextEditingError("text is required")
    if expected_occurrences < 1:
        raise TextEditingError("expected_occurrences must be at least 1")
    occurrences = content.count(needle)
    if occurrences != expected_occurrences:
        raise TextEditingError(
            f"expected {expected_occurrences} exact match(es), but found {occurrences}"
        )
    return occurrences


def append_text_file(
    output_dir: Path,
    filename: str,
    content: str,
    expected_sha256: str | None,
    max_bytes: int,
) -> dict[str, Any]:
    if not content:
        raise TextEditingError("content is required")
    root = output_dir.expanduser().resolve()
    path = resolve_text_path(root, filename)
    original, raw = _read_file(path, max_bytes)
    _check_expected_sha256(raw, expected_sha256)
    separator = "" if original.endswith(("\n", "\r")) or content.startswith(("\n", "\r")) else "\n"
    result = _atomic_write(root, path, original + separator + content, max_bytes, raw)
    result["operation"] = "append"
    return result


def replace_text_in_file(
    output_dir: Path,
    filename: str,
    old_text: str,
    new_text: str,
    expected_occurrences: int,
    expected_sha256: str | None,
    max_bytes: int,
) -> dict[str, Any]:
    root = output_dir.expanduser().resolve()
    path = resolve_text_path(root, filename)
    original, raw = _read_file(path, max_bytes)
    _check_expected_sha256(raw, expected_sha256)
    occurrences = _validate_occurrences(original, old_text, expected_occurrences)
    result = _atomic_write(root, path, original.replace(old_text, new_text), max_bytes, raw)
    result.update({"operation": "replace", "replaced_occurrences": occurrences})
    return result


def delete_text_from_file(
    output_dir: Path,
    filename: str,
    text: str,
    expected_occurrences: int,
    expected_sha256: str | None,
    max_bytes: int,
) -> dict[str, Any]:
    root = output_dir.expanduser().resolve()
    path = resolve_text_path(root, filename)
    original, raw = _read_file(path, max_bytes)
    _check_expected_sha256(raw, expected_sha256)
    occurrences = _validate_occurrences(original, text, expected_occurrences)
    result = _atomic_write(root, path, original.replace(text, ""), max_bytes, raw)
    result.update({"operation": "delete", "deleted_occurrences": occurrences})
    return result
