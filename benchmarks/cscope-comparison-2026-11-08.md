# rscope versus Cscope on chibicc — 2026-11-08

**Cscope is substantially faster and smaller on this small C corpus.**
rscope avoids the spurious header callers seen in the three sampled caller
searches, while both tools find all independently checked definitions and calls.
This experiment supports a concrete performance/quality tradeoff, not a claim
of general superiority or comprehensive extraction accuracy.

## Pinned source and tool scope

- chibicc commit `90d1f7f199cc55b13c7fdb5839d1409806633fdb` from
  [rui314/chibicc](https://github.com/rui314/chibicc/tree/90d1f7f199cc55b13c7fdb5839d1409806633fdb).
  Selection: all root `*.c`/`*.h` plus `include/**/*.h`; excludes compiler tests.
  17 files, 9,154 source lines, 242,440 source bytes.
- Downloaded pinned archive SHA-256:
  `cadb44e82354002000f6d2bbf41d3155f47a9b3e9abd50a309fdcd9b5ef791e0`.
  Selected source bytes matched the archive before benchmarking and all
  selected file hashes matched afterward. Upstream code/build scripts/apps
  were not executed or modified; sources were used only as indexing inputs.
- rscope release binary from commit `4008462` (Rust 1.99.1, pinned grammars and
  bundled SQLite); installed Cscope 15.9, compressed default format and `-q`.
  Exact executable hashes, source hashes, and observations are in the
  [measurement data](cscope-comparison-2026-11-08.json).
- rscope parsed the entire selected corpus strictly, with zero diagnostics.
  After **each build**, an untimed file query verified that the actual file set
  equals the manifest for both tools. Cscope uses `-k` to exclude installed
  system headers. The custom chibicc include headers are explicitly selected.
- Candidate screening is part of the scope: Lua 5.4.8 (61 files / 30,120 lines)
  failed strict parsing at observed `src/lauxlib.h:52:18`; bzip2 1.0.8
  (15 files / 8,127 lines) failed at observed `blocksort.c:944:23`.
  They were excluded rather than treating partial indexes as equivalent builds.
  This selected success does not imply broad production-C grammar coverage.

## Measurement method

- Linux x86_64 VM, eight available vCPUs, host reporting Intel i5-10400.
  CPU/cache access was not exclusive; no cache eviction or pinning.
- Seven **fresh full builds** per configuration (35 builds), interleaved with
  fixed shuffle seed 20261108. Each build uses a new output directory;
  Cscope additionally receives `-u` to force rebuilding. This avoids accidentally
  comparing an unchanged Cscope incremental check with an rscope full rebuild.
- Cscope defaults retain compression; `-q` adds the inverted index. All regular
  files in each fresh index directory are counted, including both `-q` sidecars.
  rscope uses the existing atomic replacement build and default queue capacity
  equal to worker count. No performance setting or durability policy was changed.
- Native `scripts/measure-command.c` measures child CPU and peak RSS with
  Linux `wait4`, avoiding inherited Python pre-exec resident memory. External
  wall time includes helper/process startup and output-file handling.
- Source hashing, manifests, directory creation, helper compilation, DB/file-set
  validation, and source checks happen **outside** the measured build.
  Input hashing and repeated validation warm caches; this is a warm/uncontrolled
  experiment, not a cold-cache or kernel-scale result.
- Thirteen query cases cover eight shared read-only modes: 0/1/2/3/4/6/7/8.
  Each case/configuration runs nine times (585 query executions), interleaved
  with shuffle seed 20261109. Queries use `-dL` and never rebuild. All output
  is captured to files. rscope assignment mode `-9` is excluded because this
  installed Cscope CLI advertises only fields 0–8; interactive change mode is
  outside the read-only comparison.
- These are the tools’ normal CLI build contracts, not equal storage schemas:
  rscope’s SQLite indexes and syntax records differ from Cscope’s cross-reference
  format. rscope explicitly synchronizes and atomically publishes its replacement.
  Cscope’s durability guarantees were not measured or made equivalent.

## Full-build results

All values are medians of seven runs. Memory is child peak RSS, not cumulative
allocation. Size is logical bytes, not filesystem allocated blocks.

| Configuration | Wall time | User / system CPU | Peak RSS | Total index bytes |
|---|---:|---:|---:|---:|
| Cscope default | 7.51 ms | 6.48 / 0.00 ms | 2,492 KiB | 209,902 |
| Cscope `-q` | 14.13 ms | 11.46 / 2.59 ms | 3,236 KiB | 659,750 |
| rscope, 1 worker | 125.66 ms | 90.86 / 4.13 ms | 11,336 KiB | 2,514,944 |
| rscope, 4 workers | 97.53 ms | 100.11 / 17.82 ms | 17,400 KiB | 2,514,944 |
| rscope, 8 workers | 99.74 ms | 110.96 / 12.33 ms | 19,088 KiB | 2,514,944 |

Cscope default built 13.0× faster than four-worker rscope and used
12.0× less index space on this selection. Against Cscope `-q`, four-worker
rscope took 6.9× as long and used 3.8× the index space.
Four workers reduced rscope build elapsed time by 22.4% versus its serial path;
eight workers did not improve the median further. The earlier 43% improvement
on 135K lines of Rust was a different corpus and baseline and is not a Cscope
comparison.

| Configuration | Seven wall-time observations in execution order (ms) |
|---|---|
| Cscope default | 7.453, 7.678, 7.479, 7.377, 7.514, 7.564, 7.567 |
| Cscope `-q` | 14.320, 13.559, 14.078, 14.217, 13.947, 17.351, 14.129 |
| rscope, 1 worker | 139.769, 125.659, 120.272, 124.303, 133.282, 123.754, 151.709 |
| rscope, 4 workers | 114.921, 97.531, 87.912, 87.479, 119.729, 111.143, 97.043 |
| rscope, 8 workers | 109.247, 91.827, 99.740, 93.262, 109.707, 94.813, 110.658 |

Index components (unchanged size across the seven runs):

| Configuration | Component bytes |
|---|---|
| Cscope default | `cscope.out` 209,902 |
| Cscope `-q` | `cscope.out.in` 90,112, `cscope.out` 209,902, `cscope.out.po` 359,736 |
| rscope, 4 workers | `cscope.out` 2,514,944 |

rscope stores 22,633 occurrence rows and no source-line/snippet table.
All 21 rscope builds passed SQLite integrity checks and matched normalized
files, occurrences, diagnostics, and metadata despite possible file-ID order
differences. This is internal consistency evidence, not a semantic oracle.

## Query latency

Median of nine fresh query processes per cell, in milliseconds. These include
helper/process startup and result formatting/output; most differences are
fractions of a millisecond. They are not in-process SQL timings and establish
no robust general latency advantage. More parser workers do not parallelize
queries; those columns refer to how the database was built.

| Query | Cscope | Cscope `-q` | rscope 1 | rscope 4 | rscope 8 |
|---|---:|---:|---:|---:|---:|
| definition-main | 1.835 | 1.705 | 2.231 | 2.328 | 2.329 |
| definition-tokenize_file | 1.833 | 1.747 | 2.440 | 2.300 | 2.408 |
| definition-parse | 1.767 | 1.748 | 2.443 | 2.664 | 2.566 |
| definition-codegen | 1.795 | 1.746 | 2.311 | 2.270 | 2.401 |
| callers-tokenize_file | 1.797 | 1.711 | 2.363 | 2.451 | 2.340 |
| callers-parse | 1.838 | 1.666 | 2.288 | 2.272 | 2.396 |
| callers-codegen | 1.750 | 1.714 | 2.221 | 2.223 | 2.324 |
| symbol | 2.132 | 1.682 | 2.465 | 2.501 | 2.362 |
| callees | 1.834 | 1.958 | 2.363 | 2.464 | 2.272 |
| text | 2.328 | 2.307 | 3.094 | 3.061 | 3.227 |
| regex | 2.257 | 2.226 | 3.086 | 3.056 | 3.083 |
| file | 1.666 | 1.698 | 2.419 | 2.338 | 2.454 |
| include | 1.839 | 1.768 | 2.735 | 2.741 | 2.862 |

Cscope’s medians were lower for every sampled query. The `-q` tradeoff on
a corpus this small is modest query savings with larger indexes and build cost.

## Source-checked navigation quality

Four unambiguous function definitions were read directly from the pinned source:
`main` → `main.c:700`, `tokenize_file` → `tokenize.c:778`, `parse` →
`parse.c:3337`, and `codegen` → `codegen.c:1585`. All configurations returned
exactly these locations for the corresponding definition searches. Definition
display contexts differ by tool, so equality here means file/line targets.

Four call sites were independently checked in the source:

| Target | Actual call locations and enclosing functions | Cscope rows | Cscope `-q` rows | rscope rows |
|---|---|---:|---:|---:|
| `tokenize_file` | `main.c:498` (`must_tokenize_file`), `preprocess.c:808` (`include_file`) | 7 | 3 | 2 |
| `parse` | `main.c:553` (`cc1`) | 6 | 2 | 1 |
| `codegen` | `main.c:561` (`cc1`) | 6 | 2 | 1 |

All actual calls were found by all variants. Cscope default additionally
returned each target’s declaration in `chibicc.h` (lines 106, 295, and 411)
five times under contexts `format`, `error`, `error_at`, `error_tok`, and
`warn_tok`. With `-q`, it returned that declaration once under `warn_tok`.
These source lines are prototypes, not calls in those functions. rscope
returned only the four actual call sites. This is a reproducible observed
false-positive difference on three queries; its parser-level cause was not
isolated, and it is not an overall precision/recall score.

Other samples: symbol `tokenize_file`, callees of `main` (30 results), literal
and regex `tokenize_file` text (seven results), file `tokenize\.c$` (one),
and includes matching `chibicc.h` (nine) matched file/line sets across tools.
Every returned path/line was checked to exist in the selected corpus. Full
line/context/snippet records are retained; snippet semantic equivalence was
not assumed. Query repetitions were stable within each configuration. All
rscope worker configurations returned identical full records; Cscope default
and `-q` intentionally remain separate because their caller records differ.

## Diagnostic follow-up and interpretation

An additional **single**, untimed-comparison four-worker build with `--profile`
after the final measurements reported 39.1ms pipeline wall, 22.5ms index
creation, and 35.3ms commit/publication (97.6ms total). The pipeline includes
21.1ms of insertion; worker parse/extract times sum to 67.9ms and overlap.
This points to SQLite index creation/publication as substantial serial costs,
but one profile does not allocate the entire gap with Cscope or establish
which optimization would work. No fsync, correctness, or durability policy
was weakened to improve the comparison.

The supported résumé/interview story is measured engineering tradeoffs:
bounded parallel parsing, reproducible comparisons, transparent performance
limits, and verified examples of navigation quality. A claim that rscope is
faster than Cscope would contradict this result. No résumé files were changed.

## Reproduce

From the repository root, download/extract the pinned chibicc archive and build
rscope. No chibicc compilation is needed:

```bash
mkdir -p target/cscope-comparison/downloads
curl --fail --location --output target/cscope-comparison/downloads/chibicc.tar.gz https://codeload.github.com/rui314/chibicc/tar.gz/90d1f7f199cc55b13c7fdb5839d1409806633fdb
```

Verify SHA-256 `cadb44e82354002000f6d2bbf41d3155f47a9b3e9abd50a309fdcd9b5ef791e0` before extraction, then:

```bash
tar -xzf target/cscope-comparison/downloads/chibicc.tar.gz -C target/cscope-comparison
cargo build --release --locked
python3 scripts/benchmark-cscope.py --source target/cscope-comparison/chibicc-90d1f7f199cc55b13c7fdb5839d1409806633fdb --output target/cscope-comparison/reproduction
```

Requires Linux, Python 3, `cc`, and Cscope (15.9 tested). The output directory
must be new. The script verifies a fixed hash of the complete source selection
and source-oracle lines before starting, checks every actual indexed file set,
captures all build/query observations, and verifies source hashes afterward.
Full raw observations and artifacts are under ignored
`target/cscope-comparison/measured/`; the committed JSON retains all build
observations, query wall-time samples, and selected result records. Initial
`results/` and `final-results/` are exploratory harness runs excluded from the
final measurements: the first exposed an empty snippet in file-search output,
and the second exposed different Cscope default/`-q` caller records. The final
harness accepts the file protocol and compares Cscope configurations separately.
