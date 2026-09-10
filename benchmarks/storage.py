"""Local run lifecycle and deliberately small, audited publications."""
import argparse
import contextlib
import fcntl
import hashlib
import json
from pathlib import Path
import shutil
import struct
import tempfile
import zlib

ROOT = Path(__file__).resolve().parent
RUNS = ROOT / '.local/runs'
RESULTS = ROOT / 'results'
MAX_BYTES = 20 * 1024 * 1024  # per publication; also enforced by CI


def require_local_output(path):
    if Path(path).resolve().is_relative_to(RESULTS.resolve()):
        raise ValueError('Only run.py promote writes benchmarks/results; choose a local output path')


def size(root):
    return sum(p.stat().st_size for p in root.rglob('*') if p.is_file())


def resolve_run(value):
    candidate = Path(value)
    path = candidate.resolve() if candidate.is_dir() else (RUNS / value).resolve()
    if not path.is_dir():
        raise ValueError(f'No run: {value}')
    return path


@contextlib.contextmanager
def locked(root):
    root.mkdir(parents=True, exist_ok=True)
    with (root / '.run.lock').open('a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError(f'Run is active: {root}')
        yield


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def png(source, destination):
    """Use the installed image loader, not OCR, to convert proof PPMs to PNG."""
    if source.suffix.lower() == '.png':
        shutil.copyfile(source, destination)
        return
    # Captures use this fixed P6 layout; no external imaging dependency in CI.
    raw = source.read_bytes()
    magic, dimensions, maximum, pixels = raw.split(b'\n', 3)
    width, height = map(int, dimensions.split())
    if magic != b'P6' or maximum != b'255' or len(pixels) != width * height * 3:
        raise ValueError(f'Unsupported proof image: {source}')
    def chunk(kind, data):
        return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))
    scanlines = b''.join(b'\0' + pixels[y * width * 3:(y + 1) * width * 3] for y in range(height))
    destination.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('!2I5B', width, height, 8, 2, 0, 0, 0))
                            + chunk(b'IDAT', zlib.compress(scanlines, 9)) + chunk(b'IEND', b''))


def compact(value):
    if isinstance(value, dict):
        return {key: compact(item) for key, item in value.items()
                if key not in ('frames', 'raw', 'samples', 'artifact', 'packed', 'captureManifest')}
    if isinstance(value, list):
        return [compact(item) for item in value]
    return value


def proof_paths(value):
    if isinstance(value, dict):
        for key, item in value.items():
            if key == 'evidence' and isinstance(item, str):
                yield item
            else:
                yield from proof_paths(item)
    elif isinstance(value, list):
        for item in value:
            yield from proof_paths(item)


