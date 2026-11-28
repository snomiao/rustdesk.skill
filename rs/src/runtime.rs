//! The machine as bin/rustdesk-skill.ts finds it, decided before anything is
//! printed, started or changed.
//!
//! "The TypeScript" is `bun bin/rustdesk-skill.ts`, and before its first
//! line bun
//!
//! - takes options and switches from variables named for it, for Node, for
//!   the libraries inside it and for the dynamic loader that starts it
//!   (BUN_*, NODE_*, MIMALLOC_*, ICU_*, LD_*, DYLD_*): any of them but
//!   BUN_INSTALL, which bun's installer puts in every profile, hands back.
//!   So does FORCE_COLOR, which makes console.error red on a pipe;
//! - reads `bunfig.toml` in the current directory, `~/.bunfig.toml` and
//!   `$XDG_CONFIG_HOME/.bunfig.toml`: any of them there hands back;
//! - reads package.json, tsconfig.json and jsconfig.json in the current
//!   directory and in every directory above it, and says on stderr what it
//!   does not like in them: one that is not plain (`plain_project_file`)
//!   hands back;
//! - loads `.env.development.local`, `.env.local`, `.env.development` and
//!   `.env` of the current directory into `process.env` (a variable that is
//!   set wins; among the files, that order wins), in a syntax of its own.
//!   Every variable of the program can come from there (RDS_SESSION_NAME in
//!   ./.env.local is the way the project means a session to be pinned), and
//!   every program it starts gets them all. A file in the plain part of that
//!   syntax is read (`read_plain_dotenv`); anything else hands back, as does
//!   a file that sets a variable that steers bun or the lookup of programs.
//!
//! bun keeps the environment as text: a variable whose name or value is not
//! UTF-8 hands back.
//!
//! While it runs, bun prints console.error in red when stdout or stderr is a
//! terminal (`Machine::terminal`); a command that may write to stderr hands
//! back there.

use std::fs;
use std::io::{ErrorKind, IsTerminal};
use std::path::Path;

use crate::js;
use crate::json::{self, Json};
use crate::HandBack;

pub struct Machine {
    /// What bun adds to `process.env` from the `.env` files.
    pub added: Vec<(String, String)>,
    /// `process.cwd()`
    pub cwd: String,
    /// The package's directory, as `import.meta.url` of its modules has it.
    pub package_root: String,
    /// Whether stdout or stderr is a terminal.
    pub terminal: bool,
}

impl Machine {
    /// `process.env[name]`
    pub fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().or_else(|| self.added.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone()))
    }

    /// `os.homedir()`, where it is simply HOME.
    pub fn home(&self) -> Result<String, HandBack> {
        std::env::var("HOME").ok().filter(|home| !home.is_empty()).ok_or(HandBack)
    }

    /// `os.tmpdir()`
    pub fn tmpdir(&self) -> String {
        tmpdir_from(|name| self.var(name))
    }
}

/// `os.tmpdir()`: TMPDIR, TMP or TEMP (the first that is not empty), else
/// /tmp, without one trailing slash (held against bun by rs/src/compare.rs).
pub fn tmpdir_from(var: impl Fn(&str) -> Option<String>) -> String {
    let mut dir = ["TMPDIR", "TMP", "TEMP"].iter().find_map(|name| var(name).filter(|value| !value.is_empty())).unwrap_or_else(|| "/tmp".into());
    if dir.len() > 1 && dir.ends_with('/') {
        dir.pop();
    }
    dir
}

/// Variables bun, Node's layer, its allocator, ICU or the dynamic loader
/// read for themselves, and FORCE_COLOR.
fn for_the_runtime(upper: &str) -> bool {
    upper != "BUN_INSTALL"
        && (["BUN_", "NODE_", "MIMALLOC_", "ICU_", "LD_", "DYLD_"].iter().any(|start| upper.starts_with(start)) || upper == "FORCE_COLOR")
}

/// Variables that would change what bun or the lookup of programs does if a
/// `.env` file set them, and those of this binary itself.
fn changes_the_runtime(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    for_the_runtime(&upper)
        || matches!(upper.as_str(), "HOME" | "USERPROFILE" | "PATH" | "PATHEXT" | "TMPDIR" | "TMP" | "TEMP" | "TZ" | "NO_COLOR" | "BUN_INSTALL")
        || upper.starts_with("XDG_")
        || upper.starts_with("RUSTDESK_SKILL_")
        || upper.starts_with("RUSTIFY_")
}

