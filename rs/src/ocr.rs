//! src/ocr.ts and the OCR half of src/screen.ts: the screenshot goes to
//! `python3 <package>/ocr/<script> <png> <lang>`, and what it prints becomes
//! boxes in screenshot pixels. The Python scripts stay as they are.

use std::fs;

use crate::args::Backend;
use crate::js;
use crate::json::{self, Json};
use crate::proc::{self, Failed, Found};
use crate::pw;
use crate::run::{Run, Stop, R};

#[derive(Debug, Clone, PartialEq)]
pub struct OcrBox {
    pub text: String,
    pub conf: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// `resolveBackend(flag)`
pub fn resolve_backend(run: &Run, flag: Option<Backend>) -> R<Backend> {
    if let Some(backend) = flag {
        return Ok(backend);
    }
    let choice = js::to_lower(&run.machine.var("RDS_OCR").unwrap_or_default()).ok_or(Stop::HandBack)?;
    Ok(match choice.as_str() {
        "paddle" => Backend::Paddle,
        "vision" => Backend::Vision,
        _ if cfg!(target_os = "macos") => Backend::Vision,
        _ => Backend::Paddle,
    })
}

/// `langArg(backend, lang)`. Its maps are plain objects, so a language named
/// like a member of Object.prototype finds that member: back.
pub fn lang_arg(backend: Backend, lang: &str) -> R<String> {
    if backend == Backend::Vision {
        let mut codes = Vec::new();
        for part in lang.split(',') {
            let part = js::trim(part);
            if js::is_object_member(part) {
                return Err(Stop::HandBack);
            }
            let code = match part {
                "ja" => "ja",
                "en" => "en-US",
                "zh" | "ch" => "zh-Hans",
                other => other,
            };
            if !code.is_empty() {
                codes.push(code);
            }
        }
        let joined = codes.join(",");
        return Ok(if joined.is_empty() { "ja,en-US".into() } else { joined });
    }
    let first = lang.split(',').next().unwrap_or("");
    let first = js::trim(if first.is_empty() { "ch" } else { first });
    if js::is_object_member(first) {
        return Err(Stop::HandBack);
    }
    Ok(match first {
        "en" => "en",
        "ja" => "japan",
        "zh" | "ch" => "ch",
        other => other,
    }
    .to_string())
}

/// `r[name] ?? 0` for a coordinate: a number, or nothing.
fn coordinate(object: &Json, name: &str) -> R<f64> {
    match object.get(name) {
        None | Some(Json::Null) => Ok(0.0),
        Some(Json::Num(n)) => Ok(*n),
        // JavaScript's arithmetic on anything else is not ported.
        Some(_) => Err(Stop::HandBack),
    }
}

/// A rounded coordinate the binary can print as JavaScript does.
fn plain(x: f64) -> R<f64> {
    let x = js::math_round(x);
    if !x.is_finite() || x.abs() >= 1e15 {
        return Err(Stop::HandBack);
    }
    Ok(x)
}

/// `normalizeBoxes(raw, pngW, pngH)`
pub fn normalize_boxes(raw: &Json, png_w: f64, png_h: f64) -> R<Vec<OcrBox>> {
    let Json::Arr(items) = raw else { return Err(Stop::HandBack) };
    let mut out = Vec::new();
    for item in items {
        if !matches!(item, Json::Obj(_)) {
            return Err(Stop::HandBack);
        }
        let (x1, y1, x2, y2) = match item.get("x1_norm") {
            Some(Json::Null) | None => (coordinate(item, "x1")?, coordinate(item, "y1")?, coordinate(item, "x2")?, coordinate(item, "y2")?),
            Some(Json::Num(x1)) => (*x1 * png_w, coordinate(item, "y1_norm")? * png_h, coordinate(item, "x2_norm")? * png_w, coordinate(item, "y2_norm")? * png_h),
            Some(_) => return Err(Stop::HandBack),
        };
        let Some(Json::Str(text)) = item.get("text") else { return Err(Stop::HandBack) };
        let conf = match item.get("confidence") {
            Some(Json::Num(n)) if n.is_finite() => *n,
            Some(Json::Num(_)) => return Err(Stop::HandBack),
            _ => 1.0,
        };
        out.push(OcrBox { text: text.clone(), conf, x: plain(x1)?, y: plain(y1)?, w: plain(x2 - x1)?, h: plain(y2 - y1)? });
    }
    Ok(out)
}

/// `runOcr(pngPath, opts)`
fn run_ocr(run: &mut Run, png: &str, backend: Backend, lang: &str, png_w: f64, png_h: f64) -> R<Vec<OcrBox>> {
    let script_name = if backend == Backend::Vision { "ocr-mac.py" } else { "ocr.py" };
    let script = js::join(&[&run.machine.package_root, "ocr", script_name]);
    if fs::metadata(&script).is_err() {
        return Err(Stop::error(format!("OCR script missing: {script}")));
    }
    let lang = lang_arg(backend, if lang.is_empty() { "ja,en" } else { lang })?;
    let name = backend.name();
    let program = match proc::find(run.machine, "python3")? {
        Found::Program(program) => program,
        Found::Missing => return Err(Stop::error(format!("OCR ({name}) failed: Executable not found in $PATH: \"python3\""))),
    };
    let output = match proc::run(run.machine, &program, &[script, png.to_string(), lang], None, None) {
        Ok(output) => output,
        Err(Failed::Big | Failed::Spawn) => return Err(Stop::HandBack),
    };
    if output.status != Some(0) {
        let what = if output.stderr.is_empty() { "unknown error" } else { &output.stderr };
        return Err(Stop::error(format!("OCR ({name}) failed: {what}")));
    }
    let text = if output.stdout.is_empty() { "[]" } else { &output.stdout };
    match json::parse(text) {
        Ok(raw) => normalize_boxes(&raw, png_w, png_h),
        Err(json::Parse::Invalid) => {
            let head = js::slice_utf16(&output.stdout, 200).ok_or(Stop::HandBack)?;
            Err(Stop::error(format!("OCR ({name}) produced non-JSON output: {head}")))
        }
        Err(json::Parse::Unsure) => Err(Stop::HandBack),
    }
}

/// `ocr(opts)` of src/screen.ts: a screenshot, written to
/// <tmpdir>/rds-ocr-<pid>.png (and left there), then OCR.
pub fn ocr(run: &mut Run, backend: Option<Backend>, lang: &str) -> R<Vec<OcrBox>> {
    let shot = pw::screenshot_png(run)?;
    let png = js::join(&[&run.machine.tmpdir(), &format!("rds-ocr-{}.png", std::process::id())]);
    if !png.starts_with('/') {
        return Err(Stop::HandBack);
    }
    let existed = fs::symlink_metadata(&png).is_ok();
    fs::write(&png, &shot.bytes).map_err(|_| Stop::HandBack)?;
    if !existed {
        run.made(png.clone().into());
    }
    let backend = match backend {
        Some(backend) => backend,
        None => resolve_backend(run, None)?,
    };
    run_ocr(run, &png, backend, lang, f64::from(shot.width), f64::from(shot.height))
}

/// `matchText(boxes, query, opts)`: the box, how many matched, whether that
/// is ambiguous.
pub fn match_text<'b>(boxes: &'b [OcrBox], query: &str, contains: bool, nth: Option<u32>) -> R<(Option<&'b OcrBox>, usize, bool)> {
    let q = js::to_lower(js::trim(query)).ok_or(Stop::HandBack)?;
    let mut matches = Vec::new();
    for b in boxes {
        let t = js::to_lower(js::trim(&b.text)).ok_or(Stop::HandBack)?;
        if if contains { t.contains(&q) } else { t == q } {
            matches.push(b);
        }
    }
    if matches.is_empty() {
        return Ok((None, 0, false));
    }
    if let Some(n) = nth {
        return Ok((n.checked_sub(1).and_then(|i| matches.get(i as usize)).copied(), matches.len(), false));
    }
    Ok((Some(matches[0]), matches.len(), matches.len() > 1))
}

