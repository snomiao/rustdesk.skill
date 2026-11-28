#!/usr/bin/env node
// Writes rs/parity.json, the parity cases for rustdesk-skill (Rust against
// TypeScript).
//
//   node rs/parity.gen.mjs            rewrite rs/parity.json
//   node rs/parity.gen.mjs --check    fail if rs/parity.json is out of date
//
// How to run them, from the repository root, after `bun install` and
// `cargo build --release --manifest-path rs/Cargo.toml`:
//
//   P=<rustify>/templates/parity/parity.mjs
//   node $P --cases rs/parity.json --ts "sh rs/parity/run.sh bun bin/rustdesk-skill.ts" \
//     --rs "env RUSTDESK_SKILL_PACKAGE_ROOT=$PWD sh rs/parity/run.sh rs/target/release/rustdesk-skill"
//   node $P --cases rs/parity.json --rs "sh rs/parity/run.sh rs/target/release/rustdesk-skill" \
//     --handback RUSTDESK_SKILL_PACKAGE_ROOT=bin/rustdesk-skill.ts
//
// (In Git Bash on Windows, $(pwd -W) instead of $PWD.) The first compares
// everything a case names; RUSTDESK_SKILL_PACKAGE_ROOT lets the binary hand
// back what it does not answer itself. The second runs the binary against a
// stand-in package and fails unless exactly the cases marked "handback" for
// the system it runs on reached the JavaScript. `sh rs/parity/proof.sh
// <parity.mjs> <binary>` runs both, in lanes, with the comparison of the
// argument gate against yargs (cargo test), on Linux, macOS or Windows: what
// the CI workflow runs on each.
//
// No case reaches a browser, a screen, OCR or a clipboard. rs/parity/run.sh
// puts stand-ins first on PATH for playwright-cli, python3, xclip and pbcopy
// (rs/parity/shim.mjs), and every case names its playwright-cli in RDS_PW_BIN
// as that stand-in too, unless it is about finding playwright-cli on PATH.
// The stand-ins write down every call (arguments, working directory, stdin,
// a digest of the environment and the RDS_* variables) into <ROOT>/calls.log,
// with every git the command starts; the cases print that log, so what each
// implementation asked of every program, in order, is compared along with
// stdout, stderr, the exit code and the files left behind. They answer from
// the case's PARITY_PW, PARITY_OCR and PARITY_CLIP (see rs/parity/shim.mjs).
// Every id and password in here is made up.
//
// Who answers. A case without `back` is answered by the binary on Linux and
// macOS. `back` says why it reaches the JavaScript instead: on every system
// (text), or on some (an object keyed by node's process.platform). On
// Windows the binary hands every invocation back (see "systems" at the end).

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const flags = new Set(process.argv.slice(2));

/** The playwright-cli stand-in, as RDS_PW_BIN names it (split at spaces, so no space in <REPO>). */
const PW = "node <REPO>/rs/parity/shim.mjs pw";
const sq = (text) => `'${String(text).replace(/'/g, `'\\''`)}'`;

// ── what a case looks at afterwards ──────────────────────────────────────────

/** Every program the command started, in order, with what it was given. */
const LOG = "cat <ROOT>/calls.log 2>/dev/null || echo '(nothing was started)'";
/** The same, each line once: for a case the binary hands back after starting something that only reads (the JavaScript starts it again). */
const LOG_ONCE = "sort -u <ROOT>/calls.log 2>/dev/null || echo '(nothing was started)'";
/** The case's directory and the temp directory (the session directory and the OCR's PNG are there). */
const FILES = 'ls -A . "$TMPDIR" "$TMPDIR/rustdesk-skill" 2>&1';
/** A file's bytes, as POSIX od prints them on every system. */
const dump = (file) => `od -An -c ${sq(file)} 2>&1 | sed 's/  */ /g'`;
/** The config directory's files, names and contents. */
const CONFIG = (dir = "<HOME>/.config/rustdesk-skill") =>
  `for f in ${dir}/*; do if [ -f "$f" ]; then echo "== $f"; cat "$f"; fi; done 2>/dev/null; true`;

// ── cases ────────────────────────────────────────────────────────────────────

const cases = [];
/**
 * One case. o.env: on top of RDS_SESSION_NAME=parity and RDS_PW_BIN=<the
 * stand-in> (null takes one out). o.pw, o.ocr, o.clip: the stand-ins'
 * answers. o.after: what to look at, on top of the log of calls. o.back: why
 * the case reaches the JavaScript (text, or per system). o.cd: PARITY_CD.
 * o.needs: stand-ins the case relies on finding on PATH (`path`), which
 * Windows does not run (see "systems").
 */
function add(name, args, o = {}) {
  const env = { RDS_SESSION_NAME: "parity", RDS_PW_BIN: PW, ...(o.env || {}) };
  if (o.pw) env.PARITY_PW = JSON.stringify(o.pw);
  if (o.ocr) env.PARITY_OCR = JSON.stringify(o.ocr);
  if (o.clip) env.PARITY_CLIP = JSON.stringify(o.clip);
  if (o.cd) env.PARITY_CD = o.cd;
  for (const [key, value] of Object.entries(env)) if (value === undefined) delete env[key];
  const testCase = { name, args };
  if (o.stdin !== undefined) testCase.stdin = o.stdin;
  testCase.env = env;
  if (o.files) testCase.files = o.files;
  if (o.setup) testCase.setup = o.setup;
  const late = typeof o.back === "string" && o.late;
  testCase.after = [late ? LOG_ONCE : LOG, ...(o.after || [])];
  if (o.normalize) testCase.normalize = o.normalize;
  if (o.tty !== undefined) testCase.tty = o.tty;
  if (o.capture) testCase.capture = o.capture;
  if (o.back) testCase.handback = o.back;
  if (o.skip) testCase.skip = o.skip;
  if (o.needs) testCase.needs = o.needs;
  if (cases.some((c) => c.name === name)) throw new Error(`two cases named ${JSON.stringify(name)}`);
  cases.push(testCase);
}

const YARGS = "yargs answers this (help, version, usage errors, a spelling the binary does not take)";
const NO_NATIVE = "RUSTDESK_SKILL_NO_NATIVE=1 hands everything back";

// ═══════════════════════════ the command line ═══════════════════════════

add("args: no command", [], { back: YARGS });
add("args: --help", ["--help"], { back: YARGS });
add("args: help", ["help"], { back: YARGS });
add("args: --version", ["--version"], { back: YARGS });
add("args: an unknown command", ["frobnicate"], { back: YARGS });
add("args: an unknown option before the command", ["--x", "list"], { back: YARGS });
add("args: list --help", ["list", "--help"], { back: YARGS });
add("args: list help", ["list", "help"], { back: YARGS });
add("args: list with a word after it", ["list", "x"], { back: YARGS });
add("args: list --json", ["list", "--json"], { back: YARGS });
add("args: scroll without dy", ["scroll"], { back: YARGS });
add("args: scroll with two numbers", ["scroll", "5", "6"], { back: YARGS });
add("args: scroll -- -5", ["scroll", "--", "-5"], { back: YARGS });
add("args: scroll 1e3", ["scroll", "1e3"], { back: YARGS });
add("args: scroll 0x10", ["scroll", "0x10"], { back: YARGS });
add("args: scroll abc (NaN)", ["scroll", "abc"], { back: YARGS });
add("args: scroll 05", ["scroll", "05"], { back: YARGS });
add("args: scroll 1.0000001 (more decimals than the binary takes)", ["scroll", "1.0000001"], { back: YARGS });
add("args: scroll 1234567890123456 (16 digits)", ["scroll", "1234567890123456"], { back: YARGS });
add("args: type without text", ["type"], { back: YARGS });
add("args: type -x", ["type", "-x"], { back: YARGS });
add("args: type -", ["type", "-"], { back: YARGS });
add("args: type help", ["type", "help"], { back: YARGS });
add("args: type true", ["type", "true"], { back: YARGS });
add("args: type --force false", ["type", "x", "--force", "false"], { back: YARGS });
add("args: type --no-force", ["type", "x", "--no-force"], { back: YARGS });
add("args: type --force twice", ["type", "x", "--force", "--force"], { back: YARGS });
add("args: key with two words", ["key", "a", "b"], { back: YARGS });
add("args: key -1", ["key", "-1"], { back: YARGS });
add("args: click --nth 2.5", ["click", "OK", "--nth", "2.5"], { back: YARGS });
add("args: click --nth -1", ["click", "OK", "--nth", "-1"], { back: YARGS });
add("args: click --nth without a value", ["click", "OK", "--nth"], { back: YARGS });
add("args: click --ocr tesseract", ["click", "OK", "--ocr", "tesseract"], { back: YARGS });
add("args: ocr --lang twice", ["ocr", "--lang", "ja", "--lang", "en"], { back: YARGS });
add("args: ocr --lang=", ["ocr", "--lang="], { back: YARGS });
add("args: connect without an id", ["connect"], { back: YARGS });
add("args: connect --rememberPassword", ["connect", "123456789", "pw", "--rememberPassword"], { back: YARGS });
add("args: move with one number", ["move", "5"], { back: YARGS });
add("args: status --json", ["status", "--json"], { back: YARGS });
add("args: --version after a command", ["list", "--version"], { back: YARGS });
add("args: NO_NATIVE hands list back", ["list"], { env: { RUSTDESK_SKILL_NO_NATIVE: "1" }, back: NO_NATIVE });
add("args: NO_NATIVE hands scroll back", ["scroll", "5"], { env: { RUSTDESK_SKILL_NO_NATIVE: "1" }, back: NO_NATIVE });

