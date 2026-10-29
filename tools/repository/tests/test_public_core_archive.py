"""Keep archive producers alive until their trailing output has been consumed."""

from importlib.util import module_from_spec, spec_from_file_location
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch


SPEC = spec_from_file_location("public_archive_export", Path(__file__).parents[1] / "export-public-core.py")
assert SPEC and SPEC.loader
EXPORT = module_from_spec(SPEC)
SPEC.loader.exec_module(EXPORT)


class ObservedStream(io.BytesIO):
    def __init__(self, value):
        super().__init__(value)
        self.length = len(value)
        self.consumed_before_close = None

    def close(self):
        if not self.closed:
            self.consumed_before_close = self.tell()
        super().close()


class ArchiveProcess:
    def __init__(self, archive, return_code=0, stderr=b""):
        self.stdout = ObservedStream(archive)
        self.stderr = ObservedStream(stderr)
        self.return_code = return_code
        self.waited = False

    def wait(self):
        self.waited = True
        # Model a producer that encounters a broken pipe when the consumer
        # closes after the tar end marker instead of reading the final padding.
        if self.stdout.consumed_before_close != self.stdout.length:
            return -13
        return self.return_code


def archive_bytes(unsupported=False):
    result = io.BytesIO()
    with tarfile.open(fileobj=result, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        member = tarfile.TarInfo("Cargo.toml")
        if unsupported:
            member.type = tarfile.SYMTYPE
            member.linkname = "another-file"
            archive.addfile(member)
        else:
            content = b"[workspace]\n"
            member.size = len(content)
            member.mode = 0o644
            archive.addfile(member, io.BytesIO(content))
    # This padding lies beyond tarfile's read-ahead buffer and must be drained
    # even though it contains no additional archive members.
    return result.getvalue() + bytes(256 * 1024)


class PublicArchiveTests(unittest.TestCase):
    def test_export_consumes_padding_before_closing_the_producer_pipe(self):
        process = ArchiveProcess(archive_bytes())
        with tempfile.TemporaryDirectory() as directory, patch.object(
            EXPORT.subprocess, "Popen", return_value=process
        ):
            destination = Path(directory)
            EXPORT.export_archive(destination, destination, "a" * 40)
            self.assertEqual((destination / "Cargo.toml").read_bytes(), b"[workspace]\n")
        self.assertEqual(process.stdout.consumed_before_close, process.stdout.length)
        self.assertTrue(process.stdout.closed)
        self.assertTrue(process.stderr.closed)
        self.assertTrue(process.waited)

    def test_producer_failure_preserves_its_return_code_and_stderr(self):
        for return_code, stderr, detail in [
            (-9, b"", "no stderr output"),
            (128, b"fatal: missing object\n", "fatal: missing object"),
        ]:
            with self.subTest(return_code=return_code):
                process = ArchiveProcess(archive_bytes(), return_code, stderr)
                with tempfile.TemporaryDirectory() as directory, patch.object(
                    EXPORT.subprocess, "Popen", return_value=process
                ):
                    with self.assertRaises(EXPORT.ExportError) as raised:
                        EXPORT.export_archive(Path(directory), Path(directory), "a" * 40)
                self.assertEqual(
                    str(raised.exception),
                    f"git archive failed (return code {return_code}): {detail}",
                )
                self.assertTrue(process.waited)

    def test_invalid_entry_preserves_the_original_error_and_reaps_the_producer(self):
        process = ArchiveProcess(archive_bytes(unsupported=True))
        with tempfile.TemporaryDirectory() as directory, patch.object(
            EXPORT.subprocess, "Popen", return_value=process
        ):
            with self.assertRaisesRegex(EXPORT.ExportError, "archive contains unsupported entry"):
                EXPORT.export_archive(Path(directory), Path(directory), "a" * 40)
        self.assertTrue(process.stdout.closed)
        self.assertTrue(process.stderr.closed)
        self.assertTrue(process.waited)


if __name__ == "__main__":
    unittest.main()
