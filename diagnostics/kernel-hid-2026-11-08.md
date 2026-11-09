# Kernel HID parsing diagnosis

Source: `/path/to/linux-7.3.3/drivers/hid`, recursively selected `.c` and
`.h` files. Grammar: pinned tree-sitter-c 0.24.2, runtime 0.25.10. No source
files were modified and no index was built during diagnosis.

## Corpus scan

| Measurement | Observed |
|---|---:|
| Selected files | 298 |
| C files | 241 |
| Headers | 57 |
| Source bytes | 5,144,323 |
| Source lines | 163,300 |
| Files without parser error nodes | 131 (87 C, 44 headers) |
| Files with parser error nodes | 167 (154 C, 13 headers) |
| Read failures | 0 |
| Outermost error/missing-node regions | 1,310 |
| Parser-only times summed across files | 0.611 seconds |
| Diagnostic scan elapsed | 0.664 seconds |

This is a parser diagnostic scan, not an index-build benchmark. The scan reused
one parser, did no symbol extraction, and did no SQLite writes. The prior strict
build stopped at the first failing file: exit 1 after 0.008 seconds, with no DB
produced. That failed-build timing must not be reported as a complete build.

The 1,310 regions include recovery artifacts; they are not 1,310 independent
bugs. Nested error nodes inside an already counted error region were not counted
again. Of the regions, 967 are ERROR nodes and 343 are missing-token nodes.
Some error regions span multiple lines (246 regions); skipping error subtrees
could omit substantial source sections. For example, one initializer-list error
in `bpf/progs/XPPen__Deco02.bpf.c` spans lines 180-237.

A clean syntax tree does not establish correct or complete extraction. These
figures measure grammar acceptance, not reference/call-graph recall.

## First failure and correction

The first strict-build failure is line 46 of
`amd-sfh-hid/amd_sfh_client.c`:

```c
mp2 = container_of(in_data, struct amd_mp2_dev, in_data);
```

It is not the `guard(mutex)(&mp2->lock)` statement on line 47. The earlier
discussion attributed the failure to guard prematurely; its minimal form
parses without errors.

