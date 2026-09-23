//! CCL command field. GNU `src/ccl.c` assigns every 5-bit opcode.

/// One CCL command. Discriminants are the GNU opcode numbers (`code & 0x1F`).
///
/// The driver matches this enum with no wildcard, so adding a command is a
/// compile error until that command has an execution arm. [`strum::FromRepr`]
/// generates `from_repr`, a safe `const` match from those discriminants.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::FromRepr)]
pub(super) enum CclCommand {
    SetRegister = 0x00,
    SetShortConst = 0x01,
    SetConst = 0x02,
    SetArray = 0x03,
    Jump = 0x04,
    JumpCond = 0x05,
    WriteRegisterJump = 0x06,
    WriteRegisterReadJump = 0x07,
    WriteConstJump = 0x08,
    WriteConstReadJump = 0x09,
    WriteStringJump = 0x0a,
    WriteArrayReadJump = 0x0b,
    ReadJump = 0x0c,
    Branch = 0x0d,
    ReadRegister = 0x0e,
    WriteExprConst = 0x0f,
    ReadBranch = 0x10,
    WriteRegister = 0x11,
    WriteExprRegister = 0x12,
    Call = 0x13,
    WriteConstString = 0x14,
    WriteArray = 0x15,
    End = 0x16,
    ExprSelfConst = 0x17,
    ExprSelfReg = 0x18,
    SetExprConst = 0x19,
    SetExprReg = 0x1a,
    JumpCondExprConst = 0x1b,
    JumpCondExprReg = 0x1c,
    ReadJumpCondExprConst = 0x1d,
    ReadJumpCondExprReg = 0x1e,
    Extension = 0x1f,
}
