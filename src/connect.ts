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

import { ensureSession, onRustdesk, reloadHome } from "./browser";
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
 * Pick the password INPUT field box: mentions パスワード/password but is not the
 * dialog title ("...必要"/"required") nor the 記憶/remember checkbox, preferring the
 * row just below the title. Pure & unit-tested.
 */
export function pickPasswordField(boxes: Box[], title?: Box): Box | undefined {
  const candidates = boxes.filter(
    (b) =>
      /パスワード|password/i.test(b.text) &&
      // Exclude the title ("...必要"), the subtitle ("...確認する"), and the
      // 記憶/remember checkbox — leaving just the input placeholder.
      !/必要|確認|記憶|remember|required|confirm/i.test(b.text) &&
      (!title || b.y > title.y),
  );
  candidates.sort((a, b) => a.y - b.y); // nearest below the title = the input row
  return candidates[0];
}

/** A run of mask glyphs as OCR reads a filled password field (●●●, •••, ***). */
const MASK_RE = /[•●∙·・*＊◦○]{2,}/;

/**
 * Does the password field look FILLED? An empty field shows its "パスワード" /
 * "Password" placeholder (what pickPasswordField finds); once text is entered the
 * placeholder is replaced by mask dots. Filled = mask dots seen, or the placeholder
 * gone. Pure & unit-tested.
 */
export function passwordFieldFilled(boxes: Box[], title?: Box): boolean {
  if (boxes.some((b) => MASK_RE.test(b.text) && (!title || b.y > title.y))) return true;
  return !pickPasswordField(boxes, title);
}

/** Error text RustDesk shows in the dialog when the password is rejected. */
export function passwordRejected(boxes: Box[]): boolean {
  return !!findAny(boxes, ["パスワードが間違", "間違ったパスワード", "Wrong Password", "Wrong password", "incorrect"]);
}

/** Blocking sleep (each rds command is a short-lived sync process). */
function sleepMs(ms: number): void {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

export type PasswordOutcome =
  | "no-dialog" // no password dialog was observed
  | "not-entered" // field stayed empty after typing (nothing submitted)
  | "rejected" // submitted, dialog still up (wrong password / not accepted)
  | "accepted"; // submitted and the dialog went away

const FOCUS_SETTLE_MS = 300;
const TYPE_ATTEMPTS = 2;
const ACCEPT_POLLS = 4;
const ACCEPT_POLL_MS = 1500;

/**
 * Handle the password dialog: click the field, type the password (local UI → no
 * mangling), VERIFY the field is filled (retry once), optionally tick 記憶する,
 * click OK, then VERIFY the dialog went away. Never reports success for work it
 * could not confirm (issue #1): an empty field is "not-entered" (OK not clicked),
 * a dialog that stays up is "rejected".
 */
export function handlePasswordDialog(pw: string, remember: boolean): PasswordOutcome {
  let boxes = ocr();
  if (!passwordDialogPresent(boxes)) return "no-dialog";

  // Focus the password input. Prefer the field's own OCR box — the "パスワード" /
  // "Password" placeholder that is NOT the dialog title ("...が必要です" /
  // "Required") or the 記憶/remember checkbox, sitting below the title. Fall back
  // to clicking just below the title if the field box isn't isolated. (Verified
  // against the live dialog: the title+offset guess missed the input.)
  const title = findAny(boxes, ["パスワードが必要です", "Password Required"]);
  const field = pickPasswordField(boxes, title);
  const target = field
    ? boxCenter(field)
    : title
      ? { x: boxCenter(title).x, y: boxCenter(title).y + Math.max(40, title.h * 2) }
      : undefined;

  let filled = false;
  for (let i = 0; i < TYPE_ATTEMPTS && !filled; i++) {
    if (target) clickScreenshotPx(target.x, target.y);
    sleepMs(FOCUS_SETTLE_MS); // let Flutter attach focus before keystrokes
    typeText(pw); // local dialog — no JP-layout mangling
    boxes = ocr();
    filled = passwordFieldFilled(boxes, findAny(boxes, ["パスワードが必要です", "Password Required"]));
  }
  if (!filled) return "not-entered";

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

  // Accepted = the dialog closes. An explicit wrong-password message, or a dialog
  // that is still up after the polls, means the password was not accepted.
  for (let i = 0; i < ACCEPT_POLLS; i++) {
    sleepMs(ACCEPT_POLL_MS);
    boxes = ocr();
    if (passwordRejected(boxes)) return "rejected";
    if (!passwordDialogPresent(boxes)) return "accepted";
  }
  return "rejected";
}

const OUTCOME_NOTE: Record<PasswordOutcome, string> = {
  "no-dialog": "no password dialog observed",
  "not-entered":
    "password NOT entered: the field stayed empty after typing (nothing submitted); re-run connect or fill it manually",
  rejected: "password submitted but NOT accepted: the dialog is still open (wrong password?)",
  accepted: "password accepted",
};

/**
 * Is the RustDesk home / ID-entry screen on display? The connect form shows the
 * リモートID field and the "リモートデスクトップを操作" heading; a live remote
 * session replaces all of that with the canvas. Pure & unit-tested.
 */
export function homeScreenPresent(boxes: Box[]): boolean {
  return !!findAny(boxes, [
    "リモートID",
    "Remote ID",
    "リモートデスクトップ",
    "Control Remote",
    "Remote Desktop",
  ]);
}

/**
 * Best-effort "are we viewing a remote?": on rustdesk, but NOT showing the
 * password dialog and NOT on the home/ID-entry screen. This distinguishes a live
 * session from the home page (which is also on rustdesk with no dialog) — without
 * it, status() reported connected:true even after a disconnect.
 */
export function detectConnected(): boolean {
  const boxes = ocr();
  if (passwordDialogPresent(boxes)) return false; // mid-connect
  if (homeScreenPresent(boxes)) return false; // at ID-entry home, not connected
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
  const outcome = handlePasswordDialog(pw, !!opts.rememberPassword);
  const failed = outcome === "not-entered" || outcome === "rejected";

  // Only remember a password that was actually accepted (or not challenged).
  if (opts.rememberPassword && !failed) setPassword(opts.id, pw);
  recordRecent(opts.id);

  const connected = detectConnected();
  return {
    ok: !failed,
    id: opts.id,
    connected,
    note: OUTCOME_NOTE[outcome],
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

/**
 * Disconnect: reload the RustDesk client, deterministically dropping the remote
 * session and returning to the ID-entry home. (The connected toolbar's disconnect
 * button is an icon-only Flutter overlay OCR can't target — see reloadHome.)
 */
export function disconnect(): void {
  reloadHome();
}

export { getPassword };
