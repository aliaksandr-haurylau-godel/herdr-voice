//! The one place the daemon writes to standard error.
//!
//! herdr connects the daemon's standard error to a pipe it reads. When herdr is
//! gone nobody reads it, and `eprintln!` panics on the failed write, which took
//! the connection thread down before it wrote a reply (issue #93). A journal line
//! is a courtesy to whoever reads the log, never a condition of answering, so a
//! failed write is dropped.

use std::io::Write;

/// Writes `line` and a newline to `sink`; a failed write is ignored.
pub fn write_line(sink: &mut dyn Write, line: &str) {
    let _ = writeln!(sink, "{line}");
}

/// The same, to the process's standard error.
pub fn line(line: &str) {
    write_line(&mut std::io::stderr(), line);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Write};

    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
    }

    #[test]
    fn a_line_is_written_with_a_newline() {
        let mut sink = Vec::new();
        write_line(&mut sink, "listening at x");
        assert_eq!(sink, b"listening at x\n");
    }

    #[test]
    fn a_failed_write_is_ignored() {
        write_line(&mut Broken, "request command=cancel");
    }

    #[test]
    fn the_process_writer_does_not_panic() {
        line("stderr writer check");
    }
}
