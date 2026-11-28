// What the TypeScript does with things rs/src/compare.rs asks about, run by
// bun from `cargo test`:
//
//   bun rs/parity/yargs.ts lines <file>   for each command line in <file> (a
//                                         JSON list of lists of words), the
//                                         call bin/rustdesk-skill.ts's yargs
//                                         chain (buildParser) makes for it
//   bun rs/parity/yargs.ts chars          for every code point, what
//                                         toLowerCase and trim make of it
//   bun rs/parity/yargs.ts tmpdir         os.tmpdir() under the environment
//                                         it was started with
//   bun rs/parity/yargs.ts numbers <file> String(x) and x.toFixed(2) for the
//                                         numbers in <file> (JSON, as text)
//   bun rs/parity/yargs.ts strings <file> Number(s) for the strings in <file>
//   bun rs/parity/yargs.ts values <file>  Number(JSON.parse(t)) for the JSON
//                                         texts in <file>, or what it throws
//   bun rs/parity/yargs.ts boxes <file>   for the JSON texts in <file>, what
//                                         src/ocr.ts normalizeBoxes makes of
//                                         each at 1280 by 720 and whether
//                                         matchText then throws (as connect
//                                         calls them)
//   bun rs/parity/yargs.ts sort <file>    for lists of numbers in <file>, the
//                                         order (a, b) => a.y - b.y sorts them in
//   bun rs/parity/yargs.ts includes <file> t.toLowerCase().includes(q) for
//                                         the pairs [t, q] in <file>
//
// For `lines` it prints one JSON value per line: the call, written as
// rs/src/args.rs `describe` writes a command, or null when yargs did
// anything else (printed help or an error, called no command or two, wrote
// more than the handler writes). Nothing is started: the commands' work is
// replaced by functions that only note how they were called (the `deps` of
// buildParser), and yargs does not exit.

import { readFileSync } from "node:fs";
import os from "node:os";
import { buildParser, type CliDeps } from "../../bin/rustdesk-skill.ts";
import { matchText, normalizeBoxes } from "../../src/ocr.ts";

const [what, file] = process.argv.slice(2);
const out: string[] = [];

