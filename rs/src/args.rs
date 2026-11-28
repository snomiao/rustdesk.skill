//! The command lines the binary answers itself. bin/rustdesk-skill.ts parses
//! its arguments with yargs 17 (`.strict()`, `.demandCommand(1)`,
//! `.help()`), which takes far more spellings than anyone types and answers
//! mistakes, `--help`, `--version` and a last word `help` with text of its
//! own. This is not a second yargs: it takes the plain spellings whose
//! meaning is not in doubt and gives `None` for every other line, which then
//! goes to the JavaScript untouched. rs/src/compare.rs holds it against the
//! project's own yargs chain (`buildParser`) over many generated lines.
//!
//! Taken: the command's name or alias first; `--name value` and
//! `--name=value` for an option with a value, `--name` for a flag; the
//! command's positionals, as many as it has; each option once. A value or a
//! positional of text must not start with `-`; a number is written plainly
//! (`12`, `-3`, `2.5`).
//!
//! Not taken, among others: any `-x`, `--no-flag`, `--flag=…`, a value that
//! starts with `-` or is empty, an option yargs does not know (strict mode),
//! a repeated option, `--`, and the words `help` (yargs prints the help when
//! it is the last one), `true` and `false` (yargs takes them as the value of
//! a flag before them).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Backend {
    Paddle,
    Vision,
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Backend::Paddle => "paddle",
            Backend::Vision => "vision",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Connect { id: String, pw: Option<String>, remember: bool, fresh: bool },
    Screenshot { file: Option<String> },
    Ocr { json: bool, lang: String, backend: Option<Backend> },
    Click { text: String, nth: Option<u32>, contains: bool, backend: Option<Backend>, lang: String },
    ClickXy { x: f64, y: f64 },
    Move { x: f64, y: f64 },
    Scroll { dy: f64 },
    Type { text: String, force: bool },
    Key { chord: String },
    Paste { text: String },
    Status,
    Disconnect,
    List,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Flag,
    Text,
    Choice,
    Count,
}

struct Opt {
    long: &'static str,
    kind: Kind,
}

const fn opt(long: &'static str, kind: Kind) -> Opt {
    Opt { long, kind }
}

/// A number as the binary takes it: an integer of at most 15 digits without
/// leading zeros, perhaps negative, perhaps with up to 6 decimals.
fn number(text: &str) -> Option<f64> {
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) = match unsigned.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (unsigned, None),
    };
    let whole_ok = !whole.is_empty() && whole.len() <= 15 && whole.bytes().all(|b| b.is_ascii_digit()) && (whole == "0" || !whole.starts_with('0'));
    let fraction_ok = fraction.is_none_or(|f| !f.is_empty() && f.len() <= 6 && f.bytes().all(|b| b.is_ascii_digit()));
    if !whole_ok || !fraction_ok {
        return None;
    }
    text.parse().ok()
}

/// `--nth`: a whole number from 0, written plainly.
fn count(text: &str) -> Option<u32> {
    if text.is_empty() || text.len() > 9 || !text.bytes().all(|b| b.is_ascii_digit()) || (text.len() > 1 && text.starts_with('0')) {
        return None;
    }
    text.parse().ok()
}

struct Parsed {
    positionals: Vec<String>,
    given: Vec<(&'static str, Option<String>)>,
}

impl Parsed {
    fn flag(&self, long: &str) -> bool {
        self.given.iter().any(|(name, _)| *name == long)
    }

