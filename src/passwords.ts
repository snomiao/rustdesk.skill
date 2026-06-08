// Password store for rustdesk-skill.
//
// Persists per-id RustDesk passwords at ~/.config/rustdesk-skill/passwords.json
// with file mode 0600 (dir 0700). The location is overridable via RDS_CONFIG_DIR
// so tests never touch the real user config.
//
// NOTE: this is plaintext-on-disk, same trust model as ~/.ssh keys or winrpc's
// token file — protected by filesystem perms, not encryption. Documented in README.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export type PasswordStore = Record<string, string>;

/** Resolve the config directory (RDS_CONFIG_DIR override → ~/.config/rustdesk-skill). */
export function configDir(): string {
  return process.env.RDS_CONFIG_DIR || join(homedir(), ".config", "rustdesk-skill");
}

function passwordsPath(): string {
  return join(configDir(), "passwords.json");
}

/** Load the full id→password map. Returns {} if missing/corrupt. */
export function loadPasswords(): PasswordStore {
  const p = passwordsPath();
  if (!existsSync(p)) return {};
  try {
    const raw = readFileSync(p, "utf8");
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? (parsed as PasswordStore) : {};
  } catch {
    return {};
  }
}

/** Persist the full map with restrictive perms (dir 0700, file 0600). */
export function savePasswords(store: PasswordStore): void {
  const dir = configDir();
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true, mode: 0o700 });
  // writeFileSync honors mode on create; Bun.write does not, so use node fs here.
  writeFileSync(passwordsPath(), JSON.stringify(store, null, 2) + "\n", { mode: 0o600 });
}

/** Get a remembered password for an id, or undefined. */
export function getPassword(id: string): string | undefined {
  return loadPasswords()[id];
}

/** Remember a password for an id (writes the store). */
export function setPassword(id: string, pw: string): void {
  const store = loadPasswords();
  store[id] = pw;
  savePasswords(store);
}

export class NoPasswordError extends Error {}

/**
 * Resolve which password to use for a connect:
 *   explicit arg → remembered → throw NoPasswordError.
 * Pure over an injected store so it is trivially unit-testable.
 */
export function resolvePassword(
  id: string,
  pwArg: string | undefined,
  store: PasswordStore,
): string {
  if (pwArg != null && pwArg !== "") return pwArg;
  const remembered = store[id];
  if (remembered) return remembered;
  throw new NoPasswordError(
    `no password for id ${id}; pass one (rds c ${id} <pw>) or save it with --remember-password`,
  );
}
