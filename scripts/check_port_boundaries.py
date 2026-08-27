#!/usr/bin/env python3

# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Enforce the one-way dependency boundary from Mere ports into crates."""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
CRATES = (ROOT / "crates").resolve()
PORTS = (ROOT / "ports").resolve()
WASM_TARGET = "wasm32-unknown-unknown"
FORBIDDEN_GRAPHSHELL_WEB_PACKAGES = {
    "turnstone",
    "genet-winit-host",
    "grafting",
    "scrying",
    "welding",
    "graft-engine",
    "scrying-engine",
    "weld-engine",
    # Servo application/runtime packages. Genet's portable layout cone still
    # carries small Servo-derived utility crates such as servo-url,
    # servo-pixels, and servo-malloc-size-of; those are not a browser runtime.
    "servo",
    "servoshell",
    "libservo",
    "constellation",
    "script",
    "script_traits",
    "layout_2020",
    "layout_thread_2020",
    "compositing",
    "compositing_traits",
    "embedder_traits",
    "webdriver_server",
}


def fail(message: str) -> None:
    print(f"port boundary failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def is_beneath(path: pathlib.Path, parent: pathlib.Path) -> bool:
    try:
        path.resolve().relative_to(parent)
    except ValueError:
        return False
    return True


def check_graphshell_web_cone() -> None:
    result = subprocess.run(
        [
            "cargo",
            "tree",
            "-p",
            "graphshell",
            "--target",
            WASM_TARGET,
            "--no-default-features",
            "--features",
            "web",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(f"Graphshell web cargo tree failed:\n{result.stderr}")

    package_names = {
        line.split(maxsplit=1)[0]
        for line in result.stdout.splitlines()
        if line.strip()
    }
    forbidden = sorted(
        name
        for name in package_names
        if name in FORBIDDEN_GRAPHSHELL_WEB_PACKAGES
        or name.startswith("libservo")
    )
    if forbidden:
        fail(
            "Graphshell web dependency cone contains forbidden packages: "
            + ", ".join(forbidden)
        )


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

    check_graphshell_web_cone()
    print("Mere port and Graphshell web dependency boundaries passed")


if __name__ == "__main__":
    main()
