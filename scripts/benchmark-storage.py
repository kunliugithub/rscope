"""Compare two Linux rscope binaries on identical generated Python inputs.

Each child is measured with wait4 for wall time and peak RSS. All generated
files, databases, query output, and summary.json remain in a temporary directory.
This is a synthetic single-run comparison, not a cold-cache benchmark.
"""

import argparse
import json
import os
import tempfile
import time
from pathlib import Path


def measured(root, label, command):
    started = time.perf_counter()
    pid = os.fork()
    if pid == 0:
        try:
            with (root / f"{label}.stdout").open("wb") as out, (
                root / f"{label}.stderr"
            ).open("wb") as err:
                os.dup2(out.fileno(), 1)
                os.dup2(err.fileno(), 2)
                os.execv(command[0], command)
        except BaseException:
            os._exit(127)
    _, status, usage = os.wait4(pid, 0)
    if status != 0:
        raise RuntimeError((label, status, (root / f"{label}.stderr").read_text()))
    return {
        "wall_seconds": round(time.perf_counter() - started, 3),
        "peak_rss_kib": usage.ru_maxrss,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", required=True, type=Path)
    parser.add_argument("--after", required=True, type=Path)
    parser.add_argument("--files", type=int, default=100)
    parser.add_argument("--groups", type=int, default=333)
    args = parser.parse_args()
    if args.files < 1 or args.groups < 1:
        parser.error("--files and --groups must be positive")
    binaries = {"before": str(args.before.resolve()), "after": str(args.after.resolve())}
    for binary in binaries.values():
        if not os.access(binary, os.X_OK):
            parser.error(f"not executable: {binary}")
    root = Path(tempfile.mkdtemp(prefix="rscope-storage-bench-"))
    files = []
    for i in range(args.files):
        path = root / f"file{i:03}.py"
        lines = ["def run():\n"]
        for j in range(args.groups):
            lines.extend([
                f"    value_{j} = leaf({j})\n",
                "    # needle: synthetic payload for storage comparison, not a real workload\n",
                f"    value_{j} += 1\n",
            ])
        path.write_text("".join(lines))
        files.append(str(path))
    manifest = root / "inputs.txt"
    manifest.write_text("\n".join(files) + "\n")
    summary = {
        "directory": str(root), "files": len(files),
        "lines": args.files * (1 + 3 * args.groups),
        "source_bytes": sum(Path(file).stat().st_size for file in files), "runs": {},
    }
    for label, binary in binaries.items():
        database = str(root / f"{label}.db")
        result = measured(root, label + "-build", [binary, "-b", "-f", database, "-i", str(manifest)])
        result["database_bytes"] = Path(database).stat().st_size
        for key, flag, pattern in [("literal_query", "-4", "needle"), ("symbol_query", "-3", "leaf")]:
            query = "text" if flag == "-4" else "symbol"
            result[key] = measured(root, f"{label}-{query}", [binary, "-dL", "-f", database, flag, pattern])
        summary["runs"][label] = result
    for query in ["text", "symbol"]:
        if (root / f"before-{query}.stdout").read_bytes() != (root / f"after-{query}.stdout").read_bytes():
            raise RuntimeError(f"query outputs differ: {query}; inspect {root}")
    (root / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
