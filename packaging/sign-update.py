#!/usr/bin/env python3
"""Create signed update metadata for locally built official release assets."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--key', type=Path, required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--artifact', type=Path, action='append', required=True)
    parser.add_argument('--notes', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=Path('release/update.json'))
    args = parser.parse_args()
    if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', args.version):
        raise ValueError('Use a stable three-part version.')
    key_stat = args.key.lstat()
    if not stat.S_ISREG(key_stat.st_mode) or key_stat.st_mode & 0o077:
        raise ValueError('The signing key must be a private regular file (0600).')
    public = subprocess.check_output(['openssl', 'pkey', '-in', str(args.key), '-pubout', '-outform', 'DER'])
    expected = (Path(__file__).resolve().parents[1] / 'config/update-public-key.hex').read_text().strip()
    if public[:12].hex() != '302a300506032b6570032100' or public[12:].hex() != expected:
        raise ValueError('The signing key does not match the application verification key.')
    notes = args.notes.read_text()
    if len(notes.encode()) > 8192:
        raise ValueError('Release notes exceed the limit.')
    artifacts = []
    for path in args.artifact:
        if path.name == f'ZORU-Miner-{args.version}-Linux-amd64.deb':
            target = 'linux-amd64'
            fields = subprocess.check_output(['dpkg-deb', '--field', str(path), 'Package', 'Version', 'Architecture'], text=True)
            values = dict(line.split(': ', 1) for line in fields.splitlines())
            if values != {'Package': 'zoru-miner', 'Version': args.version, 'Architecture': 'amd64'}:
                raise ValueError('Package identity differs from the release.')
        elif path.name == f'ZORU-Miner-{args.version}-Windows-x64.exe':
            target = 'windows-x64'
        else:
            raise ValueError('The artifact name does not match this release.')
        size = path.stat().st_size
        if not 0 < size <= 536870912:
            raise ValueError('The release artifact exceeds the size limit.')
        digest = hashlib.sha256()
        with path.open('rb') as file:
            for chunk in iter(lambda: file.read(65536), b''):
                digest.update(chunk)
        artifacts.append({'target': target, 'fileName': path.name,
                          'url': f'https://github.com/zoruco/zoru-miner/releases/download/v{args.version}/{path.name}',
                          'size': size, 'sha256': digest.hexdigest()})
    if len({a['target'] for a in artifacts}) != len(artifacts):
        raise ValueError('A release may have only one artifact per target.')
    now = datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0)
    payload = json.dumps({'schemaVersion': 1, 'product': 'zoru-miner', 'channel': 'stable',
                          'version': args.version, 'publishedAt': now.isoformat(),
                          'expiresAt': (now + datetime.timedelta(days=30)).isoformat(),
                          'notes': notes, 'artifacts': artifacts}, separators=(',', ':'), ensure_ascii=False).encode()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.sign-update-', dir=args.output.parent) as directory:
        message = Path(directory) / 'payload'; signature = Path(directory) / 'signature'
        message.write_bytes(payload)
        subprocess.run(['openssl', 'pkeyutl', '-sign', '-rawin', '-inkey', str(args.key),
                        '-in', str(message), '-out', str(signature)], check=True)
        envelope = {'signed': payload.hex(), 'signature': signature.read_bytes().hex()}
    args.output.write_text(json.dumps(envelope, indent=2) + '\n')
    print(f'Signed update metadata: {args.output}')


if __name__ == '__main__':
    os.umask(0o077)
    if os.name == 'posix':
        import resource
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    main()
