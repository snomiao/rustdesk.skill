---
name: rustdesk-skill
description: winrpc風(ふう)の RustDesk 接続(せつぞく) CLI。RustDesk web を Playwright で駆動(くどう)し、画面(がめん)を OCR して「位置(いち)付(つ)きテキスト」を得(え)て、ID 接続(せつぞく)・screenshot・テキスト/座標(ざひょう)クリック・型(かた)入力(にゅうりょく)を terminal から行(おこな)う。リモート PC に `rds c <id> [pw]` で繋(つな)ぎたい時(とき)、Flutter canvas な RustDesk web を agent から操作(そうさ)したい時(とき)に invoke。
---

# rustdesk-skill — RustDesk connect CLI for agents (`rustdesk-skill` / `rds`)

`winrpc` と同(おな)じ「terminal からリモート PC を操作(そうさ)する」UX を **RustDesk** で実現(じつげん)する CLI。リモートを **ID で接続(せつぞく)** し、画面(がめん)を **screenshot + OCR** して全(すべ)ての表示(ひょうじ)テキストを**座標(ざひょう)付(つ)き**で取得(しゅとく) → そのテキストで**クリック**したり画面(がめん)状態(じょうたい)を理解(りかい)したりできる。

> 公式(こうしき) `rustdesk` CLI（server/service 管理(かんり)専用(せんよう)）とは**別物(べつもの)**。bin 名(めい) `rustdesk-skill` / `rds` で衝突(しょうとつ)回避(かいひ)。

## なぜ必要(ひつよう)か
RustDesk web client V2 は **Flutter canvas**（DOM 無(な)し）なので、通常(つうじょう)の browser 自動(じどう)操作(そうさ)では要素(ようそ)が見(み)えない。この CLI は canvas を screenshot して **OCR（PaddleOCR / macOS Vision）** で**全(ぜん)テキスト＋bounding box** を復元(ふくげん)する → agent が「画面(がめん)上(じょう)の文字(もじ)」を読(よ)めて、その文字(もじ)をクリックできる。接続(せつぞく)（ID 入力(にゅうりょく)・パスワードダイアログ・パスワード記憶(きおく)）も自動(じどう)化(か)。

## 必要(ひつよう)なもの（Requirements）
- **Bun**（実行(じっこう)ランタイム）
- **`@playwright/cli`**（Microsoft 公式(こうしき)、bin 名(めい) `playwright-cli`）— browser backend。`bun add -g @playwright/cli` で導入(どうにゅう)、または `RDS_PW_BIN="bunx @playwright/cli"` で実行(じっこう)、任意(にんい)の binary を `RDS_PW_BIN` で指定(してい)も可(か)。未(み)導入(どうにゅう)だと `rds` は明確(めいかく)なエラーで止(と)まる。初回(しょかい) `open` で browser が download される場合(ばあい)あり（`RDS_PW_BROWSER=chrome` で system Chrome を使用(しよう)）。
- **`RDS_SESSION_NAME`**（session 名(めい)・実質(じっしつ)必須(ひっす)）— 各(かく) command は名前付(なまえつ)き session（`-s=<name>`）に browser を紐付(ひもづ)け、連続(れんぞく) command が同(おな)じ tab を再利用(さいりよう)する。`./.env.local` に `RDS_SESSION_NAME=<name>` を書(か)くか inline（`RDS_SESSION_NAME=<name> rds ...`）で指定(してい)。**TTY なら** git repo/branch（無(な)ければ folder 名(めい)）を既定(きてい)値(ち)として提示(ていじ)→編集(へんしゅう)可(か)→`.env.local` に保存(ほぞん)も可(か)。非(ひ)対話(たいわ)時(じ)は既定(きてい)値(ち)を使(つか)い一行(いちぎょう) note を出(だ)す。
- **OCR backend**（`ocr` / `click` 用(よう)）: macOS は標準(ひょうじゅん)の **Vision**（追加(ついか)不要(ふよう)）、それ以外(いがい)は **PaddleOCR**（`pip install paddleocr`、Python）。`--ocr` / `RDS_OCR` で選択(せんたく)。

## インストール
```bash
bun add -g @playwright/cli                     # browser backend（未(み)導入(どうにゅう)の場合(ばあい)）
cd rustdesk.skill && bun install && bun link   # rustdesk-skill / rds が PATH に
echo 'RDS_SESSION_NAME=my-session' >> .env.local   # session 名(めい)を pin（任意(にんい)・TTY なら自動(じどう)提示(ていじ)）
```

