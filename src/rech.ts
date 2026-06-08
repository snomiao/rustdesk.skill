// Low-level browser backend.
//
// DESIGN CHOICE: we WRAP the existing `rech` CLI (snomiao/rechrome,
// playwright-cli-multi-tab over an HTTP daemon) rather than embedding Playwright
// directly. Rationale: rech already owns a persistent, authenticated Chrome
// session pointed at https://rustdesk.com/web/, with coord mouse / type / press /
// screenshot / eval — exactly the primitives we need. Shelling out to it is the
// fastest correct first impl and reuses the working RustDesk-web session. A
// future native driver can implement the same `Backend` surface.
//
// rech forwards its args to playwright-cli-multi-tab. The subcommands we use:
//   eval "<js>"                      -> run JS in the page, prints JSON/string
//   screenshot --filename=<name>     -> downloads PNG to ./.playwright-cli-multi-tab/<name>
//   mousemove <x> <y> / mousedown / mouseup / mousewheel <dx> <dy>
//   type "<text>" / press <Key> / keydown <Key> / keyup <Key>
//   goto <url> / open <url>          -> NAVIGATES (only connect may call these)
//
// COORDINATE INVARIANT: playwright mouse.* uses CSS px; screenshot outputs device
// px (= CSS px * devicePixelRatio). On retina Macs DPR=2, so they differ. We make
// SCREENSHOT PX the one canonical space (what OCR sees and what click-xy accepts)
// and convert to CSS px in exactly ONE place: moveScreenshotPx(). Never mix them.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

// Read at call time (not module load) so tests can stub via RDS_RECH_BIN.
function rechBin(): string {
  return process.env.RDS_RECH_BIN || "rech";
}

// We run rech from a stable cwd so screenshot downloads land predictably in
// <sessionDir>/.playwright-cli-multi-tab/.
function sessionDir(): string {
  const d = process.env.RDS_SESSION_DIR || join(tmpdir(), "rustdesk-skill");
  if (!existsSync(d)) mkdirSync(d, { recursive: true });
  return d;
}

export interface RechResult {
  status: number;
  stdout: string;
  stderr: string;
}

/** Thrown when the `rech` CLI is not installed/available on PATH. */
export class RechNotFoundError extends Error {}

/** Run a raw rech command. Throws on non-zero unless opts.allowFail. */
export function rech(args: string[], opts: { allowFail?: boolean } = {}): RechResult {
  const bin = rechBin();
  const res = spawnSync(bin, args, {
    cwd: sessionDir(),
    encoding: "utf8",
    env: process.env,
  });
  // ENOENT = the rech binary itself is missing. Fail with an actionable message
  // (the browser backend is currently rech-only) instead of a raw stack trace.
  if ((res.error as NodeJS.ErrnoException | undefined)?.code === "ENOENT") {
    throw new RechNotFoundError(
      `'${bin}' CLI not found on PATH — install it (snomiao/rechrome: \`bun add -g rechrome\` then \`rech setup\`), ` +
        `or set RDS_RECH_BIN to its path. rech is currently the only supported browser backend.`,
    );
  }
  const out: RechResult = {
    status: res.status ?? (res.error ? 1 : 0),
    stdout: res.stdout ?? "",
    stderr: res.stderr ?? "",
  };
  if (!opts.allowFail && out.status !== 0) {
    throw new Error(
      `rech ${args[0]} failed (exit ${out.status}): ${out.stderr || out.stdout || res.error?.message || "unknown error"}`,
    );
  }
  return out;
}

/** Read big-endian uint32 PNG width/height from the IHDR chunk (dependency-free). */
export function pngSize(buf: Buffer): { width: number; height: number } {
  // PNG signature (8) + length (4) + "IHDR" (4) → width at offset 16, height at 20.
  if (buf.length < 24) throw new Error("not a valid PNG (too short)");
  return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) };
}

/**
 * Take a screenshot of the current tab (NO navigation) and return the PNG bytes
 * plus its pixel dimensions (screenshot-px space).
 */
export function screenshotPng(): { buf: Buffer; width: number; height: number } {
  const name = "rds-shot.png";
  rech(["screenshot", `--filename=${name}`]);
  const p = join(sessionDir(), ".playwright-cli-multi-tab", name);
  if (!existsSync(p)) throw new Error(`screenshot not found at ${p}`);
  const buf = readFileSync(p);
  const { width, height } = pngSize(buf);
  return { buf, width, height };
}

