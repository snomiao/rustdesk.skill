//! src/pw.ts and src/browser.ts: every step is one `playwright-cli
//! -s=<session> <args>`, started in the session directory.
//!
//! The steps that only look (`screenshot`, `eval`) leave the invocation
//! free to go back to the JavaScript, which then takes them again; any other
//! step acts on the page, and `Run::commit` comes first.

use std::fs;
use std::io::ErrorKind;

use crate::js;
use crate::json::{self, Json};
use crate::proc::{self, Failed, Found};
use crate::run::{Run, Stop, R};
use crate::session;

pub const RUSTDESK_WEB_URL: &str = "https://rustdesk.com/web/";

pub struct PwResult {
    pub status: i32,
    pub stdout: String,
}

/// `pwCmd()`: RDS_PW_BIN, split at white space, else `playwright-cli`.
fn pw_cmd(run: &Run) -> (String, Vec<String>) {
    if let Some(raw) = run.machine.var("RDS_PW_BIN") {
        let raw = js::trim(&raw);
        if !raw.is_empty() {
            let mut parts = raw.split(js::is_space).filter(|part| !part.is_empty()).map(String::from);
            let cmd = parts.next().unwrap_or_default();
            return (cmd, parts.collect());
        }
    }
    ("playwright-cli".into(), Vec::new())
}

/// `sessionDir()`: RDS_SESSION_DIR, else <tmpdir>/rustdesk-skill, made when
/// it is not there.
pub fn session_dir(run: &mut Run) -> R<String> {
    let dir = match run.machine.var("RDS_SESSION_DIR").filter(|dir| !dir.is_empty()) {
        // Used as it is written: only a plain absolute path is taken.
        Some(dir) if dir.starts_with('/') && !dir.split('/').any(|part| part == "..") => dir,
        Some(_) => return Err(Stop::HandBack),
        None => js::join(&[&run.machine.tmpdir(), "rustdesk-skill"]),
    };
    if !dir.starts_with('/') {
        return Err(Stop::HandBack);
    }
    match fs::metadata(&dir) {
        Ok(meta) if meta.is_dir() => {}
        // A file there: the step cannot start in it, as bun says in its words.
        Ok(_) => return Err(Stop::HandBack),
        Err(err) if err.kind() == ErrorKind::NotFound && fs::symlink_metadata(&dir).is_err() => {
            if fs::create_dir_all(&dir).is_err() {
                return Err(Stop::HandBack);
            }
        }
        Err(_) => return Err(Stop::HandBack),
    }
    Ok(dir)
}

/// Whether a step acts on the page.
fn acts(sub: &str) -> bool {
    !matches!(sub, "screenshot" | "eval")
}

/// `pw(args, { allowFail })`
pub fn pw(run: &mut Run, args: &[&str], allow_fail: bool) -> R<PwResult> {
    let (cmd, prefix) = pw_cmd(run);
    let session = session::resolve(run)?;
    let mut full = prefix;
    full.push(format!("-s={session}"));
    full.extend(args.iter().map(|arg| arg.to_string()));
    let dir = session_dir(run)?;
    let program = match proc::find(run.machine, &cmd)? {
        Found::Program(program) => program,
        Found::Missing => {
            return Err(Stop::known(
                5,
                format!(
                    "'{cmd}' not found on PATH — install @playwright/cli (`bun add -g @playwright/cli`), run it via `RDS_PW_BIN=\"bunx @playwright/cli\"`, or set RDS_PW_BIN to its path. First launch may download a browser; set RDS_PW_BROWSER=chrome to use system Chrome instead."
                ),
            ))
        }
    };
    if acts(args[0]) {
        run.commit();
    }
    let output = match proc::run(run.machine, &program, &full, Some(&dir), None) {
        Ok(output) => output,
        Err(Failed::Big | Failed::Spawn) => return Err(Stop::HandBack),
    };
    // A signal leaves no status and no error: 0.
    let status = output.status.unwrap_or(0);
    if !allow_fail && status != 0 {
        let what = if !output.stderr.is_empty() {
            output.stderr.as_str()
        } else if !output.stdout.is_empty() {
            output.stdout.as_str()
        } else {
            "unknown error"
        };
        return Err(Stop::error(format!("playwright-cli {} failed (exit {status}): {what}", args[0])));
    }
    Ok(PwResult { status, stdout: output.stdout })
}

/// `parseEvalResult(stdout)`
pub fn parse_eval_result(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)).collect();
    let Some(start) = lines.iter().position(|line| js::trim(line) == "### Result") else {
        return js::trim(stdout).to_string();
    };
    let mut out: Vec<&str> = Vec::new();
    for line in &lines[start + 1..] {
        if line.starts_with("### ") || line.starts_with("```") {
            break;
        }
        out.push(line);
    }
    js::trim(&out.join("\n")).to_string()
}

