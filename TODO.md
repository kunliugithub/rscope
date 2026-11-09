# rscope TODO

Updated: 2026-11-08

This is the working backlog. Priorities reflect current evidence and usage;
items are candidates, not implementation commitments or release promises.
Use [README.md](README.md) for implemented behavior.

| Item | Current priority | Reason to start |
|---|---|---|
| Bounded C parsing/navigation improvements | Next candidate | A specific gap blocks navigation in a codebase we want to use |
| File discovery and build usability | When needed | Maintaining manifests or rebuilding causes repeated friction |
| Incremental updates | Deferred | Measured rebuild delays or frequent updates interrupt actual use |
| Larger Cscope performance comparison | Deferred | The complete chosen corpus can be indexed strictly with checked navigation |
| Additional languages | Deferred | A concrete navigation need justifies another extractor |
| Packaging and platform validation | When distributing | Someone needs to build/run on a target we have not verified |

## C parsing and navigation

- [ ] Pick one bounded failure class and write small reproductions before
  changing grammar or extraction. Redis examples include typed `va_arg`
  arguments, annotation macros, and API declarations; Memcached adds
  macro-generated struct members. Do not assume one fix resolves all errors.
- [ ] Evaluate an upstream grammar change or a narrowly scoped adaptation.
  Keep strict mode the default and experimental recovery visibly partial.
- [ ] Check definitions, occurrences, callers/callees, and source locations
  around the affected constructs. Parser acceptance alone is insufficient.
- [ ] Rescan the complete original manifest and report remaining gaps.
  Update index compatibility markers if extraction/grammar semantics change.

Evidence: [larger C corpora](diagnostics/larger-c-corpora-2026-11-08.md) and
[kernel HID diagnosis](diagnostics/kernel-hid-2026-11-08.md). Redis has 30/164
affected files; Memcached has 23/93. Broad complete C indexing remains unproven.

## File discovery and build usability

- [ ] If manifest upkeep becomes a recurring problem, add explicit directory
  discovery with documented ignore rules, supported extensions, and overrides.
- [ ] Preserve explicit-file/list selection and verify the actual indexed set.
  Discovery must not silently change which files a rebuild includes.

## Incremental updates — deferred

Successful full builds are currently fast enough that incremental indexing has
no demonstrated user benefit. The larger Rust benchmark completed in roughly
0.6 seconds with parallel parsing; this is a corpus-specific observation.

- [ ] Measure rebuild delays and update frequency in actual use before starting.
- [ ] If justified, define change detection and added/deleted/renamed-file
  behavior, including grammar-version changes and failure recovery.
- [ ] Verify updated records and query results against a fresh full rebuild;
  preserve a usable prior index on failure. Measure update cost and complexity
  alongside the time saved. Do not select an implementation strategy yet.

## Larger Cscope comparison — deferred

- [ ] Revisit after the chosen complete corpus passes strict parsing and
  source-checked navigation. Keep the same source selection for both tools.
- [ ] Repeat full-build/query measurements with memory and all index sidecars,
  and report result differences alongside timings. Label partial experiments
  separately; never substitute a failed strict build or silently filtered files.

The [chibicc comparison](benchmarks/cscope-comparison-2026-11-08.md) is complete
and valid for its small corpus: Cscope was faster and smaller; rscope avoided
spurious header callers in three sampled searches. It supports no general
speed or accuracy claim. Redis is the larger candidate, currently blocked.

## Additional languages and packaging — conditional

- [ ] Select another language only for a concrete use case. Add extraction
  rules, fixtures, xcscope checks, compatibility handling, and license notices.
  C++ needs explicit language/header selection; `.h` currently means C.
- [ ] Before distribution on a new target, verify its compiler/runtime needs,
  the declared Rust 1.90 minimum, and required third-party notices. Only Linux
  x86_64 has been verified here. Publication remains a separate decision.

## Already completed

- Python, C, Rust, and Java navigation with nine query modes and xcscope checks.
- Lean SQLite storage, live text searches, atomic full replacement, and explicit
  partial-C diagnostics/warnings.
- Bounded parallel parsing, profiling, and [repeated measurements](benchmarks/parallel-2026-11-08.md).
  The measured 43% Rust-corpus improvement compares parallel and serial rscope;
  it is independent of the Cscope comparison.
