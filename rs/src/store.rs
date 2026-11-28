//! src/passwords.ts and the recents of src/connect.ts: two JSON files in
//! <RDS_CONFIG_DIR, else ~/.config/rustdesk-skill>.
//!
//! The binary reads the shapes the TypeScript itself writes (an array of
//! strings; an object of strings) and the ones it reads as empty (no file,
//! not JSON, not an array or object); anything else that JSON can hold
//! would need JavaScript's own formatting, and goes back.

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::Path;

use crate::js;
use crate::json::{self, Json};
use crate::run::{Run, Stop, R};

/// `configDir()`
pub fn config_dir(run: &Run) -> R<String> {
    match run.machine.var("RDS_CONFIG_DIR").filter(|dir| !dir.is_empty()) {
        Some(dir) => Ok(dir),
        None => Ok(js::join(&[&run.machine.home()?, ".config", "rustdesk-skill"])),
    }
}

/// What `existsSync(p)` then `JSON.parse(readFileSync(p, "utf8"))` inside a
/// try give: `None` when the TypeScript takes it for empty.
fn read_json(path: &str) -> R<Option<Json>> {
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        // existsSync: no file, or a link to nowhere.
        Err(err) if matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => return Ok(None),
        Err(_) => return Err(Stop::HandBack),
    };
    // A directory cannot be read (caught); a pipe or a device would be read
    // forever.
    if meta.is_dir() {
        return Ok(None);
    }
    if !meta.is_file() {
        return Err(Stop::HandBack);
    }
    let Ok(bytes) = fs::read(path) else { return Ok(None) };
    let text = String::from_utf8(bytes).map_err(|_| Stop::HandBack)?;
    match json::parse(&text) {
        Ok(value) => Ok(Some(value)),
        Err(json::Parse::Invalid) => Ok(None),
        Err(json::Parse::Unsure) => Err(Stop::HandBack),
    }
}

fn recents_path(run: &Run) -> R<String> {
    Ok(js::join(&[&config_dir(run)?, "recents.json"]))
}

fn passwords_path(run: &Run) -> R<String> {
    Ok(js::join(&[&config_dir(run)?, "passwords.json"]))
}

/// `listRecent()`
pub fn list_recent(run: &Run) -> R<Vec<String>> {
    match read_json(&recents_path(run)?)? {
        Some(Json::Arr(items)) => items
            .into_iter()
            .map(|item| match item {
                Json::Str(id) => Ok(id),
                _ => Err(Stop::HandBack),
            })
            .collect(),
        _ => Ok(Vec::new()),
    }
}

/// The id → password map, as JSON.parse made it.
pub struct Passwords(Vec<(String, String)>);

/// `loadPasswords()`
pub fn load_passwords(run: &Run) -> R<Passwords> {
    match read_json(&passwords_path(run)?)? {
        Some(Json::Obj(members)) => {
            let mut map: Vec<(String, String)> = Vec::new();
            for (key, value) in members {
                // A member named __proto__ is one JSON.parse makes and an
                // assignment treats in its own way.
                let Json::Str(value) = value else { return Err(Stop::HandBack) };
                if key == "__proto__" {
                    return Err(Stop::HandBack);
                }
                match map.iter_mut().find(|(k, _)| *k == key) {
                    Some(entry) => entry.1 = value,
                    None => map.push((key, value)),
                }
            }
            Ok(Passwords(map))
        }
        // An array is an object too, and `store[id] = pw` on it is not ported.
        Some(Json::Arr(_)) => Err(Stop::HandBack),
        _ => Ok(Passwords(Vec::new())),
    }
}

impl Passwords {
    /// `store[id]`, if it is truthy.
    pub fn remembered(&self, id: &str) -> R<Option<String>> {
        if js::is_object_member(id) {
            return Err(Stop::HandBack);
        }
        Ok(self.0.iter().find(|(key, _)| key == id).map(|(_, value)| value.clone()).filter(|value| !value.is_empty()))
    }

    fn set(&mut self, id: &str, pw: &str) {
        match self.0.iter_mut().find(|(key, _)| key == id) {
            Some(entry) => entry.1 = pw.to_string(),
            None => self.0.push((id.to_string(), pw.to_string())),
        }
    }

