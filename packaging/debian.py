#!/usr/bin/env python3
"""Build a native Debian/Ubuntu package from a checked ZORU Miner Linux bundle."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--bundle-dir', type=Path, required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    source = args.bundle_dir.resolve()
    if not source.is_dir():
        raise ValueError('Bundle directory not found')
    checks = json.loads((source / 'checksums.json').read_text())
    names = {str(p.relative_to(source)) for p in source.rglob('*') if p.is_file() and p.name != 'checksums.json'}
    if names != set(checks) or any(p.is_symlink() for p in source.rglob('*')):
        raise ValueError('Unexpected bundle content')
    for name, checksum in checks.items():
        p = (source / name).resolve()
        if not p.is_relative_to(source) or hashlib.sha256(p.read_bytes()).hexdigest() != checksum:
            raise ValueError('Bundle checksum mismatch')
    manifest = json.loads((source / 'deployment.json').read_text())
    if (manifest.get('chainId') != 6736 or manifest.get('mode') != 'mainnet'
            or manifest.get('schema') != 'zoru-mainnet-suite-1'):
        raise ValueError('Mainnet deployment manifest required')
    if not args.version or any(c not in '0123456789.' for c in args.version):
        raise ValueError('Invalid release version')
    output = args.output.resolve()
    if output.exists():
        raise ValueError('Refusing to overwrite existing package')
    with tempfile.TemporaryDirectory(prefix='zoru-deb-') as tmp:
        stage = Path(tmp) / 'package'
        target = stage / 'opt/zoru-miner'
        shutil.copytree(source, target)
        bindir = stage / 'usr/bin'
        bindir.mkdir(parents=True)
        for binary in ('zoru-miner-cli', 'zoru-miner-gui'):
            if not (target / 'bin' / binary).is_file():
                raise ValueError(f'Missing binary: {binary}')
            (bindir / binary).symlink_to('/opt/zoru-miner/bin/' + binary)
        desktop = stage / 'usr/share/applications/zoru-miner.desktop'
        desktop.parent.mkdir(parents=True)
        desktop.write_text(
            '[Desktop Entry]\n'
            'Type=Application\n'
            'Name=ZORU Miner\n'
            'Comment=ZORU PoW miner for VEX EVM mainnet\n'
            'Exec=zoru-miner-gui\n'
            'Icon=zoru-miner\n'
            'Terminal=false\n'
            'Categories=Utility;\n'
        )
        icons = stage / 'usr/share/icons/hicolor/128x128/apps'
        icons.mkdir(parents=True)
        shutil.copy2(ROOT / 'assets/zoru-panda.png', icons / 'zoru-miner.png')
        debian = stage / 'DEBIAN'
        debian.mkdir()
        (debian / 'control').write_text(
            'Package: zoru-miner\n'
            f'Version: {args.version}\n'
            'Section: utils\n'
            'Priority: optional\n'
            'Architecture: amd64\n'
            'Maintainer: ZORU Labs <zorucore@gmail.com>\n'
            'Depends: libc6 (>= 2.34), libstdc++6, libqt6core6 (>= 6.2), '
            'libqt6gui6 (>= 6.2), libqt6widgets6 (>= 6.2), '
            'libqt6qml6 (>= 6.2), libqt6quick6 (>= 6.2), '
            'libqt6quickcontrols2-6 (>= 6.2), qt6-qpa-plugins, '
            'qml6-module-qtquick, qml6-module-qtquick-controls, '
            'qml6-module-qtquick-dialogs, qml6-module-qtquick-layouts, '
            'qml6-module-qtquick-templates, qml6-module-qtquick-window, '
            'qml6-module-qtqml-workerscript\n'
            'Description: ZORU Miner for VEX EVM mainnet\n'
            ' Rust CLI and Qt GUI with verified deployment manifest.\n'
        )
        subprocess.run(['dpkg-deb', '--build', '--root-owner-group', str(stage), str(output)], check=True)
    print(json.dumps({'package': str(output), 'sha256': hashlib.sha256(output.read_bytes()).hexdigest()}))


if __name__ == '__main__':
    main()
