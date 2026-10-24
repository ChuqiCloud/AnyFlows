import importlib.util
from pathlib import Path
import hashlib
import json
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location('package_platform', Path(__file__).with_name('package-platform.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PackagePlatformTests(unittest.TestCase):
    def test_all_platforms_include_matching_binaries_and_verifiable_digests(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(module.subprocess, 'check_output', return_value='a' * 40):
            directory = Path(temporary)
            root = directory / 'source'
            for name in ['README.md', 'LICENSE', '.env.example', 'anyflows.conf', 'CHANGELOG.md', 'docs/online-updates.md', 'scripts/systemd/install.sh', 'scripts/systemd/anyflows.service', 'scripts/start.sh', 'scripts/start.cmd']:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('Public release fixture: ' + name, encoding='utf-8')
            binary = directory / 'fixture'
            binary.write_bytes(b'native-binary-fixture')
            launcher = directory / 'launcher'
            launcher.write_bytes(b'stable-launcher-fixture')
            for target in module.PLATFORMS:
                with self.subTest(target=target):
                    output = directory / target
                    archive, raw = module.package(root, 'v1.2.3', target, binary, output, 0, launcher)
                    self.assertEqual(raw.read_bytes(), binary.read_bytes())
                    for path in [archive, raw]:
                        digest, name = path.with_name(path.name + '.sha256').read_text().split()
                        self.assertEqual(name, path.name)
                        self.assertEqual(digest, hashlib.sha256(path.read_bytes()).hexdigest())
                    if 'windows' in target:
                        with zipfile.ZipFile(archive) as stream:
                            names = stream.namelist()
                            self.assertTrue(any(name.endswith('/anyflows.exe') for name in names))
                            info = next(name for name in names if name.endswith('/build-info.json'))
                            self.assertEqual(json.loads(stream.read(info))['target'], target)
                    else:
                        with tarfile.open(archive) as stream:
                            names = stream.getnames()
                            self.assertTrue(any(name.endswith('/anyflows') for name in names))
                            info = next(name for name in names if name.endswith('/build-info.json'))
                            self.assertEqual(json.load(stream.extractfile(info))['target'], target)
                    self.assertEqual(any(name.endswith('/install.sh') for name in names), 'linux' in target)
                    self.assertTrue(any(name.endswith('/LICENSE') for name in names))
                    self.assertTrue(any(name.endswith('/anyflows-launcher.exe' if 'windows' in target else '/anyflows-launcher') for name in names))
                    self.assertTrue(any(name.endswith('/start.cmd' if 'windows' in target else '/start.sh') for name in names))
                    before = archive.read_bytes()
                    module.package(root, 'v1.2.3', target, binary, output, 0, launcher)
                    self.assertEqual(archive.read_bytes(), before)

    def test_tag_and_target_cannot_inject_paths(self):
        with self.assertRaises(ValueError):
            module.package(Path('.'), '../escape', 'x86_64-unknown-linux-gnu', Path('missing'), Path('.'), 0, Path('missing'))
        with self.assertRaises(ValueError):
            module.package(Path('.'), 'v1.2.3', 'unknown', Path('missing'), Path('.'), 0, Path('missing'))


if __name__ == '__main__':
    unittest.main()
