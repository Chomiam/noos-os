//! Running external commands and writing files.
//!
//! This is the only place that touches the machine, which keeps the rest of
//! the engine pure and testable.

use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

use crate::error::{Error, Result};
use crate::events::{EventSink, InstallEvent, LogLine, Stream};

/// How many trailing stderr lines to keep for the error message when a
/// command fails. Enough to show the real cause, short enough to read.
const ERROR_TAIL_LINES: usize = 8;

pub struct Runner<'a> {
    sink: &'a dyn EventSink,
}

impl<'a> Runner<'a> {
    pub fn new(sink: &'a dyn EventSink) -> Self {
        Self { sink }
    }

    /// Runs a command, streaming both output streams to the sink as log lines.
    ///
    /// A real stream rather than a captured buffer: pacstrap and pacman emit
    /// progress and take minutes, and the installer shows them live.
    pub fn stream(&self, program: &str, args: &[&str]) -> Result<()> {
        self.run_streaming(None, program, args, None)
    }

    /// Same, but inside a `chroot`.
    pub fn stream_in(&self, root: Option<&str>, program: &str, args: &[&str]) -> Result<()> {
        self.run_streaming(root, program, args, None)
    }

    /// Same, with `input` written to the command's standard input.
    ///
    /// `chpasswd` is the reason this exists: it reads `user:password` from
    /// stdin and has no argument form — and putting a password in argv would
    /// publish it in the process list.
    pub fn stream_with_stdin(
        &self,
        root: Option<&str>,
        program: &str,
        args: &[&str],
        input: &str,
    ) -> Result<()> {
        self.run_streaming(root, program, args, Some(input))
    }

    fn run_streaming(
        &self,
        root: Option<&str>,
        program: &str,
        args: &[&str],
        input: Option<&str>,
    ) -> Result<()> {
        let mut command = match root {
            Some(root) => {
                let mut c = Command::new("arch-chroot");
                c.arg(root).arg(program).args(args);
                c
            }
            None => {
                let mut c = Command::new(program);
                c.args(args);
                c
            }
        };

        let mut child = command
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::io(program, e))?;

        if let Some(input) = input {
            use std::io::Write;

            let mut stdin = child.stdin.take().expect("stdin is piped");
            stdin
                .write_all(input.as_bytes())
                .map_err(|e| Error::io(program, e))?;
            // Dropping closes the pipe: `chpasswd` waits for EOF before it
            // writes anything to /etc/shadow.
            drop(stdin);
        }

        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");

        // Scoped threads: the sink outlives them, so no Arc or Mutex needed.
        // `ChildStdout` is only `Read`, so it needs a `BufReader` for `lines()`.
        let (_, err_lines) = thread::scope(|scope| {
            let out = scope.spawn(|| self.pump(BufReader::new(stdout), Stream::Stdout));
            let err = scope.spawn(|| self.pump(BufReader::new(stderr), Stream::Stderr));
            (
                out.join().unwrap_or_default(),
                err.join().unwrap_or_default(),
            )
        });

