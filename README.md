# rscope

A Rust code-navigation tool intended as a modern Cscope replacement, using
Tree-sitter for parsing and SQLite for a persistent project index. It provides
a command-line interface and integrates with Emacs through xcscope. It supports
a subset of Cscope operations, with syntax-based rather than semantic resolution.

## Status

Python, C, Rust, and Java implementations are available to build locally. They support
full index builds and nine query types, with end-to-end verification against
xcscope 1.6 in batch Emacs. This is an experimental source preview; there is no packaged release.
Corpus-specific measurements are documented below, with limitations.

Indexes built before schema 3 (file IDs and source locations without stored
source text) must be rebuilt using the new executable. Old snapshot indexes are
rejected with a rebuild message. Existing lean Python/C schema 3 indexes remain
readable, including partial C indexes with their warnings. Rebuild with Rust/Java
files in your selection to add them to the index.

## Build and try it

Install [Rust](https://www.rust-lang.org/tools/install) 1.90 or newer and native
C build tools. On Debian/Ubuntu, the `build-essential` package provides the
compiler and linker. The code has been checked with Rust 1.99.1 and GCC 14.2
on Debian 13 x86_64; the declared Rust 1.90 minimum has not been tested here.

```bash
cargo build --release --locked

# Index the included example into a separate SQLite database.
./target/release/rscope -b -f rscope.db tests/fixtures/navigation.py tests/fixtures/searches.py

./target/release/rscope -dL -f rscope.db -1 leaf   # definitions
./target/release/rscope -dL -f rscope.db -0 leaf   # identifier occurrences
./target/release/rscope -dL -f rscope.db -2 outer  # calls inside outer
./target/release/rscope -dL -f rscope.db -3 leaf   # call sites targeting leaf
./target/release/rscope -dL -f rscope.db -4 needle # literal source text
./target/release/rscope -dL -f rscope.db -6 'needle|NEEDLE' # source regex
./target/release/rscope -dL -f rscope.db -7 'searches\.py$' # indexed files
./target/release/rscope -dL -f rscope.db -8 '^package\.' # Python imports
./target/release/rscope -dL -f rscope.db -9 counter # explicit assignments
```

Select files explicitly, or pass a file list with `-b -i FILE`. Without explicit
inputs, builds read `rscope.files` in the current directory if present. There is
no automatic fallback to `cscope.files`; use `-i cscope.files` to select a legacy
list explicitly. Explicit source arguments bypass the default list. File
lists use shell-style quoting and whitespace separators; paths are resolved
relative to the command's working directory. Directory scanning is not yet
implemented. Duplicate paths are indexed once.

Every build replaces the complete selected file set, so rerun it with all files
you want indexed. Read/parse failures preserve an existing index. Builds refuse
to replace databases that are not identified as rscope indexes.

## Parallel indexing and profiling

Builds remain serial by default. Opt into parser workers with `--jobs`:

```bash
./target/release/rscope -b -f rscope.db -i rscope.files --jobs 4 --profile
```

The main thread validates/canonicalizes and deduplicates the selected paths.
Workers read and parse files independently, sending extracted records through
one bounded channel to a single SQLite writer. `--queue-capacity N` limits the
number of waiting file results (default: worker count); `0` requires a receiving
writer before a send completes. `--jobs 1` uses the serial path without threads
or a queue. The actual worker count cannot exceed the selected file count.
All three options require `-b`.

The queue is bounded by **file count, not bytes**. In addition to queued results,
each worker can hold one file's source/tree/records, and the writer can hold one
result. Large files and extra workers increase memory; this is not a hard RAM
cap. Completed files may be inserted in a different order, so database bytes,
file IDs, and the first reported failure can differ. Sorted query output,
extraction rules, and index-version compatibility are unchanged. Workers are
joined before returning; parse/read/write failure discards the replacement and
preserves the previous database.

`--profile` prints one `rscope-profile` line to stderr after a successful build:
selection, database setup, pipeline, summed worker read/parse times, insertion,
index creation, commit/publication, and total time, in seconds. Worker times
are summed wall times and can overlap each other and insertion; they must not
be added to pipeline time. Total excludes CLI argument/file-list parsing and
process startup. Normal query stdout remains the xcscope protocol.

See the [parallel indexing experiment](benchmarks/parallel-2026-11-08.md) for
repeated measurements, memory tradeoffs, correctness checks, reproduction,
and the limits of the performance claims.

## Rebuild the executable and index

After updating rscope's Rust source, rebuild the executable from the repository
root. This refreshes the release binary used by the Emacs setup below:

```bash
cd /path/to/rscope
cargo build --release --locked
```

Then rebuild the index using the same database path and complete file selection
you want Emacs to search. For the included examples:

```bash
./target/release/rscope -b -f rscope.db tests/fixtures/navigation.py tests/fixtures/searches.py
./target/release/rscope -dL -f rscope.db -4 needle
```

For your own project, run from that project's root and reuse a saved file list:

```bash
/absolute/path/to/rscope/target/release/rscope -b -f rscope.db -i rscope.files
```

Create `rscope.files` with the Python/C/Rust/Java source paths you want indexed, one per line
(paths are relative to the project's root in this example). Maintain the list
when adding or removing files. This command reads an existing list; it does
not generate one or discover files automatically.

Rebuilding replaces the whole index with the selected files; it does not append
to the old selection. You do not need to delete the existing database first.
Older rscope index formats can be replaced by a build. Each build writes a
temporary database in the destination directory and replaces the index only
after success, so a failed read/parse leaves the previous index intact. The
directory must be writable and have room for both the old and new indexes
during the rebuild. Replacing externally modified WAL-mode databases is refused;
rscope itself creates indexes in the default rollback-journal mode.

After source-file edits, only the index rebuild is needed. After an incompatible
rscope index-format change, rebuild both the executable and index. Emacs starts
the executable for each search, so no restart is needed when its configured
executable and database paths stay the same.

## Query behavior

| Flag | Behavior |
|---|---|
| `-0 NAME` | Exact identifier occurrences, including definitions |
| `-1 NAME` | Definitions according to the language coverage below |
| `-2 NAME` | Call sites in bodies of functions named NAME; context column shows the callee |
| `-3 NAME` | Call sites whose written target's final identifier is NAME; context shows the caller |
| `-4 TEXT` | Literal substring matches across every indexed source line, including comments/strings |
| `-6 PATTERN` | Regex matches across every indexed source line |
| `-7 PATTERN` | Regex matches against absolute indexed file paths; results open at line 1 |
| `-8 PATTERN` | Regex matches against written imports, modules, or C include operands |
| `-9 NAME` | Explicit assignments to this identifier, attribute, or container name |

All queries use the existing index without updating it, whether or not `-d` is
present. `-L` is accepted for single-search compatibility; output always uses
`FILE CONTEXT LINE SOURCE`, with absolute paths and one-based line numbers.
The default database name is `cscope.out`, but its contents are SQLite and are
not interchangeable with a legacy Cscope database. Use `-f rscope.db` to keep
them separate. Missing or incompatible indexes produce an error, not an empty
new database. No matches produce empty output and a successful exit status.

Repeated occurrences on the same line and in the same context produce one
result. Calls to `service.save()` are indexed as `save`; the receiver's runtime
type is not resolved. Same-named functions across scopes/files are combined.
Calls in nested function and lambda bodies are attributed to those bodies,
not the outer function. Lambda context is `<lambda>`; module/class-body calls
use `<global>`. Decorators and default arguments use their enclosing context.

Text and regex queries stream the **current contents** of the files in the
index's file list. They see edits without a rebuild, including lines without
symbol records. Newly added files are searched only after inclusion in a build.
Each matching line appears once. File lookup also finds empty indexed files.
Text/regex/file searches use `<unknown>` context because they do not infer an
enclosing function. Regex matching uses the original line, including indentation;
displayed snippets have surrounding whitespace trimmed.

Symbol, definition, call, import, and assignment locations reflect the last
successful index build. Snippets are read from source on demand, grouping results
by file so each matching file is read once. If a matched file's size or nanosecond
modification time differs from the indexed metadata, the query fails and asks
for a rebuild instead of displaying stale locations. These metadata checks are
best effort: they are not content hashes and cannot detect an edit that preserves
both values. Rebuild after changing source, even if no stale-file error appears.

Missing/unreadable files encountered by a query and detected changes during a scan produce errors on
stderr and a nonzero exit status. Output is streamed: if a later file fails,
earlier results may already have been printed. An empty output with success
means no matches; an error must not be interpreted as a complete search.

Regex searches (`-6`, `-7`, `-8`) use [Rust regex syntax](https://docs.rs/regex/latest/regex/),
not Emacs regex syntax or a complete POSIX egrep implementation. They operate
one line/path/module at a time; look-around and backreferences are unsupported.
Invalid patterns fail explicitly. `-4` treats metacharacters such as `.*`, `%`,
and `_` literally. Matching is case-sensitive; regex modes can use `(?i)`.

For Python, `-8` is the counterpart of Cscope's include search: `import a.b as c`
records `a.b`, and `from a.b import item` records `a.b`. Relative prefixes are
preserved. Aliases and imported member names are not treated as modules, and
no filesystem/import resolution or transitive dependency search is performed.
Whitespace and explicit line continuations in module names are normalized.

For Python, assignment lookup covers `=`, annotated assignments with a value, chained and
destructured assignments, augmented assignments such as `+=`, and walrus `:=`.
`obj.field = value` writes `field`; `mapping[key] = value` writes `mapping`.
Receivers, index expressions, and RHS reads are not themselves assignment
targets. Annotation-only declarations, loop/with targets, import bindings,
parameter defaults, and pattern-match bindings are outside `-9` coverage.

Comments and ordinary string contents are excluded from symbol queries;
identifiers inside Python f-string expressions are included. In Python,
assignment targets and parameters appear in `-0` results but are not definitions
for `-1` yet. Calls
through complex expressions such as `table[key]()` are not extracted as call
targets. Symbol, definition, call, and assignment queries use case-sensitive
exact names; the pattern modes described above are separate.

## Emacs integration

Install `xcscope.el` separately and build the database from the terminal. For
xcscope 1.6, configure the executable and database name, adjusting the path:

```elisp
(require 'xcscope)
(setq cscope-program "/absolute/path/to/rscope/target/release/rscope"
      cscope-database-file "rscope.db"
      cscope-option-do-not-update-database t
      cscope-option-include-directories nil
      cscope-option-disable-compression nil
      cscope-option-kernel-mode nil
      cscope-option-use-inverted-index nil
      cscope-option-other nil)
```

Open a source file under the directory containing `rscope.db` and use:

- `M-x cscope-find-this-symbol`
- `M-x cscope-find-global-definition`
- `M-x cscope-find-called-functions`
- `M-x cscope-find-functions-calling-this-function`
- `M-x cscope-find-this-text-string`
- `M-x cscope-find-egrep-pattern`
- `M-x cscope-find-this-file`
- `M-x cscope-find-files-including-file` (written imports / C includes)
- `M-x cscope-find-assignments-to-this-symbol`

With `cscope-minor-mode` enabled, the default bindings are:

| Key | Search |
|---|---|
| `C-c s s` | Symbol occurrences |
| `C-c s d` or `C-c s g` | Definitions |
| `C-c s c` | Callers |
| `C-c s C` | Callees |
| `C-c s t` | Literal text |
| `C-c s e` | Source regex |
| `C-c s f` | Files |
| `C-c s i` | Imports / includes |
| `C-c s =` | Assignments |

Rebuild manually after changing sources. xcscope's automatic indexing workflow,
interactive replacement (`-5`), and additional Cscope flags are not supported yet. Unsupported
options fail explicitly. The settings above apply to xcscope generally; use a
separate Emacs session/configuration if also navigating legacy Cscope indexes.

## Current limits

- Python `.py`, C `.c`/`.h`, Rust `.rs`, and Java `.java` files; UTF-8 input and paths are required.
- Source paths containing whitespace are rejected because the current xcscope
  line protocol cannot represent them reliably.
- Syntax errors fail the build by default. Experimental C-only recovery is
  described under Language coverage.
- Language-specific definitions and syntax-based names only; no type inference,
  import resolution, dynamic dispatch, or cross-language semantic resolution.
- Full rebuilds; parsing is serial by default, with opt-in parallel workers.
  Each active file’s source/AST/records must fit in memory; queued results add
  memory as described above. The complete repository is not buffered.
- Text searches are sequential file scans. Query results stream to stdout;
  SQLite may use temporary disk storage to sort/deduplicate location results.
- The upstream Python grammar can be slow on long consecutive comment runs;
  parser performance remains a separate concern from index storage.
- Absolute stored paths require rebuilding after moving a source tree.
- Kernel-scale validation, C++ support, macro expansion, and compiler-level
  name/type resolution are not implemented.

## Storage and scaling

SQLite stores the file list (one path per file, with a numeric ID and freshness
metadata), names, relationships, and source locations. It stores **no source
lines or display snippets**. Occurrences reference numeric file IDs rather than
repeating full paths. Caller context is stored once per occurrence rather than
duplicating it as a separate display field.

By default the builder parses/inserts one file at a time into a replacement database;
opt-in parallel parsing uses the bounded pipeline described above.
Search indexes are created after insertion. File selection still occupies
memory proportional to the number of paths. SQLite's page-cache target is
8 MiB per connection and temporary sorting uses files; this is not a hard cap
on total process memory. The AST and extracted records of the largest input
file can still be substantial; parallel workers multiply the active file working sets.

Text search reads live source with buffered Rust file I/O and the embedded
regex library. It does not require an external `rg`/`grep` executable. Rebuilds
discard obsolete database pages by replacing the file, including pages from
earlier versions that stored complete source text.

An initial [synthetic before/after comparison](benchmarks/storage-2026-11-08.md)
records database size, build/query peak memory, timings, and limitations. It is
not a kernel-scale validation.

## Intended functionality

- Index a local source repository into one SQLite database per project.
- Find definitions, symbol occurrences, callers, and callees.
- Query from the terminal or navigate results from Emacs using xcscope.
- Support the Cscope arguments and output format needed by the selected
  xcscope operations.

Full Cscope compatibility and its terminal screen interface are outside the
initial scope. A modern TUI may be considered later.

Tree-sitter provides syntax trees. Language-specific extraction rules will
determine which navigation operations are supported; parsing a language alone
does not provide complete symbol or call resolution.

## Prerequisites and packaging

The agreed default is to compile the Tree-sitter runtime, selected language
parsers, and SQLite into rscope. Users will not need to install these
components separately. This bundled build has been verified on Linux x86_64;
other platforms are not verified yet.

These embedded components will be independent of Tree-sitter, parser, and
SQLite installations used by other applications. Bundling them does not imply
that the entire executable will be statically linked; operating-system library
dependencies will be documented and verified for each supported release target.

Dependency upgrades will ship through rscope releases after compatibility and
navigation tests. Index compatibility must account for parser and extraction
changes, requiring a rebuild when existing results would become outdated.
Releases will include required third-party license notices, with each selected
parser's license checked before inclusion.

Tree-sitter's Rust API supports adding language grammars as dependencies;
see the [Tree-sitter Rust documentation](https://docs.rs/tree-sitter/latest/tree_sitter/).
SQLite can be compiled and linked through rusqlite's `bundled` feature;
see the [rusqlite documentation](https://github.com/rusqlite/rusqlite#readme).

### Running a prebuilt executable

Users running a built executable need:

- An rscope executable built for their operating system and architecture.
- Read access to the source files and write access to the index location.
- Emacs and a separately installed `xcscope.el`, if using Emacs integration.

With the planned bundled build, separate installations of Rust, the
Tree-sitter CLI, language parsers, and SQLite are not required to run rscope.
Supported platforms and any platform runtime dependencies remain to be verified.

### Building from source

Expected requirements for the planned bundled build:

- Rust 1.90 or newer, including Cargo (currently tested with 1.99.1).
- A native C compiler and linker for Tree-sitter, generated parsers, and SQLite.
- A C++ compiler if a selected grammar's native sources require one.
- Access to Cargo dependencies, either through the network or a local cache.

Cargo dependencies and their build scripts will compile the bundled components.
A separate SQLite library/development package is not expected to be necessary.
Using packaged, generated parsers should also avoid requiring Node.js or the
Tree-sitter CLI for normal builds. Regenerating grammars is a separate task
that may require additional tools.

The `sqlite3` command-line program may be useful for inspecting an index during
development, but is not intended to be an rscope prerequisite.

## Language coverage

The starting grammar pool is the programming-language parsers listed in the
[upstream Tree-sitter organization section](https://tree-sitter.github.io/tree-sitter/).
HTML, CSS, JSON, JSDoc, Regex, and embedded templates are deferred.

Language is selected by extension; one `rscope.files` can mix supported files.
The shared language dispatcher is separate from language-specific extraction.

| Language | Extensions | Definition coverage (`-1`) |
|---|---|---|
| Python | `.py` | Functions and classes |
| C | `.c`, `.h` | Functions with bodies, variable/field definitions, typedefs, type tags with bodies, enumerators, and macros |
| Rust | `.rs` | Functions/methods and trait signatures, structs/fields, enums/variants, unions, traits, type aliases, associated types, constants/statics, modules, macros, and `let` bindings |
| Java | `.java` | Types, methods (including abstract declarations), constructors, enum constants, annotation elements, record components, fields, and local variable declarators |

C prototypes, forward type declarations, uninitialized `extern` declarations,
and parameters are occurrences rather than definitions. Function-pointer
variables are distinguished from function prototypes. C `.h` files are parsed
as C; C++ headers need future C++ support. Matching names from different files
or languages remain ambiguous and are combined, not semantically linked.

For C, `-8` indexes the written operand of `#include`, removing quotes or angle
brackets. Header paths are not resolved. `#include HEADER` records `HEADER`,
without expanding it. `-9` covers initialized declarators, assignment expressions
(including compound assignments), and prefix/postfix updates. `node->field`
writes `field`, `array[index]` writes `array`, and `*pointer` writes through
`pointer`; receiver/index/RHS reads are not additional writes. Aggregate
initializers mark the declared variable, not each designated member.

C calls use their written name: `leaf()`, `node->visit()`, and `(*callback)()`
record `leaf`, `visit`, and `callback`. Function-pointer targets are not resolved;
array-dispatch calls such as `table[index]()` have no extracted callee. Macro
invocations shaped like calls are recorded syntactically. Macro bodies are
opaque to relationship extraction, and macros are not expanded. Conditional
preprocessor branches are all indexed; build configuration is not evaluated.

By default, Tree-sitter syntax errors reject the build with a source location
for C and preserve the prior index. GNU extensions and macro-heavy kernel sources may
require additional grammar/extraction work. This is initial C navigation
support, not evidence of complete Linux kernel coverage.

For explicitly experimental inspection, `-b --allow-parse-errors` permits C
recovery. Error subtrees are excluded from syntax-derived records, and recovered
nodes outside them can still be misidentified. The database stores each error
region in `parse_diagnostics`, records affected-file/error counts in metadata,
and uses a distinct partial-index version that older executables reject. Every
query of an affected index prints a partial-results warning to stderr, even if
it returns no matches. Strict mode remains the default; Python, Rust, and Java syntax failures,
missing files, and other I/O errors still abort the build. Text/regex searches
continue to read current files without depending on the recovered syntax tree.

To try the C example (this replaces the selected file set in `rscope.db`):

```bash
./target/release/rscope -b -f rscope.db tests/fixtures/navigation.c tests/fixtures/c_nodes.h
./target/release/rscope -dL -f rscope.db -2 outer
./target/release/rscope -dL -f rscope.db -8 'c_nodes\.h$'
```

Rust `-8` records written `use` paths, expanding grouped imports and ignoring
aliases (`use std::{fmt::Debug as D, io::*}` records `std::fmt::Debug` and
`std::io::*`). `extern crate` and out-of-line `mod name;` are also searchable;
module files and re-exports are not resolved. Java `-8` records ordinary/static
import paths, retaining `.*` for wildcard imports. Package declarations are not
imports. Whitespace is removed from recorded paths.

Rust calls include ordinary, qualified, generic, and method calls, plus macro
invocations by their written leaf name (`println!` is searched as `println`).
Macro definitions/arguments and attributes are opaque; generated code and calls
inside macro token trees are not extracted. Java calls include methods and
`new Type(...)` constructors. Explicit `this(...)`/`super(...)` calls retain those
literal names. Method references are occurrences, not calls. Neither extractor
resolves types, overloads, trait implementations, runtime dispatch, or aliases.
Bare names combine same-named methods. Closure/lambda calls use `<closure>` /
`<lambda>` as caller, keeping them out of the enclosing function's callees.

Rust `-9` covers initialized `let` bindings (including destructuring), constants/
statics with values, and simple/compound assignments. Java covers initialized
variable declarators, assignments, and increments/decrements. Field writes use
the field name; array/index writes use the container name; Rust dereferences
use the written pointer name. Receiver/index/RHS reads are not extra writes.
Function parameters, Rust `match`/`for` bindings, and Java enhanced-for/catch/
pattern bindings are occurrences rather than definitions or initialization
writes. Calls implicit in language constructs are not inferred.

Rust and Java builds reject Tree-sitter syntax errors, even with
`--allow-parse-errors` (which applies only to C). Parsing is not compiler type
checking and does not enforce Cargo/JDK language versions.

To index rscope's Rust implementation for navigation:

```bash
rg --files src -g '*.rs' > rscope.files
./target/release/rscope -b -f rscope.db
./target/release/rscope -dL -f rscope.db -1 parse_tree
./target/release/rscope -dL -f rscope.db -3 parse_tree
```

Further language order remains open. Each additional grammar needs its own
extraction rules, fixtures, integration checks, and license notices.

The [Rust/Java source-corpus validation](benchmarks/rust-java-2026-11-08.md)
records successful strict indexing and source-checked navigation on fd and
google-java-format, with timings, database paths, and Emacs instructions.

The [298-file kernel HID diagnosis](diagnostics/kernel-hid-2026-11-08.md)
records macro-related grammar failures, reduced cases, and a read-only diagnostic
command. Strict indexing is blocked for that subtree. A subsequent
[partial-index demonstration](benchmarks/kernel-hid-partial-2026-11-08.md)
records build metrics and a verified missed definition, with Emacs instructions.
The [Cscope source-corpus experiment](benchmarks/cscope-2026-11-08.md) provides
a second comparison: useful lookups work, but the core main definition is missed.

A separate [rscope-versus-Cscope benchmark on chibicc](benchmarks/cscope-comparison-2026-11-08.md)
compares repeated strict full builds and common query modes on an identical
17-file corpus. Cscope is faster and smaller in that experiment; rscope avoids
the spurious header callers observed in three sampled searches. Results include
source checks, memory/storage costs, and reproduction instructions.

## Validation

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

To test the actual Emacs adapter, build the debug binary with `cargo build`
and provide the directory containing your installed `xcscope.el`:

```bash
emacs --batch -Q -L /path/to/xcscope -l tests/xcscope-smoke.el
```

The smoke tests check all nine query types for Python, C, Rust, and Java and follow results to the expected
file and line. Set `RSCOPE_BINARY` to an absolute executable path to test a
different build. Set `RSCOPE_TEST_JOBS=4` to build the smoke-test indexes
with parallel workers. Test databases and source files are created under a temporary
directory. Dependency notices for the initial Linux build are included in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## Design notes

- [TODO.md](TODO.md) tracks prioritized candidates, deferred work, and when to revisit them.

Development uses public documentation, upstream dependencies, and independent
examples and tests. Cscope source is used only as a test corpus and for checking
expected navigation locations, not as an implementation dependency.

This repository starts with a selected source snapshot and fresh history.
Historical benchmark commit IDs refer to the earlier development repository;
see each report for what can be reproduced from this checkout.

## Project license

rscope is licensed under the [MIT License](LICENSE). Dependency licenses and
required notices are recorded in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
