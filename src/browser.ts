// RustDesk-web session management on top of the @playwright/cli backend.
//
// IMPORTANT (session safety): only connect() may navigate. `open`/`goto` reloads
// the page and kills any live RustDesk connection, so every other command
// (screenshot/ocr/click/type/...) acts on the CURRENT tab with zero navigation.

import { evalJs, pw } from "./pw";

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
  // Optional browser channel (e.g. RDS_PW_BROWSER=chrome to reuse system Chrome
  // instead of downloading a Playwright browser).
  const browser = process.env.RDS_PW_BROWSER?.trim();
  const browserArgs = browser ? [`--browser=${browser}`] : [];
  // `open` launches the session browser (if none) and navigates to the URL. If the
  // session browser is already up, `open` may no-op/fail, so fall back to `goto`.
  const res = pw(["open", ...browserArgs, RUSTDESK_WEB_URL], { allowFail: true });
  if (res.status !== 0) pw(["goto", RUSTDESK_WEB_URL]);
  return true;
}
