# rustdesk-skill (`rustdesk-skill` / `rds`)

winrpc-style CLI to drive **RustDesk web** from the terminal so agents can connect to a remote PC by ID, screenshot the Flutter canvas, **OCR it into positioned text**, and click/type by text or coordinates.

> Complements (does not replace) the official `rustdesk` CLI, which is server/service-only. This is the interactive *connect-and-drive* tool.

```bash
rds c 123456789 <password> --remember-password      # connect + remember pw
rds c 123456789                                      # reconnect with saved pw
rds ocr --json                                       # every on-screen text + box
rds click "OK"                                        # click by text
```

- **Usage / commands**: see [SKILL.md](./SKILL.md)
- **Browser backend**: [`@playwright/cli`](https://www.npmjs.com/package/@playwright/cli) (Microsoft's official CLI). Install once with `bun add -g @playwright/cli`, or run via `RDS_PW_BIN="bunx @playwright/cli"`, or point `RDS_PW_BIN` at any binary.
- **Session name** (required): each command scopes its browser to a named session so repeated commands reuse one tab. Set `RDS_SESSION_NAME` in `./.env.local` or inline (`RDS_SESSION_NAME=<name> rds ...`). In an interactive terminal, `rds` offers a default from your git repo/branch (falling back to the folder name), lets you edit it, and can save it to `.env.local`.
- ⚠️ **Typing into the remote is unreliable**: key events (`Enter`, …) reach the remote desktop, but text sometimes doesn't. `rds type` / `rds paste` therefore OCR the screen before and after, and **exit 8** when the text didn't visibly land (`--no-verify` for masked fields). Use `rds` to look and click; don't rely on it to run commands on the remote.
- ⚠️ **Keyboard caveat**: RustDesk web mangles shift-chars on a JP-layout remote (`@`→`2`, `+`→`=`, capitals→lowercase). For clean shell access prefer **Tailscale + SSH**; this tool is for GUI/canvas interaction. See SKILL.md.

OCR backends in `ocr/` are adapted from `pkgs/winrpc/` (PaddleOCR + macOS Vision).