/// The files bun loads when NODE_ENV is unset, strongest first.
const DOTENV_FILES: [&str; 4] = [".env.development.local", ".env.local", ".env.development", ".env"];

fn absent(path: &Path) -> bool {
    matches!(fs::symlink_metadata(path), Err(err) if matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory))
}

/// A member name given twice, at any depth: bun warns about it.
fn has_duplicate(value: &Json) -> bool {
    match value {
        Json::Obj(members) => {
            members.iter().enumerate().any(|(i, (key, _))| members[..i].iter().any(|(other, _)| other == key)) || members.iter().any(|(_, value)| has_duplicate(value))
        }
        Json::Arr(items) => items.iter().any(has_duplicate),
        _ => false,
    }
}

/// A package.json, tsconfig.json or jsconfig.json about which bun says
/// nothing on stderr, as far as the binary is sure: strict JSON (no
/// comments, no trailing commas, no byte order mark), an object, no member
/// given twice; in package.json a `type` of "module" or "commonjs" if any,
/// and no `exports`, `imports` or `browser` (whose shapes bun checks); in
/// the others no `extends` (another file to read), and `compilerOptions`,
/// if any, an object without `paths`, `jsxFactory`, `jsxFragmentFactory` or
/// `importsNotUsedAsValues` (whose values bun checks). Everything else goes
/// back, valid or not: bun says on stderr what it does not like (a syntax
/// error, `Duplicate key`, a `type` it does not know, a bad `paths`
/// pattern), and the `runtime: project file` parity cases hold the rest.
pub fn plain_project_file(name: &str, bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else { return false };
    let Ok(root @ Json::Obj(_)) = json::parse(text) else { return false };
    if has_duplicate(&root) {
        return false;
    }
    if name == "package.json" {
        let type_ok = match root.get("type") {
            None => true,
            Some(Json::Str(kind)) => kind == "module" || kind == "commonjs",
            Some(_) => false,
        };
        return type_ok && ["exports", "imports", "browser"].iter().all(|key| root.get(key).is_none());
    }
    if root.get("extends").is_some() {
        return false;
    }
    match root.get("compilerOptions") {
        None => true,
        Some(options @ Json::Obj(_)) => ["paths", "jsxFactory", "jsxFragmentFactory", "importsNotUsedAsValues"].iter().all(|key| options.get(key).is_none()),
        Some(_) => false,
    }
}

/// bun cannot load a program from a path with `?` or `\` in it.
fn loadable(path: &str) -> bool {
    !path.contains(['?', '\\'])
}

pub fn machine() -> Result<Machine, HandBack> {
    // Compared on Linux and macOS only.
    if cfg!(not(any(target_os = "linux", target_os = "macos"))) {
        return Err(HandBack);
    }
    for (name, value) in std::env::vars_os() {
        let (Some(name), Some(_)) = (name.to_str(), value.to_str()) else { return Err(HandBack) };
        if for_the_runtime(&name.to_ascii_uppercase()) {
            return Err(HandBack);
        }
    }
    let root = crate::js_fallback::package_root().ok_or(HandBack)?;
    let root = fs::canonicalize(root).map_err(|_| HandBack)?.into_os_string().into_string().map_err(|_| HandBack)?;
    if !loadable(&root) {
        return Err(HandBack);
    }
    let cwd_path = std::env::current_dir().map_err(|_| HandBack)?;
    let cwd = cwd_path.to_str().ok_or(HandBack)?.to_string();
    let home = std::env::var("HOME").ok().filter(|home| !home.is_empty()).ok_or(HandBack)?;

    let mut bunfigs = vec![cwd_path.join("bunfig.toml"), Path::new(&home).join(".bunfig.toml")];
    if let Ok(config) = std::env::var("XDG_CONFIG_HOME") {
        bunfigs.push(Path::new(&config).join(".bunfig.toml"));
    }
    if !bunfigs.iter().all(|file| absent(file)) {
        return Err(HandBack);
    }
    for dir in cwd_path.ancestors() {
        for name in ["package.json", "tsconfig.json", "jsconfig.json"] {
            let path = dir.join(name);
            if absent(&path) {
                continue;
            }
            let plain = fs::metadata(&path).is_ok_and(|meta| meta.is_file()) && fs::read(&path).is_ok_and(|bytes| plain_project_file(name, &bytes));
            if !plain {
                return Err(HandBack);
            }
        }
    }

    let mut added: Vec<(String, String)> = Vec::new();
    for file in DOTENV_FILES {
        for (key, value) in read_plain_dotenv(&cwd_path.join(file))? {
            match std::env::var_os(&key) {
                // Set and not empty: bun keeps it. Set but empty: not compared.
                Some(set) if set.is_empty() => return Err(HandBack),
                Some(_) => {}
                None if added.iter().any(|(k, _)| *k == key) => {}
                None => added.push((key, value)),
            }
        }
    }

    let terminal = std::io::stdout().is_terminal() || std::io::stderr().is_terminal();
    Ok(Machine { added, cwd, package_root: root, terminal })
}

