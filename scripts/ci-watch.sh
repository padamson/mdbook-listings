#!/usr/bin/env bash
#
# Stream GitHub Actions results for one commit: one line per job as it
# reaches a terminal state, one line per workflow when it finishes, then
# exit. Built to be driven by an agent's background monitor, but it reads
# fine in a terminal too.
#
# Usage:
#   ./scripts/ci-watch.sh                   # the current HEAD
#   ./scripts/ci-watch.sh <sha|ref>         # a specific commit
#   ./scripts/ci-watch.sh --poll 15         # poll every 15s (default 30)
#   ./scripts/ci-watch.sh --timeout 600     # give up after 10 minutes (default 3600)
#
# Flags, not an env-var prefix: the sandbox exclusion below matches the
# command line as written, and `TIMEOUT=600 ./scripts/ci-watch.sh` is not
# the excluded command, so it runs sandboxed and every `gh` call fails.
# POLL and TIMEOUT in the environment still set the defaults for a terminal.
#
# Why this is a script and not a loop written inline:
#
# The Claude Code sandbox matches `excludedCommands` against the top-level
# command line. `gh` on its own is excluded and works; `gh` inside a `for`
# or `while` loop is NOT matched, so the whole invocation stays sandboxed
# and every call fails on the read-denied ~/.config/gh -- silently, if the
# loop swallows errors. Excluding this script instead unsandboxes the whole
# process tree, and the `gh` it spawns inherits that, the same route
# `git push` takes for SSH. `.claude/settings.json` carries the exclusion.
#
# Every terminal state is reported, not just successes: a silent watcher
# looks identical to a passing one, which is the failure mode this replaces.
# The counts are taken from the current state on every poll, not
# accumulated, so a run that is re-run mid-watch and goes green is reported
# green, and its new job results are printed as they land.
set -uo pipefail

POLL="${POLL:-30}"
TIMEOUT="${TIMEOUT:-3600}"
REF=HEAD
while [ $# -gt 0 ]; do
  case "$1" in
    --poll) POLL="$2"; shift 2 ;;
    --timeout) TIMEOUT="$2"; shift 2 ;;
    --poll=*) POLL="${1#*=}"; shift ;;
    --timeout=*) TIMEOUT="${1#*=}"; shift ;;
    -h|--help) grep '^#' "$0" | sed 's/^#\{1,\} \{0,1\}//'; exit 0 ;;
    -*) echo "ci-watch: unknown flag $1" >&2; exit 2 ;;
    *) REF="$1"; shift ;;
  esac
done

SHA="$(git rev-parse "$REF")" || exit 2
SHORT="${SHA:0:7}"
DEADLINE=$(( $(date +%s) + TIMEOUT ))

# The conclusions that pass, for a job and for a run alike.
is_green() {
  case "$1" in success|skipped|neutral) return 0 ;; *) return 1 ;; esac
}

# Lines already printed: "|<run>/<job>/<conclusion>|" for a job and
# "|<run>/<conclusion>|" for a run, so a re-run's new result prints again.
seen=""

while :; do
  if [ "$(date +%s)" -ge "$DEADLINE" ]; then
    echo "ci-watch: TIMEOUT waiting on $SHORT"
    exit 2
  fi

  # `gh` exits non-zero on a network blip; tell the difference between
  # "cannot reach GitHub" and "no runs for this commit yet". `--commit`
  # filters on the server, so an older commit is found however many runs
  # have landed since.
  if ! ids=$(gh run list --commit "$SHA" --limit 100 --json databaseId \
        --jq ".[].databaseId" 2>/dev/null); then
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
  failures=0
  bad_runs=0
  for id in $ids; do
    # One call per run: each finished job, then the run's own state. A call
    # that fails leaves the run pending for this poll.
    if ! view=$(gh run view "$id" --json jobs,status,conclusion,workflowName --jq '
        (.jobs[] | select(.conclusion != null and .conclusion != "")
          | "JOB\t\(.conclusion)\t\(.name)"),
        "RUN\t\(.status)\t\(.conclusion // "-")\t\(.workflowName)"' 2>/dev/null); then
      pending=$((pending + 1))
      continue
    fi
    while IFS=$'\t' read -r kind a b c; do
      case "$kind" in
        JOB)
          conclusion="$a" name="$b"
          is_green "$conclusion" || failures=$((failures + 1))
          key="|$id/$name/$conclusion|"
          case "$seen" in *"$key"*) ;; *) seen="$seen$key"; echo "$conclusion  $name" ;; esac
          ;;
        RUN)
          status="$a" conclusion="$b" workflow="$c"
          if [ "$status" != "completed" ]; then
            pending=$((pending + 1))
            continue
          fi
          # A run can fail with no failed job: an invalid workflow file is a
          # startup_failure with no jobs at all.
          is_green "$conclusion" || bad_runs=$((bad_runs + 1))
          key="|$id/$conclusion|"
          case "$seen" in *"$key"*) ;; *) seen="$seen$key"; echo "RUN DONE: $workflow => $conclusion" ;; esac
          ;;
      esac
    done <<< "$view"
  done

  if [ "$pending" -eq 0 ]; then
    if [ "$failures" -gt 0 ] || [ "$bad_runs" -gt 0 ]; then
      echo "ci-watch: $SHORT finished with $failures failing job(s) in $bad_runs failed run(s)"
      exit 1
    fi
    echo "ci-watch: $SHORT all green"
    exit 0
  fi

  sleep "$POLL"
done