/// `boxCenter(b)`
pub fn box_center(b: &OcrBox) -> (f64, f64) {
    (js::math_round(b.x + b.w / 2.0), js::math_round(b.y + b.h / 2.0))
}

/// `findAny(boxes, candidates)` of src/connect.ts: substring matches, the
/// candidates in order.
pub fn find_any<'b>(boxes: &'b [OcrBox], candidates: &[&str]) -> R<Option<&'b OcrBox>> {
    for candidate in candidates {
        if let (Some(found), _, _) = match_text(boxes, candidate, true, None)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// `JSON.stringify(boxes, null, 2)`
pub fn boxes_json(boxes: &[OcrBox]) -> R<String> {
    if boxes.is_empty() {
        return Ok("[]".into());
    }
    let num = |x: f64| js::number(x).ok_or(Stop::HandBack);
    let mut items = Vec::new();
    for b in boxes {
        items.push(format!(
            "  {{\n    \"text\": {},\n    \"conf\": {},\n    \"x\": {},\n    \"y\": {},\n    \"w\": {},\n    \"h\": {}\n  }}",
            js::json_string(&b.text),
            num(b.conf)?,
            num(b.x)?,
            num(b.y)?,
            num(b.w)?,
            num(b.h)?
        ));
    }
    Ok(format!("[\n{}\n]", items.join(",\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(text: &str, x: f64, y: f64) -> OcrBox {
        OcrBox { text: text.into(), conf: 1.0, x, y, w: 20.0, h: 10.0 }
    }

    #[test]
    fn languages_for_each_backend() {
        assert_eq!(lang_arg(Backend::Vision, "ja,en").unwrap(), "ja,en-US");
        assert_eq!(lang_arg(Backend::Vision, " zh , ,ko").unwrap(), "zh-Hans,ko");
        assert_eq!(lang_arg(Backend::Vision, ",").unwrap(), "ja,en-US");
        assert_eq!(lang_arg(Backend::Paddle, "ja,en").unwrap(), "japan");
        assert_eq!(lang_arg(Backend::Paddle, ",en").unwrap(), "ch");
        assert_eq!(lang_arg(Backend::Paddle, "  ").unwrap(), "");
        assert!(matches!(lang_arg(Backend::Paddle, "toString"), Err(Stop::HandBack)));
        assert!(matches!(lang_arg(Backend::Vision, "en,constructor"), Err(Stop::HandBack)));
    }

    #[test]
    fn boxes_from_both_backends() {
        let raw = json::parse(r#"[{"text": "OK", "confidence": 0.9, "x1": 10, "y1": 20, "x2": 50, "y2": 40}, {"text": "Connect", "confidence": 0.8, "x1_norm": 0.5, "y1_norm": 0.25, "x2_norm": 0.6, "y2_norm": 0.3}, {"text": "n"}]"#).unwrap();
        let boxes = normalize_boxes(&raw, 1000.0, 800.0).unwrap();
        assert_eq!(boxes[0], OcrBox { text: "OK".into(), conf: 0.9, x: 10.0, y: 20.0, w: 40.0, h: 20.0 });
        assert_eq!(boxes[1], OcrBox { text: "Connect".into(), conf: 0.8, x: 500.0, y: 200.0, w: 100.0, h: 40.0 });
        assert_eq!(boxes[2], OcrBox { text: "n".into(), conf: 1.0, x: 0.0, y: 0.0, w: 0.0, h: 0.0 });
        for text in [r#"{}"#, r#"[null]"#, r#"[{"x1": 1}]"#, r#"[{"text": "a", "x1": "1"}]"#, r#"[{"text": "a", "x1_norm": "0.5"}]"#, r#"[{"text": 5}]"#] {
            assert!(matches!(normalize_boxes(&json::parse(text).unwrap(), 10.0, 10.0), Err(Stop::HandBack)), "{text}");
        }
    }

    #[test]
    fn matching_text() {
        let boxes = [b("OK", 0.0, 0.0), b("Cancel", 30.0, 0.0), b("OK", 0.0, 50.0)];
        let (found, count, ambiguous) = match_text(&boxes, "ok", false, None).unwrap();
        assert_eq!((found.unwrap().y, count, ambiguous), (0.0, 2, true));
        assert_eq!(match_text(&boxes, "OK", false, Some(2)).unwrap().0.unwrap().y, 50.0);
        assert!(match_text(&boxes, "OK", false, Some(3)).unwrap().0.is_none());
        assert!(match_text(&boxes, "OK", false, Some(0)).unwrap().0.is_none());
        assert_eq!(match_text(&boxes, "anc", true, None).unwrap().0.unwrap().text, "Cancel");
        assert!(match_text(&boxes, "nope", false, None).unwrap().0.is_none());
        assert_eq!(box_center(&OcrBox { text: "x".into(), conf: 1.0, x: 10.0, y: 20.0, w: 40.0, h: 20.0 }), (30.0, 30.0));
        assert_eq!(box_center(&OcrBox { text: "x".into(), conf: 1.0, x: 10.0, y: 20.0, w: 5.0, h: 3.0 }), (13.0, 22.0));
    }
}