// ── the files bun loads ──────────────────────────────────────────────────────

/// Reads one of the files bun loads, if it is written in the subset of bun's
/// syntax where a value is simply the text that stands there:
///
/// - blank lines, and lines starting with `#`;
/// - `KEY=value`: KEY made of letters, digits and `_`, no space before `=`.
///   The value runs to the end of the line or to the first `#`, without the
///   spaces and tabs around it;
/// - `KEY="value"` and `KEY='value'`: the text between the quotes, kept as it
///   is. After the closing quote only spaces and a `#` comment may follow.
///
/// A value, quoted or not, must hold no quote character, `$`, backslash
/// (inside quotes) or control character: those are where bun starts to
/// interpret (expansion, escapes, values over several lines). Anything else —
/// `export`, indentation, a line without `=`, a BOM, the same key twice in a
/// file, a variable that steers bun itself — is `HandBack`.
///
/// The rules were read off bun 1.4.0 by the genvx and qqdocs ports, and are
/// held here by the `runtime: .env` parity cases.
fn read_plain_dotenv(path: &Path) -> Result<Vec<(String, String)>, HandBack> {
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(err) if matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            // A link to nowhere is something.
            return if absent(path) { Ok(Vec::new()) } else { Err(HandBack) };
        }
        Err(_) => return Err(HandBack),
    };
    if !meta.is_file() {
        return Err(HandBack);
    }
    let bytes = fs::read(path).map_err(|_| HandBack)?;
    parse_plain_dotenv(std::str::from_utf8(&bytes).map_err(|_| HandBack)?)
}

const BLANKS: [char; 2] = [' ', '\t'];

/// True for text bun takes literally wherever it stands.
fn literal(text: &str) -> bool {
    text.chars().all(|c| !c.is_control() && !matches!(c, '"' | '\'' | '`' | '$' | '\\') && (c == ' ' || !js::is_space(c)))
}

/// The value bun gives to what follows `=` on a line.
fn plain_value(raw: &str) -> Result<&str, HandBack> {
    let raw = raw.trim_start_matches(BLANKS);
    let (value, rest) = match raw.chars().next() {
        Some(quote @ ('"' | '\'')) => {
            let (inner, rest) = raw[1..].split_once(quote).ok_or(HandBack)?;
            (inner, rest.trim_start_matches(BLANKS))
        }
        _ => match raw.find('#') {
            Some(at) => (raw[..at].trim_end_matches(BLANKS), &raw[at..]),
            None => (raw.trim_end_matches(BLANKS), ""),
        },
    };
    let rest_is_comment = rest.is_empty() || (rest.starts_with('#') && literal(rest));
    if literal(value) && rest_is_comment {
        Ok(value)
    } else {
        Err(HandBack)
    }
}

