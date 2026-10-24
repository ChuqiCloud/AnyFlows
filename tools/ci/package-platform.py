#!/usr/bin/env python3
"""Package a native release without requiring GNU tar on Windows or macOS."""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import time
import zipfile

PLATFORMS = {
    'x86_64-unknown-linux-gnu': 'linux-amd64',
    'aarch64-unknown-linux-gnu': 'linux-arm64',
    'x86_64-unknown-linux-musl': 'linux-amd64-musl',
    'aarch64-unknown-linux-musl': 'linux-arm64-musl',
    'x86_64-pc-windows-msvc': 'windows-amd64',
    'aarch64-pc-windows-msvc': 'windows-arm64',
    'x86_64-apple-darwin': 'macos-amd64',
    'aarch64-apple-darwin': 'macos-arm64',
}


def package(root, tag, target, binary, output, epoch, launcher):
    if not re.fullmatch(r'v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(?:alpha|beta|rc)\.[1-9]\d*)?', tag):
        raise ValueError('Invalid release tag')
    if target not in PLATFORMS:
        raise ValueError('Unsupported target')
    windows = 'windows' in target
    platform = PLATFORMS[target]
    name = f'anyflows-{tag}-{platform}'
    executable = 'anyflows.exe' if windows else 'anyflows'
    output.mkdir(parents=True, exist_ok=True)
    launcher_name = 'anyflows-launcher.exe' if windows else 'anyflows-launcher'
    files = {executable: (binary.read_bytes(), 0o755), launcher_name: (launcher.read_bytes(), 0o755)}
    start = 'start.cmd' if windows else 'start.sh'
    files[start] = ((root / 'scripts' / start).read_bytes(), 0o755)
    for path in ['README.md', 'LICENSE', '.env.example', 'anyflows.conf', 'CHANGELOG.md', 'docs/online-updates.md']:
        files[Path(path).name] = ((root / path).read_bytes(), 0o644)
    if 'linux' in target:
        for path in ['scripts/systemd/install.sh', 'scripts/systemd/anyflows.service']:
            files[Path(path).name] = ((root / path).read_bytes(), 0o755 if path.endswith('.sh') else 0o644)
    commit = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
    files['VERSION'] = (f'tag={tag}\ncommit={commit}\n'.encode(), 0o644)
    files['build-info.json'] = (json.dumps({'tag': tag, 'target': target, 'commit': commit}, sort_keys=True).encode(), 0o644)
    archive = output / (name + ('.zip' if windows else '.tar.gz'))
    if windows:
        timestamp = time.gmtime(max(epoch, 315532800))[:6]
        with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED) as stream:
            for path, (data, mode) in sorted(files.items()):
                info = zipfile.ZipInfo(f'{name}/{path}', timestamp)
                info.external_attr = (0o100000 | mode) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                stream.writestr(info, data)
    else:
        with archive.open('wb') as destination, gzip.GzipFile(filename='', fileobj=destination, mode='wb', mtime=epoch) as compressed, tarfile.open(fileobj=compressed, mode='w') as stream:
            for path, (data, mode) in sorted(files.items()):
                info = tarfile.TarInfo(f'{name}/{path}')
                info.size, info.mode, info.mtime = len(data), mode, epoch
                stream.addfile(info, io.BytesIO(data))
    raw = output / (name + ('.exe' if windows else ''))
    shutil.copyfile(binary, raw)
    raw.chmod(0o755)
    for path in [archive, raw]:
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        path.with_name(path.name + '.sha256').write_text(f'{digest}  {path.name}\n', encoding='utf-8', newline='\n')
    return archive, raw


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('tag')
    parser.add_argument('target', choices=PLATFORMS)
    parser.add_argument('binary', type=Path)
    parser.add_argument('launcher', type=Path)
    parser.add_argument('--output', type=Path, default=Path('release'))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    metadata = json.loads(subprocess.check_output([str(args.binary.resolve()), '--version'], text=True))
    if metadata['target'] != args.target or 'v' + metadata['version'] != args.tag:
        raise ValueError('Binary target/version differs from release inputs')
    launcher = json.loads(subprocess.check_output([str(args.launcher.resolve()), '--version'], text=True))
    if launcher.get('target') != args.target or metadata.get('launcher_protocol') != launcher.get('protocol'):
        raise ValueError('Launcher target/protocol differs from main program')
    epoch = int(subprocess.check_output(['git', '-C', str(root), 'show', '-s', '--format=%ct', 'HEAD'], text=True))
    package(root, args.tag, args.target, args.binary, args.output, epoch, args.launcher)


if __name__ == '__main__':
    main()
