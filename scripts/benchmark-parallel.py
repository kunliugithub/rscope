"""Linux benchmark (requires cc): repeated builds, normalized DB/query equality.

The manifest uses rscope's shell quoting; relative paths resolve from the current
working directory. Inputs must be readable UTF-8. Source hashes are verified
before/after the entire run. Source reads and validation warm the cache; this is
not a cold-cache benchmark. Generated artifacts remain under --output.
"""
import argparse
import hashlib
import json
import os
import platform
import random
import shlex
import sqlite3
import statistics
import subprocess
import time
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def measured(root, label, command):
    started = time.perf_counter()
    metrics = root / (label + '.resources.json')
    # The small native helper forks after exec, avoiding Python's RSS floor.
    with (root / (label + '.stdout')).open('wb') as out, (root / (label + '.stderr')).open('wb') as err:
        process = subprocess.run([
            str(root / 'measure-command'), str(metrics), *command,
        ], stdout=out, stderr=err)
    elapsed = time.perf_counter() - started
    if process.returncode != 0:
        raise RuntimeError(f'{label}: status {process.returncode}: {(root / (label + ".stderr")).read_text()}')
    result = json.loads(metrics.read_text())
    result['wall_s'] = elapsed
    for line in (root / (label + '.stderr')).read_text().splitlines():
        if line.startswith('rscope-profile '):
            result['profile'] = {k: float(v) for k, v in (part.split('=') for part in line.split()[1:])}
    return result


def snapshot(database):
    queries = {
        'files': 'SELECT path,byte_len,modified_ns FROM files ORDER BY path',
        'occurrences': '''SELECT f.path,o.name,o.kind,o.caller,o.line,o.column_number
            FROM occurrences o JOIN files f ON f.id=o.file_id
            ORDER BY f.path,o.name,o.kind,o.caller,o.line,o.column_number''',
        'diagnostics': '''SELECT f.path,d.line,d.column_number,d.end_line,d.end_column,d.kind
            FROM parse_diagnostics d JOIN files f ON f.id=d.file_id
            ORDER BY f.path,d.line,d.column_number,d.end_line,d.end_column,d.kind''',
        'metadata': 'SELECT key,value FROM metadata ORDER BY key',
    }
    with sqlite3.connect(database) as conn:
        if conn.execute('PRAGMA integrity_check').fetchone()[0] != 'ok':
            raise RuntimeError('database integrity failed')
        result = {}
        for name, query in queries.items():
            sha = hashlib.sha256()
            count = 0
            for row in conn.execute(query):
                sha.update((json.dumps(row, ensure_ascii=True) + '\n').encode())
                count += 1
            result[name] = dict(rows=count, sha256=sha.hexdigest())
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--baseline', type=Path)
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--jobs', default='1,2,4,8')
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--query-repeats', type=int, default=5)
    parser.add_argument('--symbol', default='main')
    parser.add_argument('--allow-parse-errors', action='store_true')
    args = parser.parse_args()
    jobs = [int(n) for n in args.jobs.split(',')]
    if min(jobs) < 1 or args.repeats < 1 or args.query_repeats < 1:
        parser.error('jobs and repeats must be positive')
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    subprocess.run(['cc', '-O2', '-Wall', '-Wextra', '-Werror',
                    str(Path(__file__).with_name('measure-command.c').resolve()),
                    '-o', str(root / 'measure-command')], check=True)
    files = sorted({Path(p).resolve() for p in shlex.split(args.manifest.read_text())})
    if not files:
        parser.error('empty manifest')
    hashes = {str(p): digest(p) for p in files}
    manifest = root / 'rscope.files'
    manifest.write_text('\n'.join(shlex.quote(str(p)) for p in files) + '\n')
    binaries = {f'jobs{n}': (str(args.binary.resolve()), n) for n in jobs}
    if args.baseline:
        binaries['before'] = (str(args.baseline.resolve()), None)
    summary = dict(host=platform.platform(), cpu_count=os.cpu_count(),
                   affinity_cpus=len(os.sched_getaffinity(0)),
                   files=len(files), lines=sum(len(p.read_bytes().splitlines()) for p in files),
                   source_bytes=sum(p.stat().st_size for p in files),
                   source_hashes=hashes, binary_hashes={key: digest(Path(binary)) for key, (binary, _) in binaries.items()},
                   cache_policy='warm/uncontrolled; source hashing and validation read inputs',
                   repeats=args.repeats, query_repeats=args.query_repeats, runs={}, schedule=[])
    expected = None
    schedule = [(name, repeat) for repeat in range(args.repeats) for name in binaries]
    random.Random(20261108).shuffle(schedule)
    for name, repeat in schedule:
        binary, workers = binaries[name]
        database = root / f'{name}.db'
        command = [binary, '-b', '-f', str(database), '-i', str(manifest)]
        if workers is not None:
            command += ['--jobs', str(workers), '--profile']
        if args.allow_parse_errors:
            command += ['--allow-parse-errors']
        result = measured(root, f'{name}-{repeat}', command)
        result['database_bytes'] = database.stat().st_size
        actual = snapshot(database)
        if expected is None:
            expected = actual
        if actual != expected:
            raise RuntimeError(f'normalized database differs: {name}-{repeat}')
        summary['runs'].setdefault(name, []).append(result)
        summary['schedule'].append([name, repeat])
        print(f'{name}-{repeat}: {result["wall_s"]:.3f}s, {result["peak_rss_kib"]} KiB; DB equal', flush=True)
    summary['normalized_database'] = expected
    summary['medians'] = {name: {metric: statistics.median(run[metric] for run in runs)
                               for metric in ['wall_s', 'peak_rss_kib', 'database_bytes']}
                          for name, runs in summary['runs'].items()}
    queries = [('0',args.symbol),('1',args.symbol),('2',args.symbol),('3',args.symbol),
               ('4',args.symbol),('6',args.symbol),('7',r'\.(rs|java|c|h|py)$'),('8','.*'),('9',args.symbol)]
    summary['queries'] = {}
    for mode, pattern in queries:
        expected_output = None
        results = {}
        for name, (binary, _) in binaries.items():
            runs = []
            for repeat in range(args.query_repeats):
                label = f'{name}-q{mode}-{repeat}'
                runs.append(measured(root, label, [binary, '-dL', '-f', str(root / f'{name}.db'), '-' + mode, pattern]))
                output_hash = digest(root / (label + '.stdout'))
                if expected_output is None:
                    expected_output = output_hash
                if output_hash != expected_output:
                    raise RuntimeError(f'query output differs: {label}')
            results[name] = dict(median_wall_s=statistics.median(run['wall_s'] for run in runs), runs=runs)
        summary['queries'][mode] = dict(pattern=pattern, output_sha256=expected_output, variants=results)
    if hashes != {str(p): digest(p) for p in files}:
        raise RuntimeError('source content changed during benchmark')
    summary['source_hashes_unchanged'] = True
    (root / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary['medians'], indent=2))


if __name__ == '__main__':
    main()