    fn value(&self, long: &str) -> Option<&str> {
        self.given.iter().find(|(name, _)| *name == long).and_then(|(_, value)| value.as_deref())
    }
}

/// `numeric`: which positionals are numbers (they may start with `-`).
fn parse(rest: &[String], opts: &[Opt], min: usize, max: usize, numeric: bool) -> Option<Parsed> {
    let mut parsed = Parsed { positionals: Vec::new(), given: Vec::new() };
    let mut words = rest.iter();
    while let Some(word) = words.next() {
        if matches!(word.as_str(), "help" | "true" | "false" | "--" | "-") {
            return None;
        }
        if let Some(body) = word.strip_prefix("--") {
            let (name, inline) = match body.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (body, None),
            };
            let opt = opts.iter().find(|opt| opt.long == name)?;
            if parsed.given.iter().any(|(given, _)| *given == opt.long) {
                return None;
            }
            let value = match (opt.kind, inline) {
                (Kind::Flag, None) => None,
                (Kind::Flag, Some(_)) => return None,
                (_, Some(value)) => Some(value.to_string()),
                (_, None) => Some(words.next()?.clone()),
            };
            if let Some(value) = &value {
                if value.is_empty() || value.starts_with(['-', '"', '\'']) || matches!(value.as_str(), "help" | "true" | "false") {
                    return None;
                }
                let fits = match opt.kind {
                    Kind::Choice => matches!(value.as_str(), "paddle" | "vision"),
                    Kind::Count => count(value).is_some(),
                    _ => true,
                };
                if !fits {
                    return None;
                }
            }
            parsed.given.push((opt.long, value));
            continue;
        }
        if word.starts_with('-') && !(numeric && number(word).is_some()) {
            return None;
        }
        if numeric && number(word).is_none() {
            return None;
        }
        if parsed.positionals.len() == max {
            return None;
        }
        parsed.positionals.push(word.clone());
    }
    (parsed.positionals.len() >= min).then_some(parsed)
}

fn backend(parsed: &Parsed) -> Option<Backend> {
    match parsed.value("ocr") {
        Some("paddle") => Some(Backend::Paddle),
        Some("vision") => Some(Backend::Vision),
        _ => None,
    }
}

pub fn recognise(args: &[String]) -> Option<Command> {
    let (name, rest) = args.split_first()?;
    let command = match name.as_str() {
        "connect" | "c" => {
            let p = parse(rest, &[opt("remember-password", Kind::Flag), opt("new", Kind::Flag)], 1, 2, false)?;
            Command::Connect {
                id: p.positionals[0].clone(),
                pw: p.positionals.get(1).cloned(),
                remember: p.flag("remember-password"),
                fresh: p.flag("new"),
            }
        }
        "screenshot" | "ss" => {
            let p = parse(rest, &[], 0, 1, false)?;
            Command::Screenshot { file: p.positionals.first().cloned() }
        }
        "ocr" => {
            let p = parse(rest, &[opt("json", Kind::Flag), opt("lang", Kind::Text), opt("ocr", Kind::Choice)], 0, 0, false)?;
            Command::Ocr { json: p.flag("json"), lang: p.value("lang").unwrap_or("ja,en").to_string(), backend: backend(&p) }
        }
        "click" => {
            let opts = [opt("nth", Kind::Count), opt("contains", Kind::Flag), opt("ocr", Kind::Choice), opt("lang", Kind::Text)];
            let p = parse(rest, &opts, 1, 1, false)?;
            Command::Click {
                text: p.positionals[0].clone(),
                nth: p.value("nth").and_then(count),
                contains: p.flag("contains"),
                backend: backend(&p),
                lang: p.value("lang").unwrap_or("ja,en").to_string(),
            }
        }
        "click-xy" | "move" => {
            let p = parse(rest, &[], 2, 2, true)?;
            let (x, y) = (number(&p.positionals[0])?, number(&p.positionals[1])?);
            if name == "move" {
                Command::Move { x, y }
            } else {
                Command::ClickXy { x, y }
            }
        }
        "scroll" => {
            let p = parse(rest, &[], 1, 1, true)?;
            Command::Scroll { dy: number(&p.positionals[0])? }
        }
        "type" => {
            let p = parse(rest, &[opt("force", Kind::Flag)], 1, 1, false)?;
            Command::Type { text: p.positionals[0].clone(), force: p.flag("force") }
        }
        "key" | "paste" => {
            let p = parse(rest, &[], 1, 1, false)?;
            let text = p.positionals[0].clone();
            if name == "key" {
                Command::Key { chord: text }
            } else {
                Command::Paste { text }
            }
        }
        "status" | "disconnect" | "list" => {
            parse(rest, &[], 0, 0, false)?;
            match name.as_str() {
                "status" => Command::Status,
                "disconnect" => Command::Disconnect,
                _ => Command::List,
            }
        }
        _ => return None,
    };
    Some(command)
}