/** Evaluate JS in the page and return trimmed stdout. */
export function evalJs(js: string): string {
  return rech(["eval", js]).stdout.trim();
}

/** Current CSS viewport width/height (cached per process). */
let _viewport: { w: number; h: number } | undefined;
export function viewport(): { w: number; h: number } {
  if (_viewport) return _viewport;
  const raw = evalJs("() => ({ w: window.innerWidth, h: window.innerHeight })");
  try {
    const o = JSON.parse(raw);
    if (o && o.w && o.h) return (_viewport = { w: o.w, h: o.h });
  } catch {
    /* fall through */
  }
  // Fallback: assume DPR=1 by deferring scale to screenshot px (handled by caller).
  return (_viewport = { w: 0, h: 0 });
}

/**
 * Convert a point in SCREENSHOT px to CSS px. Pure & unit-tested.
 *   cssX = sx * viewportW / pngW   (scale = viewportW / pngW = 1/DPR)
 * If viewport is unknown (0) we assume px==css (DPR=1).
 */
export function screenshotToCss(
  sx: number,
  sy: number,
  pngW: number,
  pngH: number,
  vpW: number,
  vpH: number,
): { x: number; y: number } {
  const scaleX = vpW > 0 && pngW > 0 ? vpW / pngW : 1;
  const scaleY = vpH > 0 && pngH > 0 ? vpH / pngH : 1;
  return { x: Math.round(sx * scaleX), y: Math.round(sy * scaleY) };
}

/** Move the pointer to a SCREENSHOT-px coordinate (converts to CSS px internally). */
export function moveScreenshotPx(sx: number, sy: number): void {
  const { width, height } = screenshotPng();
  const vp = viewport();
  const { x, y } = screenshotToCss(sx, sy, width, height, vp.w || width, vp.h || height);
  rech(["mousemove", String(x), String(y)]);
}

/** Click at a SCREENSHOT-px coordinate. */
export function clickScreenshotPx(sx: number, sy: number): void {
  moveScreenshotPx(sx, sy);
  rech(["mousedown"]);
  rech(["mouseup"]);
}

/** Scroll by dy CSS px at the current pointer position. */
export function scroll(dy: number): void {
  rech(["mousewheel", "0", String(dy)]);
}

/** Type text into the focused field (no shift-mangling protection here — caller decides). */
export function typeText(text: string): void {
  rech(["type", text]);
}

/** Press a single key chord, e.g. "Enter" or "ctrl+c". */
export function pressKey(chord: string): void {
  const { key, mods } = parseKeyChord(chord);
  for (const m of mods) rech(["keydown", m]);
  rech(["press", key]);
  for (const m of mods.slice().reverse()) rech(["keyup", m]);
}

/** Send Ctrl+V (paste on the Windows remote — NOT Cmd+V). */
export function pasteCtrlV(): void {
  rech(["keydown", "Control"]);
  rech(["press", "v"]);
  rech(["keyup", "Control"]);
}

const MOD_MAP: Record<string, string> = {
  ctrl: "Control",
  control: "Control",
  alt: "Alt",
  option: "Alt",
  shift: "Shift",
  meta: "Meta",
  cmd: "Meta",
  win: "Meta",
};

/**
 * Parse "ctrl+c" / "alt+Tab" / "Enter" into {key, mods}. Modifiers are the
 * leading tokens; the final token is the key. Pure & unit-tested.
 */
export function parseKeyChord(chord: string): { key: string; mods: string[] } {
  const parts = chord.split("+").map((s) => s.trim()).filter(Boolean);
  if (parts.length === 0) throw new Error(`empty key chord: ${chord}`);
  const key = parts[parts.length - 1];
  const mods = parts.slice(0, -1).map((m) => {
    const mapped = MOD_MAP[m.toLowerCase()];
    if (!mapped) throw new Error(`unknown modifier: ${m}`);
    return mapped;
  });
  return { key: normalizeKeyName(key), mods };
}

/** Map a few friendly key names to playwright key names. */
export function normalizeKeyName(key: string): string {
  const map: Record<string, string> = {
    enter: "Enter",
    esc: "Escape",
    escape: "Escape",
    tab: "Tab",
    backspace: "Backspace",
    delete: "Delete",
    space: "Space",
    up: "ArrowUp",
    down: "ArrowDown",
    left: "ArrowLeft",
    right: "ArrowRight",
    backquote: "Backquote",
  };
  return map[key.toLowerCase()] || key;
}

export { sessionDir };
