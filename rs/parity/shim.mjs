#!/usr/bin/env node
// The stand-in for every program rustdesk-skill starts other than git, for
// the parity cases (rs/parity.gen.mjs). It never opens a browser, never runs
// OCR and never touches a clipboard: it writes down what it was asked and
// answers from the case's script.
//
//   node rs/parity/shim.mjs <role> [args…]
//
// role   stands in for
//   pw     playwright-cli (rs/parity/run.sh puts a `playwright-cli` that runs
//          this first on PATH; a case can also name it in RDS_PW_BIN)
//   ocr    python3, which the TypeScript starts with ocr/ocr.py or
//          ocr/ocr-mac.py, the PNG and the language
//   xclip  xclip, for `paste` on Linux
//   pbcopy pbcopy, for `paste` on macOS
//
// Every call appends to the file PARITY_LOG names: the role and its
// arguments, its working directory, what it read on stdin, and a digest of
// its environment (every variable but the package root that only the
// binary's side is given), followed by the values of the RDS_* variables and
// of those PARITY_SHOW names. A case prints that log in an `after` line, so
// both implementations must have asked the same things, in the same order,
// from the same place, with the same environment.
//
// The answers (all optional) come from variables of the case, as JSON:
//
//   PARITY_PW    { "<key>": answer | [answer, …] }   for playwright-cli
//   PARITY_OCR   answer | [answer, …]                for python3
//   PARITY_CLIP  answer                              for xclip and pbcopy
//
// The key of a playwright-cli call is its subcommand (`screenshot`,
// `mousemove`, `goto`, …), and for `eval` `eval:url` when the script asks for
// location.href, `eval:viewport` when it asks for innerWidth, else `eval`. A
// list is answered by turn, its last answer for every call after it (the
// turns are counted in files beside the log). An answer:
//
//   out, err   what to print on stdout and stderr (text; `\xNN` is the byte)
//   code       the exit code (0)
//   kill       end by this signal instead (SIGKILL, SIGTERM)
//   big        print this many bytes of `a` on stdout first
//   png        for `screenshot`: [width, height] of the PNG written to the
//              --filename path ([1280, 720] when not said), "short" for a
//              file of 10 bytes, "none" for no file, "dir" for a directory
//              there, or { "at": "<path>", "size": [w, h] } to write it
//              somewhere else
//   boxes      for python3: the boxes to print as JSON (`out` wins)
//
// Without an answer, a call succeeds and prints nothing, except `eval:url`
// (the RustDesk web client's address, in playwright-cli's report form),
// `eval:viewport` (1280 by 720) and `screenshot` (a PNG of 1280 by 720),
// and python3 prints `[]`.

import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const [role, ...args] = process.argv.slice(2);
const logFile = process.env.PARITY_LOG;
if (!logFile) {
  process.stderr.write("shim.mjs: PARITY_LOG is not set\n");
  process.exit(90);
}

const quote = (text) => JSON.stringify(text);

/** The value a variable of the case gives, parsed. */
function script(name) {
  const text = process.env[name];
  if (!text) return undefined;
  try {
    return JSON.parse(text);
  } catch {
    process.stderr.write(`shim.mjs: ${name} is not JSON\n`);
    process.exit(91);
  }
}

/** The answer for `key` on this turn. */
function answerFor(key, answers) {
  if (answers === undefined) return undefined;
  if (!Array.isArray(answers)) return answers;
  const counter = `${logFile}.turn-${key.replace(/[^a-z0-9]/gi, "_")}`;
  const turn = fs.existsSync(counter) ? Number(fs.readFileSync(counter, "utf8")) : 0;
  fs.writeFileSync(counter, String(turn + 1));
  return answers[Math.min(turn, answers.length - 1)];
}

/** Text with `\xNN` turned into that byte. */
function bytes(text) {
  const parts = [];
  let rest = String(text);
  for (let at = rest.search(/\\x[0-9a-f]{2}/i); at >= 0; at = rest.search(/\\x[0-9a-f]{2}/i)) {
    parts.push(Buffer.from(rest.slice(0, at)), Buffer.from([parseInt(rest.slice(at + 2, at + 4), 16)]));
    rest = rest.slice(at + 4);
  }
  parts.push(Buffer.from(rest));
  return Buffer.concat(parts);
}

/** A PNG header of this size, and a few bytes after it: all the program reads. */
function png(width, height) {
  const head = Buffer.alloc(33);
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]).copy(head, 0);
  head.writeUInt32BE(13, 8);
  head.write("IHDR", 12, "latin1");
  head.writeUInt32BE(width, 16);
  head.writeUInt32BE(height, 20);
  head.writeUInt8(8, 24);
  head.writeUInt8(6, 25);
  return Buffer.concat([head, Buffer.from(`parity ${width}x${height}\n`)]);
}