The reduced case `container-of-type.c` fails on the type-valued argument
`struct item`; `container-of-identifier.c`, with the argument written as an
identifier, parses without errors. The pinned grammar's ordinary argument list
allows expressions/compound statements, not general type descriptors. The
[upstream grammar source](https://github.com/tree-sitter/tree-sitter-c/blob/master/grammar.js)
still has that distinction in its normal argument-list rule when inspected.
Upstream master was inspected, not built or substituted for the pinned grammar.

In the full scan, 268 error regions across 98 files are exactly `struct` under
an argument-list node. Examples occur in `container_of`, allocation macros and
list-entry macros. This is a recurring family, not a one-file anomaly.

## Reduced cases

The authored cases in `diagnostics/c-macros/` isolate parser syntax. Except for
the explicit compiler control below, they are not standalone compilable programs
and do not provide complete kernel macro definitions.

| Case | Parser result | Observation |
|---|---|---|
| `container-of-type.c` | Error | A type in a macro argument is not an ordinary expression |
| `container-of-identifier.c` | Clean | Identifier-valued arguments fit ordinary call syntax |
| `guard.c` | Clean | Nested call syntax itself is accepted |
| `counted-by.c` | Error | A field annotation macro needs declaration-aware treatment |
| `init-annotation.c` | Error | An annotation identifier between return type and name is rejected |
| `module-driver.c` | Error | A macro generating declarations is not an ordinary declaration |
| `iterator-macro.c` | Error | A macro followed by a block is interpreted as a call missing `;` |
| `initializer-macro.c` | Clean | A function-shaped initializer macro can fit existing syntax |
| `packed.c` | Clean | Parser acceptance alone does not establish annotation semantics |
| `packed-prefix.c` | Clean | Same limitation; the identifier may be assigned another syntax role |

The syntax-tree dumps for guard show nested `call_expression` nodes. The iterator
case shows an expression statement with a missing semicolon followed by a block.
These differing shapes explain why one generic extraction tweak will not solve
all failures.

Other observed annotation error tokens include `__init` (24 regions), `__exit`
(21), `__maybe_unused` (10), `__iomem` (16), `__user` (29), and `__rcu` (6).
These are exact-token region counts, not an exhaustive root-cause classifier.

For additional context, parent-node counts are: argument lists 400, expression
statements 217, declarations 136, macro-type specifiers 134, initializer lists
118, and function definitions 113. Those are syntax contexts, not disjoint
semantic cause categories; a single macro can cause errors in several contexts.
HID report-description macros and BPF declaration/annotation macros contribute
additional failures, so fixing typed arguments alone will not finish this tree.

## Preprocessing control

`preprocessing-control.c` defines a deliberately simplified, self-contained
container_of-like macro. It is not a replacement for Linux's implementation.

```bash
gcc -fsyntax-only diagnostics/c-macros/preprocessing-control.c
gcc -E -P diagnostics/c-macros/preprocessing-control.c \
  -o target/kernel-hid-test/preprocessing-control.expanded.c
```

GCC accepts the original file. Tree-sitter reports one error region on the
unexpanded source, while the expanded file parses without errors. This confirms
a preprocessor/grammar mismatch for the control, independent of rscope's SQL or
relationship-extraction code.

This agrees with [upstream issue 108](https://github.com/tree-sitter/tree-sitter-c/issues/108),
which discusses limits of parsing preprocessor-dependent syntax. It does not
prove that a dependency upgrade or full kernel preprocessing would be trivial.

## Recommended next experiment

Keep strict indexing unchanged while evaluating explicit kernel syntax support
for two well-reproduced families: type-valued macro arguments and declaration
annotations. Preserve original locations and references, verify extraction as
well as parse acceptance, then rescan the same manifest to measure the remaining
failures. Statement/declaration-generating macros and HID initializer DSLs also
need attention; these results do not establish a small complete fix.

Full configured preprocessing is another option, but needs include paths/build
configuration and original-source location mapping. It also selects conditional
branches and changes macro visibility. A best-effort mode would instead be an
explicit partial-coverage policy, not a grammar repair. Neither alternative was
implemented during this diagnosis.

## Reproduce and inspect

```bash
cargo build --release --locked --example diagnose_c
./target/release/examples/diagnose_c \
  -i target/kernel-hid-test/rscope.files \
  -o target/kernel-hid-test/diagnosis

./target/release/examples/diagnose_c \
  -o target/kernel-hid-test/repro-results --dump-ast \
  diagnostics/c-macros/*.c
```

The first manifest lists all 298 selected files with absolute paths. If it is
missing, regenerate it from `.c`/`.h` files under the source subtree. The example
accepts `-i` lists or explicit files, and writes `files.tsv` plus `errors.tsv`.
TSV fields escape backslashes, tabs and newlines. Error columns are byte-based,
one-based positions. `--dump-ast` is intended only for small examples.

Detailed local artifacts (under ignored `target/`):

- `target/kernel-hid-test/rscope.files`: selected paths.
- `target/kernel-hid-test/build-metrics.json`: failed strict-build metrics.
- `target/kernel-hid-test/build.stderr`: original build failure.
- `target/kernel-hid-test/diagnosis/files.tsv`: per-file results and parse times.
- `target/kernel-hid-test/diagnosis/errors.tsv`: locations, node kinds and excerpts.
- `target/kernel-hid-test/diagnosis/summary.json`: aggregated first-scan counts.
- `target/kernel-hid-test/repro-results/`: reduced-case trees/results.
- `target/kernel-hid-test/preprocessing-control/`: raw/expanded control trees.

At completion of this diagnosis, no kernel DB or best-effort mode existed and
the requested build benchmark was pending a parsing-policy decision. Kernel
files, application behavior, user indexes and Emacs configuration were unchanged.

**Subsequent follow-up:** Kun explicitly requested a database to inspect a missed
definition. An opt-in experimental partial mode and the 298-file test DB were
then created. See the [partial demonstration](../benchmarks/kernel-hid-partial-2026-11-08.md)
for metrics and a verified missing function at hid_bpf_dispatch.c:609. The scan
results above remain the original strict-parser diagnosis, not successful
strict-indexing results.
