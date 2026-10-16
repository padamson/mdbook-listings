#!/usr/bin/env python3
"""Renders the advisory monitor's tracking-issue body.

Usage: advisory-issue-body.py OLD_BODY REPORT OUT

OLD_BODY is the open issue's body (an empty file when there is none) and
REPORT is check-advisories.sh's output. Writes the new body to OUT and
prints what an open issue needs:

  changed    the findings changed; edit the body and comment, so watchers
             are told
  refreshed  the same findings, reported differently; edit the body only
  unchanged  leave the issue alone

With no open issue, the caller opens one with OUT whatever this prints.

The report sits between two markers, so an update rewrites only that
section and triage notes around it survive. Findings are compared by
identity, not text: cargo deny quotes Cargo.lock line numbers, which move
with every dependency bump, and a comment for each would notify watchers
about nothing.
"""

import re
import sys

START, END = "<!-- advisory-report -->", "<!-- /advisory-report -->"

INTRO = """An advisory that is not in `deny.toml`'s `[advisories].ignore` is
affecting a tree the Security & Quality gate checks, so the next push or
scheduled run will go red.

Triage each one:
- **Fix available**: bump the crate (`cargo update -p <crate>`, plus
  `--manifest-path` when the report names a lockfile outside the root),
  or bump the parent that pins it.
- **No fix / upstream-blocked**: accept the risk and add the
  `RUSTSEC-…` id, with a rationale, to **both** ignore lists: `deny.toml`
  `[advisories].ignore`, which this monitor and the `cargo deny` gates
  read, and `.cargo/audit.toml`, which the `cargo audit` gate and hook
  read. One without the other leaves a gate red.

"""

FOOTER = "\n\n_Maintained automatically by the Security Advisory Monitor workflow._\n"

# GitHub refuses an issue body over 65536 characters. A report past this
# keeps only the lines that identify its findings, so it still files.
LIMIT = 60000
KEEP = re.compile(r"^(?:## |error\[|\s*├ ID: )")
DROPPED = "(Inclusion graphs dropped to fit GitHub's size limit; the run log has the full report.)"

# A section heading names the lockfile; within it, a cargo deny
# diagnostic's first line names the check and the crate or advisory
# title, and a RUSTSEC id identifies the advisory.
HEADING = re.compile(r"^## cargo deny check advisories \((.*)\)$")
ITEM = re.compile(r"^error\[[a-z-]+\]: .*$|RUSTSEC-\d{4}-\d{4}")


def findings(text):
    """Each finding paired with its lockfile, so an advisory that reaches
    another tree is a new finding."""
    lock, found = None, set()
    for line in text.splitlines():
        heading = HEADING.match(line)
        if heading:
            lock = heading.group(1)
            continue
        found.update((lock, item) for item in ITEM.findall(line))
    return found


def render(old, report):
    report = report.rstrip("\n")
    if len(report) > LIMIT:
        kept = [line for line in report.splitlines() if KEEP.match(line)]
        report = "\n".join(kept + ["", DROPPED])[:LIMIT]
    section = f"{START}\n```\n{report}\n```\n{END}"
    if not old.strip():
        return INTRO + section + FOOTER, "changed"
    i = old.find(START)
    j = old.find(END, i) if i >= 0 else -1
    if j < 0:
        return old.rstrip("\n") + "\n\n" + section + "\n", "changed"
    j += len(END)
    if old[i:j] == section:
        return old, "unchanged"
    same = findings(old[i:j]) == findings(section)
    return old[:i] + section + old[j:], "refreshed" if same else "changed"


def main(old_path, report_path, out_path):
    # Text mode turns the CRLF endings of a body saved from GitHub's web
    # editor into LF, so an untouched report still compares equal.
    with open(old_path) as f:
        old = f.read()
    with open(report_path) as f:
        report = f.read()
    body, status = render(old, report)
    with open(out_path, "w") as f:
        f.write(body)
    print(status)


if __name__ == "__main__":
    main(*sys.argv[1:4])
