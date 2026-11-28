//! The commands of bin/rustdesk-skill.ts, each with what its handler and its
//! `deps` function do, inside `guard` (rs/src/main.rs turns a `Stop` into
//! guard's message and exit code).

use std::fs;

use crate::args::{Backend, Command};
use crate::js;
use crate::ocr::{self, JsBox};
use crate::pw;
use crate::run::{Run, Stop, R};
use crate::store;

pub fn run(run: &mut Run, command: &Command) -> R<()> {
    // bun prints console.error in red when stdout or stderr is a terminal:
    // every command but `list` may write to stderr.
    if run.machine.terminal && *command != Command::List {
        return Err(Stop::HandBack);
    }
    match command {
        Command::List => list(run),
        Command::Scroll { dy } => {
            let dy = pw::coordinate(run, *dy)?;
            pw::pw(run, &["mousewheel", "0", &dy], false).map(drop)
        }
        Command::Key { chord } => pw::press_key(run, chord),
        Command::Type { text, force } => type_text(run, text, *force),
        Command::Move { x, y } => pw::move_screenshot_px(run, *x, *y),
        Command::ClickXy { x, y } => pw::click_screenshot_px(run, *x, *y),
        Command::Screenshot { file } => screenshot(run, file.as_deref()),
        Command::Disconnect => {
            pw::pw(run, &["goto", pw::RUSTDESK_WEB_URL], false)?;
            run.error_line("[rustdesk-skill] disconnect sent");
            Ok(())
        }
        Command::Status => status(run),
        Command::Ocr { json, lang, backend } => ocr_command(run, *json, lang, *backend),
        Command::Click { text, nth, contains, backend, lang } => click(run, text, *nth, *contains, *backend, lang),
        Command::Paste { text } => paste(run, text),
        Command::Connect { id, pw, remember, fresh } => crate::connect::connect(run, id, pw.as_deref(), *remember, *fresh),
    }
}

/// `printJson(listRecent())`
fn list(run: &mut Run) -> R<()> {
    let ids = store::list_recent(run)?;
    let text = if ids.is_empty() {
        "[]".to_string()
    } else {
        let items: Vec<String> = ids.iter().map(|id| format!("  {}", js::json_string(id))).collect();
        format!("[\n{}\n]", items.join(",\n"))
    };
    run.print_json(&format!("{text}\n"));
    Ok(())
}

/// The characters SHIFT_CHARS matches, each once, in order.
fn shift_chars(text: &str) -> String {
    let mut seen = String::new();
    for c in text.chars() {
        if (c.is_ascii_uppercase() || "~!@#$%^&*()_+{}|:\"<>?".contains(c)) && !seen.contains(c) {
            seen.push(c);
        }
    }
    seen
}

/// `type(text, { force })` of src/screen.ts
fn type_text(run: &mut Run, text: &str, force: bool) -> R<()> {
    let bad = shift_chars(text);
    if !bad.is_empty() && !force {
        run.error_line(&format!(
            "[rustdesk-skill] warning: text contains shift-requiring chars [{bad}] which the JP-layout remote mangles (@→2, +→=, A→a). Refusing. Use 'rds paste' for a clipboard round-trip, a clean SSH channel, or --force to type anyway."
        ));
        return Err(Stop::Exit(4, None));
    }
    if !bad.is_empty() {
        run.error_line(&format!("[rustdesk-skill] --force: typing shift chars [{bad}] anyway (may mangle)."));
    }
    pw::pw(run, &["type", text], false).map(drop)
}

/// `screenshot(file)`, then "saved".
fn screenshot(run: &mut Run, file: Option<&str>) -> R<()> {
    let shot = pw::screenshot_png(run)?;
    let dest = file.filter(|file| !file.is_empty()).unwrap_or("rds-screenshot.png");
    // A path that cannot be written is reported in bun's words: back, as
    // nothing has been written.
    fs::write(dest, &shot.bytes).map_err(|_| Stop::HandBack)?;
    run.commit();
    run.error_line(&format!("[rustdesk-skill] saved {dest}"));
    Ok(())
}

const PASSWORD_DIALOG: [&str; 4] = ["パスワードが必要です", "Password Required", "Please enter", "パスワード"];
const HOME_SCREEN: [&str; 5] = ["リモートID", "Remote ID", "リモートデスクトップ", "Control Remote", "Remote Desktop"];

/// `passwordDialogPresent(boxes)`
pub fn password_dialog_present(boxes: &[JsBox]) -> R<bool> {
    Ok(ocr::find_any_js(boxes, &PASSWORD_DIALOG)?.is_some())
}