// ═══════════════════════════ list ═══════════════════════════

const RECENTS = (text, dir = ".config/rustdesk-skill") => [`mkdir -p <HOME>/${dir} && printf '%s' ${sq(text)} > <HOME>/${dir}/recents.json`];

add("list: nothing recorded", ["list"]);
add("list: two ids", ["list"], { setup: RECENTS('[\n  "123456789",\n  "987654321"\n]\n') });
add("list: an empty list", ["list"], { setup: RECENTS("[]") });
add("list: one id, on one line", ["list"], { setup: RECENTS('["0012345"]') });
add("list: not JSON", ["list"], { setup: RECENTS("not json") });
add("list: an empty file", ["list"], { setup: RECENTS("") });
add("list: an object", ["list"], { setup: RECENTS('{"a": "1"}') });
add("list: a string", ["list"], { setup: RECENTS('"123"') });
add("list: null", ["list"], { setup: RECENTS("null") });
add("list: a trailing comma", ["list"], { setup: RECENTS('["1",]') });
add("list: a comment", ["list"], { setup: RECENTS('// x\n["1"]') });
add("list: a byte order mark", ["list"], { setup: RECENTS('﻿["1"]') });
add("list: blanks and CR LF around", ["list"], { setup: RECENTS(' \r\n\t["1",\r\n "2"]\r\n ') });
add("list: escapes in the ids", ["list"], { setup: RECENTS('["a\\"b", "c\\\\d", "\\/", "\\u00e9\\u4e2d", "\\ud83d\\ude00", "tab\\there", "nl\\nx", "\\b\\f\\r", "\\u0001\\u001f\\u007f", "\\u2028\\u2029"]') });
add("list: raw non-ASCII ids", ["list"], { setup: RECENTS('["日本語", "é", "😀", " "]') });
add("list: an id with a lone surrogate", ["list"], { setup: RECENTS('["\\ud800"]'), back: "a lone surrogate: JSON.stringify writes it escaped; not ported" });
add("list: a raw control character in a string", ["list"], { setup: RECENTS('["a\tb"]') });
add("list: a bad escape", ["list"], { setup: RECENTS('["\\x41"]') });
add("list: a number", ["list"], { setup: RECENTS("[123456789]"), back: "an element that is not a string: JavaScript's number formatting is not ported" });
add("list: null and true in the list", ["list"], { setup: RECENTS('["1", null, true]'), back: "an element that is not a string: not ported" });
add("list: a list inside", ["list"], { setup: RECENTS('[["1"]]'), back: "an element that is not a string: not ported" });
add("list: an object inside", ["list"], { setup: RECENTS('[{"id": "1"}]'), back: "an element that is not a string: not ported" });
add("list: twenty-one ids and a duplicate", ["list"], { setup: RECENTS(JSON.stringify([...Array.from({ length: 21 }, (_, i) => String(100000000 + i)), "100000000"])) });
add("list: invalid UTF-8", ["list"], { setup: ["mkdir -p <HOME>/.config/rustdesk-skill && printf '[\"a\\377b\"]' > <HOME>/.config/rustdesk-skill/recents.json"], back: "a file that is not UTF-8: not ported" });
add("list: the file is a directory", ["list"], { setup: ["mkdir -p <HOME>/.config/rustdesk-skill/recents.json"] });
add("list: a link to nowhere", ["list"], { setup: ["mkdir -p <HOME>/.config/rustdesk-skill && ln -s nowhere <HOME>/.config/rustdesk-skill/recents.json"], skip: { win32: "Git Bash's ln -s makes no link" } });
add("list: a link to a file", ["list"], { setup: ["mkdir -p <HOME>/.config/rustdesk-skill && printf '[\"7\"]' > <HOME>/r.json && ln -s <HOME>/r.json <HOME>/.config/rustdesk-skill/recents.json"], skip: { win32: "Git Bash's ln -s copies the file" } });
add("list: RDS_CONFIG_DIR", ["list"], { env: { RDS_CONFIG_DIR: "<ROOT>/conf" }, setup: ["mkdir -p <ROOT>/conf && printf '[\"42\"]' > <ROOT>/conf/recents.json"] });
add("list: RDS_CONFIG_DIR empty is the default", ["list"], { env: { RDS_CONFIG_DIR: "" }, setup: RECENTS('["5"]') });
add("list: RDS_CONFIG_DIR relative", ["list"], { env: { RDS_CONFIG_DIR: "conf" }, setup: ["mkdir conf && printf '[\"8\"]' > conf/recents.json"] });
add("list: RDS_CONFIG_DIR with .. is resolved by its text", ["list"], { env: { RDS_CONFIG_DIR: "<ROOT>/nowhere/../conf" }, setup: ["mkdir -p <ROOT>/conf && printf '[\"9\"]' > <ROOT>/conf/recents.json"] });
add("list: RDS_CONFIG_DIR with // and /./", ["list"], { env: { RDS_CONFIG_DIR: "<ROOT>//./conf/" }, setup: ["mkdir -p <ROOT>/conf && printf '[\"10\"]' > <ROOT>/conf/recents.json"] });
add("list: XDG_CONFIG_HOME is not looked at", ["list"], { env: { XDG_CONFIG_HOME: "<ROOT>/xdg" }, setup: ["mkdir -p <ROOT>/xdg/rustdesk-skill && printf '[\"11\"]' > <ROOT>/xdg/rustdesk-skill/recents.json"] });
add("list: a big list", ["list"], { setup: RECENTS(JSON.stringify(Array.from({ length: 2000 }, (_, i) => `id-${i}`))) });
add("list: deep inside", ["list"], { setup: RECENTS(`${"[".repeat(40)}${"]".repeat(40)}`), back: "an element that is not a string: not ported" });
add("list: on a terminal", ["list"], { setup: RECENTS('["123"]'), tty: true });
add("list: stdout on a terminal", ["list"], { setup: RECENTS('["123"]'), tty: "stdout" });

// ═══════════════════════════ what bun reads first ═══════════════════════════

const GIT = "git init -q -b main";
const COMMIT = "git -c user.name=p -c user.email=p@example.com commit -q --allow-empty -m x";

