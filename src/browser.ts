// RustDesk-web session management on top of the rech backend.
//
// IMPORTANT (session safety): only connect() may navigate. rech `open`/`goto`
// reloads the page and kills any live RustDesk connection, so every other command
// (screenshot/ocr/click/type/...) acts on the CURRENT tab with zero navigation.

import { evalJs, rech } from "./rech";

export const RUSTDESK_WEB_URL = "https://rustdesk.com/web/";

/** Current page URL (empty string if no session / eval fails). */
export function currentUrl(): string {
  try {
    return evalJs("() => location.href").replace(/^["']|["']$/g, "");
  } catch {
    return "";
  }
}

/** Is a browser tab open and pointed at the RustDesk web client? */
export function onRustdesk(): boolean {
  return currentUrl().includes("rustdesk.com/web");
}

/**
 * Ensure a RustDesk-web tab is open. Navigates ONLY if not already on rustdesk
 * (or `force` to start fresh). Returns true if it navigated.
 */
export function ensureSession(force = false): boolean {
  if (!force && onRustdesk()) return false;
  // `open` starts a window if none; if a session already exists rech turns this
  // into a goto. Either way we land on the RustDesk web client.
  rech(["open", RUSTDESK_WEB_URL]);
  return true;
}
