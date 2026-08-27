#!/usr/bin/env python3
"""Apply the house MPL-2.0 header (shape C) to owned source files.

Carries the 2026-08-22 license ruling into a repository's sources. See
design_docs/2026-08-22_license_posture_brief.md for the ruling and
design_docs/mere_docs/implementation_strategy/2026-08-22_license_sweep_plan.md
for the process; invariants 1-3 are implemented here.

Provenance before license: a file receives Exhibit A only if Mark wrote it.
Paths listed in the repository's LICENSES.md are skipped entirely.

Modes:
  --dry-run   list files that would change, and why
  --apply     write the changes
  --audit     counts per repository (manifests by license, unheaded owned
              sources, Exhibit B hits, LICENSE files present)

Line endings are preserved per file. Rerunning --apply is a no-op.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

NOTICE = "Mark Alan Boykin"
YEAR = "2026"

EXHIBIT_A = [
    "This Source Code Form is subject to the terms of the Mozilla Public",
    "License, v. 2.0. If a copy of the MPL was not distributed with this",
    "file, You can obtain one at https://mozilla.org/MPL/2.0/.",
]
SPDX = "SPDX-License-Identifier: MPL-2.0"
EXHIBIT_B = "Incompatible With Secondary Licenses"

# extension -> line-comment token
COMMENT = {
    ".rs": "//",
    ".js": "//",
    ".mjs": "//",
    ".wgsl": "//",
    ".py": "#",
    ".ps1": "#",
}

# A leading comment line belongs to an existing licence header if it matches.
HEADER_PAT = re.compile(
    r"^\s*(?://|#)\s*("
    r"Copyright\b"
    r"|This Source Code Form"
    r"|License, v\. 2\.0"
    r"|file, You can obtain one"
    r"|SPDX-License-Identifier"
    r"|Licensed under"
    r"|Permission is hereby granted"
    r")",
    re.IGNORECASE,
)


def git_tracked(repo, exts):
    out = subprocess.run(
        ["git", "-C", str(repo), "ls-files", "-z"],
        capture_output=True, text=True, check=True,
    ).stdout
    files = []
    for rel in out.split("\0"):
        if not rel:
            continue
        p = Path(rel)
        if p.suffix in exts:
            files.append(p)
    return files


def load_ledger(repo):
    """Paths to skip: the first column of LICENSES.md's table rows.

    Deliberately narrow. An earlier version took any backtick-quoted string
    containing a slash, which swept up prose, upstream URLs, and the tool's
    own path. Only a leading `path` cell in a table row counts.
    """
    led = repo / "LICENSES.md"
    if not led.exists():
        return []
    skips = []
    text = led.read_text(encoding="utf-8", errors="replace")
    for line in text.splitlines():
        s = line.strip()
        if not s.startswith("|"):
            continue
        cells = [c.strip() for c in s.strip("|").split("|")]
        if not cells:
            continue
        m = re.fullmatch(r"`([^`]+)`", cells[0])
        if m:
            skips.append(m.group(1).strip().rstrip("/"))
    return sorted(set(skips))


def skipped(rel, skips):
    s = rel.as_posix()
    for sk in skips:
        if s == sk or s.startswith(sk + "/"):
            return sk
    return None


def detect_eol(raw):
    i = raw.find(b"\n")
    if i > 0 and raw[i - 1:i] == b"\r":
        return "\r\n"
    return "\n"


def build_header(tok, bare):
    lines = []
    if not bare:
        lines.append(tok + " Copyright " + YEAR + " " + NOTICE)
    for l in EXHIBIT_A:
        lines.append(tok + " " + l)
    lines.append(tok + " " + SPDX)
    return lines


def restrip(body):
    """Split off a leading shebang, then drop an existing licence header."""
    lead = []
    i = 0
    if body and body[0].startswith("#!"):
        lead.append(body[0])
        i = 1
    while i < len(body) and not body[i].strip():
        i += 1
    start = i
    while i < len(body) and HEADER_PAT.match(body[i]):
        i += 1
    if i == start:
        return lead, body[len(lead):]
    if i < len(body) and not body[i].strip():
        i += 1
    return lead, body[i:]


def process(path, tok, bare):
    raw = path.read_bytes()
    eol = detect_eol(raw)
    enc = "utf-8-sig" if raw[:3] == b"\xef\xbb\xbf" else "utf-8"
    text = raw.decode(enc)
    body = text.split("\n")
    body = [l[:-1] if l.endswith("\r") else l for l in body]

    had = any(HEADER_PAT.match(l) for l in body[:8])
    lead, rest = restrip(body)
    header = build_header(tok, bare)
    new = lead + ([""] if lead else []) + header + [""] + rest
    out = eol.join(new)
    changed = out.encode("utf-8") != raw
    why = "replace existing header" if had else "add header"
    return changed, why, out


def cmd_audit(repo):
    manifests = subprocess.run(
        ["git", "-C", str(repo), "ls-files", "*Cargo.toml"],
        capture_output=True, text=True).stdout.split()
    counts = {}
    for m in manifests:
        try:
            t = (repo / m).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        mm = re.search(r'^license\s*=\s*"([^"]+)"', t, re.M)
        if mm:
            key = mm.group(1)
        elif "license.workspace" in t:
            key = "workspace"
        else:
            key = "(none)"
        counts[key] = counts.get(key, 0) + 1

    skips = load_ledger(repo)
    srcs = git_tracked(repo, list(COMMENT))
    owned = [p for p in srcs if not skipped(p, skips)]
    unheaded = 0
    exhibit_b = 0
    for p in owned:
        try:
            t = (repo / p).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if "Mozilla Public" not in t:
            unheaded += 1
        if EXHIBIT_B in t:
            exhibit_b += 1

    lic = sorted(x.name for x in repo.glob("LICENSE*"))
    print("repo                 " + repo.name)
    print("manifests            " + str(dict(sorted(counts.items()))))
    print("owned sources        %d (of %d tracked)" % (len(owned), len(srcs)))
    print("  without Exhibit A  %d   (must be 0)" % unheaded)
    print("  Exhibit B hits     %d   (must be 0)" % exhibit_b)
    print("LICENSE files        " + (str(lic) if lic else "(none)"))
    print("ledger paths         %d  %s" % (len(skips), skips))


def main():
    ap = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo", default=".", type=Path)
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--dry-run", action="store_true")
    g.add_argument("--apply", action="store_true")
    g.add_argument("--audit", action="store_true")
    ap.add_argument("--bare", action="store_true",
                    help="Exhibit A with no copyright line (third-party-derived)")
    ap.add_argument("--only", default=None,
                    help="limit to paths under this prefix")
    args = ap.parse_args()

    repo = args.repo.resolve()
    if args.audit:
        cmd_audit(repo)
        return 0

    skips = load_ledger(repo)
    files = git_tracked(repo, list(COMMENT))
    changed = kept = skip_n = 0
    for rel in files:
        if skipped(rel, skips):
            skip_n += 1
            continue
        if args.only and not rel.as_posix().startswith(args.only):
            continue
        tok = COMMENT[rel.suffix]
        path = repo / rel
        try:
            did, why, out = process(path, tok, args.bare)
        except (UnicodeDecodeError, OSError) as e:
            print("  SKIP (unreadable) %s: %s" % (rel, e))
            continue
        if not did:
            kept += 1
            continue
        changed += 1
        if args.dry_run:
            print("  %-24s %s" % (why, rel))
        else:
            path.write_bytes(out.encode("utf-8"))

    verb = "would change" if args.dry_run else "changed"
    print("\n%s: %d   already correct: %d   ledger-skipped: %d"
          % (verb, changed, kept, skip_n))
    return 0


if __name__ == "__main__":
    sys.exit(main())