/// `evalJs(js)`
pub fn eval_js(run: &mut Run, script: &str) -> R<String> {
    Ok(parse_eval_result(&pw(run, &["eval", script], false)?.stdout))
}

pub struct Shot {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// `screenshotPng()`
pub fn screenshot_png(run: &mut Run) -> R<Shot> {
    let abs = js::join(&[&session_dir(run)?, "rds-shot.png"]);
    pw(run, &["screenshot", "--hires", &format!("--filename={abs}")], false)?;
    let candidates = [abs, js::join(&[&run.machine.cwd, "rds-shot.png"]), js::join(&[&session_dir(run)?, "rds-shot.png"])];
    let Some(found) = candidates.iter().find(|path| fs::metadata(path).is_ok()) else {
        return Err(Stop::error(format!("screenshot not found (looked in: {})", candidates.join(", "))));
    };
    // A directory there, or one that cannot be read: bun says why in its words.
    if !fs::metadata(found).is_ok_and(|meta| meta.is_file()) {
        return Err(Stop::HandBack);
    }
    let bytes = fs::read(found).map_err(|_| Stop::HandBack)?;
    if bytes.len() < 24 {
        return Err(Stop::error("not a valid PNG (too short)"));
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Ok(Shot { bytes, width, height })
}

/// JavaScript's truthiness of a parsed value.
fn truthy(value: Option<&Json>) -> bool {
    match value {
        None | Some(Json::Null) => false,
        Some(Json::Bool(b)) => *b,
        Some(Json::Num(n)) => *n != 0.0 && !n.is_nan(),
        Some(Json::Str(s)) => !s.is_empty(),
        Some(Json::Arr(_) | Json::Obj(_)) => true,
    }
}

/// `viewport()`, cached for the process.
pub fn viewport(run: &mut Run) -> R<(f64, f64)> {
    if let Some(cached) = run.viewport {
        return Ok(cached);
    }
    let raw = eval_js(run, "() => ({ w: window.innerWidth, h: window.innerHeight })")?;
    let size = match json::parse(&raw) {
        Ok(object @ Json::Obj(_)) => {
            let (w, h) = (object.get("w"), object.get("h"));
            if truthy(w) && truthy(h) {
                match (w, h) {
                    (Some(Json::Num(w)), Some(Json::Num(h))) => (*w, *h),
                    // A string, a list, …: JavaScript's arithmetic on it is not ported.
                    _ => return Err(Stop::HandBack),
                }
            } else {
                (0.0, 0.0)
            }
        }
        // Not an object: `o.w` is undefined (or o itself is null).
        Ok(_) | Err(json::Parse::Invalid) => (0.0, 0.0),
        Err(json::Parse::Unsure) => return Err(Stop::HandBack),
    };
    run.viewport = Some(size);
    Ok(size)
}

/// `screenshotToCss(sx, sy, pngW, pngH, vpW, vpH)`
pub fn screenshot_to_css(sx: f64, sy: f64, png_w: f64, png_h: f64, vp_w: f64, vp_h: f64) -> (f64, f64) {
    let scale_x = if vp_w > 0.0 && png_w > 0.0 { vp_w / png_w } else { 1.0 };
    let scale_y = if vp_h > 0.0 && png_h > 0.0 { vp_h / png_h } else { 1.0 };
    (js::math_round(sx * scale_x), js::math_round(sy * scale_y))
}

/// `String(x)` of a coordinate, or back.
pub fn coordinate(x: f64) -> R<String> {
    if !x.is_finite() || x.abs() >= 1e15 {
        return Err(Stop::HandBack);
    }
    js::number(x).ok_or(Stop::HandBack)
}

/// `moveScreenshotPx(sx, sy)`
pub fn move_screenshot_px(run: &mut Run, sx: f64, sy: f64) -> R<()> {
    let shot = screenshot_png(run)?;
    let (vw, vh) = viewport(run)?;
    let (width, height) = (f64::from(shot.width), f64::from(shot.height));
    // `vp.w || width`
    let vw = if vw != 0.0 { vw } else { width };
    let vh = if vh != 0.0 { vh } else { height };
    let (x, y) = screenshot_to_css(sx, sy, width, height, vw, vh);
    let (x, y) = (coordinate(x)?, coordinate(y)?);
    pw(run, &["mousemove", &x, &y], false)?;
    Ok(())
}

/// `clickScreenshotPx(sx, sy)`
pub fn click_screenshot_px(run: &mut Run, sx: f64, sy: f64) -> R<()> {
    move_screenshot_px(run, sx, sy)?;
    pw(run, &["mousedown"], false)?;
    pw(run, &["mouseup"], false)?;
    Ok(())
}

/// `currentUrl()`: every error is "".
pub fn current_url(run: &mut Run) -> R<String> {
    match eval_js(run, "() => location.href") {
        Ok(value) => {
            let quote = |c: Option<char>| matches!(c, Some('"' | '\''));
            let start = usize::from(quote(value.chars().next()));
            let end = if value.len() > start && quote(value.chars().last()) { value.len() - 1 } else { value.len() };
            Ok(value[start..end].to_string())
        }
        Err(Stop::HandBack) => Err(Stop::HandBack),
        Err(Stop::Exit(..)) => Ok(String::new()),
    }
}

/// `onRustdesk()`
pub fn on_rustdesk(run: &mut Run) -> R<bool> {
    Ok(current_url(run)?.contains("rustdesk.com/web"))
}

/// `parseKeyChord(chord)`: the key and the modifiers.
pub fn parse_key_chord(chord: &str) -> R<(String, Vec<&'static str>)> {
    let parts: Vec<&str> = chord.split('+').map(js::trim).filter(|part| !part.is_empty()).collect();
    let Some((key, mods)) = parts.split_last() else {
        return Err(Stop::error(format!("empty key chord: {chord}")));
    };
    // toLowerCase is the full Unicode one; the Kelvin sign and the dotted
    // capital I are the only characters it turns into ASCII letters.
    if chord.contains(['\u{212a}', '\u{130}']) {
        return Err(Stop::HandBack);
    }
    let mut mapped = Vec::new();
    for m in mods {
        let lower = m.to_ascii_lowercase();
        // MOD_MAP is a plain object: these two find Object.prototype's.
        if matches!(lower.as_str(), "constructor" | "__proto__") {
            return Err(Stop::HandBack);
        }
        mapped.push(match lower.as_str() {
            "ctrl" | "control" => "Control",
            "alt" | "option" => "Alt",
            "shift" => "Shift",
            "meta" | "cmd" | "win" => "Meta",
            _ => return Err(Stop::error(format!("unknown modifier: {m}"))),
        });
    }
    Ok((normalize_key_name(key)?, mapped))
}

/// `normalizeKeyName(key)`
fn normalize_key_name(key: &str) -> R<String> {
    let lower = key.to_ascii_lowercase();
    if matches!(lower.as_str(), "constructor" | "__proto__") {
        return Err(Stop::HandBack);
    }
    Ok(match lower.as_str() {
        "enter" => "Enter",
        "esc" | "escape" => "Escape",
        "tab" => "Tab",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "space" => "Space",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "backquote" => "Backquote",
        _ => key,
    }
    .to_string())
}

/// `pressKey(chord)`
pub fn press_key(run: &mut Run, chord: &str) -> R<()> {
    let (key, mods) = parse_key_chord(chord)?;
    for m in &mods {
        pw(run, &["keydown", m], false)?;
    }
    pw(run, &["press", &key], false)?;
    for m in mods.iter().rev() {
        pw(run, &["keyup", m], false)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_results_are_unwrapped() {
        let out = "### Result\n\"https://rustdesk.com/web/\"\n### Ran Playwright code\n```js\nawait page.evaluate('...');\n```";
        assert_eq!(parse_eval_result(out), "\"https://rustdesk.com/web/\"");
        assert_eq!(parse_eval_result("### Result\r\n{\r\n  \"w\": 1280,\r\n  \"h\": 720\r\n}\r\n### Ran"), "{\n  \"w\": 1280,\n  \"h\": 720\n}");
        assert_eq!(parse_eval_result("  raw value  "), "raw value");
        assert_eq!(parse_eval_result("x\n  ### Result  \nv\n```"), "v");
    }

    #[test]
    fn coordinates_are_converted() {
        assert_eq!(screenshot_to_css(1000.0, 600.0, 2000.0, 1200.0, 1000.0, 600.0), (500.0, 300.0));
        assert_eq!(screenshot_to_css(640.0, 480.0, 1280.0, 1024.0, 1280.0, 1024.0), (640.0, 480.0));
        assert_eq!(screenshot_to_css(100.0, 100.0, 0.0, 0.0, 0.0, 0.0), (100.0, 100.0));
    }

    #[test]
    fn key_chords() {
        let chord = |c: &str| parse_key_chord(c).ok();
        assert_eq!(chord("Enter"), Some(("Enter".into(), vec![])));
        assert_eq!(chord("ctrl+c"), Some(("c".into(), vec!["Control"])));
        assert_eq!(chord(" Alt + TAB "), Some(("Tab".into(), vec!["Alt"])));
        assert_eq!(chord("ctrl+shift+Esc"), Some(("Escape".into(), vec!["Control", "Shift"])));
        assert!(matches!(parse_key_chord("hyper+x"), Err(Stop::Exit(1, Some(m))) if m == "[rustdesk-skill] error: unknown modifier: hyper"));
        assert!(matches!(parse_key_chord("+ +"), Err(Stop::Exit(1, Some(m))) if m == "[rustdesk-skill] error: empty key chord: + +"));
        assert!(matches!(parse_key_chord("constructor+a"), Err(Stop::HandBack)));
    }
}
