#!/usr/bin/env python3

# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

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
# Built by concatenation, and never written contiguously anywhere in this
# file (comments included). Otherwise the tool matches itself: --audit counts
# it as a violation of invariant 2, and the sweep plan's grep for the Exhibit
# B notice reports the tool instead of real hits.
EXHIBIT_B = "Incompatible With " + "Secondary Licenses"

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


SKIP_SECTION = "retained licenses"


def load_ledger(repo):
    """Paths to skip: first-column cells of the Retained licenses table.

    Deliberately narrow on two axes, each for a reason found in testing:

    - Only the *first column* of a table row counts. An earlier version took
      any backtick-quoted string containing a slash, which swept up prose,
      upstream URLs, and the tool's own path.
    - Only rows under the `## Retained licenses` heading count. Other
      sections document dispositions that are not skips: a substantial
      derivative (luggage) keeps its upstream notice *and* receives Exhibit A,
      so it must not be skipped, but it must still be recorded.
    """
    led = repo / "LICENSES.md"
    if not led.exists():
        return []
    skips = []
    in_section = False
    text = led.read_text(encoding="utf-8", errors="replace")
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("#"):
            in_section = s.lstrip("#").strip().lower() == SKIP_SECTION
            continue
        if not in_section or not s.startswith("|"):
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


#: a copyright line that is Mark's own, in either the July or the current form
OWN_NOTICE = re.compile(r"Copyright\b.*\bMark\b", re.IGNORECASE)


def restrip(body):
    """Split off a shebang, then drop an existing licence header.

    Returns (lead, stripped, rest) so a caller running with --retain-notice
    can put third-party copyright lines back above the new header.
    """
    lead = []
    i = 0
    # A shebang is `#!/...`; a Rust inner attribute `#![...]` is not one and
    # the header goes above it (found on wgpu-scry and wgpu-weld, 2026-09-03).
    if body and body[0].startswith("#!") and not body[0].startswith("#!["):
        lead.append(body[0])
        i = 1
    while i < len(body) and not body[i].strip():
        i += 1
    start = i
    while i < len(body) and HEADER_PAT.match(body[i]):
        i += 1
    if i == start:
        return lead, [], body[len(lead):]
    stripped = body[start:i]
    if i < len(body) and not body[i].strip():
        i += 1
    return lead, stripped, body[i:]


#: how many leading lines an existing licence header may occupy
HEAD_SPAN = 12


def already_covered(body):
    """A file whose leading lines already carry Exhibit A is Covered Software
    and is left alone, whatever comment shape carries it.

    genet's Servo-derived files carry Exhibit A as a `/* ... */` block, which
    HEADER_PAT (line comments only) does not see; without this check the tool
    would stack a second header on nearly five hundred of them. The July form
    (`Mark AB (markik)` + a permissive SPDX) carries no Exhibit A and is still
    replaced.
    """
    return any("Mozilla Public" in l for l in body[:HEAD_SPAN])


def strip_block_header(body):
    """Remove a leading `/* ... */` comment that carries Exhibit A (Servo's
    form), returning (removed_lines, rest). Used by --renormalize only."""
    i = 0
    while i < len(body) and not body[i].strip():
        i += 1
    if i >= len(body) or not body[i].lstrip().startswith("/*"):
        return [], body
    j = i
    while j < len(body) and "*/" not in body[j]:
        j += 1
    if j >= len(body):
        return [], body
    block = body[i:j + 1]
    if not any("Mozilla Public" in l for l in block):
        return [], body
    rest = body[j + 1:]
    if rest and not rest[0].strip():
        rest = rest[1:]
    return block, rest


def process(path, tok, bare, retain=False, renormalize=False):
    raw = path.read_bytes()
    eol = detect_eol(raw)
    enc = "utf-8-sig" if raw[:3] == b"\xef\xbb\xbf" else "utf-8"
    text = raw.decode(enc)
    body = text.split("\n")
    body = [l[:-1] if l.endswith("\r") else l for l in body]

    if already_covered(body):
        if not renormalize:
            return False, "already Exhibit A", text
        # Shape normalisation (sweep plan P7): a block-form Exhibit A becomes
        # shape C; a line-form one falls through to the ordinary replace path.
        block, rest = strip_block_header(body)
        if block:
            foreign = [l for l in block if "Copyright" in l and not OWN_NOTICE.search(l)]
            header = build_header(tok, bare)
            new = foreign + header + [""] + rest
            out = eol.join(new)
            return out.encode("utf-8") != raw, "renormalize block header", out

    had = any(HEADER_PAT.match(l) for l in body[:8])
    lead, stripped, rest = restrip(body)

    # Provenance: upstream copyright lines are never removed. Mark's own line
    # is dropped, because build_header emits the current form of it.
    foreign = []
    if retain:
        foreign = [l for l in stripped
                   if "Copyright" in l and not OWN_NOTICE.search(l)]

    header = build_header(tok, bare)
    new = lead + ([""] if lead else []) + foreign + header + [""] + rest
    out = eol.join(new)
    changed = out.encode("utf-8") != raw
    why = "replace existing header" if had else "add header"
    if foreign:
        why += " (+%d retained)" % len(foreign)
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
    ap.add_argument("--retain-notice", action="store_true",
                    help="keep third-party copyright lines above the new "
                         "header (substantial derivatives: luggage, tucket, "
                         "cambium, meristem)")
    ap.add_argument("--only", default=None,
                    help="limit to paths under this prefix")
    ap.add_argument("--renormalize", action="store_true",
                    help="rewrite an existing Exhibit A header to shape C "
                         "(sweep plan P7); with --bare, keeps it copyright-free")
    args = ap.parse_args()

    repo = args.repo.resolve()
    if args.audit:
        cmd_audit(repo)
        return 0

    if args.apply:
        # Invariant 7: never sweep a dirty tree. Another lane's in-flight files
        # would receive headers that then ride into that lane's commit, or be
        # swept into this one. Learned on genet, 2026-09-03. The check covers
        # the files this tool writes, tracked sources with a comment token:
        # the sweep's own ledger, manifest, LICENSE and README edits precede
        # --apply by design and must not trip it (learned on mora, same day).
        status = subprocess.run(
            ["git", "-C", str(repo), "status", "--porcelain", "--untracked-files=no"],
            capture_output=True, text=True, check=True,
        ).stdout.splitlines()
        dirty = [l for l in status if Path(l[3:].strip().strip('"')).suffix in COMMENT]
        if dirty:
            print("refusing --apply: source files have uncommitted changes (invariant 7):")
            print("\n".join(dirty))
            return 2

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
            did, why, out = process(path, tok, args.bare, args.retain_notice, args.renormalize)
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
