//! Hot clipboard installer. One `Copy` arm, so the stub match cannot grow.

/// Serial installer. An owned arm is a different type, not a variant here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionInstaller {
    InlineSerialMain,
}
