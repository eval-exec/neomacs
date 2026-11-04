#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct BufferTextBytesSnapshot {
    bytes: Vec<u8>,
    multibyte: bool,
}

impl BufferTextBytesSnapshot {
    #[doc(hidden)]
    pub const fn new(bytes: Vec<u8>, multibyte: bool) -> Self {
        Self { bytes, multibyte }
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) const fn is_multibyte(&self) -> bool {
        self.multibyte
    }

    #[doc(hidden)]
    pub fn into_parts(self) -> (Vec<u8>, bool) {
        (self.bytes, self.multibyte)
    }
}