const ENV = (files) => ({ files });
const CONF = (dir, ids) => [`mkdir -p ${dir} && printf '%s' ${sq(JSON.stringify(ids))} > ${dir}/recents.json`];
const DOTENV = "bun loads it, in a syntax the binary does not read";
add("runtime: .env.local sets RDS_CONFIG_DIR", ["list"], { ...ENV({ ".env.local": "RDS_CONFIG_DIR=<ROOT>/conf\n" }), setup: CONF("<ROOT>/conf", ["1"]) });
add("runtime: .env.local wins over .env", ["list"], { ...ENV({ ".env": "RDS_CONFIG_DIR=<ROOT>/a\n", ".env.local": "RDS_CONFIG_DIR=<ROOT>/b\n" }), setup: [...CONF("<ROOT>/a", ["a"]), ...CONF("<ROOT>/b", ["b"])] });
add("runtime: .env.development.local wins over .env.local", ["list"], { ...ENV({ ".env.development.local": "RDS_CONFIG_DIR=<ROOT>/a\n", ".env.local": "RDS_CONFIG_DIR=<ROOT>/b\n" }), setup: [...CONF("<ROOT>/a", ["a"]), ...CONF("<ROOT>/b", ["b"])] });
add("runtime: .env.development wins over .env", ["list"], { ...ENV({ ".env.development": "RDS_CONFIG_DIR=<ROOT>/a\n", ".env": "RDS_CONFIG_DIR=<ROOT>/b\n" }), setup: [...CONF("<ROOT>/a", ["a"]), ...CONF("<ROOT>/b", ["b"])] });
add("runtime: the environment wins over .env.local", ["list"], { env: { RDS_CONFIG_DIR: "<ROOT>/a" }, ...ENV({ ".env.local": "RDS_CONFIG_DIR=<ROOT>/b\n" }), setup: [...CONF("<ROOT>/a", ["a"]), ...CONF("<ROOT>/b", ["b"])] });
add("runtime: .env.production is not loaded", ["list"], { ...ENV({ ".env.production": "RDS_CONFIG_DIR=<ROOT>/a\n", ".env.test": "RDS_CONFIG_DIR=<ROOT>/a\n" }), setup: CONF("<ROOT>/a", ["a"]) });
add("runtime: .env with comments, quotes and CR LF", ["list"], { ...ENV({ ".env": "# config\r\n\r\nRDS_CONFIG_DIR='<ROOT>/a b' # the dir\r\nOTHER=\"x # y\"\r\n" }), setup: CONF("'<ROOT>/a b'", ["q"]) });
add("runtime: .env.local pins the session", ["scroll", "5"], { env: { RDS_SESSION_NAME: null }, files: { ".env.local": "RDS_SESSION_NAME=pinned\n" } });
add("runtime: what .env sets reaches playwright-cli", ["scroll", "5"], { env: { PARITY_SHOW: "FROM_DOTENV" }, files: { ".env": "FROM_DOTENV=yes\nRDS_PW_BROWSER=chrome\n" } });
add("runtime: .env with export", ["list"], { files: { ".env": "export RDS_CONFIG_DIR=<ROOT>/a\n" }, back: DOTENV });
add("runtime: .env with $ expansion", ["list"], { files: { ".env": "RDS_CONFIG_DIR=$HOME/a\n" }, back: DOTENV });
add("runtime: .env with an escape", ["list"], { files: { ".env.local": 'X="a\\nb"\n' }, back: DOTENV });
add("runtime: .env.local sets PATH", ["scroll", "5"], { files: { ".env.local": "PATH=/nowhere\n" }, back: "bun loads it, and it changes how programs are found" });
add("runtime: .env sets HOME", ["list"], { files: { ".env": "HOME=<ROOT>\n" }, back: "bun loads it, and it changes os.homedir()" });
add("runtime: .env sets TMPDIR", ["scroll", "5"], { files: { ".env": "TMPDIR=<ROOT>/t\n" }, back: "bun loads it, and it changes os.tmpdir()" });
add("runtime: .env sets NODE_ENV", ["list"], { files: { ".env": "NODE_ENV=production\n" }, back: "bun loads it, and it steers bun" });
add("runtime: .env sets the same name twice", ["list"], { files: { ".env": "A=1\nA=2\n" }, back: DOTENV });
add("runtime: .env.local is a directory", ["list"], { setup: ["mkdir .env.local"], back: "a .env.local that is not a file" });
add("runtime: RDS_SESSION_NAME set but empty, and in .env.local", ["scroll", "5"], { env: { RDS_SESSION_NAME: "" }, files: { ".env.local": "RDS_SESSION_NAME=fromfile\n" }, back: "a variable set but empty and also in a .env file: not compared" });
add("runtime: bunfig.toml here", ["list"], { files: { "bunfig.toml": "" }, back: "bun reads bunfig.toml" });
add("runtime: ~/.bunfig.toml", ["list"], { setup: ["touch <HOME>/.bunfig.toml"], back: "bun reads ~/.bunfig.toml" });
add("runtime: $XDG_CONFIG_HOME/.bunfig.toml", ["list"], { setup: ["mkdir -p <HOME>/.config && touch <HOME>/.config/.bunfig.toml"], back: "bun reads $XDG_CONFIG_HOME/.bunfig.toml" });
add("runtime: package.json here", ["list"], { files: { "package.json": "{}" } });
add("runtime: a broken package.json above", ["scroll", "5"], { files: { "package.json": "{oops", "sub/x": "" }, cd: "sub", back: "bun reads package.json here and above, and may say so on stderr" });
add("runtime: tsconfig.json here", ["list"], { files: { "tsconfig.json": "{}" } });
const PKG = JSON.stringify({ name: "some-app", version: "1.2.3", private: true, type: "module", scripts: { test: "bun test" }, dependencies: { yargs: "^17.7.2" }, devDependencies: { typescript: "^5" } }, null, 2) + "\n";
const TSC = JSON.stringify({ compilerOptions: { target: "ESNext", module: "ESNext", strict: true, baseUrl: ".", types: ["bun-types"] }, include: ["src"] }, null, 2) + "\n";
const PF = "bun reads the project files here and above and says on stderr what it does not like; the binary takes only plain ones";
add("runtime: project file: a typical package.json here", ["list"], { files: { "package.json": PKG }, setup: RECENTS('["1"]') });
add("runtime: project file: a typical package.json here, scroll", ["scroll", "5"], { files: { "package.json": PKG }, env: { RDS_SESSION_NAME: null } });
add("runtime: project file: all three above and here", ["scroll", "5"], { files: { "package.json": PKG, "tsconfig.json": TSC, "jsconfig.json": "{\n}\n", "app/package.json": '{"name": "inner", "type": "commonjs", "main": "index.js"}', "app/tsconfig.json": TSC }, cd: "app", env: { RDS_SESSION_NAME: null } });
add("runtime: project file: in a git repository with a package.json", ["scroll", "5"], { files: { "package.json": PKG }, setup: [GIT, COMMIT], env: { RDS_SESSION_NAME: null } });
add("runtime: project file: type weird", ["list"], { files: { "package.json": '{"type": "weird"}' }, back: PF });
add("runtime: project file: a name given twice", ["list"], { files: { "package.json": '{"name": "a", "name": "b"}' }, back: PF });
add("runtime: project file: a name given twice, deeper", ["list"], { files: { "package.json": '{"scripts": {"t": "1", "t": "2"}}' }, back: PF });
add("runtime: project file: comments", ["list"], { files: { "tsconfig.json": '// settings\n{"compilerOptions": {"strict": true,},}\n' }, back: PF });
add("runtime: project file: exports", ["list"], { files: { "package.json": '{"exports": {".": "./index.js"}}' }, back: PF });
add("runtime: project file: extends", ["list"], { files: { "tsconfig.json": '{"extends": "./base.json"}' }, back: PF });
add("runtime: project file: paths", ["list"], { files: { "tsconfig.json": '{"compilerOptions": {"paths": {"a*b*": ["./x"]}}}' }, back: PF });
add("runtime: project file: empty", ["list"], { files: { "package.json": "" }, back: PF });
add("runtime: project file: a list", ["list"], { files: { "jsconfig.json": "[]" }, back: PF });
add("runtime: project file: a byte order mark", ["list"], { files: { "package.json": "\ufeff{}" }, back: PF });
add("runtime: project file: a directory", ["list"], { setup: ["mkdir package.json"], back: PF });
add("runtime: BUN_OPTIONS", ["list"], { env: { BUN_OPTIONS: "--smol" }, back: "a variable that steers bun" });
add("runtime: NODE_OPTIONS", ["list"], { env: { NODE_OPTIONS: "" }, back: "a variable that steers bun" });
add("runtime: BUN_INSTALL is no matter", ["list"], { env: { BUN_INSTALL: "<ROOT>/bun" } });
add("runtime: FORCE_COLOR", ["scroll", "5"], { env: { FORCE_COLOR: "1" }, back: "FORCE_COLOR makes console.error red" });
add("runtime: NO_COLOR on pipes", ["scroll", "5"], { env: { NO_COLOR: "1", RDS_SESSION_NAME: null } });
add("runtime: LD_LIBRARY_PATH", ["list"], { env: { LD_LIBRARY_PATH: "" }, back: "a variable for the loader that starts bun" });
add("runtime: scroll with stdout on a terminal", ["scroll", "5"], { tty: "stdout", back: "on a terminal console.error is red" });
add("runtime: scroll with stderr on a terminal", ["scroll", "5"], { tty: "stderr", back: "on a terminal console.error is red" });
add("runtime: scroll on a terminal, no session name, not saved", ["scroll", "5"], { tty: true, env: { RDS_SESSION_NAME: null }, stdin: [["[cwd]: [cwd] ", "my sess\n"], ["[Y/n]:", "n\n"]], after: [FILES], back: "on a terminal the TypeScript asks for the session name" });
add("runtime: scroll on a terminal, no session name, saved", ["scroll", "5"], { tty: true, env: { RDS_SESSION_NAME: null }, files: { ".env.local": "A=1\n\n\n" }, stdin: [["[cwd]: [cwd] ", "\n"], ["[Y/n]:", "\n"]], after: ["cat .env.local"], back: "on a terminal the TypeScript asks for the session name" });
add("runtime: stdin alone on a terminal", ["scroll", "5"], { tty: "stdin", env: { RDS_SESSION_NAME: null } });
add("runtime: NO_COLOR on a terminal for list", ["list"], { tty: true, env: { NO_COLOR: "1" }, setup: RECENTS('["1"]') });

