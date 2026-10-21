#!/usr/bin/env python3
"""Collect licenses and corresponding Ubuntu source references for bundled tools."""
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.parse

appdir, paths = map(Path, sys.argv[1:])
packages = {'xkb-data', 'poppler-data', 'poppler-utils', 'wl-clipboard'}
for name in set(paths.read_text().splitlines()):
    path = Path(name)
    candidates = [str(path), str(path.resolve())]
    if name.startswith('/lib/'):
        candidates.append('/usr' + name)
    result = subprocess.run(['dpkg-query', '-S', *candidates], capture_output=True, text=True)
    for line in result.stdout.splitlines():
        if ': /' in line:
            packages.add(line.split(': /', 1)[0])

destination = appdir / 'usr/share/licenses/pinepaper/third-party'
destination.mkdir(parents=True, exist_ok=True)
inventory = ['Binary package\tBinary version\tSource package\tSource version\tSource URL']
for package in sorted(packages):
    fields = subprocess.check_output([
        'dpkg-query', '-W', '-f=${binary:Package}\t${Version}\t${source:Package}\t${source:Version}',
        package], text=True).split('\t')
    copyright_file = Path('/usr/share/doc') / package.split(':', 1)[0] / 'copyright'
    if not copyright_file.is_file():
        raise RuntimeError(f'Missing redistribution notice: {copyright_file}')
    shutil.copyfile(copyright_file, destination / f'{package}.copyright')
    source_url = 'https://launchpad.net/ubuntu/+source/' + '/'.join(
        urllib.parse.quote(field, safe='') for field in fields[2:])
    inventory.append('\t'.join([*fields, source_url]))
for license_file in Path('/usr/share/common-licenses').iterdir():
    if license_file.is_file():
        shutil.copyfile(license_file, destination / license_file.name)
(destination / 'SOURCES.tsv').write_text('\n'.join(inventory) + '\n')
