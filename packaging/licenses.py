#!/usr/bin/env python3
"""Collect dependency notices from Cargo's locally verified registry sources."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import urllib.request
import urllib.parse

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--target', default='x86_64-unknown-linux-gnu')
    parser.add_argument('--fetch-missing', action='store_true')
    args = parser.parse_args()
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1', '--filter-platform', args.target,
                                                    '--manifest-path', str(ROOT/'miner/Cargo.toml')], text=True))
    out = ROOT/'licenses/dependencies'
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    records = []
    request_cache = {}
    for package in metadata['packages']:
        if package['source'] is None:
            continue
        source = Path(package['manifest_path']).parent
        target = out/(package['name']+'-'+package['version'])
        files = [p for p in source.iterdir() if p.is_file() and (p.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE', 'UNLICENSE')))]
        if package.get('license_file'):
            files.append(source/package['license_file'])
        files = sorted(set(files))
        target.mkdir()
        for path in files:
            shutil.copy2(path, target/path.name)
        upstream = []
        if not files and args.fetch_missing:
            vcs = source/'.cargo_vcs_info.json'
            repository = urllib.parse.urlsplit(package.get('repository') or '')
            if repository.hostname == 'github.com' and vcs.exists():
                info = json.loads(vcs.read_text())
                repo = '/'.join(repository.path.strip('/').removesuffix('.git').split('/')[:2])
                commit = info['git']['sha1']
                prefix = info.get('path_in_vcs', '').strip('/')
                candidates = ['LICENSE', 'LICENSE-MIT', 'LICENSE-APACHE', 'COPYING', 'UNLICENSE']
                for name in candidates:
                    for relative in [name, prefix+'/'+name]:
                        url = f'https://raw.githubusercontent.com/{repo}/{commit}/{relative}'
                        try:
                            if url not in request_cache:
                                try:
                                    with urllib.request.urlopen(url, timeout=3) as response:
                                        request_cache[url] = response.read(262145)
                                except Exception:
                                    request_cache[url] = None
                            content = request_cache[url]
                            if content is None:
                                continue
                            if len(content) > 262144:
                                raise ValueError('License notice too large')
                        except Exception:
                            continue
                        (target/name).write_bytes(content)
                        upstream.append({'file': name, 'source': url})
                        break
        records.append({'name': package['name'], 'version': package['version'], 'license': package.get('license'),
                        'repository': package.get('repository'), 'notices': [str(p.relative_to(ROOT/'licenses')) for p in sorted(target.iterdir())],
                        'upstreamNotices': upstream})
    (ROOT/'licenses/dependencies.json').write_text(json.dumps(records, indent=2)+'\n')
    print(json.dumps({'packages': len(records), 'withoutLocalNotice': [p['name'] for p in records if not p['notices']]}))


if __name__ == '__main__':
    main()
