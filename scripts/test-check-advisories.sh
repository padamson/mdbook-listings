#!/usr/bin/env bash
#
# Self-test for check-advisories.sh and advisory-issue-body.py, the two
# halves of advisory-monitor.yml.
#
# The monitor runs on a schedule nobody watches, so a classifier that calls
# a hit "broken" fails one job and files nothing, and one that calls a
# fetch error a hit files an issue about the network. A stub `cargo` on
# PATH replays real cargo-deny 0.20 output, and that output is what pins
# the classifier to the format it was written against. The checker runs
# from a scratch repo with a root and a nested lockfile, so the tree list
# is tested as well. The renderer cases pin when watchers get a comment.

set -euo pipefail

# A pre-commit hook inherits GIT_INDEX_FILE and friends from the commit
# that runs it, and the git calls below would then write the scratch
# repo's files into that commit's index.
# shellcheck disable=SC2046
unset $(git rev-parse --local-env-vars)

here=$(cd "$(dirname "$0")" && pwd)
renderer="$here/advisory-issue-body.py"
# An explicit template: BSD mktemp does not take TMPDIR into account for a
# bare `-d`, so leaving it out puts the fixtures somewhere macOS may refuse.
work=$(mktemp -d "${TMPDIR:-/tmp}/check-advisories-test.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir "$work/bin" "$work/fx"

# The checker resolves the repo from its own path, so it runs as a copy
# inside a repo whose tracked lockfiles the test controls. The untracked
# one stands in for a stray build tree, and oldCargo.lock for a fixture
# whose name only ends in Cargo.lock; no gate should read either.
repo="$work/repo"
mkdir -p "$repo/scripts" "$repo/fuzz" "$repo/scratch"
cp "$here/check-advisories.sh" "$repo/scripts/"
checker="$repo/scripts/check-advisories.sh"
touch "$repo/Cargo.lock" "$repo/fuzz/Cargo.lock" "$repo/fuzz/oldCargo.lock" "$repo/scratch/Cargo.lock"
git -C "$repo" init -q
git -C "$repo" add Cargo.lock fuzz/Cargo.lock fuzz/oldCargo.lock

# Replays the root fixture for the root manifest and the nested fixture
# for any other.
cat > "$work/bin/cargo" <<'STUB'
#!/usr/bin/env bash
echo "$*" >> "$STUB_DIR/calls"
[ "$1" = deny ] || { echo "stub cargo: unexpected subcommand: $*" >&2; exit 99; }
fx=$STUB_NESTED
while [ $# -gt 0 ]; do
    if [ "$1" = --manifest-path ] && [ "$2" = Cargo.toml ]; then fx=$STUB_ROOT; fi
    shift
done
cat "$STUB_DIR/fx/$fx.out"
exit "$(cat "$STUB_DIR/fx/$fx.code")"
STUB
chmod +x "$work/bin/cargo"

fixture() {
    printf '%s\n' "$2" > "$work/fx/$1.code"
    cat > "$work/fx/$1.out"
}

fixture clean 0 <<'OUT'
advisories ok
OUT

fixture hit 1 <<'OUT'
error[unmaintained]: Bincode is unmaintained
   ┌─ /home/runner/work/my-project/my-project/Cargo.lock:13:1
   │
13 │ bincode 1.3.3 registry+https://github.com/rust-lang/crates.io-index
   │ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ unmaintained advisory detected
   │
   ├ ID: RUSTSEC-2025-0141
   ├ Advisory: https://rustsec.org/advisories/RUSTSEC-2025-0141
   ├ Solution: No safe upgrade is available!
   ├ bincode v1.3.3
     └── syntect v5.3.0

advisories FAILED
OUT

fixture vulnerability 1 <<'OUT'
error[vulnerability]: Buffer overflow in SmallVec::insert_many
   ┌─ /home/runner/work/my-project/my-project/fuzz/Cargo.lock:20:1
   │
20 │ smallvec 1.6.0 registry+https://github.com/rust-lang/crates.io-index
   │ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ security vulnerability detected
   │
   ├ ID: RUSTSEC-2021-0003
   ├ Advisory: https://rustsec.org/advisories/RUSTSEC-2021-0003
   ├ Solution: Upgrade to >=0.6.14, <1.0.0 OR >=1.6.1 (try `cargo update -p smallvec`)
   ├ smallvec v1.6.0
     └── my-project-fuzz v0.0.0

advisories FAILED
OUT

cat "$work/fx/hit.out" "$work/fx/vulnerability.out" | fixture both 1

fixture broken 1 <<'OUT'
2026-10-10 14:12:54 [ERROR] failed to acquire advisory database lock: failed to obtain lock file '/nonexistent/advisory-dbs': failed to create parent directories for lock path
OUT

# A typo in deny.toml: same diagnostic shape as a finding, but no advisory.
fixture config-error 1 <<'OUT'
error[unexpected-keys]: found 1 unexpected keys, expected: ["version", "db-path", "db-urls", "vulnerability", "notice", "unmaintained", "unsound", "yanked", "ignore", "severity-threshold", "git-fetch-with-cli", "disable-yank-checking", "maximum-db-staleness", "unused-ignored-advisory"]
  ┌─ /home/runner/work/my-project/my-project/deny.toml:2:1
  │
2 │ ignroe = []
  │ ──────

2026-10-10 14:28:34 [ERROR] failed to deserialize config from '/home/runner/work/my-project/my-project/deny.toml'
OUT

failures=0

# fail NAME [CONTEXT_FILE]: report a failed check, indenting the context.
fail() {
    echo "FAIL: $1"
    if [ -n "${2:-}" ]; then sed 's/^/    /' "$2" | head -n 20; fi
    failures=$((failures + 1))
}

# run ROOT NESTED, then expect NAME STATUS TEXT against its result. An
# empty TEXT asserts the report is empty.
run() {
    : > "$work/calls"
    status=0
    PATH="$work/bin:$PATH" STUB_DIR="$work" STUB_ROOT="$1" STUB_NESTED="$2" \
        "$checker" > "$work/out" 2> "$work/stderr" || status=$?
}

expect() {
    local name="$1" status_want="$2" text="$3"
    if [ "$status" -ne "$status_want" ]; then
        cat "$work/out" "$work/stderr" > "$work/context"
        fail "$name: exit $status, expected $status_want" "$work/context"
    elif [ -z "$text" ] && [ -s "$work/out" ]; then
        fail "$name: expected an empty report" "$work/out"
    elif [ -n "$text" ] && ! grep -qF -- "$text" "$work/out"; then
        fail "$name: report did not contain '$text'" "$work/out"
    else
        echo "ok: $name"
    fi
}

run clean clean
expect "every tree clean is clean" 0 ""
if [ "$(grep -c -- '--log-level error --color never --workspace --manifest-path .* check advisories$' "$work/calls")" -eq 2 ]; then
    echo "ok: the quiet and whole-workspace flags are passed"
else
    fail "the quiet and whole-workspace flags were not passed" "$work/calls"
fi
if grep -qx -- '.*--manifest-path Cargo.toml .*' "$work/calls" &&
    grep -qx -- '.*--manifest-path fuzz/Cargo.toml .*' "$work/calls" &&
    ! grep -qE 'scratch|oldCargo' "$work/calls"; then
    echo "ok: one cargo deny per tracked Cargo.lock, and none for an untracked or merely similar one"
else
    fail "expected one cargo deny per tracked lockfile" "$work/calls"
fi

run hit clean
expect "a root diagnostic is a hit" 1 "RUSTSEC-2025-0141"
expect "and the report names the lockfile" 1 "## cargo deny check advisories (Cargo.lock)"
# The renderer cases below read the checker's own reports, so a change to
# its format reaches them.
cp "$work/out" "$work/report.txt"

run clean vulnerability
expect "a nested lockfile's diagnostic is a hit" 1 "RUSTSEC-2021-0003"
expect "and the report names that lockfile" 1 "## cargo deny check advisories (fuzz/Cargo.lock)"

run broken clean
expect "a failure without a diagnostic is broken, with no report" 2 ""

run config-error clean
expect "a deny.toml error is broken, not an advisory" 2 ""

run hit broken
expect "a hit beside a broken tree is still filed, and the job told" 3 "RUSTSEC-2025-0141"

run hit vulnerability
cp "$work/out" "$work/more.txt"
run both both
cp "$work/out" "$work/spread.txt"

git -C "$repo" rm -q --cached Cargo.lock fuzz/Cargo.lock
run clean clean
expect "no tracked Cargo.lock is broken, not clean" 2 ""

# render OLD_BODY_FILE REPORT_FILE, then rendered NAME STATUS TEXT. TEXT
# is a line the new body must contain.
render() {
    status_out=$("$renderer" "$1" "$2" "$work/body.txt")
}

rendered() {
    local name="$1" want="$2" text="$3"
    if [ "$status_out" != "$want" ]; then
        fail "$name: rendered as '$status_out', expected '$want'"
    elif ! grep -qF -- "$text" "$work/body.txt"; then
        fail "$name: body did not contain '$text'" "$work/body.txt"
    else
        echo "ok: $name"
    fi
}

: > "$work/empty.txt"
render "$work/empty.txt" "$work/report.txt"
rendered "a first report carries the triage steps" changed "Triage each one:"
cp "$work/body.txt" "$work/opened.txt"

render "$work/opened.txt" "$work/report.txt"
rendered "the same report leaves the issue alone" unchanged "RUSTSEC-2025-0141"

sed 's/$/\r/' "$work/opened.txt" > "$work/crlf.txt"
render "$work/crlf.txt" "$work/report.txt"
rendered "a body saved with CRLF endings is still unchanged" unchanged "RUSTSEC-2025-0141"

sed 's/Cargo\.lock:13:1/Cargo.lock:17:1/; s/^13 │/17 │/' "$work/report.txt" > "$work/moved.txt"
render "$work/opened.txt" "$work/moved.txt"
rendered "a finding whose lockfile line moved refreshes without a comment" refreshed "Cargo.lock:17:1"

render "$work/opened.txt" "$work/more.txt"
rendered "a new finding is a change" changed "RUSTSEC-2021-0003"

# Every heading, title and id in spread.txt is already in more.txt, only
# now each advisory is in both trees.
cp "$work/body.txt" "$work/two-trees.txt"
render "$work/two-trees.txt" "$work/spread.txt"
rendered "a known advisory reaching another lockfile is a change" changed "RUSTSEC-2021-0003"

{ printf 'Earlier: <!-- /advisory-report -->\n\n'; cat "$work/opened.txt"; } > "$work/stray.txt"
render "$work/stray.txt" "$work/report.txt"
rendered "an end marker before the report does not duplicate it" unchanged "RUSTSEC-2025-0141"

for n in $(seq 400); do sed "s|(Cargo.lock)|(crates/c$n/Cargo.lock)|" "$work/report.txt"; done > "$work/huge.txt"
render "$work/empty.txt" "$work/huge.txt"
rendered "an oversized report keeps its findings and drops the graphs" changed "Inclusion graphs dropped"
if [ "$(wc -c < "$work/body.txt")" -lt 65536 ] && grep -qF "crates/c400/Cargo.lock" "$work/body.txt"; then
    echo "ok: and it fits under GitHub's body limit with every lockfile named"
else
    fail "an oversized report did not fit, or lost a lockfile: $(wc -c < "$work/body.txt") bytes"
fi
cp "$work/body.txt" "$work/huge-opened.txt"
sed 's/Cargo\.lock:13:1/Cargo.lock:17:1/' "$work/huge.txt" > "$work/huge-moved.txt"
render "$work/huge-opened.txt" "$work/huge-moved.txt"
rendered "an oversized report with the same findings leaves the issue alone" unchanged "crates/c1/Cargo.lock"

printf 'Triage: waiting on syntect.\n\n%s\n' "$(cat "$work/opened.txt")" > "$work/noted.txt"
render "$work/noted.txt" "$work/more.txt"
rendered "triage notes outside the report survive an update" changed "Triage: waiting on syntect."

printf 'Hand-written body with no markers.\n' > "$work/bare.txt"
render "$work/bare.txt" "$work/report.txt"
rendered "a body without markers keeps its text and gains the report" changed "Hand-written body with no markers."

if [ "$failures" -ne 0 ]; then
    echo "$failures check(s) failed"
    exit 1
fi
echo "all checks passed"