// ═══════════════════════════ the session name ═══════════════════════════

const S = (name, args, o = {}) => add(`session: ${name}`, args, { ...o, env: { RDS_SESSION_NAME: null, ...(o.env || {}) } });
add("session: RDS_SESSION_NAME is sanitised", ["scroll", "5"], { env: { RDS_SESSION_NAME: "  My Proj/Main #1 " } });
add("session: RDS_SESSION_NAME of nothing but marks", ["scroll", "5"], { env: { RDS_SESSION_NAME: "///" } });
add("session: RDS_SESSION_NAME in Unicode white space", ["scroll", "5"], { env: { RDS_SESSION_NAME: "　日本 x﻿" } });
add("session: RDS_SESSION_NAME of blanks is not set", ["scroll", "5"], { env: { RDS_SESSION_NAME: " \t " } });
S("not a repository: the folder's name", ["scroll", "5"]);
S("a repository without commits", ["scroll", "5"], { setup: [GIT] });
S("a repository on main", ["scroll", "5"], { setup: [GIT, COMMIT] });
S("a branch with a slash", ["scroll", "5"], { setup: [GIT, COMMIT, "git checkout -q -b feature/x"] });
S("a detached HEAD", ["scroll", "5"], { setup: [GIT, COMMIT, "git checkout -q --detach"] });
S("in a folder of a repository", ["scroll", "5"], { setup: [GIT, COMMIT, "mkdir -p sub/deeper"], cd: "sub/deeper" });
S("a repository whose folder has a space", ["scroll", "5"], { setup: ["mkdir 'my repo' && cd 'my repo' && git init -q -b dev"], cd: "my repo" });
S("a folder named in Japanese only", ["scroll", "5"], { setup: ["mkdir 日本語"], cd: "日本語" });
S("no git", ["scroll", "5"], { env: { PARITY_NO_GIT: "1" } });
S("in the root directory", ["scroll", "5"], { cd: "/", skip: { win32: "the root of a drive in Git Bash is not one directory" } });
S("asked again by every step", ["key", "ctrl+shift+a"], { setup: [GIT, COMMIT] });
S("a repository with GIT_DIR elsewhere", ["scroll", "5"], { setup: ["git init -q -b other <ROOT>/elsewhere"], env: { GIT_DIR: "<ROOT>/elsewhere/.git" } });

// ═══════════════════════════ playwright-cli ═══════════════════════════

const FAIL = (sub, answer) => ({ pw: { [sub]: answer } });
add("pw: found on PATH by its name", ["scroll", "5"], { env: { RDS_PW_BIN: null }, needs: ["path"] });
add("pw: RDS_PW_BIN of blanks is not set", ["scroll", "5"], { env: { RDS_PW_BIN: "   " }, needs: ["path"] });
add("pw: not on PATH", ["scroll", "5"], { env: { RDS_PW_BIN: null, PARITY_NO_PW: "1", PARITY_NO_GIT: "1" }, needs: ["path"] });
add("pw: RDS_PW_BIN names a file that is not there", ["scroll", "5"], { env: { RDS_PW_BIN: "<ROOT>/nowhere/playwright-cli" } });
add("pw: RDS_PW_BIN names a program that is nowhere on PATH", ["scroll", "5"], { env: { RDS_PW_BIN: "rds-parity-no-such-program" } });
add("pw: RDS_PW_BIN with blanks around and between", ["scroll", "5"], { env: { RDS_PW_BIN: " \tnode  <REPO>/rs/parity/shim.mjs　pw " } });
add("pw: RDS_PW_BIN names a file that may not be run", ["scroll", "5"], { env: { RDS_PW_BIN: "<ROOT>/pw" }, setup: ["printf '#!/bin/sh\\n' > <ROOT>/pw"], back: "a program bun cannot start: bun's own words", skip: { win32: "no execute bit" } });
add("pw: RDS_PW_BIN names a directory", ["scroll", "5"], { env: { RDS_PW_BIN: "<ROOT>" }, back: "a program bun cannot start: bun's own words" });
add("pw: RDS_PW_BIN names a program without a #! line", ["scroll", "5"], { env: { RDS_PW_BIN: "<ROOT>/pw" }, setup: ["printf 'exit 0\\n' > <ROOT>/pw && chmod +x <ROOT>/pw"], back: "a program without a #! line: bun fails with ENOEXEC", skip: { win32: "no execute bit" } });
add("pw: RDS_PW_BIN with a relative path", ["scroll", "5"], { env: { RDS_PW_BIN: "./pw" }, files: { pw: "#!/bin/sh\n" }, back: "a relative path: not compared" });
add("pw: a step that fails, with stderr", ["scroll", "5"], FAIL("mousewheel", { code: 3, out: "o", err: "boom\n" }));
add("pw: a step that fails, with stdout only", ["scroll", "5"], FAIL("mousewheel", { code: 2, out: "only out\n" }));
add("pw: a step that fails silently", ["scroll", "5"], FAIL("mousewheel", { code: 1 }));
add("pw: a step that fails with exit 255", ["scroll", "5"], FAIL("mousewheel", { code: 255, err: "x" }));
add("pw: a step killed by a signal counts as done", ["scroll", "5"], { ...FAIL("mousewheel", { kill: "SIGKILL", err: "dying" }), skip: { win32: "no signals" } });
add("pw: a step that fails with bytes that are not UTF-8", ["scroll", "5"], FAIL("mousewheel", { code: 1, err: "a\\xffb\\xc0\\x80c\\xed\\xa0\\x80d\\xf0\\x9f\\x98e" }));
add("pw: a step that fails with a byte order mark", ["scroll", "5"], FAIL("mousewheel", { code: 1, err: "\\xef\\xbb\\xbfbom" }));
add("pw: a step that fails with % in its message", ["scroll", "5"], FAIL("mousewheel", { code: 1, err: "%s %d %% %o %j %c" }));
add("pw: a step that prints a lot and succeeds", ["scroll", "5"], FAIL("mousewheel", { big: 900000 }));
add("pw: a step that prints and succeeds", ["scroll", "5"], FAIL("mousewheel", { out: "### Result\nok\n", err: "warn\n" }));

// ═══════════════════════════ scroll, key, type, disconnect ═══════════════════════════

for (const dy of ["300", "-300", "0", "-0", "2.5", "-0.5", "123456789012345", "1.000001", "0.000001"]) add(`scroll: ${dy}`, ["scroll", dy]);
add("scroll: without a session name", ["scroll", "-120"], { env: { RDS_SESSION_NAME: null } });
add("scroll: in RDS_SESSION_DIR", ["scroll", "1"], { env: { RDS_SESSION_DIR: "<ROOT>/sessions/one" } });
add("scroll: RDS_SESSION_DIR empty is the default", ["scroll", "1"], { env: { RDS_SESSION_DIR: "" } });
add("scroll: RDS_SESSION_DIR that is there", ["scroll", "1"], { env: { RDS_SESSION_DIR: "<ROOT>/s" }, setup: ["mkdir <ROOT>/s"] });
add("scroll: RDS_SESSION_DIR with // and a trailing slash", ["scroll", "1"], { env: { RDS_SESSION_DIR: "<ROOT>//s/" } });
add("scroll: RDS_SESSION_DIR is a file", ["scroll", "1"], { env: { RDS_SESSION_DIR: "<ROOT>/f" }, setup: ["touch <ROOT>/f"], back: "a session directory that is a file: bun's own words" });
add("scroll: RDS_SESSION_DIR relative", ["scroll", "1"], { env: { RDS_SESSION_DIR: "s" }, back: "a relative session directory: not compared" });
add("scroll: RDS_SESSION_DIR with ..", ["scroll", "1"], { env: { RDS_SESSION_DIR: "<ROOT>/a/../s" }, back: "a session directory with ..: not compared" });

