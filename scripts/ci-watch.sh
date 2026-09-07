#!/usr/bin/env bash
#
# Stream GitHub Actions results for one commit: one line per job as it
# reaches a terminal state, one line per workflow when it finishes, then
# exit. Built to be driven by an agent's background monitor, but it reads
# fine in a terminal too.
#
# Usage:
#   ./scripts/ci-watch.sh                 # the current HEAD
#   ./scripts/ci-watch.sh <sha|ref>       # a specific commit
#   POLL=15 ./scripts/ci-watch.sh         # override the 30s poll interval
#   TIMEOUT=600 ./scripts/ci-watch.sh     # give up after 10 minutes
#
# Why this is a script and not a loop written inline:
#
# The Claude Code sandbox matches `excludedCommands` against the top-level
# command line. `gh` on its own is excluded and works; `gh` inside a `for`
# or `while` loop is NOT matched, so the whole invocation stays sandboxed
# and every call fails on the read-denied ~/.config/gh -- silently, if the
# loop swallows errors. Excluding this script instead unsandboxes the whole
# process tree, and the `gh` it spawns inherits that, the same route
# `git push` takes for SSH. See ~/notes/cross-repo (sandbox policy) for the
# general rule; `.claude/settings.json` carries the exclusion.
#
# Every terminal state is reported, not just successes: a silent watcher
# looks identical to a passing one, which is the failure mode this replaces.
set -uo pipefail

SHA="$(git rev-parse "${1:-HEAD}")"
SHORT="${SHA:0:7}"
POLL="${POLL:-30}"
DEADLINE=$(( $(date +%s) + ${TIMEOUT:-3600} ))

seen_jobs=""
announced=""
failures=0

while :; do
  if [ "$(date +%s)" -ge "$DEADLINE" ]; then
    echo "ci-watch: TIMEOUT waiting on $SHORT"
    exit 2
  fi

  # `gh` exits non-zero on a network blip; tell the difference between
  # "cannot reach GitHub" and "no runs for this commit yet".
  if ! ids=$(gh run list --limit 30 --json databaseId,headSha \
        --jq ".[] | select(.headSha == \"$SHA\") | .databaseId" 2>/dev/null); then
    echo "ci-watch: could not reach GitHub, retrying in ${POLL}s"
    sleep "$POLL"
    continue
  fi

  if [ -z "$ids" ]; then
    echo "ci-watch: no workflow runs for $SHORT yet"
    sleep "$POLL"
    continue
  fi

  pending=0
  for id in $ids; do
    jobs=$(gh run view "$id" --json jobs --jq \
      '.jobs[] | select(.conclusion != null and .conclusion != "") | "\(.conclusion)\t\(.name)"' 2>/dev/null)
    while IFS=$'\t' read -r conclusion name; do
      [ -z "$name" ] && continue
      key="$id/$name"
      case "$seen_jobs" in *"|$key|"*) continue ;; esac
      seen_jobs="$seen_jobs|$key|"
      echo "$conclusion  $name"
      case "$conclusion" in success|skipped|neutral) ;; *) failures=$((failures + 1)) ;; esac
    done <<< "$jobs"

    read -r status conclusion workflow <<< "$(gh run view "$id" --json status,conclusion,workflowName \
      --jq '"\(.status) \(.conclusion // "-") \(.workflowName)"' 2>/dev/null)"
    if [ "$status" = "completed" ]; then
      case "$announced" in
        *"|$id|"*) ;;
        *) announced="$announced|$id|"; echo "RUN DONE: $workflow => $conclusion" ;;
      esac
    else
      pending=$((pending + 1))
    fi
  done

  if [ "$pending" -eq 0 ]; then
    if [ "$failures" -gt 0 ]; then
      echo "ci-watch: $SHORT finished with $failures failing job(s)"
      exit 1
    fi
    echo "ci-watch: $SHORT all green"
    exit 0
  fi

  sleep "$POLL"
done
