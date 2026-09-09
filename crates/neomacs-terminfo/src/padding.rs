//! Native tputs with a terminal-specific output descriptor.
use std::io::{self, Write};

/// Padding policy for one output device. The duplicated descriptor keeps baud
/// rate discovery tied to this device, including secondary terminals.
#[derive(Clone, Debug)]
pub struct Padding {
    term: String,
    file: std::sync::Arc<std::fs::File>,
}

impl Padding {
    #[cfg(unix)]
    pub fn new(term: &str, output: &impl std::os::fd::AsFd) -> io::Result<Self> {
        if term.is_empty() || term.contains('\0') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid terminal name",
            ));
        }
        Ok(Self {
            term: term.to_owned(),
            file: std::sync::Arc::new(output.as_fd().try_clone_to_owned()?.into()),
        })
    }

    /// Emit an already-expanded capability, including its padding. Pass glyph
    /// text directly to the writer instead. `affected_lines` scales `*` delays.
    /// The writer must not reenter this crate from write/flush callbacks.
    pub fn write(
        &self,
        output: &mut dyn Write,
        sequence: &[u8],
        affected_lines: usize,
    ) -> io::Result<()> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            use std::os::fd::AsRawFd;
            crate::native::write_padded(
                &self.term,
                self.file.as_raw_fd(),
                output,
                sequence,
                affected_lines,
            )
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = (&self.term, &self.file, output, sequence, affected_lines);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                crate::Error::UnsupportedPlatform,
            ))
        }
    }
}
