#!/usr/bin/env python3
"""Package an administrator-owned plugin for the AnyFlows install screen.

Only the supplied distribution directory and public manifest are read. Signing
keys are never generated or loaded; optional flags attach an existing signature.
"""

from __future__ import annotations

import argparse
import base64
import binascii
import json
import os
from pathlib import Path
import shutil
import stat
import sys
import tempfile
import zipfile
import zlib


MAX_UPLOAD_BYTES = 128 * 1024 * 1024
MAX_EXPANDED_BYTES = 512 * 1024 * 1024
MAX_ENTRIES = 100_000
MAX_METADATA_BYTES = 256 * 1024
FIXED_TIMESTAMP = (1980, 1, 1, 0, 0, 0)
COPY_BYTES = 1024 * 1024


class PackageError(RuntimeError):
    """The input cannot be represented by the supported plugin package format."""


def validate_relative_path(value: str) -> str:
    if not value or len(value.encode("utf-8")) > 900:
        raise PackageError("package paths must contain 1–900 UTF-8 bytes")
    if "\\" in value or ":" in value or any(ord(char) < 32 or ord(char) == 127 for char in value):
        raise PackageError("use POSIX relative paths without control characters")
    components = value.split("/")
    if len(components) > 32:
        raise PackageError("package paths must not exceed 32 components")
    for component in components:
        stem = component.split(".", 1)[0].upper()
        reserved = stem in {"CON", "PRN", "AUX", "NUL"} or stem in {
            f"{prefix}{number}" for prefix in ("COM", "LPT") for number in range(1, 10)
        }
        if (
            component in {"", ".", ".."}
            or len(component.encode("utf-8")) > 255
            or component.endswith((".", " "))
            or reserved
        ):
            raise PackageError("package contains a path unsupported by AnyFlows")
    return value


def load_manifest(path: Path) -> dict:
    if path.stat().st_size > MAX_METADATA_BYTES:
        raise PackageError("manifest is larger than the package metadata limit")
    with path.open(encoding="utf-8-sig") as source:
        manifest = json.load(source, parse_constant=lambda value: invalid_number(value))
    if not isinstance(manifest, dict):
        raise PackageError("manifest must be a JSON object")
    # Full contract and JSON Schema validation remains the installer's job.
    if not isinstance(manifest.get("pluginId"), str) or not isinstance(manifest.get("pluginVersion"), str):
        raise PackageError("manifest must contain pluginId and pluginVersion")
    return manifest


def invalid_number(value: str) -> None:
    raise PackageError(f"manifest contains unsupported JSON number {value}")


def package_metadata(args: argparse.Namespace) -> bytes:
    envelope: dict = {
        "manifest": load_manifest(args.manifest_file),
        "executablePath": validate_relative_path(args.executable_path),
    }
    if bool(args.signer_key_id) != bool(args.signature_file):
        raise PackageError("use --signer-key-id and --signature-file together")
    if args.signer_key_id:
        if len(args.signer_key_id.encode("utf-8")) > 128 or any(
            ord(char) < 32 or ord(char) == 127 for char in args.signer_key_id
        ):
            raise PackageError("signer key ID is invalid")
        if args.signature_file.stat().st_size > 1024:
            raise PackageError("signature file must contain one Base64 Ed25519 signature")
        signature = args.signature_file.read_text(encoding="utf-8-sig").strip()
        try:
            decoded = base64.b64decode(signature, validate=True)
        except (ValueError, binascii.Error) as error:
            raise PackageError("signature file is not valid Base64") from error
        if len(signature) != 88 or len(decoded) != 64:
            raise PackageError("signature must be a Base64-encoded 64-byte Ed25519 signature")
        envelope["signature"] = {
            "signerKeyId": args.signer_key_id,
            "signatureBase64": signature,
        }
    metadata = json.dumps(envelope, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")
    if len(metadata) > MAX_METADATA_BYTES:
        raise PackageError("combined package metadata exceeds 256 KiB")
    return metadata


def collect_payload(root: Path) -> list[tuple[str, Path, bool, int]]:
    entries: list[tuple[str, Path, bool, int]] = []
    names: set[str] = set()
    total = 0
    for directory, directories, files in os.walk(root, followlinks=False):
        for name in directories + files:
            path = Path(directory) / name
            metadata = path.lstat()
            is_directory = stat.S_ISDIR(metadata.st_mode)
            if stat.S_ISLNK(metadata.st_mode) or not (is_directory or stat.S_ISREG(metadata.st_mode)):
                raise PackageError("payload must contain only regular files and directories, without links")
            relative = path.relative_to(root).as_posix()
            validate_relative_path(f"payload/{relative}")
            folded = relative.lower()
            if folded in names:
                raise PackageError("payload paths must be unique even when compared without case")
            names.add(folded)
            total += 0 if is_directory else metadata.st_size
            if total > MAX_EXPANDED_BYTES or len(entries) + 3 > MAX_ENTRIES:
                raise PackageError("payload exceeds the expanded size or entry limit")
            entries.append((relative, path, is_directory, metadata.st_size))
    return sorted(entries, key=lambda entry: entry[0].encode("utf-8"))


def zip_info(name: str, mode: int, compression: int) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, FIXED_TIMESTAMP)
    info.create_system = 3
    info.external_attr = mode << 16
    if stat.S_ISDIR(mode):
        info.external_attr |= 0x10
    info.compress_type = compression
    info.extra = b""
    info.comment = b""
    return info


