//! Shared fixtures for the client's unit tests.

use super::*;
use std::collections::VecDeque;

/// A reader that replays a script of chunks and would-block errors, so reply
/// handling is testable without a socket.
pub enum Step {
    Data(&'static [u8]),
    WouldBlock,
}

pub struct ScriptedReader {
    steps: VecDeque<Step>,
}

impl ScriptedReader {
    pub fn new(steps: impl IntoIterator<Item = Step>) -> Self {
        Self {
            steps: steps.into_iter().collect(),
        }
    }
}

impl Read for ScriptedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.steps.pop_front() {
            Some(Step::Data(bytes)) => {
                assert!(
                    bytes.len() <= buf.len(),
                    "a script chunk must fit one read buffer"
                );
                buf[..bytes.len()].copy_from_slice(bytes);
                Ok(bytes.len())
            }
            Some(Step::WouldBlock) => Err(io::Error::new(io::ErrorKind::WouldBlock, "timed out")),
            None => Ok(0),
        }
    }
}

/// Parse `args` and drive `read_responses` over `steps`, returning the
/// outcome plus everything the client wrote to stdout and stderr.
pub fn read_script(
    steps: impl IntoIterator<Item = Step>,
    args: &[&str],
) -> (Result<ReplyOutcome, String>, String, String) {
    let mut reader = ScriptedReader::new(steps);
    let options = parse_options("client", args.iter().copied().map(OsString::from)).unwrap();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let result = read_responses(&mut reader, &options, None, &mut out, &mut err);
    (
        result,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}
