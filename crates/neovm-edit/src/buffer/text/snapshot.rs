#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BufferTextBytesSnapshot {
    bytes: Vec<u8>,
    multibyte: bool,
}

impl BufferTextBytesSnapshot {
    pub(crate) const fn new(bytes: Vec<u8>, multibyte: bool) -> Self {
        Self { bytes, multibyte }
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) const fn is_multibyte(&self) -> bool {
        self.multibyte
    }

    pub(crate) fn into_parts(self) -> (Vec<u8>, bool) {
        (self.bytes, self.multibyte)
    }
}