def file_compression(path: Path, size: int) -> int:
    # Match the importer's per-entry expansion guard. Very repetitive data is
    # stored so a package created by this tool does not trip that guard.
    if size <= COPY_BYTES:
        return zipfile.ZIP_DEFLATED
    compressor = zlib.compressobj(wbits=-15)
    compressed = 0
    with path.open("rb") as source:
        while block := source.read(COPY_BYTES):
            compressed += len(compressor.compress(block))
    compressed += len(compressor.flush())
    return zipfile.ZIP_STORED if size > compressed * 200 + COPY_BYTES else zipfile.ZIP_DEFLATED


def write_package(args: argparse.Namespace) -> Path:
    if args.package_root.is_symlink():
        raise PackageError("package root must not be a symbolic link")
    root = args.package_root.resolve(strict=True)
    if not root.is_dir():
        raise PackageError("package root must be a directory")
    output = args.output.resolve()
    if output.is_relative_to(root):
        raise PackageError("place the output archive outside the payload directory")
    if output.suffix.lower() not in {".zip", ".afplugin"}:
        raise PackageError("output must end with .afplugin or .zip")
    if output.exists():
        raise PackageError("output already exists; choose a new path")
    metadata = package_metadata(args)
    entries = collect_payload(root)
    if not any(relative == args.executable_path and not directory for relative, _, directory, _ in entries):
        raise PackageError("executable path must name a regular file inside the package root")
    if len(metadata) + sum(size for _, _, directory, size in entries if not directory) > MAX_EXPANDED_BYTES:
        raise PackageError("package exceeds the 512 MiB expanded limit")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(prefix=".afplugin-", suffix=".tmp", dir=output.parent, delete=False) as temporary:
        temporary_path = Path(temporary.name)
    try:
        with zipfile.ZipFile(temporary_path, "w", allowZip64=True) as archive:
            archive.writestr(zip_info("anyflows-plugin.json", stat.S_IFREG | 0o644, zipfile.ZIP_STORED), metadata)
            archive.writestr(zip_info("payload/", stat.S_IFDIR | 0o755, zipfile.ZIP_STORED), b"")
            for relative, path, directory, _ in entries:
                name = f"payload/{relative}"
                if directory:
                    archive.writestr(zip_info(f"{name}/", stat.S_IFDIR | 0o755, zipfile.ZIP_STORED), b"")
                    continue
                mode = stat.S_IFREG | (0o755 if relative == args.executable_path else 0o644)
                info = zip_info(name, mode, file_compression(path, path.stat().st_size))
                with path.open("rb") as source, archive.open(info, "w") as target:
                    shutil.copyfileobj(source, target, COPY_BYTES)
        if temporary_path.stat().st_size > MAX_UPLOAD_BYTES:
            raise PackageError("archive exceeds the 128 MiB upload limit")
        # A same-directory hard link publishes the finished archive atomically
        # and fails if another process created the chosen output path meanwhile.
        os.link(temporary_path, output)
    finally:
        temporary_path.unlink(missing_ok=True)
    return output


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package_root", type=Path, help="clean directory containing only distributable plugin files")
    parser.add_argument("manifest_file", type=Path, help="public PluginManifest JSON file")
    parser.add_argument("executable_path", help="POSIX entrypoint path relative to package_root, without payload/")
    parser.add_argument("output", type=Path, help="new .afplugin or .zip output path outside package_root")
    parser.add_argument("--signer-key-id", help="optional existing publisher signer ID")
    parser.add_argument("--signature-file", type=Path, help="optional file containing an existing Base64 Ed25519 signature")
    args = parser.parse_args()
    try:
        output = write_package(args)
    except (OSError, UnicodeError, json.JSONDecodeError, PackageError) as error:
        parser.exit(1, f"package-plugin: {error}\n")
    print(f"Created {output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
