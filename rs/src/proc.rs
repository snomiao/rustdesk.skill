//! `spawnSync` as bun does it, for the programs the TypeScript starts
//! (playwright-cli, git, python3, xclip, pbcopy).
//!
//! bun looks a program up itself: a name with a `/` is taken as it is, any
//! other in the directories of PATH, where it takes the first regular file
//! it may execute. A program it cannot find is ENOENT ("Executable not found
//! in $PATH"). It then starts the program by that path, with the name as it
//! was written for argv[0], and without the shell: a file that is neither a
//! script with a `#!` line nor a binary fails with ENOEXEC. `find` answers
//! only where it is sure the two agree: a PATH with an empty or a relative
//! entry, a relative path with a `/`, a file there that cannot be executed,
//! and one that is neither a script nor a binary all hand back.
//!
//! Output is decoded as UTF-8 with U+FFFD for what is not, as bun's
//! `encoding: "utf8"` does. bun reads stdout and stderr until both end, a
//! read at a time (at most 64 KiB, what a pipe holds), and once either
//! stream holds more than `MAX_BUFFER` bytes (node's `maxBuffer`, 1 MiB) it
//! sends the program SIGTERM, stops reading and waits for it: the result has
//! the error ENOBUFS, the output read so far (a little more than a mebibyte:
//! how much depends on how the reads fell, in bun as here) and the status
//! the program ended with, which is no status when the signal ended it.
//! `start` and `Started::finish` do the same. A program that cannot be
//! started has no status and no output; bun says why in `error.message`.

use std::fs;
use std::io::{ErrorKind, Read as _, Write as _};
use std::os::fd::AsRawFd as _;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::runtime::Machine;
use crate::HandBack;

/// node's default `maxBuffer` for `spawnSync`, which bun keeps.
pub const MAX_BUFFER: usize = 1024 * 1024;
/// What one read takes at most.
const READ: usize = 64 * 1024;

pub struct Program {
    /// The file bun starts.
    pub path: PathBuf,
    /// The name as the TypeScript wrote it: argv[0].
    pub name: String,
}

pub enum Found {
    Program(Program),
    /// ENOENT
    Missing,
}

/// What `spawnSync` returns, for a program that was started.
pub struct Output {
    /// `res.status`: the exit code, `None` when a signal ended the program.
    pub status: Option<i32>,
    /// `res.error` is ENOBUFS: a stream passed `MAX_BUFFER`.
    pub overflow: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    /// `res.error?.message` of a program that ran: bun's words for ENOBUFS.
    pub fn error_message(&self, name: &str) -> Option<String> {
        self.overflow.then(|| format!("spawnSync {name} ENOBUFS (stdout or stderr buffer reached maxBuffer size limit)"))
    }
}

/// A program bun could not start.
pub enum NotStarted {
    /// ENOENT: gone since `find` saw it.
    Missing,
    /// `res.error.message` for an error whose words are known (E2BIG).
    Said(String),
    /// Any other error: its words are not known here.
    Other,
}

/// A started program whose output is still to be read.
pub struct Started {
    child: Child,
    writer: Option<std::thread::JoinHandle<()>>,
}

fn executable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt as _;
    let Ok(meta) = fs::metadata(path) else { return false };
    if !meta.is_file() {
        return false;
    }
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
    // SAFETY: a valid NUL-terminated path.
    unsafe { libc::access(c_path.as_ptr(), libc::X_OK) == 0 }
}

/// A binary of this system, or a script whose `#!` names one.
fn startable(path: &Path, depth: u32) -> bool {
    let mut head = [0u8; 256];
    let Ok(mut file) = fs::File::open(path) else { return false };
    let mut read = 0;
    while read < head.len() {
        match file.read(&mut head[read..]) {
            Ok(0) => break,
            Ok(n) => read += n,
            Err(_) => return false,
        }
    }
    let head = &head[..read];
    let binary = head.starts_with(b"\x7fELF")
        || [[0xfe, 0xed, 0xfa, 0xce], [0xfe, 0xed, 0xfa, 0xcf], [0xce, 0xfa, 0xed, 0xfe], [0xcf, 0xfa, 0xed, 0xfe], [0xca, 0xfe, 0xba, 0xbe]]
            .iter()
            .any(|magic| head.starts_with(magic));
    if binary {
        return true;
    }
    let Some(line) = head.strip_prefix(b"#!") else { return false };
    let Some(end) = line.iter().position(|b| *b == b'\n') else { return false };
    let Ok(line) = std::str::from_utf8(&line[..end]) else { return false };
    let Some(interpreter) = line.split([' ', '\t']).find(|word| !word.is_empty()) else { return false };
    depth == 0 && interpreter.starts_with('/') && executable(Path::new(interpreter)) && startable(Path::new(interpreter), depth + 1)
}

