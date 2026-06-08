#!/usr/bin/env bun
// rustdesk-skill / rds — RustDesk web connect CLI for agents.
//
// Bins: `rustdesk-skill` (full) and `rds` (alias) both point here.
// Subcommands (see SKILL.md): connect|c, screenshot|ss, ocr, click, click-xy,
// move, scroll, type, key, paste, status, disconnect, list.
//
// Browser backend = wrap `rech` (see src/rech.ts for the design rationale).
// Arg parsing is yargs (kept from the scaffold). Command actions are injected via
// `deps` so dispatch/parsing can be unit-tested without touching the backend.

import yargs from "yargs";
import { hideBin } from "yargs/helpers";

import * as screen from "../src/screen";
import * as conn from "../src/connect";
import { NoPasswordError } from "../src/passwords";
import { ShiftMangleError } from "../src/screen";
import type { OcrBackend } from "../src/ocr";
import type { Box } from "../src/ocr";

/** Injectable command actions (real impls by default; spied in tests). */
export interface CliDeps {
  connect: (o: conn.ConnectOptions) => conn.ConnectResult;
  status: () => unknown;
  disconnect: () => void;
  listRecent: () => string[];
  screenshot: (file?: string) => string;
  ocr: (o: { backend?: OcrBackend; lang?: string }) => Box[];
  clickText: (
    text: string,
    o: { nth?: number; contains?: boolean; backend?: OcrBackend; lang?: string },
  ) => Box;
  clickXY: (x: number, y: number) => void;
  move: (x: number, y: number) => void;
  scroll: (dy: number) => void;
  type: (text: string, o: { force?: boolean }) => void;
  key: (chord: string) => void;
  paste: (text: string) => void;
}

const realDeps: CliDeps = {
  connect: conn.connect,
  status: conn.status,
  disconnect: conn.disconnect,
  listRecent: conn.listRecent,
  screenshot: screen.screenshot,
  ocr: screen.ocr,
  clickText: screen.clickText,
  clickXY: screen.clickXY,
  move: screen.move,
  scroll: screen.scroll,
  type: screen.type,
  key: screen.key,
  paste: screen.paste,
};

/** Print a value as JSON (machine) — used for --json and structured results. */
function printJson(v: unknown): void {
  process.stdout.write(JSON.stringify(v, null, 2) + "\n");
}

/** Run a handler, mapping known errors to clean exit codes/messages. */
function guard(fn: () => void): void {
  try {
    fn();
  } catch (e) {
    if (e instanceof NoPasswordError) {
      console.error(`[rustdesk-skill] ${e.message}`);
      process.exit(3);
    }
    if (e instanceof ShiftMangleError) {
      // The warning was already printed to stderr by screen.type().
      process.exit(4);
    }
    console.error(`[rustdesk-skill] error: ${e instanceof Error ? e.message : String(e)}`);
    process.exit(1);
  }
}

