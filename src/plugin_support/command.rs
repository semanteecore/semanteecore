use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::process::{Command, Stdio};

pub struct PipedCommand<'a> {
    name: &'static str,
    command: Command,
    input: Option<&'a str>,
}

impl<'a> PipedCommand<'a> {
    pub fn new(name: &'static str, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Self {
        let mut command = Command::new(name);

        // Log the full command invocation in debug level
        if log::log_enabled!(log::Level::Debug) {
            let args = args.into_iter().collect::<Vec<_>>();
            let mut line = format!("{} ", name);
            for arg in &args {
                write!(line, "{} ", arg.as_ref().to_string_lossy()).unwrap();
            }
            log::debug!("executing {:?}", line.trim());
            command.args(&args);
        } else {
            command.args(args);
        }

        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped());

        PipedCommand {
            name,
            command,
            input: None,
        }
    }

    pub fn input(&mut self, input: &'a str) -> &mut Self {
        self.input = Some(input);
        self
    }

    pub fn join(&mut self, level: log::Level) -> Result<(), failure::Error> {
        let mut child = self
            .command
            .spawn()
            .map_err(|err| failure::format_err!("failed to execute command {:?}: {}", self.name, err))?;

        // Write the input to stdio
        if let Some(input) = self.input.take() {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| failure::format_err!("failed to attach stdin of process {:?}", self.name))?;
            stdin.write_all(input.as_bytes())?;
        }

        // Attach the stdout and stderr
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| failure::format_err!("failed to attach stdout of process {:?}", self.name))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| failure::format_err!("failed to attach stderr of process {:?}", self.name))?;

        // Line buffer
        let mut buffer = String::new();
        let flush_buffer = |buffer: &mut String| {
            buffer.lines().for_each(|line| log::log!(level, ">> {}", line));
            buffer.clear();
        };

        let code = loop {
            if let Some(code) = child.try_wait()? {
                break code;
            } else {
                stdout.read_to_string(&mut buffer)?;
                flush_buffer(&mut buffer);
                stderr.read_to_string(&mut buffer)?;
                flush_buffer(&mut buffer);
            }
        };

        if !code.success() {
            Err(failure::format_err!(
                "command {:?} failed with code {}",
                self.name,
                code
            ))
        } else {
            Ok(())
        }
    }
}
