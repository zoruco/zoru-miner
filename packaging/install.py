#!/usr/bin/env python3
"""Verify and copy a mainnet package into a fresh user-selected directory."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import shlex
import shutil


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--prefix', required=True)
    args = parser.parse_args()
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        raise ValueError('This package requires Linux x86_64')
    source = Path(__file__).resolve().parent
    dest = Path(args.prefix).resolve()
    if dest.exists():
        raise ValueError('Choose a fresh installation directory')
    checksums = json.loads((source/'checksums.json').read_text())
    actual = {str(p.relative_to(source)) for p in source.rglob('*') if p.is_file() and p.name != 'checksums.json'}
    if actual != set(checksums) or any(p.is_symlink() for p in source.rglob('*')):
        raise ValueError('Unexpected package files')
    for relative, expected in checksums.items():
        path = (source/relative).resolve()
        if not path.is_relative_to(source) or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError('Package checksum mismatch')
    shutil.copytree(source, dest)
    config = dest/'connection.json'
    config.write_text(json.dumps({'manifest': str(dest/'deployment.json')}, indent=2)+'\n')
    launcher = dest/'bin/zoru-miner-desktop'
    launcher.write_text('#!/bin/sh\nexec '+shlex.quote(str(dest/'bin/zoru-miner-gui'))+' --config '+shlex.quote(str(config))+' "$@"\n')
    launcher.chmod(0o755)
    print(json.dumps({'status': 'PASS', 'prefix': str(dest)}))


if __name__ == '__main__':
    main()
