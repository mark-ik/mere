#!/usr/bin/env python3

# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Regenerate the Circuit recipe's second dataset: mere's own workspace graph.

The Circuit scene reads a wired, content-addressed provenance DAG (training
runs, adapters, Flora contributions). Its founding dataset is the workspace
dependency graph, which is what proves the recipe is not Distillery's skin —
same blocks, same traces, an entirely different owner. This script writes that
graph out once so the test that reads it never has to run `cargo metadata`,
and so a reader can diff two generations of it.

Only workspace members appear. Edges are the normal and build dependencies
between members; dev-dependencies are deliberately excluded, because a
dev-dependency may legitimately point back at a crate that depends on it and
the fixture is asserted to be acyclic.

Run from anywhere; paths are resolved from this file. The output is written
with sorted keys and a trailing newline, so two runs against one commit
produce byte-identical files.
"""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "ports" / "distillery" / "tests" / "fixtures" / "circuit" / "workspace_graph.json"
# `kind` on a cargo dependency entry: null for a normal dependency, "build" for
# a build-script one, "dev" for a test-only one.
KEPT_KINDS = (None, "build")


def head_short_sha() -> str:
    """The commit this graph was read from, so a stale fixture is visible."""
    return subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def workspace_metadata() -> dict:
    """`cargo metadata` without the registry graph: members are all we read."""
    return json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )


def graph(metadata: dict) -> dict:
    members = {package["name"] for package in metadata["packages"]}
    edges = set()
    for package in metadata["packages"]:
        for dependency in package["dependencies"]:
            if dependency.get("kind") not in KEPT_KINDS:
                continue
            if dependency["name"] in members:
                edges.add((package["name"], dependency["name"]))
    return {
        "generated_from": head_short_sha(),
        "packages": sorted(members),
        "edges": sorted([source, target] for source, target in edges),
    }


def main() -> int:
    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    document = json.dumps(graph(workspace_metadata()), indent=2, sort_keys=True)
    FIXTURE.write_text(document + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {FIXTURE.relative_to(ROOT).as_posix()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
