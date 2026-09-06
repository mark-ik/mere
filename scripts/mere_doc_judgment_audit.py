#!/usr/bin/env python3
"""Verify D2 judgment coverage for Mere's active design-document tree."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "design_docs"
LEDGER = ROOT / "support" / "doc-audit" / "d2"
SNAPSHOT = LEDGER / "snapshot_281_aggregate.json"
CORRECTIONS = LEDGER / "legacy_corrections.json"
SNAPSHOT_SHA256 = "f0b64eef4cd5514157bb1b49d389b84db29a1e823d6f3e2cebf827337fe56abf"

DOC_HEADER = re.compile(r"^##\s+`?(.+?\.md)`?\s*$", re.MULTILINE)
DISPOSITION = re.compile(
    r"^- disposition:\s*`?(current|historical-marked|historical-unmarked|superseded|dead)`?\s*$",
    re.MULTILINE | re.IGNORECASE,
)
STATUS = re.compile(
    r'^- status line:\s*".*?"\s*[—–-]+\s*accurate:\s*`?(yes|no|n/a)`?\s*$',
    re.MULTILINE | re.IGNORECASE,
)
COUNTS = re.compile(
    r"^- claims checked:\s*(\d+)\s*[—–-]+\s*holds:\s*(\d+),\s*stale:\s*(\d+),\s*unverifiable:\s*(\d+)\s*$",
    re.MULTILINE | re.IGNORECASE,
)
REQUIRED_SECTIONS = (
    "### Stale claims",
    "### Contradictions",
    "### Recommended action",
    "### Notes",
)


def active_paths() -> list[str]:
    return sorted(
        path.relative_to(DOCS).as_posix()
        for path in DOCS.rglob("*.md")
        if "archive_docs" not in path.relative_to(DOCS).parts
    )


def parse_supplements() -> tuple[dict[str, dict[str, int | str]], list[str]]:
    records: dict[str, dict[str, int | str]] = {}
    errors: list[str] = []
    for batch in sorted(LEDGER.glob("batch_*.md")):
        text = batch.read_text(encoding="utf-8")
        headers = list(DOC_HEADER.finditer(text))
        for index, header in enumerate(headers):
            path = header.group(1).replace("\\", "/")
            if path.startswith("design_docs/"):
                path = path.removeprefix("design_docs/")
            body = text[header.end() : headers[index + 1].start() if index + 1 < len(headers) else len(text)]
            if path in records:
                errors.append(f"duplicate supplemental record: {path}")
                continue
            disposition = DISPOSITION.search(body)
            status = STATUS.search(body)
            counts = COUNTS.search(body)
            missing_sections = [section for section in REQUIRED_SECTIONS if section not in body]
            if not disposition:
                errors.append(f"{batch.name}: {path}: missing or invalid disposition")
            if not status:
                errors.append(f"{batch.name}: {path}: missing or invalid status verdict")
            if not counts:
                errors.append(f"{batch.name}: {path}: missing or invalid claim counts")
            if missing_sections:
                errors.append(f"{batch.name}: {path}: missing sections: {', '.join(missing_sections)}")
            if not (disposition and status and counts) or missing_sections:
                continue
            checked, holds, stale, unverifiable = map(int, counts.groups())
            if checked != holds + stale + unverifiable:
                errors.append(
                    f"{batch.name}: {path}: counts do not sum "
                    f"({checked} != {holds}+{stale}+{unverifiable})"
                )
            records[path] = {
                "disposition": disposition.group(1).lower(),
                "status_accurate": status.group(1).lower(),
                "claims": checked,
                "holds": holds,
                "stale": stale,
                "unverifiable": unverifiable,
            }
    return records, errors


def audit() -> dict[str, object]:
    snapshot_bytes = SNAPSHOT.read_bytes()
    snapshot_hash = hashlib.sha256(snapshot_bytes).hexdigest()
    snapshot = json.loads(snapshot_bytes)
    legacy = {path: dict(record) for path, record in snapshot.get("docs", {}).items()}
    corrections = json.loads(CORRECTIONS.read_text(encoding="utf-8"))
    for path, correction in corrections.items():
        if path not in legacy:
            raise KeyError(f"legacy correction names an unknown record: {path}")
        for field in ("claims", "holds", "stale", "unverifiable"):
            if field in correction:
                legacy[path][field] = correction[field]
    active = active_paths()
    supplements, errors = parse_supplements()
    if snapshot_hash != SNAPSHOT_SHA256:
        errors.append(
            f"snapshot aggregate digest changed: {snapshot_hash} != {SNAPSHOT_SHA256}"
        )

    basenames: dict[str, list[str]] = {}
    for path in active:
        basenames.setdefault(Path(path).name, []).append(path)
    duplicate_basenames = {name: paths for name, paths in basenames.items() if len(paths) > 1}
    if duplicate_basenames:
        errors.extend(
            f"duplicate active basename {name}: {', '.join(paths)}"
            for name, paths in sorted(duplicate_basenames.items())
        )

    legacy_root_names = {name for name in legacy if "/" not in name and "\\" not in name}
    legacy_active = {
        path
        for path in active
        if path in legacy or ("/" not in path and Path(path).name in legacy_root_names)
    }
    supplemental_active = {path for path in active if path in supplements}
    covered = legacy_active | supplemental_active
    missing = sorted(set(active) - covered)
    unknown_supplements = sorted(set(supplements) - set(active))
    overlap = sorted(legacy_active & supplemental_active)
    if missing:
        errors.extend(f"active document lacks D2 record: {path}" for path in missing)
    if unknown_supplements:
        errors.extend(f"supplemental record is not active: {path}" for path in unknown_supplements)
    if overlap:
        errors.extend(f"supplement duplicates legacy record: {path}" for path in overlap)

    active_set = set(active)
    inactive_legacy = sorted(
        name
        for name in legacy
        if name.replace("\\", "/") not in active_set
        and not ("/" not in name and "\\" not in name and name in basenames)
    )
    combined_records = [*legacy.values(), *supplements.values()]
    dispositions = Counter(str(record.get("disposition", "(missing)")) for record in combined_records)
    totals = {
        field: sum(int(record.get(field, 0) or 0) for record in combined_records)
        for field in ("claims", "holds", "stale", "unverifiable")
    }
    for path, record in legacy.items():
        classified = sum(int(record.get(field, 0) or 0) for field in ("holds", "stale", "unverifiable"))
        if int(record.get("claims", 0) or 0) != classified:
            errors.append(
                f"legacy record counts do not sum after corrections: {path} "
                f"({record.get('claims', 0)} != {classified})"
            )
    return {
        "active_docs": len(active),
        "covered_active_docs": len(covered),
        "legacy_records": len(legacy),
        "legacy_snapshot_sha256": snapshot_hash,
        "legacy_corrections": len(corrections),
        "legacy_active_records": len(legacy_active),
        "supplemental_records": len(supplements),
        "inactive_legacy_records": len(inactive_legacy),
        "combined_records": len(combined_records),
        "combined_dispositions": dict(sorted(dispositions.items())),
        "combined_claim_totals": totals,
        "missing_active_records": missing,
        "unknown_supplemental_records": unknown_supplements,
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    report = audit()
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(
            "D2 coverage: "
            f"{report['covered_active_docs']}/{report['active_docs']} active; "
            f"{report['legacy_active_records']} legacy + "
            f"{report['supplemental_records']} supplemental; "
            f"{report['inactive_legacy_records']} inactive legacy"
        )
        for error in report["errors"]:
            print(f"error: {error}")
    return 1 if report["errors"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
