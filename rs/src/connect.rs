//! `connect` of src/connect.ts: the password first, then the RustDesk tab,
//! the id, the connect button, the password dialog, the stores, and what
//! the screen shows at the end.
//!
//! Everything the end of it reads or writes is looked at before the first
//! step that acts on the page: the password store and the recents must be
//! in a shape the binary writes as the TypeScript does, both files must be
//! writable, and stdout must not be a reader that has gone away. From that
//! step on there is no way back, so everything after it is answered as the
//! TypeScript answers it: the screens are read as JavaScript reads any
//! output of the OCR script (rs/src/ocr.rs `normalize_js`), looked up with
//! its fixed texts whatever script is on screen, and the field of the
//! password dialog is picked with JavaScriptCore's own sort.

use crate::commands::{detect_connected, password_dialog_present};
use crate::js;
use crate::jssort;
use crate::ocr::{self, JsBox};
use crate::pw;
use crate::run::{Run, Stop, R};
use crate::store;

/// `/パスワード|password/i` and the like: the `i` of a regular expression
/// without `u` matches the ASCII letters of the pattern in either case, and
/// no other character for them.
fn mentions(text: &str, words: &[&str]) -> bool {
    let lower = text.to_ascii_lowercase();
    words.iter().any(|word| lower.contains(word))
}

/// `pickPasswordField(boxes, title)`: the boxes' texts are strings by now
/// (looking for the dialog threw otherwise).
pub fn pick_password_field<'b>(boxes: &'b [JsBox], title: Option<&JsBox>) -> Option<&'b JsBox> {
    let mut candidates: Vec<&JsBox> = boxes
        .iter()
        .filter(|b| {
            let text = b.text.as_deref().unwrap_or_default();
            mentions(text, &["パスワード", "password"])
                && !mentions(text, &["必要", "確認", "記憶", "remember", "required", "confirm"])
                && title.is_none_or(|title| b.y > title.y)
        })
        .collect();
    // `candidates.sort((a, b) => a.y - b.y)`
    jssort::sort(&mut candidates, |a, b| a.y - b.y < 0.0);
    candidates.first().copied()
}

fn click_center(run: &mut Run, b: &JsBox) -> R<()> {
    let (x, y) = ocr::center(b);
    pw::click_screenshot_px(run, x, y)
}

/// `enterId(id)`
fn enter_id(run: &mut Run, id: &str) -> R<()> {
    let boxes = ocr::ocr_js(run)?;
    if let Some(label) = ocr::find_any_js(&boxes, &["リモートID", "Remote ID", "Control Remote", "ID"])?.cloned() {
        click_center(run, &label)?;
    }
    pw::pw(run, &["type", id], false)?;
    pw::press_key(run, "Enter")
}

/// `triggerConnect()`
fn trigger_connect(run: &mut Run) -> R<()> {
    let boxes = ocr::ocr_js(run)?;
    if let Some(button) = ocr::find_any_js(&boxes, &["接続", "Connect", "Connect to"])?.cloned() {
        click_center(run, &button)?;
    }
    Ok(())
}

/// `handlePasswordDialog(pw, remember)`
fn handle_password_dialog(run: &mut Run, password: &str, remember: bool) -> R<bool> {
    let boxes = ocr::ocr_js(run)?;
    if !password_dialog_present(&boxes)? {
        return Ok(false);
    }
    let title = ocr::find_any_js(&boxes, &["パスワードが必要です", "Password Required"])?.cloned();
    if let Some(field) = pick_password_field(&boxes, title.as_ref()).cloned() {
        click_center(run, &field)?;
    } else if let Some(title) = &title {
        let (x, y) = ocr::center(title);
        pw::click_screenshot_px(run, x, y + js::math_max(40.0, title.h * 2.0))?;
    }
    pw::pw(run, &["type", password], false)?;
    if remember {
        if let Some(check) = ocr::find_any_js(&boxes, &["記憶する", "Remember", "Remember password"])?.cloned() {
            click_center(run, &check)?;
        }
    }
    match ocr::find_any_js(&boxes, &["OK", "确定", "確定", "接続", "Connect"])?.cloned() {
        Some(ok) => click_center(run, &ok)?,
        None => pw::press_key(run, "Enter")?,
    }
    Ok(true)
}

