use crossterm::tty::IsTty;
use rustyline_async::{Readline, ReadlineError, SharedWriter};
use std::io::{BufRead, Lines, Stderr, StdinLock, Stdout, Write};

pub struct ShellRead {
    prompt: String,
    inner: ShellReadInner,
}

#[derive(Clone)]
pub struct ShellWrite {
    inner: ShellWriteInner,
}

enum ShellReadInner {
    Interactive(Readline, SharedWriter),
    Stream(Lines<StdinLock<'static>>),
}

enum ShellWriteInner {
    Interactive(SharedWriter),
    Stream(Stdout, Stderr),
}

pub fn new_shell(prompt: String, disable_interactive: bool) -> (ShellRead, ShellWrite) {
    if !disable_interactive && std::io::stdout().is_tty() {
        let (read_line, writer) = Readline::new(prompt.clone()).unwrap();
        (
            ShellRead {
                prompt,
                inner: ShellReadInner::Interactive(read_line, writer.clone()),
            },
            ShellWrite {
                inner: ShellWriteInner::Interactive(writer),
            },
        )
    } else {
        (
            ShellRead {
                prompt,
                inner: ShellReadInner::Stream(std::io::stdin().lock().lines()),
            },
            ShellWrite {
                inner: ShellWriteInner::Stream(std::io::stdout(), std::io::stderr()),
            },
        )
    }
}

impl ShellRead {
    pub fn read_line(&mut self) -> String {
        match &mut self.inner {
            ShellReadInner::Interactive(read, writer) => {
                let line = match futures::executor::block_on(read.readline()) {
                    Ok(line) => line,
                    Err(ReadlineError::IO(err)) => {
                        eprintln!("An error occurred: {}", err);
                        proc_exit::Code::UNKNOWN.process_exit();
                    }
                    Err(ReadlineError::Eof | ReadlineError::Closed) => {
                        proc_exit::Code::SIGHUP.process_exit()
                    }
                    Err(ReadlineError::Interrupted) => proc_exit::Code::SIGINT.process_exit(),
                };

                read.add_history_entry(line.clone());

                // echo back the line
                writeln!(writer, "{}{}", self.prompt, line).unwrap();

                line
            }
            ShellReadInner::Stream(stream) => match stream.next() {
                Some(Ok(line)) => line,
                Some(Err(err)) => {
                    eprintln!("An error occurred: {}", err);
                    proc_exit::Code::UNKNOWN.process_exit();
                }
                None => proc_exit::Code::UNKNOWN.process_exit(),
            }
        }
    }
}

impl ShellWrite {
    pub fn out(&mut self) -> &mut dyn Write {
        match &mut self.inner {
            ShellWriteInner::Interactive(writer) => writer,
            ShellWriteInner::Stream(stdout, _) => stdout,
        }
    }

    pub fn err(&mut self) -> &mut dyn Write {
        match &mut self.inner {
            ShellWriteInner::Interactive(writer) => writer,
            ShellWriteInner::Stream(_, stderr) => stderr,
        }
    }
}

impl Clone for ShellWriteInner {
    fn clone(&self) -> Self {
        match self {
            ShellWriteInner::Interactive(w) => ShellWriteInner::Interactive(w.clone()),
            ShellWriteInner::Stream(_, _) => {
                ShellWriteInner::Stream(std::io::stdout(), std::io::stderr())
            }
        }
    }
}