/// `homeScreenPresent(boxes)`
pub fn home_screen_present(boxes: &[JsBox]) -> R<bool> {
    Ok(ocr::find_any_js(boxes, &HOME_SCREEN)?.is_some())
}

/// `detectConnected()`
pub fn detect_connected(run: &mut Run) -> R<bool> {
    let boxes = ocr::ocr_js(run)?;
    if password_dialog_present(&boxes)? || home_screen_present(&boxes)? {
        return Ok(false);
    }
    pw::on_rustdesk(run)
}

/// `printJson(status())`
fn status(run: &mut Run) -> R<()> {
    let on = pw::on_rustdesk(run)?;
    let connected = if on { detect_connected(run)? } else { false };
    let url = if on { "rustdesk.com/web" } else { "" };
    run.print_json(&format!("{{\n  \"onRustdesk\": {on},\n  \"connected\": {connected},\n  \"url\": \"{url}\"\n}}\n"));
    Ok(())
}

/// The `ocr` command: the boxes, as JSON or one line each.
fn ocr_command(run: &mut Run, json: bool, lang: &str, backend: Option<Backend>) -> R<()> {
    let boxes = ocr::ocr(run, backend, lang)?;
    if json {
        let text = ocr::boxes_json(&boxes)?;
        run.print_json(&format!("{text}\n"));
        return Ok(());
    }
    let mut lines = String::new();
    for b in &boxes {
        let num = |x: f64| js::number(x).ok_or(Stop::HandBack);
        lines.push_str(&format!(
            "{},{}  {}x{}  {}  {}\n",
            js::pad_start(&num(b.x)?, 5),
            js::pad_start(&num(b.y)?, 5),
            js::pad_start(&num(b.w)?, 4),
            js::pad_start(&num(b.h)?, 4),
            js::to_fixed_2(b.conf).ok_or(Stop::HandBack)?,
            b.text
        ));
    }
    run.write(&lines);
    Ok(())
}

/// `clickText(query, opts)`, then "clicked".
fn click(run: &mut Run, query: &str, nth: Option<u32>, contains: bool, backend: Option<Backend>, lang: &str) -> R<()> {
    let boxes = ocr::ocr(run, backend, lang)?;
    let (found, count, ambiguous) = ocr::match_text(&boxes, query, contains, nth)?;
    let Some(found) = found.cloned() else {
        return Err(Stop::error(format!("text not found on screen: \"{query}\" ({} boxes scanned)", boxes.len())));
    };
    if ambiguous {
        run.error_line(&format!(
            "[rustdesk-skill] warning: \"{query}\" matched {count} boxes; clicking the first. Use --nth to disambiguate."
        ));
    }
    // Worked out before the click, which cannot be taken back.
    let clicked = format!("[rustdesk-skill] clicked \"{}\" @ {},{}", found.text, pw::coordinate(run, found.x)?, pw::coordinate(run, found.y)?);
    let (cx, cy) = ocr::box_center(&found);
    pw::click_screenshot_px(run, cx, cy)?;
    run.error_line(&clicked);
    Ok(())
}

/// `paste(text)` of src/screen.ts
fn paste(run: &mut Run, text: &str) -> R<()> {
    use crate::proc::{self, Found};
    let mac = cfg!(target_os = "macos");
    let (name, args): (&str, &[&str]) = if mac { ("pbcopy", &[]) } else { ("xclip", &["-selection", "clipboard"]) };
    let output = match proc::find(run.machine, name)? {
        Found::Program(program) => {
            let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            // One that cannot be started has set nothing: bun's words.
            let started = proc::start(run.machine, &program, &args, None, Some(text.as_bytes())).map_err(|_| Stop::HandBack)?;
            run.commit();
            Some(started.finish().map_err(|_| Stop::HandBack)?)
        }
        // pbcopy's message would hold bun's text for the spawn error.
        Found::Missing if mac => return Err(Stop::HandBack),
        Found::Missing => None,
    };
    let failed = output.as_ref().is_none_or(|output| output.status != Some(0));
    if failed {
        if mac {
            // `res.stderr || res.error?.message`
            let what = match &output {
                Some(output) if !output.stderr.is_empty() => output.stderr.clone(),
                Some(output) => output.error_message("pbcopy").unwrap_or_else(|| "undefined".into()),
                None => "undefined".into(),
            };
            return Err(Stop::error(format!("pbcopy failed: {what}")));
        }
        return Err(Stop::error("clipboard set failed (need xclip on Linux)"));
    }
    pw::pw(run, &["keydown", "Control"], false)?;
    pw::pw(run, &["press", "v"], false)?;
    pw::pw(run, &["keyup", "Control"], false)?;
    Ok(())
}
