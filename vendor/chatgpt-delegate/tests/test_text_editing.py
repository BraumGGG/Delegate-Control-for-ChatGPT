import hashlib
import multiprocessing
import tempfile
import unittest
from pathlib import Path

from chatgpt_delegate.text_editing import (
    TextEditingError,
    append_text_file,
    append_handoff_entry,
    delete_text_from_file,
    read_text_file,
    replace_text_in_file,
)


def _append_handoff_worker(root: str, entry_id: str, expected_sha256: str, result_queue) -> None:
    try:
        result = append_handoff_entry(
            Path(root),
            entry_id,
            "DEV",
            "REVIEW",
            "DONE",
            "P1",
            f"Concurrent {entry_id}",
            [],
            "Concurrent append test.",
            "Exactly one writer must succeed.",
            "Inspect the resulting file.",
            expected_sha256,
            1024 * 1024,
        )
        result_queue.put({"ok": True, "result": result})
    except Exception as exc:  # pragma: no cover - exercised in the child process
        result_queue.put({"ok": False, "error": str(exc)})


class TextEditingTestCase(unittest.TestCase):
    def test_read_returns_content_and_sha256(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            path = root / "notes.md"
            path.write_text("标题\n正文", encoding="utf-8", newline="")

            result = read_text_file(root, "notes.md", max_bytes=1024)

        self.assertEqual(result["content"], "标题\n正文")
        self.assertEqual(result["sha256"], hashlib.sha256("标题\n正文".encode()).hexdigest())

    def test_rejects_absolute_parent_and_symlink_escape_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir, tempfile.TemporaryDirectory() as outside_dir:
            root = Path(temp_dir)
            outside = Path(outside_dir) / "secret.txt"
            outside.write_text("secret", encoding="utf-8")
            for filename in ("C:\\secret.txt", "\\\\server\\secret.txt", "../secret.txt", "nested/../secret.txt"):
                with self.subTest(filename=filename), self.assertRaises(TextEditingError):
                    read_text_file(root, filename, max_bytes=1024)
            link = root / "link.txt"
            try:
                link.symlink_to(outside)
            except (OSError, NotImplementedError):
                self.skipTest("symlink creation is unavailable")
            with self.assertRaises(TextEditingError):
                read_text_file(root, "link.txt", max_bytes=1024)

    def test_rejects_non_utf8_and_oversized_files(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "binary.txt").write_bytes(b"\xff\xfe")
            (root / "large.txt").write_text("12345", encoding="utf-8")
            with self.assertRaises(TextEditingError):
                read_text_file(root, "binary.txt", max_bytes=1024)
            with self.assertRaises(TextEditingError):
                read_text_file(root, "large.txt", max_bytes=4)

    def test_append_adds_only_needed_separator(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            path = root / "notes.md"
            path.write_text("one", encoding="utf-8", newline="")
            result = append_text_file(root, "notes.md", "two", None, 1024)
            self.assertEqual(path.read_text(encoding="utf-8"), "one\ntwo")
            self.assertEqual(result["operation"], "append")
            append_text_file(root, "notes.md", "\nthree", None, 1024)
            self.assertEqual(path.read_text(encoding="utf-8"), "one\ntwo\nthree")

    def test_replace_requires_unique_exact_match(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            path = root / "notes.md"
            path.write_text("alpha\nbeta\nalpha", encoding="utf-8", newline="")
            with self.assertRaises(TextEditingError):
                replace_text_in_file(root, "notes.md", "alpha", "new", 1, None, 1024)
            with self.assertRaises(TextEditingError):
                replace_text_in_file(root, "notes.md", "missing", "new", 1, None, 1024)
            result = replace_text_in_file(root, "notes.md", "alpha", "new", 2, None, 1024)
            self.assertEqual(path.read_text(encoding="utf-8"), "new\nbeta\nnew")
            self.assertEqual(result["replaced_occurrences"], 2)

    def test_delete_and_sha_conflict_preserve_original(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            path = root / "notes.md"
            path.write_text("keep\nremove\nkeep", encoding="utf-8", newline="")
            before = read_text_file(root, "notes.md", 1024)
            path.write_text("changed", encoding="utf-8", newline="")
            with self.assertRaises(TextEditingError):
                delete_text_from_file(root, "notes.md", "remove", 1, before["sha256"], 1024)
            self.assertEqual(path.read_text(encoding="utf-8"), "changed")
            path.write_text("keep\nremove\nkeep", encoding="utf-8", newline="")
            result = delete_text_from_file(root, "notes.md", "remove", 1, None, 1024)
            self.assertEqual(path.read_text(encoding="utf-8"), "keep\n\nkeep")
            self.assertEqual(result["deleted_occurrences"], 1)
            self.assertTrue(Path(result["backup_path"]).is_file())

    def test_writes_are_limited_by_final_size(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            path = root / "notes.md"
            path.write_text("one", encoding="utf-8", newline="")
            with self.assertRaises(TextEditingError):
                append_text_file(root, "notes.md", "too-long", None, 4)
            self.assertEqual(path.read_text(encoding="utf-8"), "one")

    def _create_handoff(self, root: Path) -> Path:
        path = root / "PRODUCT_DESIGNER_DEVELOPER_HANDOFF.md"
        path.write_text("# Handoff\n", encoding="utf-8", newline="")
        return path

    def test_structured_handoff_append_serializes_canonical_entry(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            path = self._create_handoff(root)
            before = read_text_file(root, path.name, 1024 * 1024)

            result = append_handoff_entry(
                root,
                "PDH-20260928-010",
                "DEV",
                "REVIEW",
                "DONE",
                "P1",
                "Structured append implemented",
                ["PDH-20260928-009"],
                "The shared structured append operation is implemented.",
                "Validation, SHA protection, backup and atomic replacement pass.",
                "Request PD review.",
                before["sha256"],
                1024 * 1024,
            )

            content = path.read_text(encoding="utf-8")
            backup_exists = Path(result["backup_path"]).is_file()

        self.assertEqual(result["operation"], "append_handoff_entry")
        self.assertEqual(result["entry_id"], "PDH-20260928-010")
        self.assertEqual(result["sha256"], hashlib.sha256(content.encode("utf-8")).hexdigest())
        self.assertIn("### PDH-20260928-010 [DEV] REVIEW — Structured append implemented", content)
        self.assertIn("- **Related Entry:** PDH-20260928-009", content)
        self.assertIn("**Acceptance / Expected Outcome**", content)
        self.assertTrue(backup_exists)

    def test_structured_handoff_rejects_invalid_fields_and_empty_body(self) -> None:
        cases = (
            {"entry_id": "BAD-ID"},
            {"author": "SYSTEM"},
            {"entry_type": "UNKNOWN"},
            {"status": "UNKNOWN"},
            {"priority": "P4"},
            {"title": ""},
            {"related_entries": ["not-an-id"]},
            {"summary": ""},
            {"acceptance": ""},
            {"next_action": ""},
        )
        for override in cases:
            with self.subTest(override=override), tempfile.TemporaryDirectory() as temp_dir:
                root = Path(temp_dir)
                path = self._create_handoff(root)
                before = read_text_file(root, path.name, 1024 * 1024)
                fields = {
                    "entry_id": "PDH-20260928-011",
                    "author": "DEV",
                    "entry_type": "REVIEW",
                    "status": "DONE",
                    "priority": "P1",
                    "title": "Valid title",
                    "related_entries": [],
                    "summary": "Valid summary",
                    "acceptance": "Valid acceptance",
                    "next_action": "Valid next action",
                    "expected_sha256": before["sha256"],
                    "max_bytes": 1024 * 1024,
                }
                fields.update(override)
                with self.assertRaises(TextEditingError):
                    append_handoff_entry(root, **fields)
                self.assertEqual(path.read_bytes(), b"# Handoff\n")

    def test_structured_handoff_rejects_duplicate_and_stale_sha_without_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            path = self._create_handoff(root)
            before = read_text_file(root, path.name, 1024 * 1024)
            append_handoff_entry(
                root,
                "PDH-20260928-012",
                "DEV",
                "REVIEW",
                "DONE",
                "P1",
                "First entry",
                [],
                "Summary",
                "Acceptance",
                "Next action",
                before["sha256"],
                1024 * 1024,
            )
            after_first = path.read_bytes()
            current = read_text_file(root, path.name, 1024 * 1024)

            with self.assertRaises(TextEditingError):
                append_handoff_entry(
                    root,
                    "PDH-20260928-012",
                    "DEV",
                    "REVIEW",
                    "DONE",
                    "P1",
                    "Duplicate",
                    [],
                    "Summary",
                    "Acceptance",
                    "Next action",
                    current["sha256"],
                    1024 * 1024,
                )
            self.assertEqual(path.read_bytes(), after_first)

            path_before_stale = path.read_bytes()
            with self.assertRaises(TextEditingError):
                append_handoff_entry(
                    root,
                    "PDH-20260928-013",
                    "DEV",
                    "REVIEW",
                    "DONE",
                    "P1",
                    "Stale",
                    [],
                    "Summary",
                    "Acceptance",
                    "Next action",
                    before["sha256"],
                    1024 * 1024,
                )
            self.assertEqual(path.read_bytes(), path_before_stale)

    def test_structured_handoff_concurrent_append_has_one_success(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            path = self._create_handoff(root)
            expected_sha = read_text_file(root, path.name, 1024 * 1024)["sha256"]
            context = multiprocessing.get_context("spawn")
            result_queue = context.Queue()
            processes = [
                context.Process(
                    target=_append_handoff_worker,
                    args=(str(root), f"PDH-20260928-01{index}", expected_sha, result_queue),
                )
                for index in (4, 5)
            ]
            for process in processes:
                process.start()
            results = [result_queue.get(timeout=15) for _ in processes]
            for process in processes:
                process.join(timeout=15)

            content = path.read_text(encoding="utf-8")

        self.assertEqual(sum(result["ok"] for result in results), 1)
        self.assertEqual(
            content.count("### PDH-20260928-014") + content.count("### PDH-20260928-015"),
            1,
        )
        self.assertEqual(content.count("Concurrent append test."), 1)


if __name__ == "__main__":
    unittest.main()
