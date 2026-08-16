#!/usr/bin/env python3
"""Verify explicit release-owner approvals for scoped audit exceptions.

The Core 3 release policy (docs/v3/security-review.md) allows exact-advisory
audit exceptions, but only when the release owner explicitly approves them
before shipping. This script enforces that for the `RUSTSEC-2026-0009`
(`time`) exception: the matching gate in docs/release-checklist.md must be
checked (`[x]`), and the exception must remain scoped to exactly that
advisory ID in .cargo/audit.toml.

A human marks the checklist item checked once they have reviewed the
exception for the current release. A wildcard ignore or a changed advisory
set fails the release-guard job instead of silently shipping.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

import tomllib

ROOT = Path(__file__).resolve().parents[1]
CHECKLIST = ROOT / "docs" / "release-checklist.md"
AUDIT_TOML = ROOT / ".cargo" / "audit.toml"

# The exact advisory exception that requires release-owner approval.
TARGET_ADVISORY = "RUSTSEC-2026-0009"

# The gate line in release-checklist.md must mention the advisory id.
GATE_PATTERN = re.compile(r"^\s*-\s*\[(?P<state>[ xX])\]\s+.*" + re.escape(TARGET_ADVISORY), re.MULTILINE)

# In audit.toml the ignore list must contain the exact ID. Any wildcard
# ("RUSTSEC-...") or extra RUSTSEC entries beyond the reviewed set is a
# scoping violation that must fail the release.
REVIEWED_IGNORES = frozenset(
    {
        "RUSTSEC-2024-0436",
        "RUSTSEC-2026-0192",
        "RUSTSEC-2026-0206",
        "RUSTSEC-2026-0009",
    }
)
WILDCARD = re.compile(r"RUSTSEC-\*")


def load_audit_ignores() -> list[str]:
    """Return the advisory IDs listed under [advisories] ignore in audit.toml."""
    with AUDIT_TOML.open("rb") as source:
        data = tomllib.load(source)
    return list(data.get("advisories", {}).get("ignore", []))


def checklist_gate_state() -> str | None:
    """Return 'x', ' ' or None for the RUSTSEC-2026-0009 gate line."""
    text = CHECKLIST.read_text(encoding="utf-8")
    match = GATE_PATTERN.search(text)
    if not match:
        return None
    return match.group("state").lower()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--advisory",
        default=TARGET_ADVISORY,
        help="Advisory ID requiring release-owner approval (default: RUSTSEC-2026-0009)",
    )
    args = parser.parse_args()
    advisory = args.advisory

    failures: list[str] = []

    if not CHECKLIST.exists():
        failures.append(f"missing release checklist: {CHECKLIST.relative_to(ROOT)}")
    else:
        state = checklist_gate_state()
        if state is None:
            failures.append(
                f"release checklist has no gate for {advisory} "
                f"(expected a '- [ ]/{advisory}' line in docs/release-checklist.md)"
            )
        elif state != "x":
            failures.append(
                f"release-owner approval for {advisory} is not checked: "
                f"mark the gate as [x] in docs/release-checklist.md after review"
            )

    if not AUDIT_TOML.exists():
        failures.append(f"missing audit config: {AUDIT_TOML.relative_to(ROOT)}")
    else:
        ignores = load_audit_ignores()
        if any(WILDCARD.fullmatch(item) for item in ignores):
            failures.append(
                "audit.toml contains a wildcard ignore; "
                "exceptions must be exact advisory IDs"
            )
        if advisory not in ignores:
            failures.append(
                f"{advisory} is not listed in .cargo/audit.toml ignore; "
                f"the exception must exist to be approved"
            )
        unexpected = [item for item in ignores if item not in REVIEWED_IGNORES]
        if unexpected:
            failures.append(
                "audit.toml ignores advisory IDs outside the reviewed set: "
                + ", ".join(sorted(unexpected))
            )

    if failures:
        for message in failures:
            print(f"release-approval error: {message}", file=sys.stderr)
        raise SystemExit(1)
    print(
        f"release approvals verified: {advisory} gate [x] and "
        f"scoped to exactly {advisory} in audit.toml"
    )


if __name__ == "__main__":
    main()
