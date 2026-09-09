#!/usr/bin/env python3
"""Generate historical fixtures with Rust and verify every byte against the old manifest."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
MANIFEST = ROOT / 'fixtures/metadata.json'


def verify(directory):
    count = 0
    for fixture in json.loads(MANIFEST.read_text()):
        for record in [dict(fixture, filename=fixture['name'] + '.md'), *fixture.get('images', [])]:
            path = directory / record['filename']
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            if digest != record['sha256']:
                raise ValueError(f'{path}: SHA-256 mismatch: {digest}')
            count += 1
    if (directory / 'metadata.json').read_bytes() != MANIFEST.read_bytes():
        raise ValueError('Historical metadata changed')
    return count


def generate(directory):
    # Generate into a sibling first. A failed hash check never replaces fixtures.
    directory.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='hashline-fixtures-') as temp:
        binary = str(Path(temp) / 'generate')
        staging = Path(temp) / 'fixtures'
        subprocess.run(['rustc', '--edition=2021', '-O', str(ROOT / 'generate.rs'), '-o', binary], check=True)
        subprocess.run([binary, str(staging)], check=True)
        verify(staging)
        import shutil
        for source in staging.iterdir():
            shutil.copyfile(source, directory / source.name)


def ensure(directory):
    manifest = json.loads(MANIFEST.read_text())
    expected = ['metadata.json']
    for fixture in manifest:
        expected += [fixture['name'] + '.md', *[image['filename'] for image in fixture.get('images', [])]]
    if any(not (directory / name).is_file() for name in expected):
        generate(directory)
    return verify(directory)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--out', type=Path, default=ROOT / 'generated')
    args = parser.parse_args()
    if not args.check:
        generate(args.out)
    print(f'{verify(args.out)} historical files verified (SHA-256), plus metadata')


if __name__ == '__main__':
    main()