/// `connect(opts)`, then `printJson(result)`.
pub fn connect(run: &mut Run, id: &str, pw_arg: Option<&str>, remember: bool, fresh: bool) -> R<()> {
    let store = store::load_passwords(run)?;
    let password = match pw_arg.filter(|pw| !pw.is_empty()) {
        Some(pw) => pw.to_string(),
        None => match store.remembered(id)? {
            Some(pw) => pw,
            None => {
                return Err(Stop::known(3, format!("no password for id {id}; pass one (rds c {id} <pw>) or save it with --remember-password")));
            }
        },
    };
    // What the end of the flow reads and writes, looked at now.
    if js::is_object_member(id) {
        return Err(Stop::HandBack);
    }
    store::list_recent(run)?;
    store::writable(run, "recents.json")?;
    if remember {
        store::writable(run, "passwords.json")?;
    }
    // The OCR's PNG where the binary writes it (a relative temp directory
    // goes back), and printJson at the end, which fails in bun's words, with
    // a stack trace, when nothing reads stdout any more.
    if !ocr::png_path(run).starts_with('/') || crate::run::stdout_unwritable() {
        return Err(Stop::HandBack);
    }

    // ensureSession(fresh)
    if fresh || !pw::on_rustdesk(run)? {
        let browser = run.machine.var("RDS_PW_BROWSER").map(|b| js::trim(&b).to_string()).filter(|b| !b.is_empty());
        let browser_arg = browser.map(|b| format!("--browser={b}"));
        let mut args = vec!["open"];
        if let Some(arg) = &browser_arg {
            args.push(arg);
        }
        args.push(pw::RUSTDESK_WEB_URL);
        if pw::pw(run, &args, true)?.status != 0 {
            pw::pw(run, &["goto", pw::RUSTDESK_WEB_URL], false)?;
        }
    }
    enter_id(run, id)?;
    trigger_connect(run)?;
    let handled = handle_password_dialog(run, &password, remember)?;
    if remember {
        store::set_password(run, id, &password)?;
    }
    store::record_recent(run, id)?;
    let connected = detect_connected(run)?;
    let note = if handled { "password dialog handled" } else { "no password dialog observed" };
    run.print_json(&format!("{{\n  \"ok\": true,\n  \"id\": {},\n  \"connected\": {connected},\n  \"note\": \"{note}\"\n}}\n", js::json_string(id)));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(text: &str, y: f64) -> JsBox {
        JsBox { text: Ok(text.into()), x: 0.0, y, w: 10.0, h: 10.0 }
    }

    #[test]
    fn the_password_field_is_picked_as_the_typescript_picks_it() {
        let title = b("※** パスワードが必要です", 250.0);
        let boxes = [
            title.clone(),
            b("RustDesk のパスワードを確認する", 302.0),
            b("白 パスワード", 350.0),
            b("パスワードを記憶する", 406.0),
            b("✕ キャンセル", 458.0),
            b("<OK", 460.0),
        ];
        assert_eq!(pick_password_field(&boxes, Some(&title)).unwrap().y, 350.0);
        let mut above = vec![b("パスワード", 100.0)];
        above.extend(boxes.iter().cloned());
        assert_eq!(pick_password_field(&above, Some(&title)).unwrap().y, 350.0);
        assert_eq!(pick_password_field(&above, None).unwrap().y, 100.0);
        assert!(pick_password_field(&[b("PassWord Required", 1.0)], None).is_none());
        assert_eq!(pick_password_field(&[b("Enter PASSWORD", 1.0)], None).unwrap().y, 1.0);
        // a.y - b.y is 0 for 0 and -0: the first stays first.
        let mut zero = b("パスワード", 0.0);
        zero.x = 100.0;
        let mut minus = b("パスワード", -0.0);
        minus.x = 300.0;
        assert_eq!(pick_password_field(&[zero.clone(), minus.clone()], None).unwrap().x, 100.0);
        assert_eq!(pick_password_field(&[minus, zero], None).unwrap().x, 300.0);
    }
}
