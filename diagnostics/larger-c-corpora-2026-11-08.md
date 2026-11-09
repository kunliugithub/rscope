# Larger C benchmark target screening — 2026-11-08

Kun requested a larger codebase after the 9,154-line chibicc comparison.
The selected next target is **Redis 7.2.4 core source**, with 164 files and
148,785 lines (16.3 times chibicc's lines). A complete strict rscope benchmark
is currently blocked by grammar acceptance. No comparative timing claim is
made from these failed strict builds or read-only diagnostic scans.

## Preselected source scopes

| Candidate | Scope | Files | Lines | Bytes | Clean files | Files with errors | Error regions |
|---|---|---:|---:|---:|---:|---:|---:|
| Redis 7.2.4 | All `src/*.c` and `src/*.h`, nonrecursive | 164 | 148,785 | 5,873,709 | 134 | 30 | 915 |
| Memcached 1.6.45 | All root `*.c` and `*.h`, nonrecursive | 93 | 50,788 | 1,739,525 | 70 | 23 | 345 |

No files were dropped after seeing their parsing results. The Redis selection
excludes `src/modules`, nested test trees, and vendored dependency source trees;
it is the named core-source selection, not the entire repository. Memcached's
root selection includes its root test/benchmark C files. Both scans had zero
read failures. File acceptance and diagnostic-region counts do not measure
symbol accuracy, missed source lines, or the volume a partial index would lose.

Archives were obtained from pinned upstream tags:

- [Redis 7.2.4](https://github.com/redis/redis/tree/7.2.4):
  `https://codeload.github.com/redis/redis/tar.gz/refs/tags/7.2.4`
  SHA-256 `0a62b9ae89b4be4e8d40c0035c83a72cb6776f4b62fe53553981a57f0f4ff73d`.
- [Memcached 1.6.45](https://github.com/memcached/memcached/tree/1.6.45):
  `https://codeload.github.com/memcached/memcached/tar.gz/refs/tags/1.6.45`
  SHA-256 `d74c79ebc5095e30979e9a95bb9b45fea14defda9f9cf74ed2ee15c80347ebf1`.

These are fixed benchmark candidates, not a recommendation to deploy these
versions. Every selected source file still matched its archive bytes after
checking. No upstream applications or build scripts were run. Downloaded source
is ignored input under `target/cscope-comparison/`, not vendored implementation.

## Observed blockers

The four-worker strict Redis build first reported `src/bio.c:171:59`, in:

```c
job->free_args.free_args[i] = va_arg(valist, void *);
```

The pinned Tree-sitter C grammar reports the pointer token as an ERROR under an
argument list. This is a typed macro-argument parsing boundary, not evidence
that the original C source is invalid. `src/bitops.c:606` also has a missing
semicolon region at the annotation macro `REDIS_NO_SANITIZE("alignment")`.

The full diagnostic scan, not just the first failure, found:

- `redismodule.h`: 687 error regions, mostly around declaration syntax.
- `quicklist.c`: 74 regions.
- `listpack.c`: 43 regions.
- 27 additional affected files, with 111 regions in total.

These counts are outermost ERROR/missing regions; root causes were not reduced
for every region. Fixing one `va_arg` example would not establish that the
whole Redis selection can be indexed strictly or extracted correctly.

The strict Memcached build first reported `cache.h:28:17`, in a struct member:

```c
STAILQ_HEAD(cache_head, cache_free_s) head;
```

Its scan also found 263 regions in `xxhash.h`, 19 in `logger.c`, and 12 in
`queue.h`, among other files. It does not supply an immediate complete strict
alternative to Redis.

## Cscope suitability and next step

Installed Cscope 15.9 successfully built the Redis selection with `-b -u -k`.
Its `-7 '.*'` output contained exactly the same 164 manifest paths, with no
missing or additional discovered headers. This was a suitability check, not a
measured repeated-build comparison. The resulting Cscope database is retained
separately as `target/cscope-comparison/redis-cscope.out`.

Redis is a useful larger target because it exposes concrete production-C gaps
while providing a much larger workload. Before presenting a complete strict
comparison, address and validate its grammar/extraction boundaries. A partial
Redis experiment would need explicit labeling, diagnostic/lost-result checks,
and a separate interpretation of speed; silently filtering to 134 clean files
would change the corpus and conceal those gaps.

No grammar/product code was changed and no partial Redis or Memcached database
was built during this screening. The earlier chibicc result remains valid for
its small corpus.

## Reproduction and artifacts

After extracting the pinned archives, generate each manifest from its selected
directory (absolute paths recommended) using `rg --files` with a nonrecursive
selection, or the equivalent Python `Path.iterdir()` suffix filter. Then:

```bash
./target/release/rscope -b --jobs 4 -f target/cscope-comparison/redis-strict.db -i target/cscope-comparison/redis.files
./target/release/examples/diagnose_c -i target/cscope-comparison/redis.files -o target/cscope-comparison/redis-diagnosis-retry
cscope -b -u -k -i target/cscope-comparison/redis.files -f target/cscope-comparison/redis-cscope-retry.out
```

Build the read-only diagnostic executable if needed with
`cargo build --release --locked --example diagnose_c`. It scans pinned grammar
acceptance without calling extraction or creating an index.

[Recorded screening data](larger-c-corpora-2026-11-08.json) includes all affected
file counts, source hashes, exact scopes, archive hashes, and the first observed
strict-build errors. Full `files.tsv` and `errors.tsv` are under
`target/cscope-comparison/{redis,memcached}-diagnosis/`; manifests, archives,
strict-build stderr, and `larger-selection.json` are retained alongside them.
