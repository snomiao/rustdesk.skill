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
//! `encoding: "utf8"` does. bun stops a program whose output on a stream
//! passes a mebibyte (ENOBUFS, after the next read: 1,064,960 bytes of
//! stdout in a test with bun 1.4.0) and reports what it had; a stream of
//! more than `MOST` bytes is `Big`, which the binary does not imitate.

use std::fs;
use std::io::{ErrorKind, Read as _, Write as _};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::runtime::Machine;
use crate::HandBack;

pub const MOST: usize = 1_000_000;

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

pub struct Output {
    /// The exit code; `None` when a signal ended the program.
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub enum Failed {
    /// The program printed more than `MOST` bytes.
    Big,
    /// It could not be started, or its output not be read.
    Spawn,
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

/// `spawnSync(name, args, { cwd, input, encoding: "utf8", env: process.env })`
pub fn run(machine: &Machine, program: &Program, args: &[String], cwd: Option<&str>, input: Option<&[u8]>) -> Result<Output, Failed> {
    let mut command = Command::new(&program.path);
    command.arg0(&program.name).args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    for (key, value) in &machine.added {
        command.env(key, value);
    }
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|_| Failed::Spawn)?;
    let stdin = child.stdin.take();
    let input = input.map(<[u8]>::to_vec);
    let writer = std::thread::spawn(move || {
        if let (Some(mut stdin), Some(input)) = (stdin, input) {
            // A program that does not read it all is not an error.
            let _ = stdin.write_all(&input);
        }
    });
    let output = child.wait_with_output().map_err(|_| Failed::Spawn)?;
    let _ = writer.join();
    if output.stdout.len() > MOST || output.stderr.len() > MOST {
        return Err(Failed::Big);
    }
    Ok(Output {
        status: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