def promote(source, name=None, force=False):
    from run import report
    source = resolve_run(str(source))
    name = name or source.name
    if name in ('.', '..') or Path(name).name != name:
        raise ValueError('Publication name must be a single directory name')
    target = RESULTS / name
    if target.exists():
        raise ValueError(f'Publication already exists: {target}')
    RESULTS.mkdir(parents=True, exist_ok=True)
    with locked(source), tempfile.TemporaryDirectory(prefix='.promote-', dir=RESULTS.parent) as temporary:
        stage = Path(temporary) / name
        stage.mkdir()
        original = json.loads((source / 'report.json').read_text())
        output = compact(original)
        problems = []
        measured = [r for r in original['rows'] if r.get('status') != 'planned' and not r.get('warmup')]
        if not measured:
            problems.append('run is empty or only planned')
        total = original.get('progress', {}).get('total', len(original.get('matrix', [])) or None)
        complete = original.get('completed') and (total is None or total == len(original['rows']))
        if not complete and not original.get('stoppedBy'):
            problems.append('incomplete run without stoppedBy')
        chosen = {}
        for row in sorted(measured, key=lambda r: (r.get('warmup', False), r.get('status') != 'ok')):
            key = tuple(row.get(k) for k in ('viewer', 'renderer', 'group', 'fixture', 'refreshHz', 'sessionKind'))
            if key in chosen:
                continue
            for value in proof_paths(row):
                path = Path(value)
                if not path.is_absolute():
                    path = source / path
                if not path.is_file():
                    continue
                if not path.resolve().is_relative_to(source):
                    raise ValueError(f'Proof outside local run: {path}')
                relative = f'proofs/{len(chosen):04d}.png'
                (stage / 'proofs').mkdir(exist_ok=True)
                png(path, stage / relative)
                chosen[key] = relative
                break
        # Strip local evidence references everywhere, then attach the one cell proof.
        def strip(value):
            if isinstance(value, dict):
                return {k: (None if k == 'evidence' and isinstance(v, str) else strip(v)) for k, v in value.items()}
            if isinstance(value, list):
                return [strip(v) for v in value]
            return value
        output = strip(output)
        for row in output['rows']:
            key = tuple(row.get(k) for k in ('viewer', 'renderer', 'group', 'fixture', 'refreshHz', 'sessionKind'))
            row['proofImage'] = chosen.get(key)
            if not isinstance(row.get('evidence'), dict):
                row['evidence'] = chosen.get(key)
        output['publication'] = {'sourceName': source.name, 'forced': force, 'limitBytes': MAX_BYTES}
        (stage / 'report.json').write_text(json.dumps(output, indent=2) + '\n')
        (stage / 'report.md').write_text(report(output))
        files = []
        for path in sorted(source.rglob('*')):
            if path.is_symlink():
                files.append({'path': str(path.relative_to(source)), 'symlink': str(path.readlink())})
            elif path.is_file() and path.name != '.run.lock':
                files.append({'path': str(path.relative_to(source)), 'bytes': path.stat().st_size, 'sha256': digest(path)})
        (stage / 'MANIFEST.json').write_text(json.dumps({'schemaVersion': 1, 'sourceName': source.name,
                                                       'localBytes': size(source), 'files': files}, indent=2) + '\n')
        amount = size(stage)
        print(f'Publication: {amount:,} bytes ({amount / 1048576:.2f} MiB); limit {MAX_BYTES:,} bytes')
        if amount > MAX_BYTES:
            problems.append('publication exceeds hard size limit (CI will reject it even with --force)')
        if problems and not force:
            raise ValueError('; '.join(problems) + '; use --force to override')
        if problems:
            print('Forced: ' + '; '.join(problems))
        stage.rename(target)
    print(target)
    return target


def check_results(root=None):
    root = root or RESULTS
    errors = [f'Forbidden raw directory: {path}' for path in root.rglob('raw') if path.is_dir()]
    for path in root.iterdir() if root.exists() else []:
        amount = size(path) if path.is_dir() else path.stat().st_size
        if amount > MAX_BYTES:
            errors.append(f'{path}: {amount:,} bytes exceeds {MAX_BYTES:,}')
    if errors:
        raise ValueError('\n'.join(errors))


def cli(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    publish = commands.add_parser('promote')
    publish.add_argument('run')
    publish.add_argument('--as', dest='name')
    publish.add_argument('--force', action='store_true')
    commands.add_parser('list')
    prune = commands.add_parser('prune', help='Dry run unless --delete is given; only explicitly named local runs')
    prune.add_argument('runs', nargs='+')
    prune.add_argument('--delete', action='store_true')
    commands.add_parser('check-results')
    args = parser.parse_args(argv)
    try:
        if args.command == 'promote':
            promote(args.run, args.name, args.force)
        elif args.command == 'check-results':
            check_results()
            print('Published results: no raw directories; sizes within limit')
        elif args.command == 'list':
            for path in sorted(RUNS.iterdir() if RUNS.exists() else []):
                if path.is_dir():
                    try:
                        data = json.loads((path / 'report.json').read_text())
                        progress = data.get('progress', {})
                        state = 'complete' if data.get('completed') else data.get('stoppedBy', 'incomplete')
                        print(f"{path.name}\t{size(path) / 1048576:.2f} MiB\t{progress.get('done', len(data.get('rows', [])))}/{progress.get('total', '?')}\t{state}")
                    except (OSError, ValueError):
                        print(f'{path.name}\t{size(path) / 1048576:.2f} MiB\tno readable report')
        elif args.command == 'prune':
            for value in args.runs:
                path = resolve_run(value)
                if path.parent != RUNS.resolve():
                    raise ValueError('prune only removes direct children of benchmarks/.local/runs')
                with locked(path):
                    print(f'{"Deleting" if args.delete else "Would delete"}: {path.name}: {size(path):,} bytes')
                    if args.delete:
                        shutil.rmtree(path)
    except (OSError, ValueError, KeyError) as error:
        parser.error(str(error))
    return 0