fn parse_plain_dotenv(text: &str) -> Result<Vec<(String, String)>, HandBack> {
    let mut vars: Vec<(String, String)> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.contains('\r') {
            return Err(HandBack);
        }
        if line.trim_matches(BLANKS).is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, raw) = line.split_once('=').ok_or(HandBack)?;
        let key_is_plain =
            key.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !key_is_plain || changes_the_runtime(key) || vars.iter().any(|(k, _)| k.eq_ignore_ascii_case(key)) {
            return Err(HandBack);
        }
        vars.push((key.to_string(), plain_value(raw)?.to_string()));
    }
    Ok(vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Result<Vec<(String, String)>, HandBack> {
        Ok(list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    #[test]
    fn plain_dotenv_lines_are_read() {
        assert_eq!(parse_plain_dotenv(""), pairs(&[]));
        assert_eq!(parse_plain_dotenv("A=1\nB=two words\n"), pairs(&[("A", "1"), ("B", "two words")]));
        assert_eq!(parse_plain_dotenv("# c\r\n\r\nA=1\r\nB= \t padded \t \r\nC="), pairs(&[("A", "1"), ("B", "padded"), ("C", "")]));
        for (line, value) in [("K=\"a b\"", "a b"), ("K='a # b'", "a # b"), ("K=a # c", "a"), ("K=a#c", "a"), ("K=\"x\" # c", "x")] {
            assert_eq!(parse_plain_dotenv(line), pairs(&[("K", value)]), "{line:?}");
        }
    }

    #[test]
    fn anything_bun_would_interpret_goes_back() {
        for text in [
            "A=$HOME/x", "A=\"a\\nb\"", "A=\"unterminated", "A=`x`", "A=it's", "export A=1", " A=1", "A =1", "A", "A.B=1", "A=1\nA=2",
            "\u{feff}A=1", "NODE_ENV=production", "BUN_ENV=test", "PATH=/x", "HOME=/x", "TZ=Asia/Tokyo", "TMPDIR=/x", "XDG_CONFIG_HOME=/x",
            "RUSTDESK_SKILL_NO_NATIVE=1", "FORCE_COLOR=1", "NO_COLOR=1", "LD_PRELOAD=x", "A=a\\b", "A=\"a\\b\"",
        ] {
            assert_eq!(parse_plain_dotenv(text), Err(HandBack), "{text:?}");
        }
    }

    #[test]
    fn project_files_bun_says_nothing_about() {
        for (name, text) in [
            ("package.json", "{}"),
            ("package.json", r#"{"name": "x", "version": "1.0.0", "type": "module", "scripts": {"t": "bun test"}, "dependencies": {"y": "^1"}}"#),
            ("package.json", r#"{"type": "commonjs", "main": 5, "workspaces": ["a"]}"#),
            ("tsconfig.json", r#"{"compilerOptions": {"strict": true, "target": "ESNext", "baseUrl": "."}, "include": ["src"]}"#),
            ("jsconfig.json", "{\n}\n"),
        ] {
            assert!(plain_project_file(name, text.as_bytes()), "{name} {text}");
        }
        for (name, text) in [
            ("package.json", "{oops"),
            ("package.json", ""),
            ("package.json", "[]"),
            ("package.json", r#"{"type": "weird"}"#),
            ("package.json", r#"{"type": 5}"#),
            ("package.json", r#"{"a": 1, "a": 2}"#),
            ("package.json", r#"{"x": {"a": 1, "a": 2}}"#),
            ("package.json", r#"{"x": [{"a": 1, "a": 2}]}"#),
            ("package.json", r#"{"exports": "./a.js"}"#),
            ("package.json", r#"{"imports": {}}"#),
            ("package.json", r#"{"browser": false}"#),
            ("package.json", "// c\n{}"),
            ("package.json", "{\"a\": 1,}"),
            ("package.json", "\u{feff}{}"),
            ("tsconfig.json", r#"{"extends": "./base.json"}"#),
            ("tsconfig.json", r#"{"compilerOptions": {"paths": {}}}"#),
            ("tsconfig.json", r#"{"compilerOptions": {"jsxFactory": "h"}}"#),
            ("tsconfig.json", r#"{"compilerOptions": 5}"#),
            ("jsconfig.json", r#"{"compilerOptions": {"importsNotUsedAsValues": "remove"}}"#),
        ] {
            assert!(!plain_project_file(name, text.as_bytes()), "{name} {text}");
        }
    }

    #[test]
    fn variables_for_the_runtime() {
        for name in ["BUN_OPTIONS", "NODE_ENV", "NODE_OPTIONS", "MIMALLOC_VERBOSE", "ICU_DATA", "BUN_INSTALL_CACHE_DIR", "LD_PRELOAD", "DYLD_INSERT_LIBRARIES", "FORCE_COLOR"] {
            assert!(for_the_runtime(name), "{name}");
        }
        for name in ["BUN_INSTALL", "HOME", "NO_COLOR", "RDS_PW_BIN", "NODENV_ROOT"] {
            assert!(!for_the_runtime(name), "{name}");
        }
    }
}