        let status = child.wait().map_err(|e| Error::io(program, e))?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::Command {
                cmd: describe(root, program, args),
                stderr: tail(&err_lines, ERROR_TAIL_LINES),
            })
        }
    }

    /// Runs a command and returns its stdout as a string.
    pub fn capture(&self, program: &str, args: &[&str]) -> Result<String> {
        let output = Command::new(program)
            .args(args)
            .output()
            .map_err(|e| Error::io(program, e))?;

        if !output.status.success() {
            return Err(Error::Command {
                cmd: describe(None, program, args),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Like [`capture`](Self::capture) but inside a chroot.
    pub fn capture_in(&self, root: &str, program: &str, args: &[&str]) -> Result<String> {
        let mut full = vec![root, program];
        full.extend_from_slice(args);
        self.capture("arch-chroot", &full)
    }

    /// Runs a command for its exit status only, discarding output.
    ///
    /// Returns `Ok(false)` when the command failed, for the many probes where
    /// failure is a normal answer rather than an error.
    pub fn probe(&self, program: &str, args: &[&str]) -> bool {
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// Reads one line of command output, trimmed.
    pub fn capture_line(&self, program: &str, args: &[&str]) -> Result<String> {
        Ok(self.capture(program, args)?.trim().to_string())
    }

    fn pump<R: BufRead>(&self, reader: R, stream: Stream) -> Vec<String> {
        let mut collected = Vec::new();
        for line in reader.lines() {
            let Ok(line) = line else { break };
            self.sink.emit(InstallEvent::Log(LogLine {
                stream,
                line: line.clone(),
            }));
            collected.push(line);
        }
        collected
    }
}

fn describe(root: Option<&str>, program: &str, args: &[&str]) -> String {
    let inner = format!("{program} {}", args.join(" "));
    match root {
        Some(root) => format!("arch-chroot {root} {}", inner.trim_end()),
        None => inner.trim_end().to_string(),
    }
}

fn tail(lines: &[String], count: usize) -> String {
    let start = lines.len().saturating_sub(count);
    lines[start..].join("\n")
}

/// Writes a file, creating parents, and sets its mode.
///
/// Config files written by an installer routinely need specific permissions
/// (`/etc/shadow`, sudoers drop-ins), so the mode is not optional here.
pub fn write_file(path: impl AsRef<Path>, contents: &str, mode: u32) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    fs::write(path, contents).map_err(|e| Error::io(path, e))?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| Error::io(path, e))?;
    Ok(())
}

/// Creates a directory (and its parents) with an explicit mode.
pub fn make_dir(path: impl AsRef<Path>, mode: u32) -> Result<()> {
    let path = path.as_ref();
    fs::create_dir_all(path).map_err(|e| Error::io(path, e))?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| Error::io(path, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::NullSink;

    #[test]
    fn describe_includes_the_chroot_prefix() {
        assert_eq!(describe(None, "ls", &["-l"]), "ls -l");
        assert_eq!(
            describe(Some("/mnt"), "pacman", &["-S", "vim"]),
            "arch-chroot /mnt pacman -S vim"
        );
    }

    #[test]
    fn tail_keeps_the_last_lines() {
        let lines: Vec<String> = (1..=20).map(|n| n.to_string()).collect();
        let t = tail(&lines, 3);
        assert_eq!(t, "18\n19\n20");
    }

    #[test]
    fn tail_handles_short_input() {
        let lines = vec!["only".to_string()];
        assert_eq!(tail(&lines, 8), "only");
    }

    #[test]
    fn capture_returns_stdout() {
        let sink = NullSink;
        let runner = Runner::new(&sink);
        let out = runner.capture("echo", &["bonjour"]).expect("echo doit marcher");
        assert_eq!(out.trim(), "bonjour");
    }

    #[test]
    fn failing_command_reports_stderr() {
        let sink = NullSink;
        let runner = Runner::new(&sink);
        let err = runner
            .capture("sh", &["-c", "echo boom >&2; exit 3"])
            .expect_err("doit échouer");

        let message = err.to_string();
        assert!(message.contains("boom"), "{message}");
        assert!(message.contains("sh -c"), "{message}");
    }

    #[test]
    fn probe_reports_failure_without_erroring() {
        let sink = NullSink;
        let runner = Runner::new(&sink);
        assert!(runner.probe("true", &[]));
        assert!(!runner.probe("false", &[]));
        // A program that does not exist is a failure, not a panic.
        assert!(!runner.probe("noos-commande-inexistante", &[]));
    }

    #[test]
    fn stream_reports_failure() {
        let sink = NullSink;
        let runner = Runner::new(&sink);
        assert!(runner.stream("true", &[]).is_ok());
        assert!(runner.stream("false", &[]).is_err());
    }

    #[test]
    fn stdin_is_written_and_closed() {
        // `cat` echoes stdin back; a correct implementation both writes the
        // data and closes the pipe, otherwise cat would hang forever.
        let sink = NullSink;
        let runner = Runner::new(&sink);
        assert!(runner.stream_with_stdin(None, "cat", &[], "bonjour\n").is_ok());
    }
}
