// High-level screen actions: screenshot, ocr, click-by-text, click-xy, move,
// scroll, type (with shift-mangling guard), key, paste.

import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import {
  clickScreenshotPx,
  moveScreenshotPx,
  pasteCtrlV,
  pressKey,
  scroll as pwScroll,
  screenshotPng,
  sleepMs,
  typeText,
} from "./pw";
import {
  boxCenter,
  matchText,
  ocrBuffer,
  type Box,
  type MatchOptions,
  type OcrBackend,
} from "./ocr";

// Shift-requiring characters per SKILL.md / IMPL_SPEC.md. On a JP-layout remote
// RustDesk web drops Shift, so these get mangled (@→2, +→=, A→a, etc.).
const SHIFT_CHARS = /[A-Z~!@#$%^&*()_+{}|:"<>?]/;

/** Which shift-requiring characters appear in `text` (unique, in order). Pure. */
export function shiftChars(text: string): string[] {
  const seen = new Set<string>();
  for (const ch of text) {
    if (SHIFT_CHARS.test(ch)) seen.add(ch);
  }
  return [...seen];
}

/** True if `text` contains any shift-requiring char. Pure. */
export function needsShiftWarning(text: string): boolean {
  return SHIFT_CHARS.test(text);
}

/** Take a screenshot and write it to `file` (default ./rds-screenshot.png). */
export function screenshot(file?: string): string {
  const { buf } = screenshotPng();
  const dest = file || "rds-screenshot.png";
  writeFileSync(dest, buf);
  return dest;
}

/** OCR the current canvas into screenshot-px boxes. */
export function ocr(opts: { backend?: OcrBackend; lang?: string } = {}): Box[] {
  const { buf, width, height } = screenshotPng();
  return ocrBuffer(buf, width, height, opts);
}

/** Click on-screen text via OCR. Returns the matched box. Throws if not found. */
export function clickText(
  query: string,
  opts: MatchOptions & { backend?: OcrBackend; lang?: string } = {},
): Box {
  const boxes = ocr({ backend: opts.backend, lang: opts.lang });
  const { box, matches, ambiguous } = matchText(boxes, query, opts);
  if (!box) {
    throw new Error(`text not found on screen: "${query}" (${boxes.length} boxes scanned)`);
  }
  if (ambiguous) {
    console.error(
      `[rustdesk-skill] warning: "${query}" matched ${matches.length} boxes; clicking the first. Use --nth to disambiguate.`,
    );
  }
  const c = boxCenter(box);
  clickScreenshotPx(c.x, c.y);
  return box;
}

/** Click an absolute SCREENSHOT-px coordinate. */
export function clickXY(x: number, y: number): void {
  clickScreenshotPx(x, y);
}

/** Move the pointer to a SCREENSHOT-px coordinate. */
export function move(x: number, y: number): void {
  moveScreenshotPx(x, y);
}

/** Scroll by dy CSS px. */
export function scroll(dy: number): void {
  pwScroll(dy);
}

export class ShiftMangleError extends Error {}

/**
 * Type into the focused REMOTE field. If the text contains shift-requiring chars
 * and `force` is not set, we WARN on stderr and refuse (throw), because the
 * JP-layout remote will mangle them. With `force`, type anyway.
 */
export function type(text: string, opts: { force?: boolean; verify?: boolean } = {}): void {
  const bad = shiftChars(text);
  if (bad.length > 0 && !opts.force) {
    console.error(
      `[rustdesk-skill] warning: text contains shift-requiring chars [${bad.join("")}] which the JP-layout remote mangles (@→2, +→=, A→a). ` +
        `Refusing. Use 'rds paste' for a clipboard round-trip, a clean SSH channel, or --force to type anyway.`,
    );
    throw new ShiftMangleError(`shift-requiring chars in type input: ${bad.join("")}`);
  }
  if (bad.length > 0) {
    console.error(`[rustdesk-skill] --force: typing shift chars [${bad.join("")}] anyway (may mangle).`);
  }
  withVerify("type", text, opts.verify !== false, () => typeText(text));
}

/** Press a key chord, e.g. "Enter", "Escape", "ctrl+c". */
export function key(chord: string): void {
  pressKey(chord);
}

/** Set the local clipboard via pbcopy (macOS) and send Ctrl+V to the remote. */
export function paste(text: string, opts: { verify?: boolean } = {}): void {
  withVerify("paste", text, opts.verify !== false, () => setClipboardAndPaste(text));
}

function setClipboardAndPaste(text: string): void {
  if (process.platform === "darwin") {
    const res = spawnSync("pbcopy", { input: text, encoding: "utf8" });
    if (res.status !== 0) {
      throw new Error(`pbcopy failed: ${res.stderr || res.error?.message}`);
    }
  } else {
    // xclip/xsel on Linux — best effort.
    const res = spawnSync("xclip", ["-selection", "clipboard"], { input: text, encoding: "utf8" });
    if (res.status !== 0) {
      throw new Error("clipboard set failed (need xclip on Linux)");
    }
  }
  // NOTE: RustDesk Mac→remote clipboard sync is unreliable; Ctrl+V may paste the
  // remote's old clipboard. Documented in README. Ctrl+V (not Cmd+V) for Windows remote.
  pasteCtrlV();
}

// --- input verification (issue #4) ---
//
// Key events reach the remote but text input sometimes silently does not, and the
// backend still exits 0. So type/paste OCR the screen before and after and only
// succeed when the text visibly landed. Masked fields (passwords) can't be seen:
// callers pass --no-verify there.

/** Thrown when typed/pasted text could not be seen on screen afterwards. */
export class UnverifiedInputError extends Error {}

const VERIFY_SETTLE_MS = 800;

/** Lowercase and drop whitespace so OCR box splits/spacing don't matter. Pure. */
function squash(s: string): string {
  return s.toLowerCase().replace(/\s+/g, "");
}

function countOccurrences(hay: string, needle: string): number {
  let n = 0;
  for (let i = hay.indexOf(needle); i >= 0; i = hay.indexOf(needle, i + 1)) n++;
  return n;
}

/** Screen text in reading order (top→bottom, left→right), squashed. Pure. */
function screenText(boxes: Box[]): string {
  return squash(
    boxes
      .slice()
      .sort((a, b) => a.y - b.y || a.x - b.x)
      .map((b) => b.text)
      .join(""),
  );
}

/**
 * Did `text` land? True when it appears on screen MORE times after the input than
 * before (so text that was already visible doesn't count). Pure & unit-tested.
 */
export function textLanded(before: Box[], after: Box[], text: string): boolean {
  const needle = squash(text);
  if (!needle) return true; // nothing visible to check
  return countOccurrences(screenText(after), needle) > countOccurrences(screenText(before), needle);
}

function withVerify(what: string, text: string, verify: boolean, send: () => void): void {
  if (!verify) {
    send();
    console.error(`[rustdesk-skill] ${what}: sent, NOT verified (--no-verify).`);
    return;
  }
  const before = ocr();
  send();
  sleepMs(VERIFY_SETTLE_MS);
  if (!textLanded(before, ocr(), text)) {
    console.error(
      `[rustdesk-skill] ${what}: sent, but the text did NOT show up on screen (it may not have reached the remote, ` +
        `or OCR misread it). Check with 'rds ocr' / 'rds ss'. Use --no-verify for masked fields.`,
    );
    throw new UnverifiedInputError(`${what} input not verified on screen`);
  }
}
