// Low-level browser backend.
//
// DESIGN CHOICE: we WRAP Microsoft's official `@playwright/cli` (binary
// `playwright-cli`) rather than embedding Playwright directly. It is built for
// coding agents and exposes exactly the primitives we need — a persistent NAMED
// session (`-s=<name>`), coord mouse / type / press / screenshot / eval — over a
// background browser it keeps alive across separate CLI invocations. Shelling out
// to it reuses that live, authenticated RustDesk-web tab with no embedded state of
// our own. The whole backend is isolated here so a future native driver can offer
// the same surface.
//
// SESSION: every command carries `-s=<name>` (see src/session.ts) so repeated `rds`
// invocations share one browser instance instead of spawning a fresh one each time.
//
// The @playwright/cli subcommands we use:
//   eval "<js>"                      -> run JS in the page, prints the result
//   screenshot --hires --filename=<abs> -> save PNG (device px via --hires)
//   mousemove <x> <y> / mousedown / mouseup / mousewheel <dx> <dy>
//   type "<text>" / press <Key> / keydown <Key> / keyup <Key>
//   goto <url> / open <url>          -> NAVIGATES (only connect may call these)
//
// COORDINATE INVARIANT: playwright mouse.* uses CSS px; `screenshot --hires` outputs
// device px (= CSS px * devicePixelRatio). On retina Macs DPR=2, so they differ. We
// make SCREENSHOT PX the one canonical space (what OCR sees and what click-xy
// accepts) and convert to CSS px in exactly ONE place: moveScreenshotPx(). Never mix
// them. (Passing --hires keeps this identical to the previous device-px behavior.)

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { resolveSessionName } from "./session";

// Read at call time (not module load) so tests can stub via RDS_PW_BIN. The value
// is whitespace-split so it may carry leading args, e.g. RDS_PW_BIN="bunx @playwright/cli".
function pwCmd(): { cmd: string; prefix: string[] } {
  const raw = process.env.RDS_PW_BIN?.trim();
  if (raw) {
    const parts = raw.split(/\s+/);
    return { cmd: parts[0], prefix: parts.slice(1) };
  }
  return { cmd: "playwright-cli", prefix: [] };
}

// We run the backend from a stable cwd and hand screenshots an absolute --filename,
// so downloads land in <sessionDir>/rds-shot.png regardless of the caller's cwd.
function sessionDir(): string {
  const d = process.env.RDS_SESSION_DIR || join(tmpdir(), "rustdesk-skill");
  if (!existsSync(d)) mkdirSync(d, { recursive: true });
  return d;
}

export interface PwResult {
  status: number;
  stdout: string;
  stderr: string;
}

/** Thrown when the `playwright-cli` (@playwright/cli) binary is not available. */
export class PwNotFoundError extends Error {}

/**
 * Run a raw @playwright/cli command, scoped to the resolved session. Throws on
 * non-zero unless opts.allowFail.
 */
export function pw(args: string[], opts: { allowFail?: boolean } = {}): PwResult {
  const { cmd, prefix } = pwCmd();
  const session = resolveSessionName();
  const full = [...prefix, `-s=${session}`, ...args];
  const res = spawnSync(cmd, full, {
    cwd: sessionDir(),
    encoding: "utf8",
    env: process.env,
  });
  // ENOENT = the binary itself is missing. Fail with an actionable message
  // (the browser backend is @playwright/cli-only) instead of a raw stack trace.
  if ((res.error as NodeJS.ErrnoException | undefined)?.code === "ENOENT") {
    throw new PwNotFoundError(
      `'${cmd}' not found on PATH — install @playwright/cli (\`bun add -g @playwright/cli\`), ` +
        `run it via \`RDS_PW_BIN="bunx @playwright/cli"\`, or set RDS_PW_BIN to its path. ` +
        `First launch may download a browser; set RDS_PW_BROWSER=chrome to use system Chrome instead.`,
    );
  }
  const out: PwResult = {
    status: res.status ?? (res.error ? 1 : 0),
    stdout: res.stdout ?? "",
    stderr: res.stderr ?? "",
  };
  if (!opts.allowFail && out.status !== 0) {
    throw new Error(
      `playwright-cli ${args[0]} failed (exit ${out.status}): ${out.stderr || out.stdout || res.error?.message || "unknown error"}`,
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
 * plus its pixel dimensions (screenshot-px space). --hires => device px.
 */
export function screenshotPng(): { buf: Buffer; width: number; height: number } {
  const abs = join(sessionDir(), "rds-shot.png");
  pw(["screenshot", "--hires", `--filename=${abs}`]);
  // Be tolerant about where the file landed: honor the absolute path first, then a
  // couple of likely fallbacks in case the CLI writes basename-relative.
  const candidates = [abs, join(process.cwd(), "rds-shot.png"), join(sessionDir(), "rds-shot.png")];
  const p = candidates.find((c) => existsSync(c));
  if (!p) throw new Error(`screenshot not found (looked in: ${candidates.join(", ")})`);
  const buf = readFileSync(p);
  const { width, height } = pngSize(buf);
  return { buf, width, height };
}

/** Evaluate JS in the page and return trimmed stdout. */
export function evalJs(js: string): string {
  return pw(["eval", js]).stdout.trim();
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
  pw(["mousemove", String(x), String(y)]);
}

/** Click at a SCREENSHOT-px coordinate. */
export function clickScreenshotPx(sx: number, sy: number): void {
  moveScreenshotPx(sx, sy);
  pw(["mousedown"]);
  pw(["mouseup"]);
}

/** Scroll by dy CSS px at the current pointer position. */
export function scroll(dy: number): void {
  pw(["mousewheel", "0", String(dy)]);
}

/** Type text into the focused field (no shift-mangling protection here — caller decides). */
export function typeText(text: string): void {
  pw(["type", text]);
}

/** Press a single key chord, e.g. "Enter" or "ctrl+c". */
export function pressKey(chord: string): void {
  const { key, mods } = parseKeyChord(chord);
  for (const m of mods) pw(["keydown", m]);
  pw(["press", key]);
  for (const m of mods.slice().reverse()) pw(["keyup", m]);
}

/** Send Ctrl+V (paste on the Windows remote — NOT Cmd+V). */
export function pasteCtrlV(): void {
  pw(["keydown", "Control"]);
  pw(["press", "v"]);
  pw(["keyup", "Control"]);
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
