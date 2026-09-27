import hashlib
import tempfile
import unittest
from pathlib import Path

from chatgpt_delegate.text_editing import (
    TextEditingError,
    append_text_file,
    delete_text_from_file,
    read_text_file,
    replace_text_in_file,
)


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


if __name__ == "__main__":
    unittest.main()
