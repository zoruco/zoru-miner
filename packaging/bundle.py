#!/usr/bin/env python3
"""Package the native Linux binaries with checksums and license notices."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', required=True)
    parser.add_argument('--build-dir', required=True)
    parser.add_argument('--manifest', required=True)
    parser.add_argument('--expected-token', required=True, help='Reviewed token address for this release')
    parser.add_argument('--expected-ledger', required=True, help='Reviewed ledger proxy address for this release')
    args = parser.parse_args()
    for expected in (args.expected_token, args.expected_ledger):
        if not re.fullmatch(r'0x[0-9a-fA-F]{40}', expected) or int(expected, 16) == 0:
            raise ValueError('Expected token and ledger must be nonzero EVM addresses')
    manifest = json.loads(Path(args.manifest).read_text())
    if (not isinstance(manifest, dict) or manifest.get('chainId') != 6736
            or manifest.get('schema') != 'zoru-mainnet-suite-1' or manifest.get('mode') != 'mainnet'):
        raise ValueError('Only a verified ZORU mainnet manifest is packageable')
    for role, expected in (('token', args.expected_token), ('ledger', args.expected_ledger)):
        actual = manifest.get(role)
        if not isinstance(actual, str) or actual.lower() != expected.lower():
            raise ValueError(f'Manifest {role} does not match the intended release')
    declared_hash = manifest.pop('manifestSha256', None)
    actual_hash = hashlib.sha256(json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    if not isinstance(declared_hash, str) or actual_hash != declared_hash.lower():
        raise ValueError('Manifest self-checksum mismatch')
    epoch = int(os.environ.get('SOURCE_DATE_EPOCH', '0'))
    if not 0 <= epoch <= 0xffffffff:
        raise ValueError('SOURCE_DATE_EPOCH must be an unsigned 32-bit integer')
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        raise ValueError('This package target is Linux x86_64')
    out = Path(args.output).resolve()
    if out.exists():
        raise ValueError('Choose a fresh output directory')
    build = Path(args.build_dir).resolve()
    binaries = ['zoru-miner-cli', 'zoru-miner-gui']
    for name in binaries:
        subprocess.run(['ldd', str(build/name)], check=True, stdout=subprocess.DEVNULL)
    out.mkdir(parents=True)
    (out/'bin').mkdir()
    for name in binaries:
        shutil.copy2(build/name, out/'bin'/name)
    for name in ['LICENSE', 'README.md', 'SECURITY.md', 'CONTRIBUTING.md']:
        shutil.copy2(ROOT/name, out/name)
    shutil.copytree(ROOT/'licenses', out/'licenses')
    shutil.copytree(ROOT/'docs', out/'docs')
    shutil.copy2(ROOT/'packaging/install.py', out/'install.py')
    shutil.copy2(args.manifest, out/'deployment.json')
    checksums = {str(p.relative_to(out)): hashlib.sha256(p.read_bytes()).hexdigest()
                 for p in sorted(out.rglob('*')) if p.is_file()}
    (out/'checksums.json').write_text(json.dumps(checksums, indent=2)+'\n')
    archive = Path(str(out) + '.tar.gz')
    def metadata(member):
        member.uid = member.gid = 0
        member.uname = member.gname = ''
        member.mtime = epoch
        member.mode = 0o755 if member.isdir() or member.mode & 0o111 else 0o644
        member.pax_headers = {}
        return member

    with archive.open('wb') as raw:
        with gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=epoch) as compressed:
            with tarfile.open(fileobj=compressed, mode='w', format=tarfile.PAX_FORMAT) as tar:
                tar.add(out, arcname=out.name, filter=metadata)
    print(json.dumps({'archive': str(archive), 'sha256': hashlib.sha256(archive.read_bytes()).hexdigest(),
                      'files': len(checksums), 'platform': 'Linux x86_64, Qt >=6.2 system libraries', 'mainnetOnly': True}))


if __name__ == '__main__':
    main()
