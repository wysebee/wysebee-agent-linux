#!/usr/bin/env python3
"""Write the licenses of every crate built into wysebee-agent for one target.

    python3 scripts/third_party_licenses.py <target triple> <output file>

It asks cargo which crates `agent-cli` depends on for that target (normal dependencies
only: no build or dev ones, which are not in the binary), then copies each crate's own
license and notice files from cargo's source cache. A crate that ships no such file is
listed with its declared license expression. build_linux.sh puts the result in each
release package as THIRD_PARTY_LICENSES.txt.
"""

import json
import os
import subprocess
import sys

PREFIXES = ("license", "licence", "copying", "notice", "copyright", "unlicense")
ROOT_PACKAGE = "agent-cli"


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    target, out_path = sys.argv[1], sys.argv[2]
    meta = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    workspace = set(meta["workspace_members"])
    root = next(i for i in workspace if packages[i]["name"] == ROOT_PACKAGE)

    # Walk normal dependencies only.
    seen, todo = set(), [root]
    while todo:
        node = nodes[todo.pop()]
        for dep in node["deps"]:
            normal = any(k.get("kind") is None for k in dep["dep_kinds"])
            if normal and dep["pkg"] not in seen:
                seen.add(dep["pkg"])
                todo.append(dep["pkg"])
    crates = sorted((packages[i] for i in seen - workspace), key=lambda p: (p["name"], p["version"]))

    parts = [
        "Third-party licenses for wysebee-agent\n",
        f"The crates below are built into the binary for {target}.\n",
        "Each is followed by its own license and notice files, as it ships them.\n",
    ]
    for p in crates:
        folder = os.path.dirname(p["manifest_path"])
        header = f"\n{'=' * 78}\n{p['name']} {p['version']}  —  {p.get('license') or 'see below'}\n"
        if p.get("repository"):
            header += f"{p['repository']}\n"
        parts.append(header + "=" * 78 + "\n")
        files = sorted(
            f for f in os.listdir(folder)
            if f.lower().startswith(PREFIXES) and os.path.isfile(os.path.join(folder, f))
        )
        if not files and p.get("license_file"):
            files = [p["license_file"]]
        if not files:
            parts.append(f"\n(No license file in the crate; its declared license is {p.get('license')}.)\n")
        for f in files:
            with open(os.path.join(folder, f), encoding="utf-8", errors="replace") as fh:
                parts.append(f"\n--- {f} ---\n\n{fh.read().rstrip()}\n")

    with open(out_path, "w", encoding="utf-8") as out:
        out.write("".join(parts))
    print(f"== {out_path}: {len(crates)} crates")


if __name__ == "__main__":
    main()
