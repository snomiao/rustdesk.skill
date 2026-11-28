//! src/session.ts: the name of the playwright-cli session, asked for again
//! by every step (`resolveSessionName` in `pw()`), so a step that has no
//! RDS_SESSION_NAME starts git twice and prints its note each time.
//!
//! The question on a terminal (both stdin and stdout one) is not ported:
//! there every command that would ask goes back (the runtime check in
//! rs/src/commands.rs hands back any command that writes to stderr while
//! stdout or stderr is a terminal, which covers it).

use std::io::IsTerminal;

use crate::js;
use crate::proc::{self, Found};
use crate::run::{Run, Stop, R};

/// `sanitizeSessionName(raw)`
pub fn sanitize(raw: &str) -> String {
    let mut out = String::new();
    let mut in_run = false;
    for c in js::trim(raw).chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            out.push(c);
            in_run = false;
        } else if !in_run {
            out.push('-');
            in_run = true;
        }
    }
    // /-{2,}/g → "-", then no "-" at either end.
    let mut collapsed = String::with_capacity(out.len());
    for c in out.chars() {
        if c == '-' && collapsed.ends_with('-') {
            continue;
        }
        collapsed.push(c);
    }
    collapsed.trim_matches('-').to_string()
}

/// `spawnSync("git", args, { encoding: "utf8" })`: the exit code and stdout.
fn git(run: &mut Run, args: &[&str]) -> R<(Option<i32>, String)> {
    let program = match proc::find(run.machine, "git")? {
        Found::Program(program) => program,
        // ENOENT: no status, so `status !== 0`.
        Found::Missing => return Ok((None, String::new())),
    };
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let output = match proc::start(run.machine, &program, &args, None, None) {
        // Any error: no status, so `status !== 0`.
        Err(_) => return Ok((None, String::new())),
        Ok(started) => started.finish().map_err(|_| Stop::HandBack)?,
    };
    // Past bun's buffer, before the first act: back.
    if output.overflow && !run.is_live() {
        return Err(Stop::HandBack);
    }
    Ok((output.status, output.stdout))
}

/// `gitDefaultSessionName()`
fn git_default(run: &mut Run) -> R<Option<String>> {
    let (status, top) = git(run, &["rev-parse", "--show-toplevel"])?;
    if status != Some(0) {
        return Ok(None);
    }
    let repo = js::basename(js::trim(&top)).to_string();
    if repo.is_empty() {
        return Ok(None);
    }
    let (status, branch) = git(run, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let branch = if status == Some(0) { js::trim(&branch).to_string() } else { String::new() };
    let raw = if !branch.is_empty() && branch != "HEAD" { format!("{repo}/{branch}") } else { repo };
    let name = sanitize(&raw);
    Ok((!name.is_empty()).then_some(name))
}

/// `resolveSessionName()`
pub fn resolve(run: &mut Run) -> R<String> {
    if let Some(name) = run.machine.var("RDS_SESSION_NAME") {
        let name = js::trim(&name);
        if !name.is_empty() {
            return Ok(sanitize(name));
        }
    }
    let default = match git_default(run)? {
        Some(name) => name,
        None => sanitize(js::basename(&run.machine.cwd)),
    };
    // The question is not ported.
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        return Err(Stop::HandBack);
    }
    if default.is_empty() {
        return Err(Stop::known(
            6,
            "RDS_SESSION_NAME is not set and no default could be derived from git or the current folder. \
             Set it in ./.env.local (RDS_SESSION_NAME=<name>) or inline: `RDS_SESSION_NAME=<name> rds ...`.",
        ));
    }
    run.error_line(&format!(
        "[rustdesk-skill] RDS_SESSION_NAME not set; using \"{default}\" (from git/cwd). Set RDS_SESSION_NAME in ./.env.local or inline to pin it."
    ));
    Ok(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sanitised_as_the_typescript_does() {
        for (raw, name) in [
            ("my repo/feature #1", "my-repo-feature-1"),
            ("  --Foo.Bar_baz--  ", "Foo.Bar_baz"),
            ("a--b", "a-b"),
            ("a - b", "a-b"),
            ("///", ""),
            ("日本語/main", "main"),
            ("x😀y", "x-y"),
            ("-a-", "a"),
            ("\u{3000}a b\u{feff}", "a-b"),
        ] {
            assert_eq!(sanitize(raw), name, "{raw:?}");
        }
    }
}
