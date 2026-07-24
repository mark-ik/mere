#!/usr/bin/env python3
"""Enforce the one-way dependency boundary from Mere ports into crates."""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
CRATES = (ROOT / "crates").resolve()
PORTS = (ROOT / "ports").resolve()


def fail(message: str) -> None:
    print(f"port boundary failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def is_beneath(path: pathlib.Path, parent: pathlib.Path) -> bool:
    try:
        path.resolve().relative_to(parent)
    except ValueError:
        return False
    return True


def main() -> None:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(f"cargo metadata failed:\n{result.stderr}")

    packages = json.loads(result.stdout)["packages"]
    graphshell_manifests = []
    for package in packages:
        manifest = pathlib.Path(package["manifest_path"]).resolve()
        if package["name"] == "graphshell":
            graphshell_manifests.append(manifest)
        if not is_beneath(manifest, CRATES):
            continue
        for dependency in package["dependencies"]:
            path = dependency.get("path")
            if path is not None and is_beneath(pathlib.Path(path), PORTS):
                fail(
                    f"{manifest.relative_to(ROOT)} depends on port package "
                    f"{dependency['name']} at {pathlib.Path(path).relative_to(ROOT)}"
                )

    expected = (PORTS / "graphshell" / "Cargo.toml").resolve()
    if graphshell_manifests != [expected]:
        rendered = [str(path.relative_to(ROOT)) for path in graphshell_manifests]
        fail(f"graphshell manifests are {rendered}, expected ports/graphshell/Cargo.toml")

    print("Mere port boundary passed")


if __name__ == "__main__":
    main()