/// Where bun finds `name`.
pub fn find(machine: &Machine, name: &str) -> Result<Found, HandBack> {
    let program = |path: PathBuf| Found::Program(Program { path, name: name.to_string() });
    if name.contains('/') {
        if !name.starts_with('/') {
            return Err(HandBack);
        }
        let path = PathBuf::from(name);
        return match fs::symlink_metadata(&path) {
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(Found::Missing),
            Ok(_) if executable(&path) && startable(&path, 0) => Ok(program(path)),
            _ => Err(HandBack),
        };
    }
    if name.is_empty() {
        return Err(HandBack);
    }
    let path_var = machine.var("PATH").ok_or(HandBack)?;
    let dirs: Vec<&str> = path_var.split(':').collect();
    if dirs.iter().any(|dir| !dir.starts_with('/')) {
        return Err(HandBack);
    }
    for dir in dirs {
        let candidate = Path::new(dir).join(name);
        if executable(&candidate) {
            return if startable(&candidate, 0) { Ok(program(candidate)) } else { Err(HandBack) };
        }
    }
    Ok(Found::Missing)
}

/// The start of `spawnSync(name, args, { cwd, input, encoding: "utf8", env:
/// process.env })`.
pub fn start(machine: &Machine, program: &Program, args: &[String], cwd: Option<&str>, input: Option<&[u8]>) -> Result<Started, NotStarted> {
    let mut command = Command::new(&program.path);
    command.arg0(&program.name).args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    for (key, value) in &machine.added {
        command.env(key, value);
    }
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|err| match err.raw_os_error() {
        Some(libc::ENOENT) => NotStarted::Missing,
        Some(libc::E2BIG) => NotStarted::Said(format!("E2BIG: argument list too long, posix_spawn '{}'", program.name)),
        _ => NotStarted::Other,
    })?;
    let stdin = child.stdin.take();
    let input = input.map(<[u8]>::to_vec);
    let writer = std::thread::spawn(move || {
        if let (Some(mut stdin), Some(input)) = (stdin, input) {
            // A program that does not read it all is not an error.
            let _ = stdin.write_all(&input);
        }
    });
    Ok(Started { child, writer: Some(writer) })
}

impl Started {
    /// The rest of `spawnSync`: the output, read as bun reads it, and the
    /// status. `Err` when the output could not be read.
    pub fn finish(mut self) -> Result<Output, HandBack> {
        let read = self.read();
        if read.as_ref().is_ok_and(|(_, overflow)| *overflow) || read.is_err() {
            // SAFETY: the child has not been waited for, so the pid is its own.
            unsafe { libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM) };
        }
        // Our ends of its pipes close here, before the wait.
        drop(self.child.stdout.take());
        drop(self.child.stderr.take());
        let status = self.child.wait();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        let ([out, err], overflow) = read.map_err(|_| HandBack)?;
        let status = status.map_err(|_| HandBack)?;
        Ok(Output {
            status: if status.signal().is_some() { None } else { status.code() },
            overflow,
            stdout: String::from_utf8_lossy(&out).into_owned(),
            stderr: String::from_utf8_lossy(&err).into_owned(),
        })
    }

    /// Both streams until both end, or until one holds more than
    /// `MAX_BUFFER` bytes (`true`).
    fn read(&mut self) -> std::io::Result<([Vec<u8>; 2], bool)> {
        let fds = [self.child.stdout.as_ref().map(|s| s.as_raw_fd()), self.child.stderr.as_ref().map(|s| s.as_raw_fd())];
        let mut bufs = [Vec::new(), Vec::new()];
        let mut open = [fds[0].is_some(), fds[1].is_some()];
        let mut chunk = vec![0u8; READ];
        while open[0] || open[1] {
            let mut polled: Vec<(usize, libc::pollfd)> = (0..2)
                .filter(|&i| open[i])
                .map(|i| (i, libc::pollfd { fd: fds[i].unwrap_or(-1), events: libc::POLLIN, revents: 0 }))
                .collect();
            let mut raw: Vec<libc::pollfd> = polled.iter().map(|(_, p)| *p).collect();
            // SAFETY: `raw` holds `raw.len()` pollfd structures.
            let ready = unsafe { libc::poll(raw.as_mut_ptr(), raw.len() as libc::nfds_t, -1) };
            if ready < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == ErrorKind::Interrupted {
                    continue;
                }
                return Err(err);
            }
            for ((i, _), p) in polled.iter_mut().zip(&raw) {
                if p.revents == 0 {
                    continue;
                }
                // SAFETY: `chunk` has room for `READ` bytes; the fd is open.
                let n = unsafe { libc::read(p.fd, chunk.as_mut_ptr().cast(), READ) };
                if n < 0 {
                    let err = std::io::Error::last_os_error();
                    if matches!(err.kind(), ErrorKind::Interrupted | ErrorKind::WouldBlock) {
                        continue;
                    }
                    return Err(err);
                }
                if n == 0 {
                    open[*i] = false;
                    continue;
                }
                bufs[*i].extend_from_slice(&chunk[..n as usize]);
                if bufs[*i].len() > MAX_BUFFER {
                    return Ok((bufs, true));
                }
            }
        }
        Ok((bufs, false))
    }
}
