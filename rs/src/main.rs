//! rustdesk-skill — Rust entry point.
//!
//! Installed from npm, this binary is the command users run (`rustdesk-skill`
//! and `rds`). A command runs here once it has been ported and the parity
//! cases (rs/parity.json) show it behaves like the TypeScript; everything
//! else is handed back to the JavaScript the package still ships,
//! bin/rustdesk-skill.ts, so nothing changes for users.
//!
//! Native, on Linux and macOS: every command (list, scroll, key, type, move,
//! click-xy, screenshot, disconnect, status, ocr, click, paste, connect) in
//! its plain spellings (rs/src/args.rs), on a machine the runtime checks of
//! rs/src/runtime.rs find plain. Each works out what it can before it acts:
//! until its first step that acts on the page (or sets the clipboard, or
//! writes a file of the user's) it can still decline, and its output is kept
//! until then. On Windows everything is handed back: none of it is compiled
//! there.

mod js_fallback;

#[cfg(unix)]
mod args;
#[cfg(unix)]
mod commands;
#[cfg(all(unix, test))]
mod compare;
#[cfg(unix)]
mod connect;
#[cfg(unix)]
mod js;
#[cfg(unix)]
mod json;
#[cfg(unix)]
mod jssort;
#[cfg(unix)]
mod jsv;
#[cfg(unix)]
mod ocr;
#[cfg(unix)]
mod proc;
#[cfg(unix)]
mod pw;
#[cfg(unix)]
mod run;
#[cfg(unix)]
mod runtime;
#[cfg(unix)]
mod session;
#[cfg(unix)]
mod store;

/// The package's JavaScript entry, relative to the package root: what `bin`
/// in package.json names for both commands, run by bun (`#!/usr/bin/env bun`).
const JS_ENTRY: &str = "bin/rustdesk-skill.ts";

/// "Not mine": the invocation has to go to the JavaScript.
#[cfg(unix)]
pub use js_fallback::HandBack;

/// `RUSTDESK_SKILL_NO_NATIVE=1` hands every command back to the JavaScript: the way
/// out for a user if a ported command misbehaves.
#[cfg_attr(not(unix), allow(dead_code))]
fn native() -> bool {
    std::env::var_os("RUSTDESK_SKILL_NO_NATIVE").is_none()
}

fn main() {
    // Arguments that are not valid UTF-8 are for the JavaScript to deal with.
    let args: Option<Vec<String>> = std::env::args_os().skip(1).map(|arg| arg.into_string().ok()).collect();
    #[cfg(unix)]
    if let (Some(args), true) = (&args, native()) {
        answer(args);
    }
    drop(args);
    js_fallback::run(JS_ENTRY)
}

/// Answers a ported command and ends the process, or returns for the
/// invocation to be handed back. A panic before the command has acted is a
/// hand-back too (`js_fallback::attempt`): a bug in a ported command must
/// not cost the user their command.
#[cfg(unix)]
fn answer(args: &[String]) {
    use run::{Run, Stop};
    let outcome = js_fallback::attempt(|| {
        let command = args::recognise(args).ok_or(HandBack)?;
        let machine = runtime::machine()?;
        let mut run = Run::new(&machine);
        let mut code = match commands::run(&mut run, &command) {
            Ok(()) => 0,
            Err(Stop::Exit(code, message)) => {
                if let Some(message) = message {
                    run.error_line(&message);
                }
                code
            }
            Err(Stop::HandBack) if run.is_live() => {
                // Meant not to happen: everything the binary does not
                // imitate is looked for before the first step that acts,
                // and every outcome of what comes after it is answered.
                // What is left (rs/parity/README or the ledger names it) is
                // stopped without another step; running it again would
                // repeat what was done.
                run.error_line("[rustdesk-skill] stopped: this build does not handle what it met after acting on the page; nothing further was done");
                1
            }
            Err(Stop::HandBack) => {
                run.undo();
                return Err(HandBack);
            }
        };
        // Printing the answer is the point of no return too; printJson that
        // cannot write before then goes back.
        run.finish()?;
        if let Some(err) = run.stdout_failed.take() {
            // After acting: bun's report of the failed write, without the
            // stack trace and the version line it adds.
            run.error_line(&bun_write_error(&err));
            code = 1;
        }
        Ok(code)
    });
    if let Some(code) = outcome {
        std::process::exit(code);
    }
}

/// The first lines of bun's report of a write that failed.
#[cfg(unix)]
fn bun_write_error(err: &std::io::Error) -> String {
    let (code, text) = match err.raw_os_error() {
        Some(libc::EPIPE) => ("EPIPE", "broken pipe"),
        Some(libc::ENOSPC) => ("ENOSPC", "no space left on device"),
        _ => return format!("{err}"),
    };
    let errno = err.raw_os_error().unwrap_or(0);
    format!("{code}: {text}, write\n syscall: \"write\",\n   errno: -{errno},\n    code: \"{code}\"\n")
}