export function buildParser(argv: string[], deps: CliDeps = realDeps) {
  return yargs(argv)
    .scriptName("rustdesk-skill")
    .usage("$0 <command> [options]   (alias: rds)")
    .command(
      ["connect <id> [pw]", "c <id> [pw]"],
      "Connect to a RustDesk remote by ID. Omit pw to use the remembered one.",
      (y) =>
        y
          .positional("id", { type: "string", describe: "RustDesk remote ID" })
          .positional("pw", { type: "string", describe: "Password (omit to use remembered)" })
          .option("remember-password", { type: "boolean", default: false, describe: "Save pw for this id" })
          .option("new", { type: "boolean", default: false, describe: "Force a fresh connection" }),
      (a) =>
        guard(() => {
          const res = deps.connect({
            id: String(a.id),
            pw: a.pw as string | undefined,
            rememberPassword: a["remember-password"] as boolean,
            fresh: a["new"] as boolean,
          });
          printJson(res);
        }),
    )
    .command(
      ["screenshot [file]", "ss [file]"],
      "Screenshot the remote canvas",
      (y) => y.positional("file", { type: "string", describe: "Output PNG path" }),
      (a) =>
        guard(() => {
          const dest = deps.screenshot(a.file as string | undefined);
          console.error(`[rustdesk-skill] saved ${dest}`);
        }),
    )
    .command(
      "ocr",
      "OCR the canvas -> positioned text",
      (y) =>
        y
          .option("json", { type: "boolean", default: false })
          .option("lang", { type: "string", default: "ja,en" })
          .option("ocr", { choices: ["paddle", "vision"] as const, describe: "OCR backend" }),
      (a) =>
        guard(() => {
          const boxes = deps.ocr({
            backend: a.ocr as OcrBackend | undefined,
            lang: a.lang as string,
          });
          if (a.json) {
            printJson(boxes);
          } else {
            for (const b of boxes) {
              console.log(
                `${String(b.x).padStart(5)},${String(b.y).padStart(5)}  ${String(b.w).padStart(4)}x${String(b.h).padStart(4)}  ${b.conf.toFixed(2)}  ${b.text}`,
              );
            }
          }
        }),
    )
    .command(
      "click <text>",
      "Click on-screen text (via OCR)",
      (y) =>
        y
          .positional("text", { type: "string", describe: "Visible text to click" })
          .option("nth", { type: "number", describe: "1-based index when ambiguous" })
          .option("contains", { type: "boolean", default: false, describe: "Substring match" })
          .option("ocr", { choices: ["paddle", "vision"] as const })
          .option("lang", { type: "string", default: "ja,en" }),
      (a) =>
        guard(() => {
          const box = deps.clickText(String(a.text), {
            nth: a.nth as number | undefined,
            contains: a.contains as boolean,
            backend: a.ocr as OcrBackend | undefined,
            lang: a.lang as string,
          });
          console.error(`[rustdesk-skill] clicked "${box.text}" @ ${box.x},${box.y}`);
        }),
    )
    .command(
      "click-xy <x> <y>",
      "Click absolute canvas coords (screenshot px)",
      (y) =>
        y
          .positional("x", { type: "number", describe: "X (screenshot px)" })
          .positional("y", { type: "number", describe: "Y (screenshot px)" }),
      (a) => guard(() => deps.clickXY(Number(a.x), Number(a.y))),
    )
    .command(
      "move <x> <y>",
      "Move pointer (screenshot px)",
      (y) => y.positional("x", { type: "number" }).positional("y", { type: "number" }),
      (a) => guard(() => deps.move(Number(a.x), Number(a.y))),
    )
    .command(
      "scroll <dy>",
      "Scroll by dy (CSS px; negative = up)",
      (y) => y.positional("dy", { type: "number" }),
      (a) => guard(() => deps.scroll(Number(a.dy))),
    )
    .command(
      "type <text>",
      "Type into focused field (⚠ JP-layout mangling — see SKILL.md)",
      (y) =>
        y
          .positional("text", { type: "string" })
          .option("force", { type: "boolean", default: false, describe: "Type shift-chars anyway" }),
      (a) => guard(() => deps.type(String(a.text), { force: a.force as boolean })),
    )
    .command(
      "key <key>",
      "Press a key (Enter, Escape, ctrl+c, ...)",
      (y) => y.positional("key", { type: "string" }),
      (a) => guard(() => deps.key(String(a.key))),
    )
    .command(
      "paste <text>",
      "Set clipboard + Ctrl+V (mangling workaround)",
      (y) => y.positional("text", { type: "string" }),
      (a) => guard(() => deps.paste(String(a.text))),
    )
    .command("status", "Connection status", () => {}, () => guard(() => printJson(deps.status())))
    .command("disconnect", "Disconnect", () => {}, () =>
      guard(() => {
        deps.disconnect();
        console.error("[rustdesk-skill] disconnect sent");
      }),
    )
    .command("list", "Recent remote IDs", () => {}, () => guard(() => printJson(deps.listRecent())))
    .demandCommand(1)
    .strict()
    .help();
}

// Only auto-run when invoked as a binary (not when imported by tests).
if (import.meta.main) {
  buildParser(hideBin(process.argv)).parse();
}
