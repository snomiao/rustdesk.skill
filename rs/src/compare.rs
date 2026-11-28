//! The binary's imitations held against the TypeScript's own code, run by
//! bun (rs/parity/yargs.ts):
//!
//! - the argument gate (rs/src/args.rs) against bin/rustdesk-skill.ts's yargs
//!   chain, over some tens of thousands of generated command lines: every
//!   line the gate takes must make yargs call the same command with the
//!   same values, and nothing else;
//! - `toLowerCase` and `trim` for every code point (rs/src/js.rs), but the
//!   ones `lowercase_unsure` hands back: the binary answers from the table
//!   rs/unicode.gen.mjs recorded from bun (rs/src/unicode.rs), and this holds
//!   it against the bun at hand, whatever toolchain built the test;
//! - `os.tmpdir()` under the variables it reads;
//! - `String(x)`, `x.toFixed(2)` and `Math.round(x)` for numbers of the kinds
//!   OCR and coordinates give;
//! - and for what connect answers after it has acted, where it can no longer
//!   hand back: `String(x)` for every kind of double, `Number(s)` of
//!   strings, `Number(v)` of JSON values, src/ocr.ts `normalizeBoxes` and
//!   `matchText` themselves on generated OCR output, the order
//!   `(a, b) => a.y - b.y` sorts NaN and infinities in, and the fixed texts
//!   connect looks for found among the scripts whose lower case is not
//!   certain.
//!
//! These need bun and the package's node_modules (`bun install`), as the
//! parity cases do. bun runs in a directory of its own, with HOME there.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("rds-compare-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `bun rs/parity/yargs.ts <args>` in `dir`, with only these variables.
fn bun(dir: &Path, args: &[&str], vars: &[(&str, &str)]) -> String {
    let script = repo().join("rs").join("parity").join("yargs.ts");
    assert!(repo().join("node_modules").join("yargs").is_dir(), "the comparison with the TypeScript needs `bun install` first");
    let mut command = Command::new("bun");
    command.arg(&script).args(args).current_dir(dir).env_clear().env("PATH", std::env::var_os("PATH").unwrap()).env("HOME", dir);
    for (name, value) in vars {
        command.env(name, value);
    }
    let output = command.output().expect("bun, to compare with the TypeScript");
    assert!(output.status.success(), "bun {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

/// Every line `[command, w1, …, wn]` for n up to `most`, the words from `words`.
fn lines(commands: &[&str], words: &[&str], most: usize, out: &mut Vec<Vec<String>>) {
    let mut tails: Vec<Vec<String>> = vec![vec![]];
    let mut all = tails.clone();
    for _ in 0..most {
        tails = tails.iter().flat_map(|tail| words.iter().map(move |w| [tail.clone(), vec![w.to_string()]].concat())).collect();
        all.extend(tails.iter().cloned());
    }
    for command in commands {
        for tail in &all {
            out.push([vec![command.to_string()], tail.clone()].concat());
        }
    }
}

const JUNK: [&str; 14] = ["", "x", "help", "true", "--", "-", "-x", "--x", "--help", "--version", "a b", "'q'", "\"q\"", "-h"];
const NUMBERS: [&str; 24] = [
    "0", "5", "-5", "12", "2.5", "-2.5", "0.5", "1.", ".5", "1e3", "05", "-0", "0x10", "abc", " 5", "5 ", "1.000001", "1.0000001", "123456789012345", "1234567890123456",
    "-0.0", "00", "Infinity", "9007199254740993",
];

fn all_lines() -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let with = |extra: &[&'static str]| -> Vec<&'static str> { JUNK.iter().copied().chain(extra.iter().copied()).collect() };
    lines(&["connect"], &with(&["123456789", "0123", "pw", "5", "-5", "--new", "--remember-password", "--rememberPassword", "--new=true", "--no-new", "false", "-n"]), 3, &mut out);
    lines(&["c"], &with(&["123", "pw", "--new", "--remember-password"]), 2, &mut out);
    lines(&["screenshot", "ss"], &with(&["a.png", "d/a.png", "5", "--file"]), 2, &mut out);
    lines(&["ocr"], &with(&["--json", "--lang", "--lang=en", "ja,en", "en", "--ocr", "paddle", "vision", "--ocr=paddle", "--json=false", "--no-json", "5"]), 3, &mut out);
    lines(&["click"], &with(&["OK", "--nth", "2", "--nth=2", "0", "02", "-1", "2.5", "--contains", "--ocr", "paddle", "--lang", "en", "--lang=", "--ocr=x"]), 3, &mut out);
    let numbers: Vec<&str> = NUMBERS.iter().copied().chain(["", "help", "--", "-", "--x", "-x"]).collect();
    lines(&["click-xy", "move"], &numbers, 3, &mut out);
    lines(&["scroll"], &numbers, 2, &mut out);
    lines(&["type"], &with(&["--force", "Hello@", "--force=true", "--no-force", "false", "5", "-5", "0123", "1e3"]), 3, &mut out);
    lines(&["key", "paste"], &with(&["ctrl+c", "Enter", "5", "-5", "0123", "1e3", "--force"]), 2, &mut out);
    lines(&["status", "disconnect", "list"], &with(&["--json", "5"]), 2, &mut out);
    lines(&["frobnicate", "", "--help", "C", "LIST", "click_xy"], &["x", "list"], 1, &mut out);
    out
}

/// A list of lists of words as JSON.
fn json_lines(lines: &[Vec<String>]) -> String {
    let rows: Vec<String> = lines.iter().map(|line| format!("[{}]", line.iter().map(|w| crate::js::json_string(w)).collect::<Vec<_>>().join(","))).collect();
    format!("[{}]", rows.join(",\n"))
}

#[test]
fn the_gate_takes_only_what_yargs_does_the_same_with() {
    let all = all_lines();
    let scratch = Scratch::new("yargs");
    // In parts, at once: yargs takes a few milliseconds a line.
    let parts = 4;
    let size = all.len().div_ceil(parts);
    let answers: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = all
            .chunks(size)
            .enumerate()
            .map(|(n, chunk)| {
                let file = scratch.0.join(format!("lines-{n}.json"));
                std::fs::write(&file, json_lines(chunk)).unwrap();
                let dir = scratch.0.clone();
                scope.spawn(move || bun(&dir, &["lines", file.to_str().unwrap()], &[]))
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap().lines().map(String::from).collect::<Vec<_>>()).collect()
    });
    assert_eq!(answers.len(), all.len());
    let mut taken = 0;
    let mut wrong = Vec::new();
    for (line, answer) in all.iter().zip(&answers) {
        let Some(command) = crate::args::recognise(line) else { continue };
        taken += 1;
        let ours = crate::js::json_string(&crate::args::describe(&command));
        if *answer != ours {
            wrong.push(format!("{line:?}: yargs {answer}, the binary {ours}"));
        }
    }
    assert!(wrong.is_empty(), "{} of {taken} lines differ:\n{}", wrong.len(), wrong.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    assert!(taken > 1000, "only {taken} lines taken");
    eprintln!("{} lines, {taken} taken by the binary, each as yargs takes it", all.len());
}

#[test]
fn lower_case_and_white_space_as_bun_has_them() {
    let scratch = Scratch::new("chars");
    let listed = bun(&scratch.0, &["chars"], &[]);
    let mut theirs = std::collections::HashMap::new();
    for line in listed.lines() {
        let (code, rest) = line.split_once(' ').unwrap();
        let (lower, space) = rest.rsplit_once(' ').unwrap();
        let lower = match crate::json::parse(lower).unwrap() {
            crate::json::Json::Str(s) => s,
            _ => panic!("{line}"),
        };
        theirs.insert(u32::from_str_radix(code, 16).unwrap(), (lower, space == "true"));
    }
    let mut wrong = Vec::new();
    let mut answered = 0;
    for code in (0..=0x10ffffu32).filter(|c| !(0xd800..=0xdfff).contains(c)) {
        let c = char::from_u32(code).unwrap();
        let (their_lower, their_space) = theirs.get(&code).cloned().unwrap_or((c.to_string(), false));
        if crate::js::is_space(c) != their_space {
            wrong.push(format!("U+{code:04X} space: bun {their_space}"));
        }
        let ours = crate::js::to_lower(&c.to_string());
        if ours.is_some() && ours.as_deref() != Some(their_lower.as_str()) {
            wrong.push(format!("U+{code:04X} lower: bun {their_lower:?}, the binary {ours:?}"));
        }
        answered += usize::from(ours.is_some());
    }
    assert!(
        wrong.is_empty(),
        "{} code points differ (the binary answers from rs/src/unicode.rs: is it what this bun says? `bun rs/unicode.gen.mjs --check`):\n{}",
        wrong.len(),
        wrong.iter().take(60).cloned().collect::<Vec<_>>().join("\n")
    );
    // Every character of bun's Unicode but the few unsure ones.
    assert!(answered > 290_000, "{answered}");
}

#[test]
fn tmpdir_as_bun_has_it() {
    let scratch = Scratch::new("tmpdir");
    let cases: [&[(&str, &str)]; 12] = [
        &[],
        &[("TMPDIR", "/x/")],
        &[("TMPDIR", "")],
        &[("TMP", "/y")],
        &[("TEMP", "/z")],
        &[("TMP", "/y"), ("TEMP", "/z")],
        &[("TMPDIR", ""), ("TMP", "/y/")],
        &[("TMPDIR", "/")],
        &[("TMPDIR", "//")],
        &[("TMPDIR", "/a//")],
        &[("TMPDIR", "rel/")],
        &[("TMPDIR", "/a/b"), ("TMP", "/y")],
    ];
    for vars in cases {
        let theirs = bun(&scratch.0, &["tmpdir"], vars);
        let ours = crate::runtime::tmpdir_from(|name| vars.iter().find(|(n, _)| *n == name).map(|(_, v)| v.to_string()));
        assert_eq!(theirs.trim(), crate::js::json_string(&ours), "{vars:?}");
    }
}

#[test]
fn numbers_as_bun_writes_them() {
    let mut texts: Vec<String> = Vec::new();
    for whole in [0, 1, 2, 9, 10, 99, 123, 1280, 65535, 999999] {
        for fraction in ["", ".005", ".015", ".125", ".375", ".5", ".995", ".9999", ".49999999999999994", ".1", ".2", ".3", ".25", ".75", ".045"] {
            texts.push(format!("{whole}{fraction}"));
            texts.push(format!("-{whole}{fraction}"));
        }
    }
    for n in 0..=1000 {
        texts.push(format!("0.{n:04}"));
        texts.push(format!("{}", n as f64 / 7.0));
        texts.push(format!("{}", n as f64 * 0.1));
    }
    for text in ["1e21", "1e-7", "1.5e-7", "0.000001", "123456789012345", "4503599627370495.5", "1e15", "1e20", "0.1e-6", "5e-324", "1.7976931348623157e308"] {
        texts.push(text.into());
    }
    let scratch = Scratch::new("numbers");
    let file = scratch.0.join("numbers.json");
    std::fs::write(&file, format!("[{}]", texts.iter().map(|t| crate::js::json_string(t)).collect::<Vec<_>>().join(","))).unwrap();
    let answers = bun(&scratch.0, &["numbers", file.to_str().unwrap()], &[]);
    let mut checked = 0;
    for (text, answer) in texts.iter().zip(answers.lines()) {
        let x: f64 = text.parse().unwrap();
        let theirs = match crate::json::parse(answer).unwrap() {
            crate::json::Json::Arr(items) => items,
            _ => panic!("{answer}"),
        };
        let ours = [crate::js::number(x), crate::js::to_fixed_2(x), crate::js::number(crate::js::math_round(x))];
        // Where the binary has no answer, it hands back: nothing to compare.
        for (mine, bun_says) in ours.iter().zip(&theirs) {
            if let Some(mine) = mine {
                assert_eq!(&crate::json::Json::Str(mine.clone()), bun_says, "{text}");
                checked += 1;
            }
        }
    }
    assert!(checked > 3000, "{checked}");
}

// ── what connect answers after it has acted ─────────────────────────────────

/// A small deterministic generator, so that every run tries the same inputs.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

fn json_list<T: AsRef<str>>(items: &[T]) -> String {
    format!("[{}]", items.iter().map(|t| crate::js::json_string(t.as_ref())).collect::<Vec<_>>().join(",\n"))
}

/// `String(x)`, with -0 as such.
fn shown(x: f64) -> String {
    if x == 0.0 && x.is_sign_negative() {
        "-0".into()
    } else {
        crate::js::number_exact(x)
    }
}

fn answers(name: &str, mode: &str, items: &[String]) -> Vec<String> {
    let scratch = Scratch::new(name);
    let file = scratch.0.join("items.json");
    std::fs::write(&file, json_list(items)).unwrap();
    let out = bun(&scratch.0, &[mode, file.to_str().unwrap()], &[]);
    let lines: Vec<String> = out.lines().map(String::from).collect();
    assert_eq!(lines.len(), items.len(), "bun {mode} answered {} of {}", lines.len(), items.len());
    lines
}

fn text_of(answer: &str) -> String {
    match crate::json::parse(answer).unwrap() {
        crate::json::Json::Str(s) => s,
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_number_as_bun_writes_it() {
    let mut dice = Dice(0x5eed_1234);
    let mut xs: Vec<f64> = Vec::new();
    for _ in 0..60_000 {
        let x = f64::from_bits(dice.next());
        if x.is_finite() {
            xs.push(x);
        }
    }
    // Where two shortest spellings are as near: 2^50 to 2^51 in quarters,
    // and numbers of 16 and 17 digits around.
    for _ in 0..20_000 {
        let n = (1u64 << 50) + dice.next() % (1u64 << 50);
        xs.push(n as f64 + *dice.pick(&[0.25, 0.75, 0.5]));
        xs.push(((1u64 << 53) + dice.next() % (1u64 << 60)) as f64);
        xs.push((dice.next() % 10_000_000_000_000_000) as f64 / 10f64.powi(dice.below(20) as i32));
    }
    for x in [1459077941253781.0 + 0.25, 0.1 + 0.2, 5e-324, f64::MAX, f64::MIN_POSITIVE, 1e21, 1e-7, 123456789012345680000.0, 9007199254740993.0] {
        xs.push(x);
        xs.push(-x);
    }
    let texts: Vec<String> = xs.iter().map(|x| format!("{x:e}")).collect();
    let answers = answers("exact", "numbers", &texts);
    let mut wrong = Vec::new();
    for (x, answer) in xs.iter().zip(&answers) {
        let theirs = match crate::json::parse(answer).unwrap() {
            crate::json::Json::Arr(items) => items,
            other => panic!("{other:?}"),
        };
        let ours = [crate::js::number_exact(*x), crate::js::number_exact(crate::js::math_round(*x))];
        for (mine, bun_says) in ours.iter().zip([&theirs[0], &theirs[2]]) {
            if &crate::json::Json::Str(mine.clone()) != bun_says {
                wrong.push(format!("{x:e}: bun {bun_says:?}, the binary {mine}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{} differ:\n{}", wrong.len(), wrong.iter().take(30).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn strings_as_bun_reads_them_as_numbers() {
    let mut items: Vec<String> = [
        "", " ", "0", "-0", "+0", "00", "007", "1.", ".5", ".", "+.5", "-.5e-3", "1e", "1e+", "1e+5", "1E5", "1e5.5", "0x", "0x10", "0X1F", "-0x10", "+0x10", "0xg", "0o17", "0O8", "0b101", "0b2",
        "0x1fffffffffffff", "0x20000000000001", "0x20000000000003", "0x" , "Infinity", "-Infinity", "+Infinity", "infinity", "INFINITY", "NaN", "nan", "inf", "1_000", "1,5", "12abc", "\t12\n", "\u{a0}7\u{3000}",
        "\u{feff}8", "\u{180e}9", "\u{2028}1\u{2029}", "1e400", "-1e400", "1e-400", "4.9e-324", "2.4703282292062327e-324", "2.4703282292062328e-324", "0.30000000000000004", "9007199254740993",
        "123456789012345678901234567890", "٣", "１２",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    items.push(format!("0x{}", "f".repeat(300)));
    items.push(format!("0b1{}", "0".repeat(1100)));
    let alphabet = ["0", "1", "5", "9", ".", "e", "E", "+", "-", "x", "X", "o", "b", "a", "f", " ", "\t", "\n", "\u{a0}", "I", "nfinity", "_", "0x", "e-"];
    let mut dice = Dice(42);
    for _ in 0..40_000 {
        let length = dice.below(9);
        items.push((0..length).map(|_| *dice.pick(&alphabet)).collect());
    }
    let answers = answers("strings", "strings", &items);
    let mut wrong = Vec::new();
    for (item, answer) in items.iter().zip(&answers) {
        let ours = shown(crate::js::string_to_number(item));
        if text_of(answer) != ours {
            wrong.push(format!("{item:?}: bun {answer}, the binary {ours}"));
        }
    }
    assert!(wrong.is_empty(), "{} differ:\n{}", wrong.len(), wrong.iter().take(30).cloned().collect::<Vec<_>>().join("\n"));
}

/// A JSON text of a value of the kinds OCR output and a viewport may hold.
fn value(dice: &mut Dice, depth: usize, names: &[&str]) -> String {
    let scalars = ["null", "true", "false", "0", "-0", "5", "-1.5", "1e21", "1e400", "-1e400", "0.30000000000000004", "123456789012345680000", "\"\"", "\" 7 \"", "\"0x10\"", "\"abc\"", "\"Infinity\"", "\"1e3\"", "\"-0\"", "\"\\t\""];
    match if depth >= 3 { 0 } else { dice.below(6) } {
        0..=2 => dice.pick(&scalars).to_string(),
        3 | 4 => format!("[{}]", (0..dice.below(3)).map(|_| value(dice, depth + 1, names)).collect::<Vec<_>>().join(", ")),
        _ => format!("{{{}}}", (0..dice.below(3)).map(|_| format!("{}: {}", crate::js::json_string(dice.pick(names)), value(dice, depth + 1, names))).collect::<Vec<_>>().join(", ")),
    }
}

#[test]
fn values_as_bun_turns_them_into_numbers() {
    let names = ["toString", "valueOf", "a", "trim", "w", "__proto__"];
    let mut dice = Dice(7);
    let items: Vec<String> = (0..30_000).map(|_| value(&mut dice, 0, &names)).collect();
    let answers = answers("values", "values", &items);
    let mut wrong = Vec::new();
    for (item, answer) in items.iter().zip(&answers) {
        let ours = match crate::jsv::to_number(&crate::json::parse_js(item).unwrap()) {
            Ok(x) => shown(x),
            Err(crate::jsv::Coerce::NoDefault) => "throws No default value".into(),
            Err(crate::jsv::Coerce::Deep) => continue,
        };
        if text_of(answer) != ours {
            wrong.push(format!("{item}: bun {answer}, the binary {ours}"));
        }
    }
    assert!(wrong.is_empty(), "{} differ:\n{}", wrong.len(), wrong.iter().take(30).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn boxes_as_the_typescript_makes_them() {
    use crate::json::Json;
    let names = ["text", "x1", "y1", "x2", "y2", "x1_norm", "y1_norm", "x2_norm", "y2_norm", "confidence", "trim", "toString", "__proto__"];
    let texts = ["\"OK\"", "\"リモートID\"", "\" Connect \"", "\"ΣΥΝΔΕΣΗ\"", "\"\"", "\"é😀\""];
    let mut dice = Dice(99);
    let mut items = Vec::new();
    for _ in 0..20_000 {
        let item = match dice.below(10) {
            0 => value(&mut dice, 0, &names),
            _ => {
                let boxes: Vec<String> = (0..dice.below(4))
                    .map(|_| {
                        if dice.below(8) == 0 {
                            return value(&mut dice, 1, &names);
                        }
                        let mut members = Vec::new();
                        if dice.below(6) != 0 {
                            members.push(format!("\"text\": {}", if dice.below(5) == 0 { value(&mut dice, 2, &names) } else { dice.pick(&texts).to_string() }));
                        }
                        for _ in 0..dice.below(6) {
                            members.push(format!("{}: {}", crate::js::json_string(dice.pick(&names)), value(&mut dice, 2, &names)));
                        }
                        format!("{{{}}}", members.join(", "))
                    })
                    .collect();
                format!("[{}]", boxes.join(", "))
            }
        };
        items.push(item);
    }
    let answers = answers("boxes", "boxes", &items);
    let mut wrong = Vec::new();
    let mut checked = 0;
    for (item, answer) in items.iter().zip(&answers) {
        let raw = crate::json::parse_js(item).unwrap();
        let found = crate::ocr::normalize_js(&raw, 1280.0, 720.0).and_then(|boxes| match crate::ocr::find_any_js(&boxes, &["リモートID"]) {
            Ok(_) => Ok(boxes.clone()),
            Err(stop) => Err(stop),
        });
        let ours = match found {
            Ok(boxes) => {
                let rows: Vec<Json> = boxes
                    .iter()
                    .map(|b| Json::Arr(vec![Json::Str(crate::js::number_exact(b.x)), Json::Str(crate::js::number_exact(b.y)), Json::Str(crate::js::number_exact(b.w)), Json::Str(crate::js::number_exact(b.h)), Json::Str(b.text.clone().unwrap())]))
                    .collect();
                Json::Arr(rows)
            }
            Err(crate::run::Stop::Exit(1, Some(message))) => Json::Str(format!("throws {}", message.strip_prefix("[rustdesk-skill] error: ").unwrap())),
            Err(crate::run::Stop::HandBack) => continue,
            Err(other) => panic!("{other:?}"),
        };
        checked += 1;
        let theirs = crate::json::parse(answer).unwrap();
        if theirs != ours {
            wrong.push(format!("{item}\n    bun {answer}\n    the binary {ours:?}"));
        }
    }
    assert!(wrong.is_empty(), "{} of {checked} differ:\n{}", wrong.len(), wrong.iter().take(20).cloned().collect::<Vec<_>>().join("\n"));
    assert!(checked > 19_000, "{checked}");
}

#[test]
fn sorted_as_bun_sorts() {
    let pools: [&[&str]; 3] = [&["NaN", "Infinity", "-Infinity", "0", "-0", "1", "2", "3", "-1", "5"], &["NaN", "0", "1", "2"], &["0", "-0", "1", "1", "2", "7", "Infinity"]];
    let mut dice = Dice(2024);
    let mut lists: Vec<Vec<&str>> = Vec::new();
    for _ in 0..6_000 {
        let pool = *dice.pick(&pools);
        let n = if dice.below(4) == 0 { dice.below(400) } else { dice.below(40) };
        lists.push((0..n).map(|_| *dice.pick(pool)).collect());
    }
    let scratch = Scratch::new("sort");
    let file = scratch.0.join("lists.json");
    std::fs::write(&file, format!("[{}]", lists.iter().map(|l| json_list(l)).collect::<Vec<_>>().join(",\n"))).unwrap();
    let answers = bun(&scratch.0, &["sort", file.to_str().unwrap()], &[]);
    let mut wrong = Vec::new();
    for (list, answer) in lists.iter().zip(answers.lines()) {
        let mut items: Vec<(usize, f64)> = list.iter().map(|y| crate::js::string_to_number(y)).enumerate().collect();
        crate::jssort::sort(&mut items, |a, b| a.1 - b.1 < 0.0);
        let ours = format!("[{}]", items.iter().map(|(i, _)| i.to_string()).collect::<Vec<_>>().join(","));
        if ours != answer {
            wrong.push(format!("{list:?}: bun {answer}, the binary {ours}"));
        }
    }
    assert_eq!(answers.lines().count(), lists.len());
    assert!(wrong.is_empty(), "{} of {} differ:\n{}", wrong.len(), lists.len(), wrong.iter().take(5).cloned().collect::<Vec<_>>().join("\n"));
}

#[test]
fn fixed_texts_found_as_bun_finds_them() {
    let candidates = ["リモートID", "Remote ID", "Control Remote", "ID", "接続", "Connect", "Connect to", "パスワードが必要です", "Password Required", "Please enter", "パスワード", "記憶する", "Remember", "Remember password", "OK", "确定", "確定", "リモートデスクトップ", "Remote Desktop"];
    let pieces = ["Σ", "σ", "ς", "ΣΣ", "Ⰰ", "ⰰ", "𞤀", "𞤢", "Ꜳ", "ꜳ", "Ɦ", "İ", "K", "k", "I", "D", "i", "d", "O", "o", "K", "C", "onnect", "ONNECT", "Remote", " ", "　", "パスワード", "リモート", "ID", "接続", "a", "Á", "\u{301}"];
    let mut dice = Dice(5);
    let mut pairs = Vec::new();
    for _ in 0..40_000 {
        let t: String = (0..dice.below(7)).map(|_| *dice.pick(&pieces)).collect();
        pairs.push((t, dice.pick(&candidates).to_string()));
    }
    let scratch = Scratch::new("includes");
    let file = scratch.0.join("pairs.json");
    std::fs::write(&file, format!("[{}]", pairs.iter().map(|(t, q)| format!("[{},{}]", crate::js::json_string(t), crate::js::json_string(q))).collect::<Vec<_>>().join(",\n"))).unwrap();
    let answers = bun(&scratch.0, &["includes", file.to_str().unwrap()], &[]);
    let mut wrong = Vec::new();
    for ((t, q), answer) in pairs.iter().zip(answers.lines()) {
        let ours = crate::js::lower_for_fixed(crate::js::trim(t)).contains(&crate::js::lower_for_fixed(crate::js::trim(q)));
        if ours.to_string() != answer {
            wrong.push(format!("{t:?} {q:?}: bun {answer}"));
        }
    }
    assert!(wrong.is_empty(), "{} differ:\n{}", wrong.len(), wrong.iter().take(20).cloned().collect::<Vec<_>>().join("\n"));
}
