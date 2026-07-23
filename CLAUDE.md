# CLAUDE.md — rustdesk-skill

## ⚠️ THIS REPOSITORY IS PUBLIC — github.com/snomiao/rustdesk.skill

**Never commit company / personal / local-only data.** Before every commit, make sure none of these slipped in:

- ❌ Real RustDesk IDs or passwords → use placeholders (`123456789`, `<password>`).
- ❌ Real emails, Tailscale / `100.x` IPs, hostnames, machine names, internal URLs, tokens, API keys, SSH/private keys.
- ❌ Local / absolute paths (`/Users/…`, `~/ws/…`, `C:\Users\…`) or references to private/internal tooling or a monorepo layout. Keep this package **self-contained and generic**.
- ❌ `.env`, credentials, `~/.config/rustdesk-skill/passwords.json`, or screenshots / recordings of real sessions.

All docs, examples, and test fixtures must use **fake placeholders**. **When in doubt, leave it out.**

A **pre-commit hook** at `.githooks/pre-commit` scans staged changes for the above and blocks the commit if it finds something. Install it once:

```bash
bash scripts/install-hooks.sh      # sets core.hooksPath = .githooks
```

To scan for extra private strings (e.g. your org's names) without publishing the list, put one regex/string per line in **`.secret-denylist`** (gitignored). Bypass intentionally with `git commit --no-verify` (only when you're sure).

## What this is

`rustdesk-skill` / `rds` — a winrpc-style CLI to drive **RustDesk web** (connect by ID, screenshot the Flutter canvas, OCR it into positioned text, click/type by text or coords). See [SKILL.md](./SKILL.md) for usage. Complements (does not replace) the official server-only `rustdesk` CLI.

## Conventions

- Stack: **Bun + TypeScript**; **yargs** for all argument parsing. Code comments in English.
- OSS-friendly: no hard dependency on private tools; any optional external helper must be documented and degrade gracefully if absent.
- Browser backend = Microsoft's official **`@playwright/cli`** (`playwright-cli`), wrapped in `src/pw.ts` — the single shell-out choke point (a future native driver implements the same surface). Override the binary with `RDS_PW_BIN`.
- **Session name**: every command carries `-s=<RDS_SESSION_NAME>` so repeated `rds` calls reuse one browser tab. Resolution lives in `src/session.ts`: env / `.env.local` → TTY-guided default (git repo/branch, else folder name) → non-interactive default with a note. `.env.local` is gitignored (`.env.*`).