if (what === "lines") {
  const lines: string[][] = JSON.parse(readFileSync(file, "utf8"));
  const write = process.stdout.write.bind(process.stdout);
  const log = console.log;
  const error = console.error;
  const s = (v: unknown) => JSON.stringify(v) ?? "null";
  for (const line of lines) {
    const calls: string[] = [];
    let printed = "";
    let json = false;
    const deps: CliDeps = {
      connect: (o) => (calls.push(`connect ${s(o)}`), { ok: true, id: "x", connected: false }),
      status: () => (calls.push("status"), {}),
      disconnect: () => void calls.push("disconnect"),
      listRecent: () => (calls.push("listRecent"), []),
      screenshot: (f) => (calls.push(`screenshot ${s(f)}`), "f"),
      ocr: (o) => (calls.push(`ocr ${s({ json: "?", ...o })}`), []),
      clickText: (t, o) => (calls.push(`clickText ${s(t)} ${s(o)}`), { text: "t", conf: 1, x: 0, y: 0, w: 1, h: 1 }),
      clickXY: (x, y) => void calls.push(`clickXY ${String(x)} ${String(y)}`),
      move: (x, y) => void calls.push(`move ${String(x)} ${String(y)}`),
      scroll: (dy) => void calls.push(`scroll ${String(dy)}`),
      type: (t, o) => void calls.push(`type ${s(t)} ${s(o)}`),
      key: (k) => void calls.push(`key ${s(k)}`),
      paste: (t) => void calls.push(`paste ${s(t)}`),
    };
    (process.stdout as { write: unknown }).write = (text: string) => ((printed += `W:${text}`), true);
    console.log = (...a: unknown[]) => void (printed += `L:${a.join(" ")}\n`);
    console.error = (...a: unknown[]) => void (printed += `E:${a.join(" ")}\n`);
    let failed = false;
    try {
      buildParser(line, deps)
        .exitProcess(false)
        .fail(() => {
          failed = true;
        })
        .parse();
    } catch {
      failed = true;
    }
    (process.stdout as { write: unknown }).write = write;
    console.log = log;
    console.error = error;
    // What the handler itself prints for the noted call's answer.
    const expected: Record<string, string> = {
      connect: 'W:{\n  "ok": true,\n  "id": "x",\n  "connected": false\n}\n',
      status: "W:{}\n",
      listRecent: "W:[]\n",
      screenshot: "E:[rustdesk-skill] saved f\n",
      clickText: 'E:[rustdesk-skill] clicked "t" @ 0,0\n',
      disconnect: "E:[rustdesk-skill] disconnect sent\n",
    };
    let call: string | null = calls.length === 1 && !failed ? calls[0] : null;
    if (call !== null) {
      const name = call.split(" ")[0];
      if (name === "ocr") {
        json = printed === "W:[]\n";
        if (!json && printed !== "") call = null;
        else call = call.replace('"json":"?"', `"json":${json}`);
      } else if (printed !== (expected[name] ?? "")) {
        call = null;
      }
    }
    out.push(JSON.stringify(call));
  }
} else if (what === "chars") {
  for (let c = 0; c <= 0x10ffff; c++) {
    if (c >= 0xd800 && c <= 0xdfff) continue;
    const ch = String.fromCodePoint(c);
    const lower = ch.toLowerCase();
    const space = ch.trim() === "";
    if (lower !== ch || space) out.push(`${c.toString(16)} ${JSON.stringify(lower)} ${space}`);
  }
} else if (what === "tmpdir") {
  out.push(JSON.stringify(os.tmpdir()));
} else if (what === "numbers") {
  const numbers: string[] = JSON.parse(readFileSync(file, "utf8"));
  for (const text of numbers) {
    const x = Number(text);
    out.push(JSON.stringify([String(x), x.toFixed(2), String(Math.round(x))]));
  }
} else if (what === "strings" || what === "values") {
  // -0 as such: String(-0) is "0".
  const show = (x: number) => (Object.is(x, -0) ? "-0" : String(x));
  const items: string[] = JSON.parse(readFileSync(file, "utf8"));
  for (const item of items) {
    try {
      out.push(JSON.stringify(show(Number(what === "strings" ? item : JSON.parse(item)))));
    } catch (e) {
      out.push(JSON.stringify(`throws ${(e as Error).message}`));
    }
  }
} else if (what === "boxes") {
  const items: string[] = JSON.parse(readFileSync(file, "utf8"));
  for (const item of items) {
    try {
      const boxes = normalizeBoxes(JSON.parse(item), 1280, 720);
      matchText(boxes, "リモートID", { contains: true });
      out.push(JSON.stringify(boxes.map((b) => [String(b.x), String(b.y), String(b.w), String(b.h), b.text])));
    } catch (e) {
      out.push(JSON.stringify(`throws ${(e as Error).message}`));
    }
  }
} else if (what === "sort") {
  const lists: (number | string)[][] = JSON.parse(readFileSync(file, "utf8"));
  for (const list of lists) {
    const items = list.map((y, i) => ({ i, y: Number(y) }));
    items.sort((a, b) => a.y - b.y);
    out.push(JSON.stringify(items.map((b) => b.i)));
  }
} else if (what === "includes") {
  const pairs: [string, string][] = JSON.parse(readFileSync(file, "utf8"));
  for (const [t, q] of pairs) out.push(JSON.stringify(t.trim().toLowerCase().includes(q.trim().toLowerCase())));
} else {
  console.error("usage: bun rs/parity/yargs.ts lines <file> | chars | tmpdir | numbers <file> | strings <file> | values <file> | boxes <file> | sort <file> | includes <file>");
  process.exit(2);
}
process.stdout.write(out.join("\n") + "\n");
