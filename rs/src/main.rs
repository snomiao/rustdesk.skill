//! rustdesk-skill — Rust entry point.
//!
//! Installed from npm, this binary is the command users run. A command runs
//! here once it has been ported and a parity test shows it behaves like the
//! TypeScript; until then it is handed back to the JavaScript the package
//! still ships, so nothing changes for users.

mod js_fallback;

/// "Not mine": the invocation has to go to the JavaScript.
#[cfg(unix)]
pub use js_fallback::HandBack;
#[cfg(unix)]
#[allow(dead_code)] // used once the commands are switched over
mod args;
#[cfg(unix)]
#[allow(dead_code)] // used once the commands are switched over
mod js;
#[cfg(unix)]
#[allow(dead_code)] // used once the commands are switched over
mod json;
#[cfg(unix)]
#[allow(dead_code)] // used once the commands are switched over
mod runtime;

/// The package's JavaScript entry, relative to the package root.
const JS_ENTRY: &str = "bin/rustdesk-skill.ts";

/// `RUSTDESK_SKILL_NO_NATIVE=1` hands every command back to the JavaScript: the way
/// out for a user if a ported command misbehaves.
fn native() -> bool {
    std::env::var_os("RUSTDESK_SKILL_NO_NATIVE").is_none()
}

fn main() {
    // Arguments that are not valid UTF-8 are for the JavaScript to deal with.
    let args: Option<Vec<String>> = std::env::args_os().skip(1).map(|arg| arg.into_string().ok()).collect();
    if let Some(args) = args.filter(|_| native()) {
        // Ported commands go here, one arm each, once their parity cases pass.
        // Everything the binary answers itself belongs in this match,
        // `--version` included, so that `RUSTDESK_SKILL_NO_NATIVE=1` switches it off.
        //
        // An arm takes only the invocations it implements exactly. For the
        // rest it falls out of the match, before it has printed or changed
        // anything, and the JavaScript answers. `js_fallback::attempt` is the
        // guard for that: the command works out its whole answer first and
        // returns it, or returns `Err(js_fallback::HandBack)`; and should it
        // panic, the invocation is handed back just the same.
        //
        //   Some("status") => {
        //       if let Some(text) = js_fallback::attempt(|| status::answer(&args[1..])) {
        //           // A reader that has gone away (`| head`) is not an error.
        //           let _ = std::io::Write::write_all(&mut std::io::stdout(), text.as_bytes());
        //           return;
        //       }
        //   }
        // Until the match has two arms of its own, clippy would rather see no
        // match, or an `if let`.
        #[allow(clippy::match_single_binding, clippy::single_match)]
        match args.first().map(String::as_str) {
            _ => {}
        }
    }
    js_fallback::run(JS_ENTRY)
}