for (const chord of ["Enter", "enter", "ctrl+c", "Ctrl + Shift + K", "alt+shift+Tab", "CMD+option+esc", "win+up", "meta+Down", "control+left", "shift+RIGHT", "Backquote", "BACKSPACE", "delete", "space", "a+", "+a", "ctrl++", "é", "F5", "ctrl+日"]) {
  add(`key: ${chord}`, ["key", chord]);
}
add("key: an unknown modifier", ["key", "hyper+x"]);
add("key: a modifier in another script", ["key", "ｃｔｒｌ+x"]);
add("key: only plus signs", ["key", "+ +"]);
add("key: an empty chord", ["key", ""]);
add("key: a modifier named constructor", ["key", "constructor+x"], { back: "a name Object.prototype has: not ported" });
add("key: a key named __proto__", ["key", "__proto__"], { back: "a name Object.prototype has: not ported" });
add("key: the Kelvin sign", ["key", "bacKquote"], { back: "toLowerCase turns it into ASCII: not ported" });
add("key: keydown fails", ["key", "ctrl+shift+x"], FAIL("keydown", [{}, { code: 1, err: "no\n" }]));
add("key: press fails after the modifiers", ["key", "ctrl+x"], FAIL("press", { code: 4 }));
add("key: without a session name", ["key", "alt+F4"], { env: { RDS_SESSION_NAME: null } });

for (const text of ["hello-123.4/5", "日本語 テキスト", "two\nlines", "a b c", "", "Ａ fullwidth", "'quoted'", "x=y;z"]) add(`type: ${JSON.stringify(text)}`, ["type", text]);
add("type: shift characters are refused", ["type", "Secret@1"]);
add("type: every shift character, each once", ["type", "AZ~!@#$%^&*()_+{}|:\"<>?AZ!"]);
add("type: shift characters with --force", ["type", "Hello@World+", "--force"]);
add("type: --force first", ["type", "--force", "Password!"]);
add("type: --force without shift characters", ["type", "plain", "--force"]);
add("type: the step fails", ["type", "x"], FAIL("type", { code: 1, err: "typing failed\n" }));

add("disconnect: goes home", ["disconnect"]);
add("disconnect: the step fails", ["disconnect"], FAIL("goto", { code: 1, err: "net::ERR\n" }));
add("disconnect: without a session name", ["disconnect"], { env: { RDS_SESSION_NAME: null } });

// ═══════════════════════════ screenshot, move, click-xy ═══════════════════════════

const SHOT = 'cksum rds-screenshot.png shot.png "$TMPDIR/rustdesk-skill/rds-shot.png" rds-shot.png 2>&1';
const viewport = (w, h) => ({ "eval:viewport": { out: `### Result\n${JSON.stringify({ w, h }, null, 2)}\n### Ran Playwright code\n` } });
const VIEW = (text) => ({ "eval:viewport": { out: text } });
add("screenshot: to the default file", ["screenshot"], { after: [SHOT, FILES] });
add("screenshot: ss to a file", ["ss", "shot.png"], { after: [SHOT] });
add("screenshot: an empty file name is the default", ["ss", ""], { after: [SHOT] });
add("screenshot: into a folder that is there", ["ss", "d/shot.png"], { setup: ["mkdir d"], after: ["cksum d/shot.png"] });
add("screenshot: into a folder that is not there", ["ss", "d/shot.png"], { back: "a file that cannot be written: bun's own words", late: true });
add("screenshot: over a directory", ["ss", "d"], { setup: ["mkdir d"], back: "a file that cannot be written: bun's own words", late: true });
add("screenshot: over a file that is there", ["ss", "shot.png"], { files: { "shot.png": "old" }, after: [SHOT] });
add("screenshot: an absolute path", ["ss", "<ROOT>/abs.png"], { after: ["cksum <ROOT>/abs.png"] });
add("screenshot: in RDS_SESSION_DIR", ["ss"], { env: { RDS_SESSION_DIR: "<ROOT>/sess" }, after: [SHOT, "cksum <ROOT>/sess/rds-shot.png"] });
add("screenshot: playwright-cli writes it into this directory instead", ["ss"], { pw: { screenshot: { png: { at: "<CWD>/rds-shot.png", size: [800, 600] } } }, after: [SHOT] });
add("screenshot: no file is written", ["ss"], { pw: { screenshot: { png: "none" } }, after: [FILES] });
add("screenshot: too short to be a PNG", ["ss"], { pw: { screenshot: { png: "short" } }, after: [FILES] });
add("screenshot: a directory where the PNG should be", ["ss"], { pw: { screenshot: { png: "dir" } }, back: "a screenshot that cannot be read: bun's own words", late: true });
add("screenshot: the step fails", ["ss"], { pw: { screenshot: { code: 1, err: "no page\n" } }, after: [FILES] });
add("screenshot: the step prints too much", ["ss"], { pw: { screenshot: { big: 2000000 } }, back: "a step that prints more than a mebibyte: bun stops it and says so in its own words", late: true });
add("screenshot: without a session name", ["ss"], { env: { RDS_SESSION_NAME: null }, after: [SHOT] });
add("screenshot: a PNG of 0 by 0", ["ss"], { pw: { screenshot: { png: [0, 0] } }, after: [SHOT] });
add("screenshot: a PNG of the largest size", ["ss"], { pw: { screenshot: { png: [4294967295, 4294967295] } }, after: [SHOT] });

for (const [x, y] of [["640", "360"], ["0", "0"], ["-5", "7"], ["1.5", "2.5"], ["1279", "719"]]) add(`move: ${x} ${y}`, ["move", x, y]);
add("move: a retina screenshot halves", ["move", "1001", "3"], { pw: { screenshot: { png: [2560, 1440] } } });
add("move: a viewport of another shape", ["move", "333", "111"], { pw: { screenshot: { png: [1000, 1000] }, ...viewport(1280, 720) } });
add("move: the viewport is not JSON", ["move", "10", "20"], { pw: { screenshot: { png: [2000, 1000] }, ...VIEW("### Result\nundefined\n") } });
add("move: the viewport without a report", ["move", "10", "20"], { pw: { screenshot: { png: [2000, 1000] }, ...VIEW('{"w": 1000, "h": 500}') } });
add("move: the viewport in CR LF", ["move", "10", "20"], { pw: { screenshot: { png: [2000, 1000] }, ...VIEW('### Result\r\n{\r\n  "w": 1000,\r\n  "h": 500\r\n}\r\n### Ran Playwright code\r\n') } });
add("move: a viewport width of 0", ["move", "10", "20"], { pw: { screenshot: { png: [2000, 1000] }, ...viewport(0, 500) } });
add("move: a negative viewport", ["move", "10", "20"], { pw: { screenshot: { png: [2000, 1000] }, ...viewport(-1000, 500) } });
add("move: a viewport with fractions", ["move", "333", "333"], { pw: { screenshot: { png: [2000, 1000] }, ...viewport(1234.5, 617.25) } });
add("move: a viewport that is a list", ["move", "10", "20"], { pw: { screenshot: { png: [2000, 1000] }, ...VIEW("### Result\n[1000, 500]\n") } });
add("move: a viewport that is null", ["move", "10", "20"], { pw: { screenshot: { png: [2000, 1000] }, ...VIEW("### Result\nnull\n") } });
add("move: a viewport given twice", ["move", "10", "20"], { pw: { screenshot: { png: [2000, 1000] }, ...VIEW('### Result\n{"w": 1, "h": 1, "w": 1000, "h": 500}\n') } });
add("move: a viewport given in strings", ["move", "10", "20"], { pw: { screenshot: { png: [2000, 1000] }, ...VIEW('### Result\n{"w": "1000", "h": "500"}\n') }, back: "JavaScript's arithmetic on a string: not ported", late: true });
add("move: a viewport too large to be a number", ["move", "10", "20"], { pw: { screenshot: { png: [2000, 1000] }, ...VIEW('### Result\n{"w": 1e400, "h": 500}\n') }, back: "a coordinate that is not finite: not ported", late: true });
add("move: a PNG of width 0", ["move", "10", "20"], { pw: { screenshot: { png: [0, 0] } } });
add("move: the viewport step fails", ["move", "10", "20"], { pw: { "eval:viewport": { code: 1, err: "eval broke\n" } } });
add("move: the screenshot step fails", ["move", "10", "20"], { pw: { screenshot: { code: 7 } } });
add("move: the mouse step fails", ["move", "10", "20"], { pw: { mousemove: { code: 1, err: "no mouse\n" } } });
add("move: no screenshot file", ["move", "10", "20"], { pw: { screenshot: { png: "none" } } });
add("move: without a session name", ["move", "1", "2"], { env: { RDS_SESSION_NAME: null } });
add("click-xy: clicks", ["click-xy", "685", "315"]);
add("click-xy: a retina screenshot", ["click-xy", "685", "315"], { pw: { screenshot: { png: [2560, 1440] } } });
add("click-xy: mousedown fails", ["click-xy", "1", "1"], { pw: { mousedown: { code: 1, err: "down\n" } } });
add("click-xy: mouseup fails", ["click-xy", "1", "1"], { pw: { mouseup: { code: 9 } } });

