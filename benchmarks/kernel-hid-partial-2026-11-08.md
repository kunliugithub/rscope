# Kernel HID partial-index demonstration

Built at Kun's explicit request to inspect missing definitions, following the
strict-parser diagnosis. This is an experimental partial index, not a complete
kernel index or an accuracy benchmark. Kernel source and Emacs configuration
were not modified. All 298 selected paths were included; files with parser
errors were not silently removed from the selection.

## Build and observed metrics

```bash
./target/release/rscope -b --allow-parse-errors \
  -f target/kernel-hid-test/rscope.db \
  -i target/kernel-hid-test/rscope.files
```

Source subtree: `/path/to/linux-7.3.3/drivers/hid`.

| Measurement | Observed |
|---|---:|
| Files selected/indexed | 298 |
| Files with parser errors | 167 |
| Saved outermost error regions | 1,310 |
| Stored occurrences | 281,423 |
| Elapsed build time | 2.029 seconds |
| User CPU time | 1.450 seconds |
| System CPU time | 0.056 seconds |
| Peak RSS | 27,916 KiB (27.3 MiB) |
| Database bytes | 37,756,928 (36.0 MiB) |
| Exit status | 0, with explicit PARTIAL INDEX warnings |
| SQLite integrity check | ok |

This was one release-build run after prior source scans; caches were not evicted.
Wall/CPU/RSS measurements use `perf_counter` and Linux `wait4` around the build
process. Counts come from the completed SQLite database. Neither timing nor
successful SQLite integrity validation establishes semantic completeness.

The raw metrics and build logs are under ignored `target/kernel-hid-test/`:
`partial-build-metrics.json`, `partial-build.stdout`, and `partial-build.stderr`.
The earlier strict failure remains recorded separately in `build-metrics.json`.

## A verified missing function definition

Source file:
`/path/to/linux-7.3.3/drivers/hid/bpf/hid_bpf_dispatch.c`, line 609:

```c
int hid_bpf_connect_device(struct hid_device *hdev)
{
```

The function body continues through line 628. It is inside the selected file
set. There is no definition record for `hid_bpf_connect_device` anywhere in
this test database:

```bash
./target/release/rscope -dL -f target/kernel-hid-test/rscope.db \
  -1 hid_bpf_connect_device
```

The command prints a PARTIAL INDEX warning on stderr and no result on stdout.
Live text search does find its signature at line 609:

```bash
./target/release/rscope -dL -f target/kernel-hid-test/rscope.db \
  -4 'int hid_bpf_connect_device'
```

The database contains an ERROR diagnostic from line 609 column 1 to column 52,
covering the signature. The surrounding BTF declaration macros have produced
recovery errors as well. Records inside the body at lines 611-612 are attributed
to `BTF_ID_FLAGS`, rather than to `hid_bpf_connect_device`. This is observed
misattribution, not merely an absence of results. The exact repair remains open.

For a positive comparison, definition lookup for `amd_sfh_set_report` returns
`amd-sfh-hid/amd_sfh_client.c:21` from the same index.

## Another missing definition, with a misleading alternative

`amd-sfh-hid/sfh1_1/amd_sfh_init.h:24` defines the initialized static object
`sfh1_1_ops`. Its definition is absent from the index. Querying that name returns
a same-named struct field at `amd-sfh-hid/amd_sfh_common.h:66`. Since the tool
matches bare names without semantic resolution, this alternative result must
not be mistaken for proof that the desired object definition was found.

## Try it in Emacs

With the existing xcscope installation loaded, evaluate:

```elisp
(setq cscope-program "/path/to/rscope/target/release/rscope"
      cscope-database-file "rscope.db"
      cscope-initial-directory "/path/to/rscope/target/kernel-hid-test/"
      cscope-database-regexps nil)
```

This selects the separate test database even when visiting kernel files outside
the rscope directory. It affects the current session's xcscope settings.

Open `drivers/hid/bpf/hid_bpf_dispatch.c` in the supplied kernel tree and go to
line 609. With Kun's bindings:

1. `M-.`, enter `hid_bpf_connect_device`: warning, no navigable definition.
2. `M-,`, enter `int hid_bpf_connect_device`: source result at line 609.
3. `M-.`, enter `amd_sfh_set_report`: a recognized definition at line 21 of
   `amd_sfh_client.c`.

Batch Emacs with the installed, unmodified xcscope 1.6 verified that definition
lookup had no navigation properties, that the warning was visible, and that
text lookup opened the actual kernel file at line 609. The local replay script
is `target/kernel-hid-test/emacs-smoke.el`.

When done, set `cscope-initial-directory` back to nil and restore the prior
database settings to resume normal project discovery. No permanent .emacs or
xcscope.el edits are needed.

## Policy and limitations

The new `--allow-parse-errors` flag is build-only and explicitly experimental.
Strict mode still rejects C parser errors. This mode skips ERROR/missing
subtrees, stores diagnostic locations, and retains recovered records, including
potentially wrong records outside error regions. Partial index versions are
distinct so older executables cannot silently query them as complete indexes.
Every query prints a warning for partial indexes. Python syntax failures and
I/O errors are not relaxed.

This demonstration does not fix the grammar, resolve macros, validate all
definitions, or justify a full-kernel completeness claim. Its purpose is to
make the consequences tangible before choosing a longer-term parsing policy.
