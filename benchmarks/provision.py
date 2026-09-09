"""Pinned local source builds and package extraction, with complete build logs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parent
LOCAL = ROOT / '.provision'


def build_environment():
    env = dict(os.environ)
    deps = LOCAL / 'build-deps/root/usr'
    if deps.exists():
        env['PATH'] = str(deps / 'bin') + os.pathsep + env['PATH']
        env['LD_LIBRARY_PATH'] = str(deps / 'lib/x86_64-linux-gnu') + os.pathsep + env.get('LD_LIBRARY_PATH', '')
        env['PKG_CONFIG_PATH'] = str(deps / 'lib/x86_64-linux-gnu/pkgconfig') + os.pathsep + env.get('PKG_CONFIG_PATH', '')
    return env


def installed(spec):
    binary = spec.get('system_binary')
    if not binary or not Path(binary).is_file():
        return False
    result = subprocess.run(['dpkg-query', '-W', '-f=${Version}', spec['package']], capture_output=True, text=True)
    return result.returncode == 0 and result.stdout == spec['package_version']


def locate(spec):
    candidate = LOCAL / spec['directory'] / spec['binary']
    if candidate.is_file():
        return candidate
    if installed(spec):
        return Path(spec['system_binary'])
    return None


def provision(name, spec):
    LOCAL.mkdir(exist_ok=True)
    log_path = LOCAL / (name + '-provision.log')
    result = {'viewer': name, 'source': spec['source'], 'version': spec['version'],
              'revision': spec.get('revision'), 'log': str(log_path), 'status': 'missing'}
    try:
        directory = LOCAL / spec['directory']
        with log_path.open('w') as log:
            def run(command, cwd=LOCAL):
                log.write(json.dumps(command) + '\n'); log.flush()
                subprocess.run(command, cwd=cwd, env=build_environment(), stdout=log,
                               stderr=subprocess.STDOUT, check=True, timeout=3600)
            if installed(spec):
                result['method'] = 'Exact installed package version'
                for package in spec.get('extra_packages', []):
                    pkg, version = package.split('=', 1)
                    check = subprocess.run(['dpkg-query', '-W', '-f=${Version}', pkg], capture_output=True, text=True)
                    if check.returncode or check.stdout != version:
                        raise RuntimeError(f'Missing required backend: {package}')
            elif spec.get('revision'):
                if not directory.exists():
                    run(['git', 'clone', '--no-checkout', spec['source'], str(directory)])
                head = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=directory, capture_output=True, text=True).stdout.strip()
                if head != spec['revision']:
                    run(['git', 'fetch', 'origin', spec['revision']], directory)
                    run(['git', 'checkout', '--detach', spec['revision']], directory)
                run(['git', 'submodule', 'update', '--init', '--recursive'], directory)
                for command in spec['build']:
                    run(command, directory)
                result['method'] = 'Locked source build'
            else:
                directory.mkdir(exist_ok=True)
                run(['apt-get', 'download', spec['package'] + '=' + spec['package_version'],
                     *spec.get('extra_packages', [])], directory)
                for package in directory.glob('*.deb'):
                    run(['dpkg-deb', '-x', str(package), str(directory)], directory)
                result['method'] = 'Exact package extraction; runtime dependencies must be installed'
            binary = locate(spec)
            if binary is None:
                raise RuntimeError('Build did not produce the configured executable')
            linkage = subprocess.run(['ldd', str(binary)], capture_output=True, text=True)
            if 'not found' in linkage.stdout:
                raise RuntimeError('Missing runtime libraries: ' + linkage.stdout)
            result.update(status='ok', binary=str(binary), sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        result['reason'] = str(error)
    (LOCAL / (name + '-provision.json')).write_text(json.dumps(result, indent=2) + '\n')
    return result


def tool(name, spec):
    """Measurement tools live beside the viewers and are pinned the same way."""
    directory = LOCAL / spec['directory']
    root = directory / 'root'
    log_path = LOCAL / (name + '-tool.log')
    result = {'tool': name, 'packages': spec['packages'], 'log': str(log_path), 'status': 'missing',
              'method': 'Exact package extraction; nothing is installed into the system'}
    try:
        directory.mkdir(parents=True, exist_ok=True)
        with log_path.open('w') as log:
            def run(command, cwd=directory):
                log.write(json.dumps(command) + '\n'); log.flush()
                subprocess.run(command, cwd=cwd, env=build_environment(), stdout=log,
                               stderr=subprocess.STDOUT, check=True, timeout=3600)
            run(['apt-get', 'download', *spec['packages']])
            for package in sorted(directory.glob('*.deb')):
                run(['dpkg-deb', '-x', str(package), str(root)])
        binary = root / spec['binary']
        if not binary.is_file():
            raise RuntimeError(f'Extraction did not produce {binary}')
        linkage = subprocess.run(['ldd', str(binary)], capture_output=True, text=True)
        if 'not found' in linkage.stdout:
            raise RuntimeError('Missing runtime libraries: ' + linkage.stdout)
        result.update(status='ok', binary=str(binary),
                      sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        result['reason'] = str(error)
    (LOCAL / (name + '-tool.json')).write_text(json.dumps(result, indent=2) + '\n')
    return result


def catalog():
    return tomllib.loads((ROOT / 'competitors.toml').read_text())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tools', action='store_true', help='Provision the pinned measurement tools (OCR)')
    parser.add_argument('--viewers', help='Comma-separated viewer names from competitors.toml')
    args = parser.parse_args()
    if not args.tools and not args.viewers:
        parser.error('Choose --tools, --viewers, or both')
    results = []
    if args.tools:
        results += [tool(name, spec) for name, spec in catalog().get('tools', {}).items()]
    for name in (args.viewers or '').split(',') if args.viewers else []:
        viewers = catalog()['viewers']
        if name not in viewers:
            parser.error(f'Unknown viewer {name}; choose from: {", ".join(viewers)}')
        results.append(provision(name, viewers[name]))
    for result in results:
        print(result.get('tool', result.get('viewer')), result['status'], result.get('reason', ''))
    return 0 if all(result['status'] == 'ok' for result in results) else 1


if __name__ == '__main__':
    sys.exit(main())
