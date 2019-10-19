use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Write};
use subprocess::{Exec, Redirection};

pub struct PipedCommand<'a> {
    name: &'static str,
    command: Option<Exec>,
    input: Option<&'a str>,
}

impl<'a> PipedCommand<'a> {
    pub fn new(name: &'static str, args: &[&str]) -> Self {
        let cmd = Exec::cmd(name);

        // Log the full command invocation in debug level
        let cmd = if log::log_enabled!(log::Level::Debug) {
            let mut line = format!("{} ", name);
            for arg in args {
                write!(line, "{} ", arg).unwrap();
            }
            log::debug!("executing {:?}", line.trim());
            cmd.args(args)
        } else {
            cmd.args(args)
        };

        let cmd = cmd
            .stdin(Redirection::Pipe)
            .stdout(Redirection::Pipe)
            .stderr(Redirection::Merge);

        PipedCommand {
            name,
            command: Some(cmd),
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
            .take()
            .unwrap()
            .popen()
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
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| failure::format_err!("failed to attach stdout of process {:?}", self.name))?;
        let mut stdout = BufReader::new(stdout);

        // Line buffer
        let mut buffer = String::new();
        let mut empty_line = false;
        let mut flush_buffer = |buffer: &mut String| {
            let line = buffer.trim();

            // Skip all consecutive empty lines after the first empty line
            let should_write = if line.is_empty() && !empty_line {
                empty_line = true;
                true
            } else if line.is_empty() && empty_line {
                false
            } else {
                empty_line = false;
                true
            };

            if should_write {
                log::log!(level, ">> {}", line);
                log::logger().flush();
            }

            buffer.clear();
        };

        let code = loop {
            if let Some(code) = child.poll() {
                break code;
            } else {
                stdout.read_line(&mut buffer)?;
                flush_buffer(&mut buffer);
            }
        };

        // Finish reading stderr/stdout if streams aren't finished yet
        stdout
            .lines()
            .flat_map(Result::ok)
            .for_each(|line| log::log!(level, ">> {}", line));

        if !code.success() {
            Err(failure::format_err!(
                "command {:?} failed with code {:?}",
                self.name,
                code
            ))
        } else {
            Ok(())
        }
    }
}