// ═══════════════════════════ status, ocr, click ═══════════════════════════

/** A PaddleOCR box (pixels). */
const pbox = (text, x, y, w = 40, h = 20, confidence = 0.98) => ({ text, confidence, x1: x, y1: y, x2: x + w, y2: y + h });
const HOME_SCREEN = [pbox("RustDesk （Preview）", 556, 20, 168, 22), pbox("リモートデスクトップを操作@", 347, 69, 277, 27), pbox("リモートID", 358, 135, 84, 18)];
const DIALOG = [pbox("※** パスワードが必要です", 524, 250, 228, 20), pbox("RustDesk のパスワードを確認する", 514, 302, 248, 18), pbox("白 パスワード", 526, 350, 116, 22, 0.5), pbox("パスワードを記憶する", 566, 406, 132, 14), pbox("✕ キャンセル", 546, 458, 112, 22), pbox("<OK", 690, 460, 62, 18, 0.5)];
const REMOTE = [pbox("組織ダッシュボード（管理者）", 357, 389, 229, 27), pbox("新しいタブ", 370, 96, 76, 20)];
const URL = (url) => ({ "eval:url": { out: `### Result\n${url}\n### Ran Playwright code\n` } });
const OCR = { needs: ["ocr"] };
add("status: on the client and connected", ["status"], { ...OCR, ocr: { boxes: REMOTE } });
add("status: on the client, nothing read", ["status"], OCR);
add("status: on the home screen", ["status"], { ...OCR, ocr: { boxes: HOME_SCREEN } });
add("status: in the password dialog", ["status"], { ...OCR, ocr: { boxes: DIALOG } });
add("status: on another page", ["status"], { pw: URL('"about:blank"') });
add("status: an address in single quotes", ["status"], { ...OCR, pw: URL("'https://rustdesk.com/web/#/'") });
add("status: an address without quotes", ["status"], { ...OCR, pw: URL("https://rustdesk.com/web/") });
add("status: rustdesk.com/web elsewhere in the address", ["status"], { ...OCR, pw: URL('"https://example.com/?u=rustdesk.com/web"') });
add("status: just a quote", ["status"], { pw: URL('"') });
add("status: the address step fails", ["status"], { pw: { "eval:url": { code: 1, err: "no browser\n" } } });
add("status: playwright-cli is missing", ["status"], { env: { RDS_PW_BIN: "<ROOT>/nowhere" } });
add("status: no session name", ["status"], { ...OCR, env: { RDS_SESSION_NAME: null } });
add("status: no session name can be found", ["status"], { env: { RDS_SESSION_NAME: null }, cd: "/", skip: { win32: "the root of a drive in Git Bash is not one directory" } });
add("status: OCR fails", ["status"], { ...OCR, ocr: { code: 1, err: "No module named 'paddleocr'\n" } });
add("status: the second address step fails", ["status"], { ...OCR, pw: { "eval:url": [{}, { code: 1 }] } });
add("status: RDS_OCR=vision", ["status"], { ...OCR, env: { RDS_OCR: "vision" } });

const OCR_BOXES = [pbox("OK", 10, 20, 40, 20, 0.9), pbox("Cancel", 60, 20, 80, 20, 0.125), pbox("日本語 😀 %s", 5, 300, 1200, 30, 0.005), pbox("tab\there \"q\"", 0, 0, 1, 1, 1)];
add("ocr: lines", ["ocr"], { ...OCR, ocr: { boxes: OCR_BOXES } });
add("ocr: --json", ["ocr", "--json"], { ...OCR, ocr: { boxes: OCR_BOXES } });
add("ocr: nothing found", ["ocr"], OCR);
add("ocr: nothing found, --json", ["ocr", "--json"], OCR);
add("ocr: empty output", ["ocr", "--json"], { ...OCR, ocr: { out: "" } });
add("ocr: Vision boxes are fractions of the PNG", ["ocr", "--json", "--ocr", "vision"], { ...OCR, pw: { screenshot: { png: [1000, 800] } }, ocr: { boxes: [{ text: "Connect", confidence: 0.8, x1_norm: 0.5, y1_norm: 0.25, x2_norm: 0.6, y2_norm: 0.3 }, { text: "half", x1_norm: 0.0005, y1_norm: null, x2_norm: 0.0015 }] } });
add("ocr: a box without confidence or corners", ["ocr"], { ...OCR, ocr: { boxes: [{ text: "bare" }, { text: "nulls", x1_norm: null, x1: null, confidence: null }, { text: "string conf", confidence: "0.5" }] } });
add("ocr: confidences that round both ways", ["ocr"], { ...OCR, ocr: { boxes: [0, 0.004, 0.005, 0.015, 0.125, 0.375, 0.994, 0.995, 0.9999, 1, 1.005, -0.001, 12.345].map((confidence, i) => pbox(`c${i}`, i, i, 1, 1, confidence)) } });
add("ocr: coordinates that round", ["ocr", "--json"], { ...OCR, ocr: { boxes: [{ text: "r", x1: 0.5, y1: -0.5, x2: 2.4999, y2: 1.5 }, { text: "n", x1: -10.5, y1: -2.5, x2: -3, y2: 0 }] } });
add("ocr: --lang en", ["ocr", "--lang", "en"], OCR);
add("ocr: --lang=zh,en", ["ocr", "--lang=zh,en"], OCR);
add("ocr: --lang starting with a comma", ["ocr", "--lang", ",en"], OCR);
add("ocr: --lang of blanks", ["ocr", "--lang", "  "], OCR);
add("ocr: --lang ko for Vision", ["ocr", "--ocr", "vision", "--lang", "ko, ja ,en,,ch"], OCR);
add("ocr: --lang of commas for Vision", ["ocr", "--ocr", "vision", "--lang", ",,"], OCR);
add("ocr: --lang toString", ["ocr", "--lang", "toString"], { ...OCR, back: "a name Object.prototype has: not ported", late: true });
add("ocr: RDS_OCR in capitals", ["ocr"], { ...OCR, env: { RDS_OCR: "VISION" } });
add("ocr: RDS_OCR unknown", ["ocr"], { ...OCR, env: { RDS_OCR: "tesseract" } });
add("ocr: --ocr wins over RDS_OCR", ["ocr", "--ocr", "paddle"], { ...OCR, env: { RDS_OCR: "vision" } });
add("ocr: not JSON", ["ocr"], { ...OCR, ocr: { out: "Traceback: something\n" } });
add("ocr: long output that is not JSON", ["ocr"], { ...OCR, ocr: { out: `${"x".repeat(150)}${"日".repeat(100)}` } });
add("ocr: not JSON, a character cut at 200", ["ocr"], { ...OCR, ocr: { out: `${"x".repeat(199)}😀 more` }, back: "a character cut in two at 200: not ported", late: true });
add("ocr: an object", ["ocr"], { ...OCR, ocr: { out: '{"text": "a"}' }, back: "OCR output that is not a list: not ported", late: true });
add("ocr: a list with null", ["ocr"], { ...OCR, ocr: { out: "[null]" }, back: "OCR output that is not a list of objects: not ported", late: true });
add("ocr: a box with corners in strings", ["ocr"], { ...OCR, ocr: { boxes: [{ text: "s", x1: "5" }] }, back: "JavaScript's arithmetic on a string: not ported", late: true });
add("ocr: a box without text", ["ocr"], { ...OCR, ocr: { boxes: [{ x1: 5 }] }, back: "a box without text: not ported", late: true });
add("ocr: a coordinate too large", ["ocr"], { ...OCR, ocr: { boxes: [{ text: "far", x1: 1e300 }] }, back: "a number JavaScript writes in its own way: not ported", late: true });
add("ocr: OCR fails with stderr", ["ocr"], { ...OCR, ocr: { code: 2, err: "ModuleNotFoundError: paddleocr\n" } });
add("ocr: OCR fails silently", ["ocr"], { ...OCR, ocr: { code: 1 } });
add("ocr: OCR is killed", ["ocr"], { ...OCR, ocr: { kill: "SIGTERM" }, skip: { win32: "no signals" } });
add("ocr: OCR prints too much", ["ocr"], { ...OCR, ocr: { big: 2000000 }, back: "a program that prints more than a mebibyte: bun stops it and says so in its own words", late: true });
add("ocr: no python3", ["ocr"], { ...OCR, env: { PARITY_NO_GIT: "1", PARITY_NO_OCR: "1" } });
// 400 lines of console.log: bun loses some of them into a pipe that is read
// slowly (TypeScript against TypeScript differed here on a loaded machine),
// so this one prints into files, where every line arrives.
add("ocr: many boxes", ["ocr"], { ...OCR, capture: "files", ocr: { boxes: Array.from({ length: 400 }, (_, i) => pbox(`box ${i}`, i * 3, i * 2, 30 + (i % 7), 10, (i % 100) / 100)) } });
add("ocr: the screenshot step fails", ["ocr"], { ...OCR, pw: { screenshot: { code: 1, err: "gone\n" } } });
add("ocr: the PNG is left in the temp directory", ["ocr"], { ...OCR, after: ['ls "$TMPDIR"'] });

