//! The binary's imitations held against the TypeScript's own code, run by
//! bun (rs/parity/yargs.ts):
//!
//! - the argument gate (rs/src/args.rs) against bin/rustdesk-skill.ts's yargs
//!   chain, over some tens of thousands of generated command lines: every
//!   line the gate takes must make yargs call the same command with the
//!   same values, and nothing else;
//! - `toLowerCase` and `trim` for every code point (rs/src/js.rs), but the
//!   ones `lowercase_unsure` hands back;
//! - `os.tmpdir()` under the variables it reads;
//! - `String(x)`, `x.toFixed(2)` and `Math.round(x)` for numbers of the kinds
//!   OCR and coordinates give.
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
    }
    assert!(wrong.is_empty(), "{} code points differ:\n{}", wrong.len(), wrong.iter().take(60).cloned().collect::<Vec<_>>().join("\n"));
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