    /// `JSON.stringify(store, null, 2)`: the names that are array indices
    /// first, in their order, then the others as they came.
    fn to_json(&self) -> String {
        if self.0.is_empty() {
            return "{}".into();
        }
        let index = |key: &str| -> Option<u64> {
            let plain = key == "0" || (!key.is_empty() && !key.starts_with('0') && key.len() <= 10 && key.bytes().all(|b| b.is_ascii_digit()));
            plain.then(|| key.parse::<u64>().ok()).flatten().filter(|n| *n < 4_294_967_295)
        };
        let mut indexed: Vec<(u64, &(String, String))> = self.0.iter().filter_map(|entry| index(&entry.0).map(|n| (n, entry))).collect();
        indexed.sort_by_key(|(n, _)| *n);
        let named = self.0.iter().filter(|entry| index(&entry.0).is_none());
        let members: Vec<String> = indexed
            .into_iter()
            .map(|(_, entry)| entry)
            .chain(named)
            .map(|(key, value)| format!("  {}: {}", js::json_string(key), js::json_string(value)))
            .collect();
        format!("{{\n{}\n}}", members.join(",\n"))
    }
}

/// Whether `savePasswords` or `recordRecent` can write `file` (in the config
/// directory): looked at before the first step, because after it a failure
/// would have to be reported in bun's words.
pub fn writable(run: &Run, name: &str) -> R<()> {
    let dir = config_dir(run)?;
    let can = |path: &Path, mode| {
        use std::os::unix::ffi::OsStrExt as _;
        let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
        // SAFETY: a valid NUL-terminated path.
        unsafe { libc::access(c_path.as_ptr(), mode) == 0 }
    };
    match fs::metadata(&dir) {
        Ok(meta) if meta.is_dir() && can(Path::new(&dir), libc::W_OK | libc::X_OK) => {}
        Ok(_) => return Err(Stop::HandBack),
        Err(err) if err.kind() == ErrorKind::NotFound => {
            // The nearest directory that is there must take new ones.
            let mut at = Path::new(&dir);
            loop {
                at = at.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
                match fs::metadata(at) {
                    Ok(meta) if meta.is_dir() && can(at, libc::W_OK | libc::X_OK) => break,
                    Err(err) if err.kind() == ErrorKind::NotFound && at != Path::new(".") && at != Path::new("/") => continue,
                    _ => return Err(Stop::HandBack),
                }
            }
            return Ok(());
        }
        Err(_) => return Err(Stop::HandBack),
    }
    let file = js::join(&[&dir, name]);
    match fs::symlink_metadata(&file) {
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Ok(meta) if meta.is_file() && can(Path::new(&file), libc::W_OK) => Ok(()),
        _ => Err(Stop::HandBack),
    }
}

/// `mkdirSync(dir, { recursive: true, mode: 0o700 })` unless it is there,
/// then `writeFileSync(file, text, { mode: 0o600 })`. After the first step
/// a failure here is not the TypeScript's report: `Stop::HandBack`, which
/// is the stop rs/src/main.rs reports once committed.
fn write(run: &Run, file: &str, text: &str) -> R<()> {
    let dir = config_dir(run)?;
    if fs::metadata(&dir).is_err() {
        fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).map_err(|_| Stop::HandBack)?;
    }
    use std::io::Write as _;
    let mut out = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(file).map_err(|_| Stop::HandBack)?;
    out.write_all(text.as_bytes()).map_err(|_| Stop::HandBack)
}

/// `setPassword(id, pw)`: the store read again, the password set, all of it
/// written.
pub fn set_password(run: &Run, id: &str, pw: &str) -> R<()> {
    let mut store = load_passwords(run)?;
    store.set(id, pw);
    write(run, &passwords_path(run)?, &format!("{}\n", store.to_json()))
}

/// `recordRecent(id)`
pub fn record_recent(run: &Run, id: &str) -> R<()> {
    let current = list_recent(run)?;
    let next: Vec<&str> = std::iter::once(id).chain(current.iter().map(String::as_str).filter(|x| *x != id)).take(20).collect();
    let items: Vec<String> = next.iter().map(|id| format!("  {}", js::json_string(id))).collect();
    write(run, &recents_path(run)?, &format!("[\n{}\n]\n", items.join(",\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_are_written_in_javascript_order() {
        let store = Passwords(vec![("b".into(), "1".into()), ("123456789".into(), "x".into()), ("0123".into(), "y".into()), ("9999999999".into(), "z".into()), ("5".into(), "w".into())]);
        assert_eq!(store.to_json(), "{\n  \"5\": \"w\",\n  \"123456789\": \"x\",\n  \"b\": \"1\",\n  \"0123\": \"y\",\n  \"9999999999\": \"z\"\n}");
        assert_eq!(Passwords(vec![]).to_json(), "{}");
        assert_eq!(Passwords(vec![("4294967294".into(), "a".into()), ("4294967295".into(), "b".into()), ("0".into(), "c".into())]).to_json(), "{\n  \"0\": \"c\",\n  \"4294967294\": \"a\",\n  \"4294967295\": \"b\"\n}");
    }
}