const CLICK = { ...OCR, ocr: { boxes: [pbox("OK", 0, 0, 20, 10), pbox("Cancel", 30, 0, 40, 10), pbox("ok ", 0, 50, 21, 11), pbox("Ärger ÜBER", 100, 100, 50, 10), pbox("接続する", 200, 200, 60, 20)] } };
add("click: exact, in any case", ["click", "cancel"], CLICK);
add("click: two match: the first, and a warning", ["click", "OK"], CLICK);
add("click: --nth 2", ["click", "OK", "--nth", "2"], CLICK);
add("click: --nth past the matches", ["click", "OK", "--nth", "3"], CLICK);
add("click: --nth 0", ["click", "OK", "--nth=0"], CLICK);
add("click: --contains", ["click", "anc", "--contains"], CLICK);
add("click: --contains with two matches", ["click", "o", "--contains"], CLICK);
add("click: blanks around the text", ["click", "  Cancel  "], CLICK);
add("click: not on the screen", ["click", "Nope"], CLICK);
add("click: letters with marks in another case", ["click", "ärger über"], CLICK);
add("click: Japanese, by a part", ["click", "接続", "--contains"], CLICK);
add("click: an empty text with --contains", ["click", "", "--contains"], CLICK);
add("click: a capital sigma", ["click", "ΣΑΣ"], { ...CLICK, back: "Σ, whose lower case depends on its neighbours: not ported", late: true });
add("click: on a retina screenshot", ["click", "接続する"], { ...CLICK, pw: { screenshot: { png: [2560, 1440] } } });
add("click: --ocr vision --lang en", ["click", "OK", "--ocr", "vision", "--lang", "en", "--nth", "1"], CLICK);
add("click: the click fails", ["click", "Cancel"], { ...CLICK, pw: { mousedown: { code: 1, err: "nope\n" } } });
add("click: the second screenshot fails", ["click", "Cancel"], { ...CLICK, pw: { screenshot: [{}, { code: 1, err: "second\n" }] } });
add("click: OCR fails", ["click", "Cancel"], { ...OCR, ocr: { code: 1, err: "bad\n" } });
add("click: odd box sizes round up", ["click", "odd"], { ...OCR, ocr: { boxes: [pbox("odd", 3, 5, 7, 9)] } });
add("click: without a session name", ["click", "OK", "--nth", "1"], { ...CLICK, env: { RDS_SESSION_NAME: null } });

// ═══════════════════════════ paste ═══════════════════════════

const CLIP = { needs: ["clip"] };
add("paste: sets the clipboard, then Ctrl+V", ["paste", "Hello@World 日本語\nline 2"], CLIP);
add("paste: the clipboard program fails", ["paste", "x"], { ...CLIP, clip: { code: 1, err: "Error: Can't open display\n" } });
add("paste: the clipboard program fails silently", ["paste", "x"], { ...CLIP, clip: { code: 1 } });
add("paste: no clipboard program", ["paste", "x"], { ...CLIP, env: { PARITY_NO_GIT: "1", PARITY_NO_CLIP: "1" }, back: { darwin: "pbcopy that cannot be started: bun's own words" } });
add("paste: Ctrl+V fails", ["paste", "x"], { ...CLIP, pw: { press: { code: 1, err: "v\n" } } });
add("paste: an empty text", ["paste", ""], CLIP);

// ═══════════════════════════ connect ═══════════════════════════

const STORE = (dir = "<HOME>/.config/rustdesk-skill") => ["node <REPO>/rs/parity/files.mjs " + dir];
const C = (name, args, o = {}) => add(`connect: ${name}`, args, { needs: ["ocr"], ...o, after: [...(o.after || []), ...STORE(o.dir)] });
const PASSWORDS = (map) => [`mkdir -p <HOME>/.config/rustdesk-skill && printf '%s' ${sq(map)} > <HOME>/.config/rustdesk-skill/passwords.json`];
const FLOW = { ocr: [{ boxes: HOME_SCREEN }, { boxes: [pbox("接続", 700, 135, 40, 18)] }, { boxes: DIALOG }, { boxes: REMOTE }] };
C("no password, none remembered", ["connect", "123456789"]);
C("an empty password, none remembered", ["c", "123456789", ""]);
C("a password, nothing on the screen", ["c", "123456789", "pw-placeholder"]);
C("the whole way: home, connect, password dialog, remote", ["c", "123456789", "pw-placeholder"], FLOW);
C("the whole way, remembering the password", ["c", "123456789", "Pw@placeholder", "--remember-password"], FLOW);
C("the remembered password", ["c", "123456789"], { ...FLOW, setup: PASSWORDS('{"123456789": "remembered-pw", "b": "x", "5": "y"}') });
C("a remembered password that is empty", ["c", "123456789"], { setup: PASSWORDS('{"123456789": ""}') });
C("a remembered password and another given", ["c", "123456789", "given", "--remember-password"], { setup: PASSWORDS('{"123456789": "old", "0123": "z", "42": "n"}') });
C("a password store that is not JSON", ["c", "123456789"], { setup: PASSWORDS("{oops") });
C("a password store that is a list", ["c", "0", "x"], { setup: PASSWORDS('["a"]'), back: "a password store that is a list: not ported" });
C("a password store with a number in it", ["c", "1", "x"], { setup: PASSWORDS('{"2": 5}'), back: "a password store that holds more than text: not ported" });
C("a password store with __proto__", ["c", "1", "x"], { setup: PASSWORDS('{"__proto__": "x"}'), back: "a password store with __proto__: not ported" });
C("an id named like a member of Object", ["c", "constructor"], { back: "an id Object.prototype has: not ported" });
C("--new opens the client", ["c", "987654321", "pw", "--new"]);
C("not on the client: open", ["c", "987654321", "pw"], { pw: URL('"about:blank"') });
C("open fails: goto", ["c", "987654321", "pw", "--new"], { pw: { open: { code: 1, err: "already open\n" } } });
C("open and goto fail", ["c", "987654321", "pw", "--new"], { pw: { open: { code: 1 }, goto: { code: 2, err: "offline\n" } } });
C("RDS_PW_BROWSER", ["c", "987654321", "pw", "--new"], { env: { RDS_PW_BROWSER: " chrome " } });
C("playwright-cli is missing", ["c", "987654321", "pw"], { env: { RDS_PW_BIN: "<ROOT>/nowhere" } });
C("an id with leading zeros is kept", ["c", "0012345", "pw"]);
C("an id that needs escaping", ["c", 'a"b\\c', "pw"]);
C("recents move the id to the front, twenty at most", ["c", "100000005", "pw"], { setup: RECENTS(JSON.stringify(Array.from({ length: 22 }, (_, i) => String(100000000 + i)))) });
C("recents that are not JSON", ["c", "1", "pw"], { setup: RECENTS("nope") });
C("recents with a number", ["c", "1", "pw"], { setup: RECENTS("[1]"), back: "recents that hold more than text: not ported" });
C("RDS_CONFIG_DIR", ["c", "1", "pw", "--remember-password"], { env: { RDS_CONFIG_DIR: "<ROOT>/conf/deep" }, dir: "<ROOT>/conf/deep" });
C("a config directory that is a file", ["c", "1", "pw"], { setup: ["mkdir -p <HOME>/.config && touch <HOME>/.config/rustdesk-skill"], back: "a config directory that cannot be written: bun's own words" });
C("a dialog without a field: below its title", ["c", "1", "pw"], { ocr: [{}, {}, { boxes: [pbox("Password Required", 500, 250, 200, 30), pbox("OK", 690, 460)] }, {}] });
C("a dialog with neither field nor title", ["c", "1", "pw"], { ocr: [{}, {}, { boxes: [pbox("Please enter it", 500, 250, 200, 30)] }, {}] });
C("a dialog without OK: Enter", ["c", "1", "pw", "--remember-password"], { ocr: [{}, {}, { boxes: [pbox("パスワード", 500, 350), pbox("Remember password", 500, 400)] }, {}] });
C("English labels", ["c", "1", "pw"], { ocr: [{ boxes: [pbox("Control Remote Desktop", 100, 100, 200, 20), pbox("Remote ID", 100, 140, 80, 20)] }, { boxes: [pbox("Connect", 300, 140, 60, 20)] }, {}, { boxes: HOME_SCREEN }] });
C("only an ID label", ["c", "1", "pw"], { ocr: [{ boxes: [pbox("Your ID", 10, 10)] }, {}, {}, {}] });
C("a retina screen", ["c", "1", "pw"], { ...FLOW, pw: { screenshot: { png: [2560, 1440] } } });
C("OCR fails on the way", ["c", "1", "pw"], { ocr: [{}, { code: 1, err: "ocr died\n" }] });
C("a click fails on the way", ["c", "1", "pw"], { ...FLOW, pw: { mousedown: [{}, { code: 1, err: "click\n" }] } });
C("without a session name", ["c", "1", "pw"], { env: { RDS_SESSION_NAME: null } });
C("a password store in RDS_CONFIG_DIR from .env.local", ["c", "1"], { files: { ".env.local": "RDS_CONFIG_DIR=<ROOT>/c\n" }, setup: ["mkdir -p <ROOT>/c && printf '{\"1\": \"from-env-local\"}' > <ROOT>/c/passwords.json"], dir: "<ROOT>/c" });

