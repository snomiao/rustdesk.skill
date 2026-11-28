#!/bin/sh
# Starts one implementation for a parity case, in front of both
# (`bun bin/rustdesk-skill.ts` and the binary):
#
#   sh rs/parity/run.sh <command> [args…]
#
# In the case's own temp directory ($TMPDIR, which the runner makes for each
# run) it puts stand-ins first on PATH, so that nothing a case runs can reach
# a browser, a screen, OCR or the clipboard:
#
#   playwright-cli   rs/parity/shim.mjs pw
#   python3          rs/parity/shim.mjs ocr
#   xclip, pbcopy    rs/parity/shim.mjs xclip / pbcopy
#   git              notes its arguments in $PARITY_LOG and runs the real git
#
# (rs/parity/shim.mjs says what the stand-ins write down and how a case makes
# them answer.) These variables of the case steer it, and are not passed on:
#
#   PARITY_CD=<dir>     start the command in that directory: below the
#                       case's directory, or `/`
#   PARITY_NO_GIT=1     PATH holds the stand-ins (but git) and a link to
#                       bun, and nothing else of the machine
#   PARITY_NO_PW=1      no playwright-cli among the stand-ins
#   PARITY_NO_OCR=1     no python3 among the stand-ins
#   PARITY_NO_CLIP=1    no xclip and no pbcopy among the stand-ins
#
# On Windows (Git Bash) the runtime finds programs by PATHEXT and never runs
# these stand-ins, which are shell scripts: the cases that need them are
# skipped there, and every other one names its playwright-cli in RDS_PW_BIN
# as `node <REPO>/rs/parity/shim.mjs pw` (rs/parity.gen.mjs).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
shims="$TMPDIR/shims"
mkdir -p "$shims"
shims_on_path=$shims
if command -v cygpath > /dev/null 2>&1; then
  shims_on_path=$(cygpath -u "$shims")
fi
node=$(command -v node)

stand_in() {
  # stand_in <name> <role>
  printf '#!/bin/sh\nexec "%s" "%s/shim.mjs" %s "$@"\n' "$node" "$here" "$2" > "$shims/$1"
  chmod +x "$shims/$1"
}
if [ "${PARITY_NO_PW:-}" != 1 ]; then stand_in playwright-cli pw; fi
if [ "${PARITY_NO_OCR:-}" != 1 ]; then stand_in python3 ocr; fi
if [ "${PARITY_NO_CLIP:-}" != 1 ]; then
  stand_in xclip xclip
  stand_in pbcopy pbcopy
fi
if [ "${PARITY_NO_GIT:-}" != 1 ]; then
  real_git=$(command -v git)
  cat > "$shims/git" << EOF
#!/bin/sh
printf 'git %s\n  cwd %s\n' "\$*" "\$(pwd)" >> "\$PARITY_LOG"
exec "$real_git" "\$@"
EOF
  chmod +x "$shims/git"
fi

if [ "${PARITY_NO_GIT:-}" = 1 ]; then
  # bun for the TypeScript and for the binary's hand-back; the stand-ins
  # start node by its full path.
  mkdir -p "$TMPDIR/bun"
  bun=$(command -v bun)
  bun_on_path=$TMPDIR/bun
  if command -v cygpath > /dev/null 2>&1; then
    # A Windows program finds bun only as bun.exe, and Git Bash's ln -s
    # would copy it: a hard link, or a copy where that cannot be made.
    case $bun in *.exe) ;; *) bun=$bun.exe ;; esac
    ln "$bun" "$TMPDIR/bun/bun.exe" 2> /dev/null || cp "$bun" "$TMPDIR/bun/bun.exe"
    bun_on_path=$(cygpath -u "$bun_on_path")
  else
    ln -s "$bun" "$TMPDIR/bun/bun"
  fi
  PATH="$shims_on_path:$bun_on_path"
else
  PATH="$shims_on_path:$PATH"
fi
export PATH

# The package the binary is pointed at must have the OCR scripts, which the
# TypeScript finds beside its modules and only then starts python3 (here
# the stand-in) with: the runner's stand-in package for the hand-back check
# has only package.json and the entry, and gets a copy of ocr/.
pkg=${RUSTDESK_SKILL_PACKAGE_ROOT:-}
if [ -n "$pkg" ] && [ -f "$pkg/package.json" ] && [ ! -e "$pkg/ocr" ]; then
  cp -R "$here/../../ocr" "$pkg/ocr"
fi

cd_to=${PARITY_CD:-}
unset PARITY_CD PARITY_NO_GIT PARITY_NO_PW PARITY_NO_OCR PARITY_NO_CLIP
if [ -n "$cd_to" ]; then cd "$cd_to"; fi
exec "$@"
