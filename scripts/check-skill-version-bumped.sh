#!/usr/bin/env bash
#
# Fail when the shipped skill changes without a version bump.
#
# Consumers install the skill with `npx skills add` and refresh it with
# `npx skills update`. The CLI has no version concept of its own: it re-pulls
# whatever `skills/<name>/` holds and reports success either way, so the
# frontmatter's `metadata.version` is the only thing a consumer can compare
# to know whether an install moved. An edit that does not move it is
# invisible to everyone downstream.
#
# The skill versions on its own cadence, not the crate's: the crate version
# on the default branch is the last release, while the skill there describes
# the next one, so pinning them would make the skill claim a release whose
# binary it does not match.
#
# The rule: whenever content under `skills/` differs from a base revision,
# the skill's `metadata.version` must differ from it too. The pre-commit
# hook applies it per commit, index against HEAD. CI applies it with
# `--base <ref>` to everything since the merge base with that ref -- the
# aggregate that actually lands -- because the hook is per-clone and
# `--no-verify` skips it. The two can disagree on one commit (a bump in one
# commit and an unbumped edit in the next passes CI and fails the hook on
# the second), and that is the intent: the hook keeps history honest, CI
# keeps what lands honest.
#
# A ref that does not resolve or shares no history with HEAD is an error
# here. A caller that would rather skip in that case checks before calling.
set -euo pipefail

usage() {
  echo "usage: $(basename "$0") [--base <ref>]" >&2
  exit 2
}

base=HEAD
while [ $# -gt 0 ]; do
  case "$1" in
    --base)
      [ $# -ge 2 ] || usage
      base="$2"
      shift 2
      ;;
    *) usage ;;
  esac
done

skill=skills/mdbook-listings/SKILL.md

# Initial commit: nothing to compare against.
git rev-parse -q --verify HEAD >/dev/null 2>&1 || exit 0

# Everything below compares the index against `$base`. That serves both
# callers with one code path: the hook wants exactly the index, and a fresh
# CI checkout leaves the index equal to HEAD.
if [ "$base" != HEAD ]; then
  base=$(git merge-base "$base" HEAD 2>/dev/null) || {
    echo "skill version guard: --base does not resolve or shares no history with HEAD." >&2
    exit 2
  }
fi

# Nothing under the guarded path moved, so there is nothing to guard.
# pre-commit's `files:` filter already implies this on a real commit, but
# `run --all-files` runs every hook regardless of what changed. Without this
# the guard fails a clean full-tree run with "content changed but the version
# did not" -- and the obvious response to that message is a version bump
# describing no change, so the false positive has a plausible wrong fix.
# `--quiet` exits 1 for "changed"; anything else is git failing.
changed=0
git diff --cached --quiet "$base" -- skills/ || changed=$?
case "$changed" in
  0) exit 0 ;;
  1) ;;
  *)
    echo "skill version guard: git diff against $base failed (exit $changed)." >&2
    exit 2
    ;;
esac

# The same fixed-shape scan as the Rust test in this repo, on purpose: two
# readers of one field must accept exactly the same bytes, so neither gets a
# YAML parser or any grammar the other lacks. Inside the `metadata:` block,
# the first `version:` line, quotes stripped; blank lines are skipped, the
# block ends at the first unindented line. Empty input -- the commit that
# first adds the skill, read at the old revision -- is "no version", not an
# error.
read_frontmatter_version() {
  python3 -c '
import sys
text = sys.stdin.read()
if not text:
    print("")
    raise SystemExit(0)
if not text.startswith("---\n") or "\n---\n" not in text[4:]:
    sys.exit("skill version guard: no frontmatter")
front = text[4:].split("\n---\n", 1)[0]
in_metadata = False
for line in front.splitlines():
    if line.startswith("metadata:"):
        in_metadata = True
        continue
    if not in_metadata or not line.strip():
        continue
    if not line.startswith(" "):
        break
    key, sep, value = line.strip().partition(":")
    if key == "version" and sep:
        print(value.strip().strip("\"\x27"))
        raise SystemExit(0)
print("")
'
}

old=$(git show "$base:$skill" 2>/dev/null | read_frontmatter_version) || old=""
new=$(git show ":$skill" 2>/dev/null | read_frontmatter_version) || new=""

if [ -z "$new" ]; then
  echo "skill version guard: $skill has no readable metadata.version." >&2
  echo "Add one -- it is the only version a consumer can compare on update." >&2
  exit 1
fi
if [ "$old" = "$new" ]; then
  echo "skill version guard: skill content changed but $skill is still $new." >&2
  echo "Bump metadata.version so installed consumers see the update." >&2
  exit 1
fi