## コマンド
| コマンド | エイリアス | 説明(せつめい) |
|---|---|---|
| `rustdesk-skill connect <id> [pw] [--remember-password] [--new]` | `rds c <id> [pw] ...` | RustDesk web を開(ひら)き `<id>` に接続(せつぞく)。`pw` 指定(してい)で入力(にゅうりょく)、`--remember-password` で `<id>` 用(よう)に保存(ほぞん)。**`pw` 省略(しょうりゃく)で記憶(きおく)済(ず)みパスワードを使用(しよう)**。 |
| `rds screenshot [file]` | `rds ss` | 現(げん)リモート canvas の PNG。 |
| `rds ocr [--json] [--lang ja,en]` | | canvas を OCR → `{text, conf, x, y, w, h}` の一覧(いちらん)。 |
| `rds click "<text>" [--nth N]` | | OCR で `<text>` を探(さが)し中心(ちゅうしん)をクリック。 |
| `rds click-xy <x> <y>` | | canvas 上(じょう)の絶対(ぜったい)座標(ざひょう)クリック。 |
| `rds move <x> <y>` / `rds scroll <dy>` | | ポインタ操作(そうさ)。 |
| `rds type "<text>"` | | focus 中(ちゅう)のフィールドに型(かた)入力(にゅうりょく)。⚠️ 下記(かき) Keyboard caveat。 |
| `rds key <key>[+<mod>...]` | | キー押下(おうか)（`Enter` / `Escape` / `ctrl+c` 等(とう)）。 |
| `rds paste "<text>"` | | clipboard 経由(けいゆ)貼(は)り付(つ)け（mangling 回避(かいひ)、sync が効(き)く場合(ばあい)）。 |
| `rds status` / `rds disconnect` / `rds list` | | 接続(せつぞく)状態(じょうたい) / 切断(せつだん) / 最近(さいきん)の ID。 |

## 例(れい)
```bash
rds c 123456789 <password> --remember-password      # 接続(せつぞく)＋pw 保存(ほぞん)
rds c 123456789                                      # 記憶(きおく) pw で再接続(さいせつぞく)
rds ocr --json                                       # 画面(がめん)上(じょう)の全(ぜん)テキスト＋box
rds click "OK"                                        # 「OK」ボタンをテキストでクリック
rds click-xy 685 315 && rds type "hello"              # 座標(ざひょう)クリック → 入力(にゅうりょく)
```

## ⚠️ Keyboard caveat（重要(じゅうよう)）
RustDesk web は raw key event を転送(てんそう)する。**JP layout の remote では Shift が落(お)ちる**ため `@`→`2` / `+`→`=` / `/`→`*` / 大文字(おおもじ)→小文字(こもじ) / quotes が化(ば)ける。よって `rds type` が確実(かくじつ)なのは **lowercase・数字(すうじ)・`-`・`.`・`/`（unshifted）** のみ。パスワード等(とう)の特殊(とくしゅ)文字(もじ)は (a) `rds paste`（clipboard sync が効(き)けば）、(b) **SSH/Tailscale 等(とう)の clean channel** を推奨(すいしょう)。CLI は shift 必須(ひっす)文字(もじ)を含(ふく)む `type` 入力(にゅうりょく)を**警告(けいこく)**する。

> 実運用(じつうんよう)メモ: clean な shell が要(い)るなら RustDesk より **Tailscale + OpenSSH**（`ssh <user>@<tailscale-ip>`）の方(ほう)が化(ば)けず・script 可(か)・elevated。この skill は **GUI/canvas 操作(そうさ)が必要(ひつよう)な場面(ばめん)**や Tailscale が無(な)い相手(あいて)向(む)け。

## アーキテクチャ（first impl）
- **Driver = web+playwright+ocr**（pluggable。将来(しょうらい) native client driver も）。backend は `@playwright/cli`（`playwright-cli`）を `src/pw.ts` で wrap。唯一(ゆいいつ)の shell out 箇所(かしょ)なので差(さ)し替(か)え容易(ようい)。
- `@playwright/cli` 制御(せいぎょ)の Chrome → `rustdesk.com/web`。命名(めいめい) session（`-s=<name>`）で連続(れんぞく) command が同(おな)じ browser を共有(きょうゆう)。
- canvas screenshot（`--hires` = device px）→ OCR backend（`ocr/ocr.py` PaddleOCR、fallback `ocr/ocr-mac.py` macOS Vision）→ 位置(いち)付(つ)きテキスト JSON。
- 接続(せつぞく)フロー: リモートID 入力(にゅうりょく) → → クリック → 「パスワードが必要(ひつよう)です」ダイアログ処理(しょり)（pw type/paste・`--remember-password` なら「記憶(きおく)する」チェック・OK）。
- connect は**検証(けんしょう)してから成功(せいこう)を報告(ほうこく)**する: pw 入力(にゅうりょく)後(ご)に OCR でフィールドが埋(う)まった（placeholder 消失(しょうしつ)／伏字(ふせじ)）ことを確認(かくにん)（1 回(かい) retry）、OK 後(ご)はダイアログが閉(と)じるまで待(ま)つ。`note` は `password accepted` / `password NOT entered ...` / `password submitted but NOT accepted ...` / `no password dialog observed`。失敗(しっぱい)時(じ)は `ok: false`・**exit 7**、`--remember-password` でも pw は保存(ほぞん)しない。
- パスワード保存(ほぞん): `~/.config/rustdesk-skill/passwords.json`（ID 毎(ごと)、mode 600）。
- session 名(めい)解決(かいけつ): `src/session.ts`。`RDS_SESSION_NAME`（env / `.env.local`）→ TTY なら git repo/branch（無(な)ければ folder 名(めい)）を提示(ていじ)・編集(へんしゅう)・`.env.local` 保存(ほぞん)→ 非(ひ)対話(たいわ)は既定(きてい)値(ち)＋note。`RDS_SESSION_DIR` で screenshot 一時(いちじ)出力(しゅつりょく)先(さき)、`RDS_PW_BIN` で backend binary を上書(うわが)き。
