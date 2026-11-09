"""Benchmark a pinned chibicc implementation corpus with rscope and Cscope.

Linux/Python 3/cc required. Reuses the native resource helper and normalized
SQLite snapshot validation from benchmark-parallel.py. Sources are never built.
All full builds use fresh destinations; Cscope also uses -u to force rebuilding.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import sys

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location(
    'parallel_benchmark', Path(__file__).with_name('benchmark-parallel.py'))
support = importlib.util.module_from_spec(spec)
spec.loader.exec_module(support)

# Independently read from the pinned source, not inferred from either index.
DEFINITIONS = {
    'main': [('main.c', 700)],
    'tokenize_file': [('tokenize.c', 778)],
    'parse': [('parse.c', 3337)],
    'codegen': [('codegen.c', 1585)],
}
CALLERS = {
    'tokenize_file': [('main.c', 498, 'must_tokenize_file'),
                      ('preprocess.c', 808, 'include_file')],
    'parse': [('main.c', 553, 'cc1')],
    'codegen': [('main.c', 561, 'cc1')],
}
QUERIES = [('definition-' + symbol, '1', symbol) for symbol in DEFINITIONS]
QUERIES += [('callers-' + symbol, '3', symbol) for symbol in CALLERS]
QUERIES += [
    ('symbol', '0', 'tokenize_file'), ('callees', '2', 'main'),
    ('text', '4', 'tokenize_file'), ('regex', '6', 'tokenize_file'),
    ('file', '7', r'tokenize\.c$'), ('include', '8', 'chibicc.h'),
]


def records(path, source, sources):
    result = []
    for line in path.read_text().splitlines():
        fields = line.split(maxsplit=3)
        filename, context, number = fields[:3]
        snippet = fields[3] if len(fields) == 4 else ''
        file = Path(filename).resolve()
        number = int(number)
        if file not in sources or not 1 <= number <= max(1, len(sources[file])):
            raise RuntimeError(f'invalid result location: {line}')
        result.append((str(file.relative_to(source)), number, context, snippet))
    return sorted(result)


def capture(command, destination):
    out = subprocess.run(command, capture_output=True, text=True, check=True)
    destination.write_text(out.stdout)
    return out.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--rscope', type=Path, default=Path('target/release/rscope'))
    parser.add_argument('--cscope', type=Path, default=Path(shutil.which('cscope') or '/usr/bin/cscope'))
    parser.add_argument('--repeats', type=int, default=7)
    parser.add_argument('--query-repeats', type=int, default=9)
    args = parser.parse_args()
    if min(args.repeats, args.query_repeats) < 1:
        parser.error('repeats must be positive')
    source = args.source.resolve()
    files = sorted(p.resolve() for p in source.iterdir() if p.suffix in {'.c', '.h'})
    files += sorted(p.resolve() for p in (source / 'include').rglob('*.h'))
    if not files or any(any(c.isspace() for c in str(p)) for p in files):
        parser.error('expected chibicc source files with whitespace-free paths')
    sources = {p: p.read_text().splitlines() for p in files}
    hashes = {str(p.relative_to(source)): support.digest(p) for p in files}
    fingerprint = hashlib.sha256(json.dumps(hashes, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    if fingerprint != '6fd226701653c39f58319246e3cee39d31935e76ca7ede5ea105ae380834bb03':
        raise RuntimeError('source hashes do not match the pinned chibicc selection')
    # Fail if pinned source line numbers in the independent sample have drifted.
    for symbol, locations in DEFINITIONS.items():
        for filename, number in locations:
            if symbol + '(' not in sources[source / filename][number - 1]:
                raise RuntimeError('definition oracle does not match pinned source')
    for symbol, locations in CALLERS.items():
        for filename, number, _ in locations:
            if symbol + '(' not in sources[source / filename][number - 1]:
                raise RuntimeError('caller oracle does not match pinned source')
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    subprocess.run(['cc', '-O2', '-Wall', '-Wextra', '-Werror',
                    str(Path(__file__).with_name('measure-command.c').resolve()),
                    '-o', str(root / 'measure-command')], check=True)
    manifest = root / 'inputs.files'
    manifest.write_text('\n'.join(map(str, files)) + '\n')
    rscope, cscope = str(args.rscope.resolve()), str(args.cscope.resolve())
    variants = ['cscope', 'cscope-q', 'rscope-1', 'rscope-4', 'rscope-8']
    summary = dict(source_commit='90d1f7f199cc55b13c7fdb5839d1409806633fdb',
                   scope='root *.c/*.h and include/**/*.h; excludes test/',
                   host=platform.platform(), affinity_cpus=len(os.sched_getaffinity(0)),
                   files=len(files), lines=sum(map(len, sources.values())),
                   source_bytes=sum(p.stat().st_size for p in files), source_hashes=hashes,
                   source_fingerprint=fingerprint,
                   binary_hashes={'rscope': support.digest(Path(rscope)), 'cscope': support.digest(Path(cscope))},
                   versions={'rscope': subprocess.check_output([rscope, '--version'], text=True).strip(),
                             'cscope': subprocess.check_output([cscope, '-V'], text=True, stderr=subprocess.STDOUT).strip()},
                   cache_policy='warm/uncontrolled: source hashing and verification read inputs',
                   repeats=args.repeats, query_repeats=args.query_repeats,
                   builds={}, schedule=[], queries={}, samples={})
    build_order = [(name, repeat) for repeat in range(args.repeats) for name in variants]
    random.Random(20261108).shuffle(build_order)
    databases = {}
    expected_snapshot = None
    for name, repeat in build_order:
        directory = root / f'{name}-{repeat}'
        directory.mkdir()
        database = directory / 'cscope.out'
        if name.startswith('cscope'):
            command = [cscope, '-b', '-u', '-k', '-i', str(manifest), '-f', str(database)]
            if name == 'cscope-q':
                command.append('-q')
        else:
            command = [rscope, '-b', '--jobs', name.split('-')[1], '-i', str(manifest), '-f', str(database)]
        result = support.measured(root, f'{name}-{repeat}-build', command)
        artifacts = {p.name: p.stat().st_size for p in directory.iterdir() if p.is_file()}
        result.update(artifacts=artifacts, index_bytes=sum(artifacts.values()), command=command)
        binary = cscope if name.startswith('cscope') else rscope
        output = capture([binary, '-dL', '-f', str(database), '-7', '.*'], root / f'{name}-{repeat}-files.stdout')
        if {Path(line.split()[0]).resolve() for line in output.splitlines()} != set(files):
            raise RuntimeError(f'{name}: actual indexed file set differs from manifest')
        if name.startswith('rscope'):
            actual = support.snapshot(database)
            if expected_snapshot is None:
                expected_snapshot = actual
            if actual != expected_snapshot:
                raise RuntimeError('rscope normalized records differ between builds')
        summary['builds'].setdefault(name, []).append(result)
        summary['schedule'].append([name, repeat])
        databases[name] = database
        print(f'{name}-{repeat}: {result["wall_s"]*1000:.2f} ms; {result["peak_rss_kib"]} KiB; {result["index_bytes"]} bytes', flush=True)
    summary['normalized_rscope_database'] = expected_snapshot
    summary['medians'] = {name: {metric: statistics.median(run[metric] for run in runs)
                                for metric in ['wall_s', 'user_s', 'system_s', 'peak_rss_kib', 'index_bytes']}
                          for name, runs in summary['builds'].items()}
    query_order = [(label, mode, pattern, name, repeat) for label, mode, pattern in QUERIES
                   for name in variants for repeat in range(args.query_repeats)]
    random.Random(20261109).shuffle(query_order)
    expected_records = {}
    for label, mode, pattern, name, repeat in query_order:
        binary = cscope if name.startswith('cscope') else rscope
        tag = f'{name}-{label}-{repeat}'
        run = support.measured(root, tag, [binary, '-dL', '-f', str(databases[name]), '-' + mode, pattern])
        actual = records(root / (tag + '.stdout'), source, sources)
        # Cscope's -q can change results; verify stability within each variant.
        key = (label, name)
        if key in expected_records and expected_records[key] != actual:
            raise RuntimeError(f'query results changed within variant: {tag}')
        expected_records[key] = actual
        query = summary['queries'].setdefault(label, dict(mode=mode, pattern=pattern, variants={}))
        query['variants'].setdefault(name, []).append(run)
    for label, mode, pattern in QUERIES:
        sample = {}
        for family in variants:
            actual = expected_records[(label, family)]
            locations = sorted({(row[0], row[1]) for row in actual})
            item = dict(records=actual, locations=locations, result_rows=len(actual))
            if mode == '1':
                expected = set(DEFINITIONS[pattern])
                item.update(missing=sorted(expected - set(locations)), extra=sorted(set(locations) - expected))
            if mode == '3':
                actual_calls = {(row[0], row[1], row[2]) for row in actual}
                expected = set(CALLERS[pattern])
                item.update(missing=sorted(expected - actual_calls), extra=sorted(actual_calls - expected))
            sample[family] = item
        cs = set(map(tuple, sample['cscope']['locations']))
        rs = set(map(tuple, sample['rscope-1']['locations']))
        cq = set(map(tuple, sample['cscope-q']['locations']))
        if any(sample['rscope-1']['records'] != sample[name]['records'] for name in ['rscope-4', 'rscope-8']):
            raise RuntimeError('rscope query results differ between worker counts')
        sample.update(cscope_only=sorted(cs - rs), rscope_only=sorted(rs - cs),
                      cscope_q_only_vs_default=sorted(cq - cs), cscope_default_only_vs_q=sorted(cs - cq))
        summary['samples'][label] = sample
        for name, runs in summary['queries'][label]['variants'].items():
            summary['queries'][label]['variants'][name] = dict(
                median_wall_s=statistics.median(run['wall_s'] for run in runs), runs=runs)
    if hashes != {str(p.relative_to(source)): support.digest(p) for p in files}:
        raise RuntimeError('source content changed during benchmark')
    summary['source_hashes_unchanged'] = True
    summary['all_actual_file_sets_match'] = True
    (root / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary['medians'], indent=2))


if __name__ == '__main__':
    main()
