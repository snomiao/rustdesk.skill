// Connect flow + status/disconnect/list.
//
// The RustDesk web client V2 is a Flutter canvas (no DOM), so we locate the
// リモートID field, the connect trigger, and the 「パスワードが必要です」 dialog by
// OCR + coordinate clicking — the same way a human reads the screen.
//
// HEURISTIC / TUNING NOTE: the exact click offsets below are best-effort against
// the live RustDesk-web layout and MAY need tuning when the UI shifts. The flow is
// structured as small reusable steps (ensureSession → enterId → triggerConnect →
// handlePasswordDialog) so each can be verified/adjusted independently against a
// live remote. Pure logic (password resolution, remember) is unit-tested; the
// pixel-level interaction is intentionally left thin per IMPL_SPEC acceptance.
//
// Local-UI vs remote-canvas: the password dialog is LOCAL RustDesk-web UI, so
// typing into it does NOT suffer the JP-layout shift mangling that the REMOTE
// canvas does. We therefore type the password directly (no shift guard here).

import { ensureSession, onRustdesk } from "./browser";
import { ocr } from "./screen";
import { boxCenter, matchText, type Box } from "./ocr";
import { clickScreenshotPx, pressKey, typeText } from "./pw";
import {
  getPassword,
  loadPasswords,
  resolvePassword,
  setPassword,
} from "./passwords";

const RECENT_LIMIT = 20;

export interface ConnectOptions {
  id: string;
  pw?: string;
  rememberPassword?: boolean;
  fresh?: boolean; // --new
}

export interface ConnectResult {
  ok: boolean;
  id: string;
  connected: boolean;
  note?: string;
}

/** Find a box whose text matches (exact, ci) any of the candidates. */
function findAny(boxes: Box[], candidates: string[]): Box | undefined {
  for (const c of candidates) {
    const { box } = matchText(boxes, c, { contains: true });
    if (box) return box;
  }
  return undefined;
}

/**
 * Enter the remote id into the リモートID field. Heuristic: find the field
 * label/placeholder by OCR, click it, type the id, press Enter.
 */
export function enterId(id: string): void {
  const boxes = ocr();
  // The control field is labeled "リモートID" / "Remote ID" / placeholder text.
  const label = findAny(boxes, ["リモートID", "Remote ID", "Control Remote", "ID"]);
  if (label) {
    const c = boxCenter(label);
    // Click slightly below the label where the input usually sits; fall back to
    // the label itself if layouts collapse them.
    clickScreenshotPx(c.x, c.y);
  }
  // Type the numeric id (digits never need Shift → safe) and trigger connect.
  typeText(id);
  pressKey("Enter");
}

/** Trigger connect via the 接続/Connect button if present (Enter already sent). */
export function triggerConnect(): void {
  const boxes = ocr();
  const btn = findAny(boxes, ["接続", "Connect", "Connect to"]);
  if (btn) {
    const c = boxCenter(btn);
    clickScreenshotPx(c.x, c.y);
  }
}

/** Is the 「パスワードが必要です」 password dialog on screen? */
export function passwordDialogPresent(boxes: Box[]): boolean {
  return !!findAny(boxes, ["パスワードが必要です", "Password Required", "Please enter", "パスワード"]);
}

/**
 * Handle the password dialog: click the field, type the password (local UI → no
 * mangling), optionally tick 記憶する, click OK.
 */
export function handlePasswordDialog(pw: string, remember: boolean): boolean {
  const boxes = ocr();
  if (!passwordDialogPresent(boxes)) return false;

  // Focus the password input: heuristically the area under the dialog title.
  const title = findAny(boxes, ["パスワードが必要です", "Password Required", "パスワード"]);
  if (title) {
    const c = boxCenter(title);
    clickScreenshotPx(c.x, c.y + Math.max(40, title.h * 2));
  }
  typeText(pw); // local dialog — direct typing is reliable

  if (remember) {
    const chk = findAny(boxes, ["記憶する", "Remember", "Remember password"]);
    if (chk) {
      const c = boxCenter(chk);
      clickScreenshotPx(c.x, c.y);
    }
  }

  const ok = findAny(boxes, ["OK", "确定", "確定", "接続", "Connect"]);
  if (ok) {
    const c = boxCenter(ok);
    clickScreenshotPx(c.x, c.y);
  } else {
    pressKey("Enter");
  }
  return true;
}

/** Detect a connected canvas via OCR toast / absence of the dialog. Best-effort. */
export function detectConnected(): boolean {
  const boxes = ocr();
  if (passwordDialogPresent(boxes)) return false;
  // A connected session shows a toolbar / "接続完了"-style toast, or simply no
  // login UI. We treat "no password dialog + on rustdesk" as connected.
  return onRustdesk();
}

/**
 * Full connect flow. Resolves the password (arg → remembered → error), drives the
 * UI, optionally remembers the password, and records the id in recents.
 */
export function connect(opts: ConnectOptions): ConnectResult {
  const store = loadPasswords();
  // Resolve password up-front so we fail fast before touching the UI.
  const pw = resolvePassword(opts.id, opts.pw, store);

  ensureSession(opts.fresh);
  enterId(opts.id);
  triggerConnect();

  // The dialog may take a moment; we attempt once (callers can re-run connect).
  const handled = handlePasswordDialog(pw, !!opts.rememberPassword);

  if (opts.rememberPassword) setPassword(opts.id, pw);
  recordRecent(opts.id);

  const connected = detectConnected();
  return {
    ok: true,
    id: opts.id,
    connected,
    note: handled ? "password dialog handled" : "no password dialog observed",
  };
}

// --- recents (list) ---

import { existsSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { configDir } from "./passwords";

function recentsPath(): string {
  return join(configDir(), "recents.json");
}

export function recordRecent(id: string): void {
  const cur = listRecent();
  const next = [id, ...cur.filter((x) => x !== id)].slice(0, RECENT_LIMIT);
  const dir = configDir();
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true, mode: 0o700 });
  writeFileSync(recentsPath(), JSON.stringify(next, null, 2) + "\n", { mode: 0o600 });
}

export function listRecent(): string[] {
  const p = recentsPath();
  if (!existsSync(p)) return [];
  try {
    const arr = JSON.parse(readFileSync(p, "utf8"));
    return Array.isArray(arr) ? arr : [];
  } catch {
    return [];
  }
}

/** Status: are we on rustdesk and seemingly connected? */
export function status(): { onRustdesk: boolean; connected: boolean; url: string } {
  const on = onRustdesk();
  return { onRustdesk: on, connected: on ? detectConnected() : false, url: on ? "rustdesk.com/web" : "" };
}

/** Disconnect: press Escape / click a 切断 control if present. */
export function disconnect(): void {
  const boxes = ocr();
  const btn = findAny(boxes, ["切断", "Disconnect", "Close session"]);
  if (btn) {
    const c = boxCenter(btn);
    clickScreenshotPx(c.x, c.y);
  } else {
    pressKey("Escape");
  }
}

export { getPassword };