function readStdin() {
  try {
    return fs.readFileSync(0);
  } catch {
    return Buffer.alloc(0);
  }
}

function log(lines) {
  fs.appendFileSync(logFile, lines.map((line) => `${line}\n`).join(""));
}

/** The run's own directory (the log's) stands for itself in every spelling, so that both runs of a case have the same digest. */
function placeless(value) {
  const root = path.dirname(path.resolve(logFile));
  let text = value;
  for (const spelling of new Set([root, root.replace(/\\/g, "/"), root.replace(/\//g, "\\")])) {
    text = text.split(spelling).join("<ROOT>");
    if (process.platform === "win32") text = text.replace(new RegExp(spelling.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "gi"), "<ROOT>");
  }
  return text;
}

function environment() {
  const names = Object.keys(process.env)
    .filter((name) => name !== "RUSTDESK_SKILL_PACKAGE_ROOT")
    .sort();
  const digest = createHash("sha256")
    .update(names.map((name) => `${name}=${placeless(process.env[name])}`).join("\0"))
    .digest("hex")
    .slice(0, 16);
  const shown = (process.env.PARITY_SHOW || "").split(/\s+/).filter(Boolean);
  const values = names.filter((name) => name.startsWith("RDS_") || shown.includes(name)).map((name) => `${name}=${quote(process.env[name])}`);
  return `  env ${digest}${values.length ? " " + values.join(" ") : ""}`;
}

/** Carries out an answer: output, then the exit code or the signal. */
function finish(answer, defaultOut = "") {
  const a = answer || {};
  if (a.big) process.stdout.write("a".repeat(a.big));
  const out = a.out !== undefined ? a.out : defaultOut;
  if (out) process.stdout.write(bytes(out));
  if (a.err) process.stderr.write(bytes(a.err));
  if (a.kill) process.kill(process.pid, a.kill);
  process.exitCode = a.code || 0;
}

const stdin = readStdin();
log([`${role} ${args.map(quote).join(" ")}`, `  cwd ${process.cwd()}`, ...(stdin.length ? [`  stdin ${quote(stdin.toString("utf8"))}`] : []), environment()]);

if (role === "pw") {
  // [-s=<session>, subcommand, …]: the session flag comes first.
  const rest = args.filter((arg) => !arg.startsWith("-s="));
  const sub = rest[0] || "";
  let key = sub;
  if (sub === "eval") key = /location\.href/.test(rest[1] || "") ? "eval:url" : /innerWidth/.test(rest[1] || "") ? "eval:viewport" : "eval";
  const answer = answerFor(key, script("PARITY_PW")?.[key]);
  if (sub === "screenshot") {
    const target = (rest.find((arg) => arg.startsWith("--filename=")) || "").slice("--filename=".length);
    const shot = answer?.png ?? [1280, 720];
    if (shot === "short") fs.writeFileSync(target, "\x89PNG\r\n\x1a\n00");
    else if (shot === "dir") fs.mkdirSync(target, { recursive: true });
    else if (Array.isArray(shot)) fs.writeFileSync(target, png(shot[0], shot[1]));
    else if (shot && typeof shot === "object") fs.writeFileSync(shot.at, png(shot.size[0], shot.size[1]));
    finish(answer, `### Result\n- [Screenshot of viewport](${target})\n`);
  } else if (key === "eval:url") {
    finish(answer, '### Result\n"https://rustdesk.com/web/"\n### Ran Playwright code\n```js\nawait page.evaluate(\'() => location.href\');\n```\n');
  } else if (key === "eval:viewport") {
    finish(answer, '### Result\n{\n  "w": 1280,\n  "h": 720\n}\n### Ran Playwright code\n```js\nawait page.evaluate(\'() => ({ w: window.innerWidth, h: window.innerHeight })\');\n```\n');
  } else {
    finish(answer);
  }
} else if (role === "ocr") {
  // [script, png, lang]: say what the PNG the program wrote holds.
  const file = args[1];
  let what = "no file";
  try {
    const content = fs.readFileSync(file);
    what = `${content.length} bytes, sha256 ${createHash("sha256").update(content).digest("hex").slice(0, 16)}`;
  } catch (error) {
    what = error.code;
  }
  log([`  png ${what}`]);
  const answer = answerFor("ocr", script("PARITY_OCR"));
  finish(answer, answer?.boxes !== undefined ? JSON.stringify(answer.boxes) : "[]");
} else if (role === "xclip" || role === "pbcopy") {
  finish(script("PARITY_CLIP"));
} else {
  process.stderr.write(`shim.mjs: unknown role ${quote(role)}\n`);
  process.exit(92);
}
