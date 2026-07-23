// Session-name resolution for the @playwright/cli backend.
//
// Every `rds` command scopes its browser to a NAMED @playwright/cli session
// (`-s=<name>`), so separate `rds` invocations share one persistent, authenticated
// Chrome tab. We deliberately do NOT hard-code a single session name: two projects
// (or two users) sharing one machine must not collide on one "rustdesk-skill"
// session. Instead the name is derived per working directory, and the caller can
// pin it explicitly.
//
// CONTRACT:
//   1. RDS_SESSION_NAME is read from the environment. Bun auto-loads `.env.local`
//      from the caller's cwd, so `RDS_SESSION_NAME=<name>` there (or inline:
//      `RDS_SESSION_NAME=<name> rds ...`) pins the session.
//   2. Otherwise we derive a DEFAULT: the git repo/branch when inside a repo, else
//      the cwd folder name. cwd always has a name, so a default (almost) always
//      exists and gives natural per-project isolation.
//   3. On an interactive TTY we GUIDE: show that default, let the user edit it, and
//      optionally persist it to `./.env.local` so later commands reuse it silently.
//   4. Non-interactively we use the derived default and print a one-line note (so
//      the choice is visible in logs). We only ERROR in the pathological case where
//      no name can be derived at all (e.g. cwd is the filesystem root).

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

/** Thrown only when no session name can be derived and we cannot ask. */
export class SessionNameError extends Error {}

/**
 * Reduce an arbitrary string to a safe session token: keep [A-Za-z0-9._-], turn
 * every other run into a single "-", trim leading/trailing "-". Pure & testable.
 * Guarantees the value is safe both as a `-s=` arg and as an unquoted .env value.
 */
export function sanitizeSessionName(raw: string): string {
  return raw
    .trim()
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/-{2,}/g, "-")
    .replace(/^-+|-+$/g, "");
}

/**
 * Default session name from the current git repo + branch, e.g.
 * "myproject/feature-x" -> "myproject-feature-x". Undefined outside a git repo
 * (or when git is not installed).
 */
export function gitDefaultSessionName(): string | undefined {
  const top = spawnSync("git", ["rev-parse", "--show-toplevel"], { encoding: "utf8" });
  if (top.status !== 0) return undefined;
  const repo = basename(top.stdout.trim());
  if (!repo) return undefined;
  const br = spawnSync("git", ["rev-parse", "--abbrev-ref", "HEAD"], { encoding: "utf8" });
  const branch = br.status === 0 ? br.stdout.trim() : "";
  const raw = branch && branch !== "HEAD" ? `${repo}/${branch}` : repo;
  return sanitizeSessionName(raw) || undefined;
}

/**
 * The default we offer/use when RDS_SESSION_NAME is unset:
 *   git repo/branch  →  cwd folder name.
 * cwd always resolves, so this is empty only from a nameless dir (e.g. "/").
 */
export function defaultSessionName(): string {
  return gitDefaultSessionName() ?? sanitizeSessionName(basename(process.cwd()));
}

/** True when we can prompt the user (both ends of the pipe are a terminal). */
function interactive(): boolean {
  return Boolean(process.stdin.isTTY && process.stdout.isTTY);
}

/** Upsert `KEY=value` into ./.env.local (create if absent). */
function upsertEnvLocal(key: string, value: string): string {
  const p = join(process.cwd(), ".env.local");
  const existing = existsSync(p) ? readFileSync(p, "utf8") : "";
  const lines = existing.length ? existing.split(/\r?\n/) : [];
  const re = new RegExp(`^\\s*${key}\\s*=`);
  const entry = `${key}=${value}`;
  const idx = lines.findIndex((l) => re.test(l));
  if (idx >= 0) {
    lines[idx] = entry;
  } else {
    // Drop trailing blank lines so we append cleanly, then re-add one newline.
    while (lines.length && lines[lines.length - 1] === "") lines.pop();
    lines.push(entry);
  }
  writeFileSync(p, lines.join("\n") + "\n");
  return p;
}

/**
 * Resolve the session name for this process.
 *   env (RDS_SESSION_NAME / .env.local) → interactive guided default → derived
 *   default with a note → error (only if nothing can be derived).
 */
export function resolveSessionName(): string {
  const env = process.env.RDS_SESSION_NAME?.trim();
  if (env) return sanitizeSessionName(env);

  const def = defaultSessionName();

  if (!interactive()) {
    if (!def) {
      throw new SessionNameError(
        "RDS_SESSION_NAME is not set and no default could be derived from git or the current folder. " +
          "Set it in ./.env.local (RDS_SESSION_NAME=<name>) or inline: `RDS_SESSION_NAME=<name> rds ...`.",
      );
    }
    console.error(
      `[rustdesk-skill] RDS_SESSION_NAME not set; using "${def}" (from git/cwd). ` +
        "Set RDS_SESSION_NAME in ./.env.local or inline to pin it.",
    );
    return def;
  }

  // Interactive: offer the default and let the user edit it.
  const answerRaw = prompt(`RustDesk session name (RDS_SESSION_NAME) [${def}]:`, def);
  const name = sanitizeSessionName(answerRaw ?? "");
  if (!name) throw new SessionNameError("no session name provided; aborting.");

  // Offer to persist so later (even non-interactive) commands reuse it.
  const save = prompt(`Save RDS_SESSION_NAME=${name} to ./.env.local for reuse? [Y/n]:`, "Y");
  if (/^\s*y/i.test(save ?? "Y")) {
    try {
      const p = upsertEnvLocal("RDS_SESSION_NAME", name);
      console.error(`[rustdesk-skill] saved RDS_SESSION_NAME=${name} to ${p}`);
    } catch (e) {
      console.error(
        `[rustdesk-skill] warning: could not write .env.local (${e instanceof Error ? e.message : e}); using it for this run only.`,
      );
    }
  }
  // Make the rest of this process see it via the normal env branch.
  process.env.RDS_SESSION_NAME = name;
  return name;
}
