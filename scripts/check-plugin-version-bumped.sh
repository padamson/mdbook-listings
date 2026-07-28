#!/usr/bin/env bash
#
# Fail the commit when plugin content (skills/ or .claude-plugin/) changes
# without a plugin.json version bump.
#
# The explicit version in plugin.json is the consumer-facing update gate:
# /plugin update reports "already at the latest version" until it moves, so
# a skill edit without a bump silently freezes updates for every installed
# consumer. The bump is pure bookkeeping, which makes it easy to forget --
# this hook is what makes forgetting impossible.
#
# Compares the staged manifest against HEAD's, so it checks what is actually
# being committed.
set -euo pipefail

manifest=.claude-plugin/plugin.json

# Initial commit: nothing to compare against.
git rev-parse -q --verify HEAD >/dev/null 2>&1 || exit 0

read_version() {
  python3 -c 'import json,sys; print(json.load(sys.stdin).get("version",""))'
}

old=$(git show "HEAD:$manifest" 2>/dev/null | read_version || echo "")
new=$(git show ":$manifest" 2>/dev/null | read_version || echo "")

if [ -z "$new" ]; then
  echo "plugin version guard: $manifest has no version field." >&2
  echo "Add one -- it is the update gate for installed consumers." >&2
  exit 1
fi
if [ "$old" = "$new" ]; then
  echo "plugin version guard: plugin content changed but $manifest is still $new." >&2
  echo "Bump the version so installed consumers see the update." >&2
  exit 1
fi