/// What the command was given, in the form rs/parity/yargs.ts writes for the
/// call yargs's parse of the same line makes (rs/src/compare.rs).
#[cfg(test)]
pub fn describe(command: &Command) -> String {
    use crate::js::json_string as s;
    let num = |x: f64| crate::js::number(x).unwrap();
    let backend = |b: &Option<Backend>| b.map(|b| format!(",\"backend\":\"{}\"", b.name())).unwrap_or_default();
    match command {
        Command::Connect { id, pw, remember, fresh } => {
            let pw = pw.as_ref().map(|pw| format!(",\"pw\":{}", s(pw))).unwrap_or_default();
            format!("connect {{\"id\":{}{pw},\"rememberPassword\":{remember},\"fresh\":{fresh}}}", s(id))
        }
        Command::Screenshot { file } => format!("screenshot {}", file.as_ref().map(|f| s(f)).unwrap_or("null".into())),
        Command::Ocr { json, lang, backend: b } => format!("ocr {{\"json\":{json}{},\"lang\":{}}}", backend(b), s(lang)),
        Command::Click { text, nth, contains, backend: b, lang } => {
            let nth = nth.map(|n| format!("\"nth\":{n},")).unwrap_or_default();
            format!("clickText {} {{{nth}\"contains\":{contains}{},\"lang\":{}}}", s(text), backend(b), s(lang))
        }
        Command::ClickXy { x, y } => format!("clickXY {} {}", num(*x), num(*y)),
        Command::Move { x, y } => format!("move {} {}", num(*x), num(*y)),
        Command::Scroll { dy } => format!("scroll {}", num(*dy)),
        Command::Type { text, force } => format!("type {} {{\"force\":{force}}}", s(text)),
        Command::Key { chord } => format!("key {}", s(chord)),
        Command::Paste { text } => format!("paste {}", s(text)),
        Command::Status => "status".into(),
        Command::Disconnect => "disconnect".into(),
        Command::List => "listRecent".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(words: &[&str]) -> Option<Command> {
        recognise(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn plain_lines_are_taken() {
        assert_eq!(line(&["list"]), Some(Command::List));
        assert_eq!(line(&["scroll", "-300"]), Some(Command::Scroll { dy: -300.0 }));
        assert_eq!(line(&["c", "0123", "pw", "--new"]), Some(Command::Connect { id: "0123".into(), pw: Some("pw".into()), remember: false, fresh: true }));
        assert_eq!(
            line(&["click", "--nth=2", "OK", "--ocr", "vision", "--lang", "en"]),
            Some(Command::Click { text: "OK".into(), nth: Some(2), contains: false, backend: Some(Backend::Vision), lang: "en".into() })
        );
        assert_eq!(line(&["move", "1.5", "-2"]), Some(Command::Move { x: 1.5, y: -2.0 }));
    }

    #[test]
    fn everything_else_goes_back() {
        for words in [
            &[][..], &["--help"], &["list", "x"], &["list", "--help"], &["list", "help"], &["scroll"], &["scroll", "1e3"], &["scroll", "05"],
            &["scroll", "--", "5"], &["type", "-x"], &["type", "-"], &["type", "help"], &["type", "x", "--force", "false"], &["type", "x", "--no-force"],
            &["type", "x", "--force=true"], &["type", "x", "--force", "--force"], &["click", "OK", "--nth", "2.5"], &["click", "OK", "--nth"],
            &["click", "OK", "--nth", "-1"], &["click", "OK", "--nth", "02"], &["click", "OK", "--ocr", "tess"], &["ocr", "--lang="],
            &["ocr", "--lang", "a", "--lang", "b"], &["connect"], &["c", "1", "--rememberPassword"], &["c", "-5"], &["move", "5"], &["status", "--json"],
            &["list", "--version"], &["frobnicate"], &["key", "a", "b"], &["key", "-1"], &["ocr", "--lang", "-x"], &["c", "1", "-n"],
        ] {
            assert_eq!(line(words), None, "{words:?}");
        }
    }
}
