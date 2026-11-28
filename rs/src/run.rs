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
    /// What stdout holds came from `process.stdout.write` (printJson), whose
    /// failure bun reports with a stack trace and exit code 1; otherwise
    /// from console.log, whose failure bun ignores.
    strict: bool,
    /// After the first act, printJson could not write: the error.
    pub stdout_failed: Option<std::io::Error>,
    /// Files this attempt made that the TypeScript would make under another
    /// name (the OCR's PNG is named for the process): taken away again if
    /// the invocation is handed back.
    made: Vec<PathBuf>,
    /// `viewport()`'s cache, as in src/pw.ts.
    pub viewport: Option<crate::pw::Viewport>,
}

impl<'m> Run<'m> {
    pub fn new(machine: &'m Machine) -> Run<'m> {
        Run { machine, live: false, stdout: Vec::new(), stderr: Vec::new(), strict: false, stdout_failed: None, made: Vec::new(), viewport: None }
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
        // console.error's failures are ignored, as bun ignores them.
        if !self.stderr.is_empty() {
            let _ = write_fd(2, &self.stderr);
            self.stderr.clear();
        }
        if !self.stdout.is_empty() {
            if let Err(err) = write_fd(1, &self.stdout) {
                if self.strict && self.stdout_failed.is_none() {
                    self.stdout_failed = Some(err);
                }
            }
            self.stdout.clear();
        }
    }

    /// The end of a command that has not acted: what it printed goes out.
    /// When stdout cannot take what printJson wrote (its reader has gone,
    /// the disk is full), bun fails in its own words, with a stack trace:
    /// the invocation goes back, and the JavaScript fails the same way.
    pub fn finish(&mut self) -> Result<(), crate::HandBack> {
        if self.live {
            return Ok(());
        }
        if self.strict && !self.stdout.is_empty() {
            // Into one file, stderr comes first, as the TypeScript writes it;
            // into two, the order cannot be seen, and stdout goes first so
            // that nothing is out yet when it fails.
            if same_file(1, 2) && !self.stderr.is_empty() {
                let _ = write_fd(2, &self.stderr);
                self.stderr.clear();
            }
            if write_fd(1, &self.stdout).is_err() {
                self.undo();
                return Err(crate::HandBack);
            }
            self.stdout.clear();
        }
        self.commit();
        Ok(())
    }

    /// `console.error(text)`
    pub fn error_line(&mut self, text: &str) {
        self.stderr.extend_from_slice(text.as_bytes());
        self.stderr.push(b'\n');
        if self.live {
            self.flush();
        }
    }

    /// `console.log(text)` of every line in `text`
    pub fn write(&mut self, text: &str) {
        self.stdout.extend_from_slice(text.as_bytes());
        if self.live {
            self.flush();
        }
    }

    /// `process.stdout.write(text)` of printJson
    pub fn print_json(&mut self, text: &str) {
        self.strict = true;
        self.write(text);
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

/// All of `bytes` to the file descriptor, or the error of the write that
/// failed.
fn write_fd(fd: i32, mut bytes: &[u8]) -> std::io::Result<()> {
    while !bytes.is_empty() {
        // SAFETY: `bytes` is valid for its length.
        let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        bytes = &bytes[n as usize..];
    }
    Ok(())
}

fn stat(fd: i32) -> Option<libc::stat> {
    // SAFETY: fstat fills the structure it is given.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        (libc::fstat(fd, &mut st) == 0).then_some(st)
    }
}

/// Whether stdout and stderr are the same file (one pipe, one terminal).
fn same_file(a: i32, b: i32) -> bool {
    match (stat(a), stat(b)) {
        (Some(a), Some(b)) => a.st_dev == b.st_dev && a.st_ino == b.st_ino,
        _ => false,
    }
}

/// Whether stdout is seen not to take what is written to it: not open, a
/// pipe or socket that nothing reads any more, or Linux's /dev/full.
pub fn stdout_unwritable() -> bool {
    let Some(st) = stat(1) else { return true };
    let kind = st.st_mode & libc::S_IFMT;
    if kind == libc::S_IFIFO || kind == libc::S_IFSOCK {
        let mut poll = libc::pollfd { fd: 1, events: libc::POLLOUT, revents: 0 };
        // SAFETY: one pollfd structure, no waiting.
        if unsafe { libc::poll(&mut poll, 1, 0) } > 0 && poll.revents & (libc::POLLERR | libc::POLLHUP) != 0 {
            return true;
        }
    }
    // macOS does not say so of a named pipe whose reader has gone, but
    // writing nothing to it fails (on Linux writing nothing to a pipe does
    // nothing).
    if kind == libc::S_IFIFO {
        let nothing = [0u8; 1];
        // SAFETY: a write of no bytes.
        if unsafe { libc::write(1, nothing.as_ptr().cast(), 0) } < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EPIPE) {
            return true;
        }
    }
    // /dev/full, on Linux.
    cfg!(target_os = "linux") && kind == libc::S_IFCHR && st.st_rdev == libc::makedev(1, 7)
}
