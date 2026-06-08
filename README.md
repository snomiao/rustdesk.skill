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
- **Implementation spec** (status: DRAFT → being implemented): see [IMPL_SPEC.md](./IMPL_SPEC.md)
- ⚠️ **Keyboard caveat**: RustDesk web mangles shift-chars on a JP-layout remote (`@`→`2`, `+`→`=`, capitals→lowercase). For clean shell access prefer **Tailscale + SSH**; this tool is for GUI/canvas interaction. See SKILL.md.

OCR backends in `ocr/` are adapted from `pkgs/winrpc/` (PaddleOCR + macOS Vision).
