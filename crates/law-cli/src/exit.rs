use std::fmt::Display;
use std::io::{self, BufWriter, ErrorKind, Write};
use std::process::ExitCode;

/// How a run ends, as the process's exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// Nothing of severity error found, or the reader of the output went away.
    Clean,
    /// A finding of severity error, or a failed trial.
    Findings,
    /// A fault, or no configuration found.
    Fault,
}

impl Exit {
    /// Every fault on stderr, one per line; the exit of a fault.
    pub fn faults<D: Display>(faults: impl IntoIterator<Item = D>) -> Self {
        let mut err = io::stderr().lock();
        faults.into_iter().for_each(|fault| {
            let _ = writeln!(err, "{fault}");
        });
        Self::Fault
    }

    /// What a writer writes, on stdout; a closed stdout ends the command cleanly and silently.
    ///
    /// # Errors
    /// The exit that ends the command: clean when stdout was closed, a fault when it failed otherwise.
    pub fn emit(write: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> Result<(), Self> {
        let mut out = BufWriter::new(io::stdout().lock());
        write(&mut out)
            .and_then(|()| out.flush())
            .map_err(|error| match error.kind() {
                ErrorKind::BrokenPipe => Self::Clean,
                _ => Self::faults([error]),
            })
    }
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(match exit {
            Exit::Clean => 0,
            Exit::Findings => 1,
            Exit::Fault => 2,
        })
    }
}