// ═══════════════════════════ more, aimed at what may have been missed ═══════════════════════════

add("x list: HOME with a trailing slash", ["list"], { env: { HOME: "<HOME>/" }, setup: RECENTS('["1"]') });
add("x list: HOME relative", ["list"], { env: { HOME: "rel" }, setup: ["mkdir -p rel/.config/rustdesk-skill && printf '[\"r\"]' > rel/.config/rustdesk-skill/recents.json"] });
add("x list: an id of 10,000 characters", ["list"], { setup: RECENTS(JSON.stringify(["9".repeat(10000)])) });
add("x type: 10,000 characters", ["type", "a".repeat(10000)]);
add("x click: Cyrillic in another case", ["click", "ПРИВЕТ мир"], { ...OCR, ocr: { boxes: [pbox("привет МИР", 1, 2, 3, 4)] } });
add("x click: blanks only with --contains", ["click", "  ", "--contains"], CLICK);
add("x click: a box text with blanks of other scripts", ["click", "ok"], { ...OCR, ocr: { boxes: [pbox("　OK ", 1, 2, 3, 4)] } });
add("x ocr: coordinates wider than their padding", ["ocr"], { ...OCR, ocr: { boxes: [pbox("wide", 123456, -99999, 99999, -1234)] } });
add("x ocr: a text that is empty", ["ocr", "--json"], { ...OCR, ocr: { boxes: [pbox("", 1, 1)] } });
add("x ocr: a member given twice in a box", ["ocr", "--json"], { ...OCR, ocr: { out: '[{"text": "a", "text": "b", "x1": 1, "x1": 2, "confidence": 0.5, "confidence": "x"}]' } });
add("x ocr: a box with __proto__", ["ocr", "--json"], { ...OCR, ocr: { out: '[{"__proto__": {"text": "p"}, "text": "own", "x1": 3}]' } });
add("x ocr: output with blanks around", ["ocr"], { ...OCR, ocr: { out: ' \r\n [ ] \n' } });
add("x ocr: output with a byte order mark", ["ocr"], { ...OCR, ocr: { out: "\\xef\\xbb\\xbf[]" } });
add("x key: a tab inside the chord", ["key", "ctrl+\tc\n"]);
add("x key: a modifier in capitals and a key in another script", ["key", "SHIFT+ä"]);
add("x session: RDS_SESSION_NAME of Japanese only", ["scroll", "1"], { env: { RDS_SESSION_NAME: "日本" } });
add("x pw: RDS_PW_BIN of JavaScript blanks only", ["scroll", "1"], { env: { RDS_PW_BIN: " 　" }, needs: ["path"] });
add("x pw: RDS_PW_BIN from .env.local", ["scroll", "1"], { env: { RDS_PW_BIN: null }, files: { ".env.local": `RDS_PW_BIN=${PW}\n` } });
add("x status: an address that is an object", ["status"], { pw: URL('{"href": "https://rustdesk.com/web/"}'), ...OCR });
add("x connect: RDS_PW_BROWSER of blanks", ["c", "1", "pw", "--new"], { env: { RDS_PW_BROWSER: "   " }, needs: ["ocr"] });
add("x screenshot: a file named in Japanese", ["ss", "スクショ 1.png"], { after: ["cksum 'スクショ 1.png'"] });
add("x move: far coordinates", ["move", "123456789012345", "-123456789012345"]);
add("x move: a far coordinate on a retina screen", ["move", "123456789012345", "1"], { pw: { screenshot: { png: [2560, 1440] } } });
add("x scroll: a large negative fraction", ["scroll", "-999999999999999.999999"], { back: "it reads as -1e15: a number of that size is written in JavaScript's own way, not ported" });

// ═══════════════════════════ systems ═══════════════════════════
//
// The same cases run on Linux, macOS and Windows; rustify's parity runner
// reads "handback" and "skip" per system, keyed as node's process.platform
// names them.
//
// On Windows the binary hands every invocation back to the TypeScript, before
// it has done anything (rs/src/main.rs), so every case there is meant to
// reach the JavaScript. And there the runtime finds programs by PATHEXT and
// never runs rs/parity/run.sh's stand-ins, which are shell scripts: a case
// that needs one of them on PATH (playwright-cli found by its name, python3,
// xclip) would reach the machine's own program, or none, and is skipped.
const WINDOWS = "every invocation goes back to the TypeScript on Windows (rs/src/main.rs)";
for (const c of cases) if (typeof c.handback !== "string") c.handback = { ...c.handback, win32: WINDOWS };
const STAND_INS = {
  path: "it finds playwright-cli on PATH, and on Windows the runtime never runs rs/parity/run.sh's stand-in there (a shell script)",
  ocr: "it runs OCR, and on Windows the runtime never runs rs/parity/run.sh's python3 stand-in (a shell script): it would find the machine's own python3",
  clip: "it sets the clipboard, and on Windows the runtime never runs rs/parity/run.sh's xclip stand-in (a shell script)",
  tty: "a pseudo-terminal, which the runner has only on Linux and macOS",
};
for (const c of cases) {
  const needs = [...(c.needs || []), ...(c.tty ? ["tty"] : [])];
  if (needs.length) c.skip = { ...c.skip, win32: STAND_INS[needs[0]] };
  delete c.needs;
}

// ═══════════════════════════ write ═══════════════════════════

const out = {
  env: { PARITY_LOG: "<ROOT>/calls.log", GIT_CONFIG_NOSYSTEM: "1" },
  normalize: [["rds-ocr-\\d+\\.png", "rds-ocr-<PID>.png"]],
  cases,
};
const text = JSON.stringify(out, null, 1) + "\n";
const target = path.join(HERE, "parity.json");
if (flags.has("--check")) {
  const current = fs.existsSync(target) ? fs.readFileSync(target, "utf8") : "";
  if (current !== text) {
    const crlf = current.replace(/\r\n/g, "\n") === text;
    console.error(`rs/parity.json is not what rs/parity.gen.mjs writes${crlf ? " (it differs only in CR LF line ends: check out rs/ with LF)" : ""}; run \`node rs/parity.gen.mjs\``);
    process.exit(1);
  }
  console.error(`rs/parity.json is up to date (${cases.length} cases)`);
} else {
  fs.writeFileSync(target, text);
  const on = (system) => cases.filter((c) => !c.skip?.[system] && (typeof c.handback === "string" || c.handback?.[system])).length;
  const skipped = (system) => cases.filter((c) => c.skip?.[system]).length;
  console.error(`wrote rs/parity.json (${cases.length} cases; handed back on linux ${on("linux")}, darwin ${on("darwin")}, win32 ${on("win32")}; skipped on darwin ${skipped("darwin")}, win32 ${skipped("win32")})`);
}
