#!/usr/bin/env python3
"""Audit Mere's active design-doc tree without modifying it.

The documentation-policy plan's D3 exit condition is deliberately narrow:
canonical-index coverage, links into private memory directories, plan Status
lines, relative-link resolution, and cited paths under known source roots.
This script reports those facts for `design_docs/` while excluding every
`archive_docs/` subtree.  It is an audit, not a mass-rewriter.
Ambiguous bare crate roots, glob examples, and versioned Mere protocol/schema
identifiers are reported separately as informational exclusions rather than
being mistaken for concrete missing paths.

Run from any directory:

    python scripts/mere_doc_audit.py
    python scripts/mere_doc_audit.py --self-test

`--self-test` checks the accepted Status-header forms and rejects status prose,
then plants a defect in every reported category and replaces the fixture with
a clean tree.  A reported zero is therefore evidence that the detector can
produce a non-zero.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tempfile
from dataclasses import asdict, dataclass, field
from pathlib import Path
from urllib.parse import unquote


MARKDOWN_LINK = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")
CODE_SPAN = re.compile(r"`([^`\n]+)`")
FENCE_OPEN = re.compile(r"^[ \t]{0,3}(`{3,}|~{3,})")
FENCE_CLOSE = re.compile(r"^[ \t]{0,3}(`+|~+)[ \t]*(?:\r?\n)?$")
AUDIT_SUFFIX = re.compile(
    r"[ \t]+\*\((?P<label>historical citation|planned target)\)\*[ \t]+"
    r"(?P<comment><!-- doc-audit: (?P<kind>historical-link|historical-path|planned-link|planned-path) -->)"
)
HISTORICAL_COMMENT = re.compile(r"<!--[^\r\n>]*doc-audit:[^\r\n>]*-->")
AUDIT_LABEL = re.compile(r"\*\((?:historical citation|planned target)\)\*")
# A plan header may use a plain label, bold the word, or bold the whole label.
# Dated and reconciled headers put their qualifier in parentheses before the
# colon.  Keep the expression line-anchored so prose such as "the Status:"
# does not count as a plan header.
STATUS_LINE = re.compile(
    r"(?im)^\s*(?:\*\*)?status(?:\s*\([^\)\r\n]*\))?(?:\*\*)?\s*:(?:\*\*)?"
)
PLAN_HEADING = re.compile(r"(?im)^#.*\bplan\b")
LINE_SUFFIX = re.compile(r":\d+(?::\d+)?$")

# Paths in prose can be checked only when their starting point is unambiguous.
# `components/` is deliberately mapped to the Genet sibling: it is a common
# cross-repository source citation in Mere documentation, not a Mere directory.
# A crate-qualified path has an unambiguous starting point in this workspace.
# Bare `src/`, `tests/`, and `examples/` paths do not: they are commonly
# relative to whichever sibling crate a document is discussing.  Treat those
# as prose until a document supplies an explicit crate/repository prefix.
LOCAL_ROOTS = ("crates/", "ports/", "apps/", "design_docs/")
AMBIGUOUS_LOCAL_ROOTS = ("src/", "tests/", "examples/")
SIBLING_ROOTS = {
    "components/": ("genet", "components"),
    "genet/": ("genet", ""),
    "mere/": ("mere", ""),
}
MEMORY_MARKERS = (".codex/", ".claude/", "/memories/", "\\.codex\\", "\\.claude\\", "\\memories\\")
GLOB_MARKERS = frozenset("*?{}[]")
VERSIONED_MERE_PATH = re.compile(r"^mere/(?:[^/]+/)+v\d+(?:\.\d+)*$")
EXCLUSION_DETAILS = {
    "ambiguous_known_root_paths": "bare src/tests/examples path has no crate context",
    "ignored_known_root_patterns": "glob or pattern syntax is not a concrete path",
    "reserved_identifier_paths": "versioned Mere protocol/schema identifier is not a filesystem path",
}


@dataclass
class Finding:
    source: str
    subject: str
    detail: str


@dataclass
class Report:
    active_docs: int = 0
    indexed_active_docs: int = 0
    index_orphans: list[Finding] = field(default_factory=list)
    index_ghosts: list[Finding] = field(default_factory=list)
    memory_directory_links: list[Finding] = field(default_factory=list)
    statusless_plans: list[Finding] = field(default_factory=list)
    broken_relative_links: list[Finding] = field(default_factory=list)
    historical_broken_relative_links: list[Finding] = field(default_factory=list)
    planned_broken_relative_links: list[Finding] = field(default_factory=list)
    ambiguous_known_root_paths: list[Finding] = field(default_factory=list)
    ignored_known_root_patterns: list[Finding] = field(default_factory=list)
    reserved_identifier_paths: list[Finding] = field(default_factory=list)
    missing_known_root_paths: list[Finding] = field(default_factory=list)
    historical_missing_known_root_paths: list[Finding] = field(default_factory=list)
    planned_missing_known_root_paths: list[Finding] = field(default_factory=list)
    invalid_historical_annotations: list[Finding] = field(default_factory=list)
    stale_historical_annotations: list[Finding] = field(default_factory=list)

    def counts(self) -> dict[str, int]:
        return {
            "active_docs": self.active_docs,
            "indexed_active_docs": self.indexed_active_docs,
            "index_orphans": len(self.index_orphans),
            "index_ghosts": len(self.index_ghosts),
            "memory_directory_links": len(self.memory_directory_links),
            "statusless_plans": len(self.statusless_plans),
            "broken_relative_links": len(self.broken_relative_links),
            "historical_broken_relative_links": len(self.historical_broken_relative_links),
            "planned_broken_relative_links": len(self.planned_broken_relative_links),
            "ambiguous_known_root_paths": len(self.ambiguous_known_root_paths),
            "ignored_known_root_patterns": len(self.ignored_known_root_patterns),
            "reserved_identifier_paths": len(self.reserved_identifier_paths),
            "missing_known_root_paths": len(self.missing_known_root_paths),
            "historical_missing_known_root_paths": len(self.historical_missing_known_root_paths),
            "planned_missing_known_root_paths": len(self.planned_missing_known_root_paths),
            "invalid_historical_annotations": len(self.invalid_historical_annotations),
            "stale_historical_annotations": len(self.stale_historical_annotations),
        }

    def has_findings(self) -> bool:
        informational = {
            "active_docs",
            "indexed_active_docs",
            "ambiguous_known_root_paths",
            "ignored_known_root_patterns",
            "reserved_identifier_paths",
            "historical_broken_relative_links",
            "historical_missing_known_root_paths",
            "planned_broken_relative_links",
            "planned_missing_known_root_paths",
        }
        return any(value for key, value in self.counts().items() if key not in informational)


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def is_active_doc(path: Path, docs_root: Path) -> bool:
    return path.suffix.lower() == ".md" and "archive_docs" not in path.relative_to(docs_root).parts


def active_docs(docs_root: Path) -> list[Path]:
    return sorted(path for path in docs_root.rglob("*.md") if is_active_doc(path, docs_root))


def normalise_target(raw: str) -> str:
    target = raw.strip()
    # `<user-home>` is a non-portable private-location placeholder used by
    # historical docs. It is immediately followed by a Windows path, so the
    # generic angle-bracket handling below would otherwise truncate it to the
    # false relative target `user-home`.
    if target.lower().startswith("<user-home>"):
        return ""
    if target.startswith("<") and ">" in target:
        target = target[1 : target.index(">")]
    else:
        target = target.split(maxsplit=1)[0]
    return unquote(target.split("#", maxsplit=1)[0])


def markdown_targets(text: str) -> list[str]:
    return [normalise_target(match.group(1)) for match in MARKDOWN_LINK.finditer(text)]


def citation_annotation_after(
    text: str, offset: int
) -> tuple[str, str, tuple[int, int], tuple[int, int]] | None:
    """Return an exact same-line historical annotation after an occurrence."""
    match = AUDIT_SUFFIX.match(text, offset)
    if match is None:
        return None
    return match.group("label"), match.group("kind"), match.span("comment"), match.span()


def offset_in_ranges(offset: int, ranges: list[tuple[int, int]]) -> bool:
    return any(start <= offset < end for start, end in ranges)


def fenced_code_ranges(text: str) -> list[tuple[int, int]]:
    """Return CommonMark-style fenced blocks, including longer closing fences."""
    ranges: list[tuple[int, int]] = []
    active_start: int | None = None
    active_char = ""
    active_length = 0
    offset = 0
    for line in text.splitlines(keepends=True):
        if active_start is None:
            opened = FENCE_OPEN.match(line)
            if opened is not None:
                token = opened.group(1)
                active_start = offset
                active_char = token[0]
                active_length = len(token)
        else:
            closed = FENCE_CLOSE.match(line)
            if closed is not None:
                token = closed.group(1)
                if token[0] == active_char and len(token) >= active_length:
                    ranges.append((active_start, offset + len(line)))
                    active_start = None
        offset += len(line)
    if active_start is not None:
        ranges.append((active_start, len(text)))
    return ranges


def is_external_target(target: str) -> bool:
    lowered = target.lower()
    # Four legacy prose citations use Markdown-looking `(memory)` without a
    # file target. Exempt only that exact marker, not real relative paths.
    return not target or lowered == "memory" or target.startswith("#") or "://" in target or lowered.startswith(("mailto:", "data:"))


def is_memory_target(target: str) -> bool:
    lowered = target.replace("/", "\\").lower()
    return any(marker.replace("/", "\\") in lowered for marker in MEMORY_MARKERS)


def safe_resolve(base: Path, target: str) -> Path:
    return (base / target).resolve(strict=False)


def in_docs_tree(path: Path, docs_root: Path) -> bool:
    try:
        path.relative_to(docs_root)
    except ValueError:
        return False
    return True


def line_number(text: str, needle: str) -> int:
    offset = text.find(needle)
    return text.count("\n", 0, offset) + 1 if offset >= 0 else 1


def finding(path: Path, docs_root: Path, subject: str, detail: str) -> Finding:
    return Finding(path.relative_to(docs_root).as_posix(), subject, detail)


def index_targets(index: Path, docs_root: Path) -> tuple[set[Path], list[Finding]]:
    text = read_text(index)
    targets: set[Path] = set()
    ghosts: list[Finding] = []
    for target in markdown_targets(text):
        if is_external_target(target):
            continue
        resolved = safe_resolve(index.parent, target)
        if not in_docs_tree(resolved, docs_root) or not resolved.name.endswith(".md"):
            continue
        if "archive_docs" in resolved.relative_to(docs_root).parts:
            continue
        if resolved.exists() and resolved.is_file():
            targets.add(resolved)
        else:
            ghosts.append(finding(index, docs_root, target, "index target does not exist"))
    return targets, ghosts


def is_plan(path: Path, text: str) -> bool:
    lowered_name = path.name.lower()
    return "plan" in lowered_name or "implementation_strategy" in path.parts


def normalise_code_span(span: str) -> str | None:
    candidate = span.strip().replace("\\", "/")
    candidate = LINE_SUFFIX.sub("", candidate).rstrip(".,;:)")
    if not candidate or " " in candidate or "/" not in candidate or "<" in candidate or ">" in candidate:
        return None
    return candidate


def known_root_candidate(span: str) -> str | None:
    candidate = normalise_code_span(span)
    if candidate is None:
        return None
    if any(marker in candidate for marker in GLOB_MARKERS):
        return None
    if candidate.startswith(AMBIGUOUS_LOCAL_ROOTS):
        return None
    # These are protocol/schema identifiers (for example `mere/cable/v1`),
    # not filesystem paths.  Keep concrete `mere/crates/...` citations on the
    # normal sibling resolver.
    if VERSIONED_MERE_PATH.fullmatch(candidate):
        return None
    if candidate.startswith(LOCAL_ROOTS) or candidate.startswith(tuple(SIBLING_ROOTS)) or candidate.startswith("repos/"):
        return candidate
    return None


def excluded_known_root_candidate(span: str) -> tuple[str, str] | None:
    """Return an auditable exclusion and its report field, if one applies."""
    candidate = normalise_code_span(span)
    if candidate is None:
        return None
    known_prefix = candidate.startswith(LOCAL_ROOTS + AMBIGUOUS_LOCAL_ROOTS)
    known_prefix = known_prefix or candidate.startswith(tuple(SIBLING_ROOTS)) or candidate.startswith("repos/")
    if not known_prefix:
        return None
    if candidate.startswith(AMBIGUOUS_LOCAL_ROOTS):
        return candidate, "ambiguous_known_root_paths"
    if any(marker in candidate for marker in GLOB_MARKERS):
        return candidate, "ignored_known_root_patterns"
    if VERSIONED_MERE_PATH.fullmatch(candidate):
        return candidate, "reserved_identifier_paths"
    return None


def resolve_known_root(candidate: str, repo_root: Path, workspace_root: Path) -> Path | None:
    if candidate.startswith(LOCAL_ROOTS):
        return safe_resolve(repo_root, candidate)
    for prefix, (sibling, root) in SIBLING_ROOTS.items():
        if candidate.startswith(prefix):
            suffix = candidate[len(prefix) :]
            return safe_resolve(workspace_root / sibling / root, suffix)
    if candidate.startswith("repos/"):
        return safe_resolve(workspace_root.parent, candidate)
    return None


def audit(repo_root: Path) -> Report:
    repo_root = repo_root.resolve()
    docs_root = repo_root / "design_docs"
    index = docs_root / "DOC_README.md"
    if not index.is_file():
        raise FileNotFoundError(f"canonical index not found: {index}")

    report = Report()
    docs = active_docs(docs_root)
    report.active_docs = len(docs)
    indexed, report.index_ghosts = index_targets(index, docs_root)
    report.indexed_active_docs = len(indexed)
    report.index_orphans = [
        finding(path, docs_root, path.relative_to(docs_root).as_posix(), "active document is absent from DOC_README.md")
        for path in docs
        if path not in indexed and path.name not in {"DOC_README.md"}
    ]

    # Code/repos is the workspace root; its child directories are repositories.
    workspace_root = repo_root.parent
    for path in docs:
        text = read_text(path)
        fenced_ranges = fenced_code_ranges(text)
        inline_code_ranges = [
            match.span() for match in CODE_SPAN.finditer(text)
            if not offset_in_ranges(match.start(), fenced_ranges)
        ]
        markdown_link_ranges = [
            match.span() for match in MARKDOWN_LINK.finditer(text)
            if not offset_in_ranges(match.start(), fenced_ranges + inline_code_ranges)
        ]
        consumed_annotations: set[tuple[int, int]] = set()
        consumed_annotation_ranges: list[tuple[int, int]] = []
        for match in MARKDOWN_LINK.finditer(text):
            if offset_in_ranges(match.start(), fenced_ranges + inline_code_ranges):
                continue
            target = normalise_target(match.group(1))
            if is_external_target(target):
                continue
            if is_memory_target(target):
                report.memory_directory_links.append(finding(path, docs_root, target, "link enters a private memory directory"))
                continue
            resolved = safe_resolve(path.parent, target)
            annotation = citation_annotation_after(text, match.end())
            if annotation is not None:
                label, kind, marker_span, annotation_span = annotation
                consumed_annotations.add(marker_span)
                consumed_annotation_ranges.append(annotation_span)
                valid_kind = "historical-link" if label == "historical citation" else "planned-link"
                if kind != valid_kind:
                    report.invalid_historical_annotations.append(
                        finding(path, docs_root, target, f"relative link has a mismatched {kind} annotation")
                    )
                elif resolved.exists():
                    report.stale_historical_annotations.append(
                        finding(path, docs_root, target, "annotated relative link now resolves")
                    )
                elif kind == "historical-link":
                    report.historical_broken_relative_links.append(
                        finding(path, docs_root, target, "unresolved relative link is explicitly historical")
                    )
                else:
                    report.planned_broken_relative_links.append(
                        finding(path, docs_root, target, "unresolved relative link is an explicit planned target")
                    )
            elif not resolved.exists():
                report.broken_relative_links.append(finding(path, docs_root, target, "relative link target does not exist"))

        if is_plan(path, text) and STATUS_LINE.search(text) is None:
            report.statusless_plans.append(finding(path, docs_root, path.name, "plan has no Status: line"))

        for match in CODE_SPAN.finditer(text):
            if offset_in_ranges(match.start(), fenced_ranges + markdown_link_ranges):
                continue
            span = match.group(1)
            excluded = excluded_known_root_candidate(span)
            if excluded is not None:
                candidate, category = excluded
                getattr(report, category).append(
                    finding(path, docs_root, candidate, EXCLUSION_DETAILS[category])
                )
                continue
            candidate = known_root_candidate(span)
            if candidate is None:
                continue
            resolved = resolve_known_root(candidate, repo_root, workspace_root)
            annotation = citation_annotation_after(text, match.end())
            if annotation is not None:
                label, kind, marker_span, annotation_span = annotation
                consumed_annotations.add(marker_span)
                consumed_annotation_ranges.append(annotation_span)
                valid_kind = "historical-path" if label == "historical citation" else "planned-path"
                if kind != valid_kind:
                    report.invalid_historical_annotations.append(
                        finding(path, docs_root, candidate, f"known-root path has a mismatched {kind} annotation")
                    )
                elif resolved is not None and resolved.exists():
                    report.stale_historical_annotations.append(
                        finding(path, docs_root, candidate, "annotated known-root path now resolves")
                    )
                elif resolved is not None and kind == "historical-path":
                    report.historical_missing_known_root_paths.append(
                        finding(path, docs_root, candidate, f"unresolved known-root path is explicitly historical: {resolved}")
                    )
                elif resolved is not None:
                    report.planned_missing_known_root_paths.append(
                        finding(path, docs_root, candidate, f"unresolved known-root path is an explicit planned target: {resolved}")
                    )
            elif resolved is not None and not resolved.exists():
                report.missing_known_root_paths.append(
                    finding(path, docs_root, candidate, f"resolved path does not exist: {resolved}")
                )

        invalid_annotation_lines: set[int] = set()
        for marker in HISTORICAL_COMMENT.finditer(text):
            if offset_in_ranges(marker.start(), fenced_ranges + inline_code_ranges):
                continue
            if marker.span() not in consumed_annotations:
                invalid_annotation_lines.add(text.rfind("\n", 0, marker.start()) + 1)
                report.invalid_historical_annotations.append(
                    finding(path, docs_root, marker.group(0), "historical annotation is malformed, detached, or does not follow an audited citation")
                )
        for label in AUDIT_LABEL.finditer(text):
            if offset_in_ranges(label.start(), fenced_ranges + inline_code_ranges):
                continue
            if offset_in_ranges(label.start(), consumed_annotation_ranges):
                continue
            line_start = text.rfind("\n", 0, label.start()) + 1
            if line_start in invalid_annotation_lines:
                continue
            report.invalid_historical_annotations.append(
                finding(path, docs_root, label.group(0), "historical citation label has no valid same-line audit annotation")
            )
    return report


def print_report(report: Report, json_output: bool) -> None:
    if json_output:
        payload = asdict(report)
        payload["counts"] = report.counts()
        print(json.dumps(payload, indent=2))
        return
    counts = report.counts()
    print("Mere active-doc audit")
    for key, value in counts.items():
        print(f"  {key}: {value}")
    for category in (
        "index_orphans",
        "index_ghosts",
        "memory_directory_links",
        "statusless_plans",
        "broken_relative_links",
        "historical_broken_relative_links",
        "planned_broken_relative_links",
        "ambiguous_known_root_paths",
        "ignored_known_root_patterns",
        "reserved_identifier_paths",
        "missing_known_root_paths",
        "historical_missing_known_root_paths",
        "planned_missing_known_root_paths",
        "invalid_historical_annotations",
        "stale_historical_annotations",
    ):
        findings: list[Finding] = getattr(report, category)
        for item in findings[:12]:
            print(f"  {category}: {item.source}: {item.subject} ({item.detail})")
        if len(findings) > 12:
            print(f"  {category}: ... {len(findings) - 12} more")


def write_fixture(root: Path, defective: bool) -> None:
    docs = root / "design_docs"
    docs.mkdir(parents=True)
    (docs / "DOC_README.md").write_text("# Index\n- [active](active_plan.md)\n" + ("- [ghost](ghost.md)\n" if defective else ""), encoding="utf-8")
    (docs / "active_plan.md").write_text("# Active Plan\n\n**Status:** current\n", encoding="utf-8")
    if defective:
        (docs / "orphan.md").write_text("# Orphan\n", encoding="utf-8")
        (docs / "statusless_plan.md").write_text("# Statusless Plan\n", encoding="utf-8")
        (docs / "bad_links.md").write_text(
            "# Links\n\n**Status:** current\n\n"
            "[private](C:/Users/mark_/.codex/memories/secret.md)\n"
            "[user home](<user-home>\\.claude\\memory\\note.md)\n"
            "[prose](memory)\n"
            "[missing](missing.md)\n"
            "[old link](old.md) *(historical citation)* <!-- doc-audit: historical-link -->\n"
            "[`crates/old-label`](old-label.md) *(historical citation)* <!-- doc-audit: historical-link -->\n"
            "[planned link](future.md) *(planned target)* <!-- doc-audit: planned-link -->\n"
            "[wrong kind](wrong.md) *(historical citation)* <!-- doc-audit: historical-path -->\n"
            "[live marked](active_plan.md) *(historical citation)* <!-- doc-audit: historical-link -->\n"
            "[detached](detached.md)\n*(historical citation)* <!-- doc-audit: historical-link -->\n"
            "`crates/missing/src/lib.rs`\n"
            "`mere/crates/missing/src/lib.rs`\n"
            "`crates/old/src/lib.rs` *(historical citation)* <!-- doc-audit: historical-path -->\n"
            "`crates/future/src/lib.rs` *(planned target)* <!-- doc-audit: planned-path -->\n"
            "`crates/wrong/src/lib.rs` *(historical citation)* <!-- doc-audit: historical-link -->\n"
            "`src/missing.rs`\n"
            "`crates/*/design_docs/`\n"
            "`mere/cable/v1`\n"
            "`[syntax](example.md) *(historical citation)* <!-- doc-audit: historical-link -->`\n"
            "```md\n[fenced](example.md) *(historical citation)* <!-- doc-audit: historical-link -->\n````\n"
            "- nested link\n    [nested](nested.md)\n"
            "[label only](label-only.md) *(historical citation)*\n",
            encoding="utf-8",
        )
        (docs / "DOC_README.md").write_text(
            (docs / "DOC_README.md").read_text(encoding="utf-8") + "- [links](bad_links.md)\n- [statusless](statusless_plan.md)\n",
            encoding="utf-8",
        )


def run_self_test() -> None:
    status_positive = (
        "Status: current",
        "**Status**: current",
        "**Status:** current",
        "Status (2026-09-04): current",
        "**Status (2026-09-02):** current",
        "Status (reconciled to code 2026-06-23): current",
        "**Status (reconciled to code):** current",
    )
    status_negative = (
        "The Status: field is described below.",
        "Status mentioned in the findings.",
        "Status update: pending",
        "Status (2026-09-04) was checked.",
    )
    for sample in status_positive:
        if STATUS_LINE.search(sample) is None:
            raise AssertionError(f"status positive control was not detected: {sample!r}")
    for sample in status_negative:
        if STATUS_LINE.search(sample) is not None:
            raise AssertionError(f"status negative control was detected: {sample!r}")

    informational_only = Report(
        ambiguous_known_root_paths=[Finding("doc.md", "src/missing.rs", "bare root")],
        ignored_known_root_patterns=[Finding("doc.md", "crates/*/src/", "glob")],
        reserved_identifier_paths=[Finding("doc.md", "mere/cable/v1", "protocol")],
    )
    if informational_only.has_findings():
        raise AssertionError("informational exclusions should not fail the findings gate")

    with tempfile.TemporaryDirectory(prefix="mere-doc-audit-") as temp:
        root = Path(temp) / "repos" / "mere"
        write_fixture(root, defective=True)
        defective = audit(root)
        expected = {
            "index_orphans",
            "index_ghosts",
            "memory_directory_links",
            "statusless_plans",
            "broken_relative_links",
            "historical_broken_relative_links",
            "planned_broken_relative_links",
            "ambiguous_known_root_paths",
            "ignored_known_root_patterns",
            "reserved_identifier_paths",
            "missing_known_root_paths",
            "historical_missing_known_root_paths",
            "planned_missing_known_root_paths",
            "invalid_historical_annotations",
            "stale_historical_annotations",
        }
        missing = [key for key in expected if defective.counts()[key] == 0]
        if missing:
            raise AssertionError(f"positive control was not detected: {', '.join(missing)}")

        false_targets = {
            finding.subject
            for finding in defective.broken_relative_links
            if finding.subject in {"user-home", "memory"}
        }
        if false_targets:
            raise AssertionError(f"false prose target was reported: {', '.join(sorted(false_targets))}")

        missing_subjects = {finding.subject for finding in defective.missing_known_root_paths}
        if "src/missing.rs" in missing_subjects:
            raise AssertionError("ambiguous bare source root was reported")
        if "crates/*/design_docs/" in missing_subjects:
            raise AssertionError("glob path was reported as a concrete citation")
        if "mere/cable/v1" in missing_subjects:
            raise AssertionError("versioned Mere protocol identifier was reported as a path")
        for concrete in ("crates/missing/src/lib.rs", "mere/crates/missing/src/lib.rs"):
            if concrete not in missing_subjects:
                raise AssertionError(f"concrete missing path was not detected: {concrete}")

        historical_links = {finding.subject for finding in defective.historical_broken_relative_links}
        if historical_links != {"old.md", "old-label.md"}:
            raise AssertionError(f"historical link annotation was not exact: {sorted(historical_links)}")
        historical_paths = {finding.subject for finding in defective.historical_missing_known_root_paths}
        if historical_paths != {"crates/old/src/lib.rs"}:
            raise AssertionError(f"historical path annotation was not exact: {sorted(historical_paths)}")
        if "old.md" in {finding.subject for finding in defective.broken_relative_links}:
            raise AssertionError("accepted historical link remained in the failing bucket")
        if "crates/old/src/lib.rs" in missing_subjects:
            raise AssertionError("accepted historical path remained in the failing bucket")
        planned_links = {finding.subject for finding in defective.planned_broken_relative_links}
        if planned_links != {"future.md"}:
            raise AssertionError(f"planned link annotation was not exact: {sorted(planned_links)}")
        planned_paths = {finding.subject for finding in defective.planned_missing_known_root_paths}
        if planned_paths != {"crates/future/src/lib.rs"}:
            raise AssertionError(f"planned path annotation was not exact: {sorted(planned_paths)}")
        if len(defective.invalid_historical_annotations) != 4:
            raise AssertionError(
                "historical annotation controls produced an unexpected invalid count: "
                f"{len(defective.invalid_historical_annotations)}"
            )
        if len(defective.stale_historical_annotations) != 1:
            raise AssertionError("live historical annotation was not reported exactly once")

        clean_root = Path(temp) / "clean" / "repos" / "mere"
        write_fixture(clean_root, defective=False)
        clean = audit(clean_root)
        remaining = [key for key in expected if clean.counts()[key] != 0]
        if remaining:
            raise AssertionError(f"clean fixture still has findings: {', '.join(remaining)}")
    print("self-test passed: planted defects detected; clean fixture reached zero")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1], help="Mere repository root")
    parser.add_argument("--json", action="store_true", help="emit the report as JSON")
    parser.add_argument("--fail-on-findings", action="store_true", help="exit 1 if any audited finding exists")
    parser.add_argument("--self-test", action="store_true", help="run planted-defect and clean-fixture positive controls")
    args = parser.parse_args()
    if args.self_test:
        run_self_test()
        return 0
    report = audit(args.repo)
    print_report(report, args.json)
    return 1 if args.fail_on_findings and report.has_findings() else 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, FileNotFoundError) as error:
        print(f"audit failed: {error}", file=sys.stderr)
        raise SystemExit(2)
