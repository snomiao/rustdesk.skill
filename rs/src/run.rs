//! One invocation of a ported command: what it has printed so far, and
//! whether it can still be handed back.
//!
//! Until the command does something that cannot be undone (it starts a
//! playwright-cli step that acts on the page, sets the clipboard, writes a
//! file of the user's), everything it would print is kept here, and a
//! `Stop::HandBack` throws it away: the JavaScript then starts over and
//! nothing of the first attempt shows. `commit` is that point. From there on
//! the output goes out as the TypeScript writes it, and every outcome has to
//! be answered as the TypeScript answers it.

use std::io::Write as _;
use std::path::PathBuf;

use crate::runtime::Machine;

/// How a command ends other than by finishing: bin/rustdesk-skill.ts's
/// `guard`, or a hand-back.
#[derive(Debug)]
pub enum Stop {
    /// Not something the binary imitates: the JavaScript answers.
    HandBack,
    /// The exit code, and the line `guard` writes to stderr, if any.
    Exit(i32, Option<String>),
}

impl From<crate::HandBack> for Stop {
    fn from(_: crate::HandBack) -> Stop {
        Stop::HandBack
    }
}

impl Stop {
    /// Any other error: `[rustdesk-skill] error: <message>`, exit 1.
    pub fn error(message: impl AsRef<str>) -> Stop {
        Stop::Exit(1, Some(format!("[rustdesk-skill] error: {}", message.as_ref())))
    }

    /// A PwNotFoundError (5), SessionNameError (6) or NoPasswordError (3).
    pub fn known(code: i32, message: impl AsRef<str>) -> Stop {
        Stop::Exit(code, Some(format!("[rustdesk-skill] {}", message.as_ref())))
    }
}

pub type R<T> = Result<T, Stop>;

pub struct Run<'m> {
    pub machine: &'m Machine,
    live: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    /// Files this attempt made that the TypeScript would make under another
    /// name (the OCR's PNG is named for the process): taken away again if
    /// the invocation is handed back.
    made: Vec<PathBuf>,
    /// `viewport()`'s cache, as in src/pw.ts.
    pub viewport: Option<(f64, f64)>,
}

impl<'m> Run<'m> {
    pub fn new(machine: &'m Machine) -> Run<'m> {
        Run { machine, live: false, stdout: Vec::new(), stderr: Vec::new(), made: Vec::new(), viewport: None }
    }

    pub fn is_live(&self) -> bool {
        self.live
    }

    /// From here on there is no way back.
    pub fn commit(&mut self) {
        if self.live {
            return;
        }
        crate::js_fallback::commit();
        self.live = true;
        self.made.clear();
        self.flush();
    }

    fn flush(&mut self) {
        // A reader that has gone away is not an error, as it is not for bun.
        if !self.stderr.is_empty() {
            let _ = std::io::stderr().write_all(&self.stderr);
            self.stderr.clear();
        }
        if !self.stdout.is_empty() {
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(&self.stdout).and_then(|()| out.flush());
            self.stdout.clear();
        }
    }

    /// `console.error(text)`
    pub fn error_line(&mut self, text: &str) {
        self.stderr.extend_from_slice(text.as_bytes());
        self.stderr.push(b'\n');
        if self.live {
            self.flush();
        }
    }

    /// `process.stdout.write(text)`
    pub fn write(&mut self, text: &str) {
        self.stdout.extend_from_slice(text.as_bytes());
        if self.live {
            self.flush();
        }
    }

    /// A file of the attempt's own, to take away if it is handed back.
    pub fn made(&mut self, path: PathBuf) {
        if !self.live {
            self.made.push(path);
        }
    }

    /// The invocation goes back: take away what was made.
    pub fn undo(&mut self) {
        for path in self.made.drain(..) {
            let _ = std::fs::remove_file(path);
        }
    }
}
