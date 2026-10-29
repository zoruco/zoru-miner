#!/usr/bin/env python3
"""Check release archives for reproducibility and content binding."""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('zoru_bundle', Path(__file__).with_name('bundle.py'))
bundle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bundle)


class BundleTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / 'source'
        self.build = self.root / 'build'
        self.source.mkdir()
        self.build.mkdir()
        for name in ['LICENSE', 'README.md', 'SECURITY.md', 'CONTRIBUTING.md',
                     'licenses/dependency/LICENSE', 'docs/TESTING.md', 'packaging/install.py']:
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('Release fixture: ' + name + '\n')
        for name in ['zoru-miner-cli', 'zoru-miner-gui', 'libzoru_wallet.so']:
            path = self.build / name
            path.write_bytes(b'fixed executable fixture\n')
            path.chmod(0o755)
        self.manifest = self.root / 'manifest.json'
        self.token = '0x' + '11' * 20
        self.ledger = '0x' + '22' * 20
        self.write_manifest(fixture=True)

    def write_manifest(self, **extra):
        data = {'chainId': 6736, 'schema': 'zoru-mainnet-suite-1', 'mode': 'mainnet',
                'token': self.token, 'ledger': self.ledger, **extra}
        digest = hashlib.sha256(json.dumps(data, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        self.manifest.write_text(json.dumps({**data, 'manifestSha256': digest})+'\n')

    def package(self, parent, epoch='1793100000'):
        output = self.root / parent / 'zoru-miner-mainnet-linux-x86_64'
        args = ['bundle.py', '--output', str(output), '--build-dir', str(self.build),
                '--manifest', str(self.manifest), '--expected-token', self.token,
                '--expected-ledger', self.ledger]
        # These fixtures test the archiver, not ELF dependency discovery. Real
        # binaries undergo ldd and installed-application checks separately.
        with patch.object(bundle, 'ROOT', self.source), patch('sys.argv', args), \
                patch.dict(os.environ, {'SOURCE_DATE_EPOCH': epoch}), \
                patch.object(bundle.platform, 'system', return_value='Linux'), \
                patch.object(bundle.platform, 'machine', return_value='x86_64'), \
                patch.object(bundle.subprocess, 'run') as ldd, \
                contextlib.redirect_stdout(io.StringIO()):
            bundle.main()
            self.assertEqual(ldd.call_count, 3)
        return output, Path(str(output) + '.tar.gz')

    def test_identical_inputs_ignore_file_times_and_output_parent(self):
        _, first = self.package('first')
        for path in [*self.source.rglob('*'), *self.build.iterdir(), self.manifest]:
            os.utime(path, (1000000000, 1000000000))
        _, second = self.package('second')
        self.assertEqual(first.read_bytes(), second.read_bytes())

    def test_archive_has_fixed_metadata_and_executable_permissions(self):
        _, archive = self.package('metadata')
        self.assertEqual(int.from_bytes(archive.read_bytes()[4:8], 'little'), 1793100000)
        with tarfile.open(archive) as tar:
            for member in tar:
                self.assertEqual((member.uid, member.gid, member.uname, member.gname), (0, 0, '', ''))
                self.assertEqual(member.mtime, 1793100000)
                self.assertFalse(member.pax_headers)
                expected_mode = 0o755 if member.isdir() or '/bin/' in member.name else 0o644
                self.assertEqual(member.mode, expected_mode)

    def test_invalid_epoch_fails_before_creating_output(self):
        for i, epoch in enumerate(['-1', 'not-a-number', '4294967296']):
            with self.subTest(epoch=epoch):
                with self.assertRaises(ValueError):
                    self.package('invalid-' + str(i), epoch)
                self.assertFalse((self.root / ('invalid-' + str(i))).exists())

    def test_binary_change_changes_archive_and_checksum(self):
        output, first = self.package('original')
        original = json.loads((output / 'checksums.json').read_text())
        (self.build / 'zoru-miner-cli').write_bytes(b'different executable fixture\n')
        changed, second = self.package('changed-binary')
        checksums = json.loads((changed / 'checksums.json').read_text())
        self.assertNotEqual(original['bin/zoru-miner-cli'], checksums['bin/zoru-miner-cli'])
        self.assertNotEqual(first.read_bytes(), second.read_bytes())
        self.assertEqual(checksums['bin/zoru-miner-cli'],
                         hashlib.sha256((changed / 'bin/zoru-miner-cli').read_bytes()).hexdigest())

    def test_manifest_is_bound_to_package_content(self):
        output, first = self.package('original-manifest')
        original = json.loads((output / 'checksums.json').read_text())
        self.write_manifest(fixture=False)
        changed, second = self.package('changed-manifest')
        checksums = json.loads((changed / 'checksums.json').read_text())
        self.assertNotEqual(original['deployment.json'], checksums['deployment.json'])
        self.assertNotEqual(first.read_bytes(), second.read_bytes())

    def test_wrong_target_suite_rejected_before_packaging(self):
        self.token = '0x' + '33' * 20
        with self.assertRaisesRegex(ValueError, 'Manifest token does not match'):
            self.package('wrong-suite')
        self.assertFalse((self.root / 'wrong-suite').exists())

    def test_wrong_ledger_rejected_before_packaging(self):
        self.ledger = '0x' + '33' * 20
        with self.assertRaisesRegex(ValueError, 'Manifest ledger does not match'):
            self.package('wrong-ledger')
        self.assertFalse((self.root / 'wrong-ledger').exists())

    def test_tampered_manifest_checksum_rejected(self):
        data = json.loads(self.manifest.read_text())
        data['ledger'] = '0x' + '44' * 20
        self.ledger = data['ledger']
        self.manifest.write_text(json.dumps(data)+'\n')
        with self.assertRaisesRegex(ValueError, 'self-checksum mismatch'):
            self.package('tampered-suite')
        self.assertFalse((self.root / 'tampered-suite').exists())

    def test_wrong_chain_rejected(self):
        self.write_manifest(chainId=1)
        with self.assertRaisesRegex(ValueError, 'Only a verified ZORU mainnet'):
            self.package('wrong-chain')
        self.assertFalse((self.root / 'wrong-chain').exists())

    def test_local_manifest_rejected_for_mainnet_package(self):
        self.write_manifest(mode='local')
        with self.assertRaisesRegex(ValueError, 'Only a verified ZORU mainnet'):
            self.package('local-as-mainnet')
        self.assertFalse((self.root / 'local-as-mainnet').exists())


if __name__ == '__main__':
    unittest.main()
