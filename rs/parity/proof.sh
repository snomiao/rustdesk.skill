#!/bin/sh
# Every proof that the binary behaves like bin/rustdesk-skill.ts, in one go,
# on Linux, macOS or Windows (in Git Bash there):
#
#   sh rs/parity/proof.sh <rustify's templates/parity/parity.mjs> <the rustdesk-skill binary>
#
# from the repository root, after `bun install`. The CI workflow's parity job
# runs this on each platform of `parity-platforms`, against the binary it has
# just built for that platform ($RUSTIFY_PLATFORM names it; without it,
# `uname` does).
#
# First rs/parity.json is checked to be what rs/parity.gen.mjs writes. Then
# these run side by side, each into a log of its own:
#
#   lane<n>   rs/parity.json split into lanes (rs/parity/lanes.mjs), every
#             lane run twice: TypeScript against the binary (the binary may
#             hand back to the real package), then the binary alone against a
#             stand-in package, which fails unless exactly the cases marked
#             "handback" for this system reach the JavaScript. As many lanes
#             as the machine has processors, three at most on Windows
#             (PARITY_LANES says otherwise).
#   tty<n>    on Linux and macOS, the same lanes once more with stdout and
#             stderr on a pseudo-terminal (--tty stdout,stderr): there bun
#             prints console.error in red and the binary hands back every
#             command but `list`, which it answers there too. (The hand-back
#             check holds for pipes; a case's "handback" holds for every run
#             of the file.)
#   unit      cargo test in rs/: the unit tests, and rs/src/compare.rs, which
#             holds the argument gate against the project's own yargs chain
#             and the text and number helpers against bun. It builds the crate
#             again from the same sources and Cargo.lock; it does not run the
#             binary named above.
#
# Every run of the runner gets a temp directory of its own (TMPDIR, TEMP and
# TMP), removed afterwards. At the end one line is printed for each part that
# passed, and all of what a failed one wrote. The exit code is 0 only when
# all of them passed.
set -u
usage="usage: proof.sh <parity.mjs> <binary>"
# In Git Bash `pwd -W` gives a directory as Windows programs (node, the
# binary) read it.
here=$(pwd -W 2> /dev/null || pwd)
absolute() {
  case "$1" in
    /* | [A-Za-z]:[\\/]*) printf '%s\n' "$1" ;;
    *) printf '%s/%s\n' "$here" "$1" ;;
  esac
}
runner=$(absolute "${1:?$usage}")
binary=$(absolute "${2:?$usage}")
if command -v cygpath > /dev/null 2>&1; then
  runner=$(cygpath -m "$runner")
  binary=$(cygpath -m "$binary")
fi
cd "$(dirname "$0")/../.." || exit 1
root=$(pwd -W 2> /dev/null || pwd)

platform=${RUSTIFY_PLATFORM:-}
if [ -z "$platform" ]; then
  case "$(uname -s)" in
    Linux) platform=linux ;;
    Darwin) platform=darwin ;;
    MINGW* | MSYS* | CYGWIN*) platform=win32 ;;
    *) platform=$(uname -s) ;;
  esac
fi
case "$platform" in
  win32*) windows=true default_lanes=3 ;;
  *) windows=false default_lanes=$(getconf _NPROCESSORS_ONLN 2> /dev/null || echo 2) ;;
esac
lanes=${PARITY_LANES:-$default_lanes}

node rs/parity.gen.mjs --check || exit 1

# In $TMPDIR, which macOS's mktemp passes over without a template.
logs=$(mktemp -d "${TMPDIR:-/tmp}/rds-proof.XXXXXX") || exit 1
logs_for_node=$(cygpath -m "$logs" 2> /dev/null || printf '%s' "$logs")
node rs/parity/lanes.mjs rs/parity.json "$lanes" "$logs_for_node" || exit 1

start() {
  name=$1
  shift
  (
    "$@" > "$logs/$name.log" 2>&1
    echo $? > "$logs/$name.exit"
  ) &
}
# One run of the runner, in a temp directory of its own.
run_in() {
  dir=$(mktemp -d "$logs/run.XXXXXX")
  dir_for_node=$(cygpath -m "$dir" 2> /dev/null || printf '%s' "$dir")
  TMPDIR=$dir_for_node TEMP=$dir_for_node TMP=$dir_for_node node "$runner" "$@"
  code=$?
  rm -rf "$dir"
  return $code
}
ts="sh rs/parity/run.sh bun bin/rustdesk-skill.ts"
rs="env RUSTDESK_SKILL_PACKAGE_ROOT=$root sh rs/parity/run.sh $binary"
lane() {
  cases=$logs_for_node/lane-$1.json
  code=0
  run_in --cases "$cases" --ts "$ts" --rs "$rs" || code=1
  run_in --cases "$cases" --rs "sh rs/parity/run.sh $binary" --handback RUSTDESK_SKILL_PACKAGE_ROOT=bin/rustdesk-skill.ts || code=1
  return $code
}
tty_lane() {
  run_in --cases "$logs_for_node/lane-$1.json" --ts "$ts" --rs "$rs" --tty stdout,stderr
}
runs=""
n=0
while [ "$n" -lt "$lanes" ]; do
  start "lane$n" lane "$n"
  runs="$runs lane$n"
  n=$((n + 1))
done
if ! $windows; then
  n=0
  while [ "$n" -lt "$lanes" ]; do
    start "tty$n" tty_lane "$n"
    runs="$runs tty$n"
    n=$((n + 1))
  done
fi
start unit cargo test --release --locked --manifest-path rs/Cargo.toml
runs="$runs unit"
wait

echo "proof on $platform with bun $(bun --version), $lanes lane(s):$runs"
failed=0
same=0 native=0 back=0 tty=0
for name in $runs; do
  code=$(cat "$logs/$name.exit" 2> /dev/null || echo "no exit code")
  if [ "$code" != 0 ]; then
    echo "$name: FAILED ($code)"
    cat "$logs/$name.log"
    failed=1
    continue
  fi
  case $name in
    lane*)
      count=$(sed -n 's/^all \([0-9]*\) case(s) behave the same$/\1/p' "$logs/$name.log")
      same=$((same + count))
      count=$(sed -n 's/^\([0-9]*\) case(s) answered by the binary, .*/\1/p' "$logs/$name.log")
      native=$((native + count))
      count=$(sed -n 's/^.* answered by the binary, \([0-9]*\) handed back .*/\1/p' "$logs/$name.log")
      back=$((back + count))
      ;;
    tty*)
      count=$(sed -n 's/^all \([0-9]*\) case(s) behave the same$/\1/p' "$logs/$name.log")
      tty=$((tty + count))
      ;;
    unit) grep '^test result' "$logs/$name.log" | sed "s/^/$name: /" ;;
  esac
done
if [ "$failed" = 0 ]; then
  echo "parity: all $same case(s) behave the same; $native answered by the binary, $back handed back to the JavaScript, as the cases say"
  if ! $windows; then echo "parity on a terminal (stdout and stderr): all $tty case(s) behave the same"; fi
  # Each run of each lane names the cases skipped here, and why (both runs
  # of a lane skip the same ones): the reasons, summed.
  tab=$(printf '\t')
  cat "$logs"/lane*.log | sed -n "s/^ *\([0-9][0-9]*\)  \(.*\)\$/\1$tab\2/p" | sort -t "$tab" -k 2 | awk -F "$tab" '
    $2 != reason { if (reason != "") print total "\t" reason; reason = $2; total = 0 }
    { total += $1 }
    END { if (reason != "") print total "\t" reason }' | while IFS="$tab" read -r total reason; do
    echo "  skipped on $platform: $((total / 2))  $reason"
  done
fi
if $windows; then echo "tty: not here (no pseudo-terminal on Windows)"; fi
rm -rf "$logs"
exit $failed
