//! GNU's Win32 `DCB` keyword mapping, in plain data.
//!
//! GNU `serial_configure` (src/w32.c:11138-11310) edits a `DCB` in place. The
//! `DCB` type itself is Windows-only, but every VALUE GNU puts into it comes
//! from fixed, platform-free facts: the read-side defaults (`fBinary = TRUE`
//! and friends, w32.c:11161-11168), the parity arm's clear-then-set
//! (:11220-11238), and the flowcontrol arm's clear-then-set with its
//! `XonChar = 17` / `XoffChar = 19` (:11250-11276). Keeping that arithmetic in
//! this cfg-free mirror lets the whole mapping be unit-tested on any host, the
//! same way `termios_backend`'s `SPEEDS` table is host data rather than code
//! that must run on a serial-capable machine. The `w32_backend` only converts
//! between [`SerialDcbSettings`] and the real `DCB` at the `GetCommState` /
//! `SetCommState` boundary.
//!
//! The DCB `f*` members are one C bitfield; `winbase.h` fixes the layout that
//! `pack_flags`/`unpack_flags` encode, and the layout is part of the ABI
//! `SetCommState` consumes, so it is pinned here by tests rather than trusted.

use super::{SerialByteSize, SerialFlowControl, SerialParity, SerialStopBits};

/// `DTR_CONTROL_DISABLE` / `RTS_CONTROL_DISABLE` / handshake values, exactly
/// the four constants GNU names (src/w32.c:11250-11260, via winbase.h).
pub(crate) const DTR_CONTROL_DISABLE: u32 = 0;
pub(crate) const DTR_CONTROL_HANDSHAKE: u32 = 2;
pub(crate) const RTS_CONTROL_DISABLE: u32 = 0;
pub(crate) const RTS_CONTROL_HANDSHAKE: u32 = 2;

/// `NOPARITY` / `ODDPARITY` / `EVENPARITY` (winbase.h; w32.c:11224-11236).
pub(crate) const NOPARITY: u8 = 0;
pub(crate) const ODDPARITY: u8 = 1;
pub(crate) const EVENPARITY: u8 = 2;

/// `ONESTOPBIT` / `TWOSTOPBITS` (winbase.h; w32.c:11247-11249).
pub(crate) const ONESTOPBIT: u8 = 0;
pub(crate) const TWOSTOPBITS: u8 = 2;

/// GNU's local copy of the `DCB` (`struct _DCB dcb`, w32.c:11143) with the
/// bitfield members broken out into named fields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SerialDcbSettings {
    /// `dcb.DCBlength` is `sizeof (DCB)` on Windows; the mirror keeps the
    /// field so the round-trip never loses it, but it carries no keyword
    /// meaning.
    pub(crate) dcb_length: u32,
    pub(crate) baud_rate: u32,
    pub(crate) f_binary: bool,
    pub(crate) f_parity: bool,
    pub(crate) f_outx_cts_flow: bool,
    pub(crate) f_outx_dsr_flow: bool,
    pub(crate) f_dtr_control: u32,
    pub(crate) f_dsr_sensitivity: bool,
    pub(crate) f_tx_continue_on_xoff: bool,
    pub(crate) f_out_x: bool,
    pub(crate) f_in_x: bool,
    pub(crate) f_error_char: bool,
    pub(crate) f_null: bool,
    pub(crate) f_rts_control: u32,
    pub(crate) f_abort_on_error: bool,
    pub(crate) xon_lim: u16,
    pub(crate) xoff_lim: u16,
    pub(crate) byte_size: u8,
    pub(crate) parity: u8,
    pub(crate) stop_bits: u8,
    pub(crate) xon_char: i8,
    pub(crate) xoff_char: i8,
    pub(crate) error_char: i8,
    pub(crate) eof_char: i8,
    pub(crate) evt_char: i8,
}

/// GNU's read-side defaults, applied right after `GetCommState` fills the
/// struct from the device (w32.c:11161-11168). `XonLim`/`XoffLim` and the
/// speeds GNU never touches keep whatever `GetCommState` produced; the comment
/// at w32.c:11167 says so explicitly.
pub(crate) fn apply_w32_read_defaults(settings: &mut SerialDcbSettings) {
    settings.f_binary = true;
    settings.f_null = false;
    settings.f_abort_on_error = false;
    settings.error_char = 0;
    settings.eof_char = 0;
    settings.evt_char = 0;
}

/// GNU's `:speed` arm, w32.c:11170-11176: the fixnum is stored raw -- there is
/// no `cfsetspeed` conversion on Windows and no table, so any integer (in C, a
/// possibly-wrapping `DWORD` cast) reaches the driver, which accepts or
/// rejects it at `SetCommState`.
pub(crate) fn set_w32_speed(settings: &mut SerialDcbSettings, speed: i64) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let baud = speed as u32;
    settings.baud_rate = baud;
}

/// GNU's `ByteSize` arm, w32.c:11183-11192.
pub(crate) fn set_w32_byte_size(settings: &mut SerialDcbSettings, size: SerialByteSize) {
    settings.byte_size = match size {
        SerialByteSize::Seven => 7,
        SerialByteSize::Eight => 8,
    };
}

/// GNU's parity arm, w32.c:11206-11238: the defaults are cleared for EVERY
/// value (including `nil`), then `even`/`odd` set their own. GNU reports the
/// character for the process `:summary` in the same arm; that stays in the
/// caller, which owns the contact plist.
pub(crate) fn set_w32_parity(settings: &mut SerialDcbSettings, parity: SerialParity) {
    settings.f_parity = false;
    settings.parity = NOPARITY;
    settings.f_error_char = false;
    match parity {
        SerialParity::None => {}
        SerialParity::Even => {
            settings.f_parity = true;
            settings.parity = EVENPARITY;
            settings.f_error_char = true;
        }
        SerialParity::Odd => {
            settings.f_parity = true;
            settings.parity = ODDPARITY;
            settings.f_error_char = true;
        }
    }
}

/// GNU's stopbits arm, w32.c:11240-11249.
pub(crate) fn set_w32_stop_bits(settings: &mut SerialDcbSettings, bits: SerialStopBits) {
    settings.stop_bits = match bits {
        SerialStopBits::One => ONESTOPBIT,
        SerialStopBits::Two => TWOSTOPBITS,
    };
}

/// GNU's flowcontrol arm, w32.c:11251-11276: the whole handshake block is
/// zeroed for every value (nil included), `XonChar`/`XoffChar` are set to
/// Ctrl-Q/Ctrl-S unconditionally, then `hw`/`sw` set their bits.
pub(crate) fn set_w32_flow_control(settings: &mut SerialDcbSettings, flow: SerialFlowControl) {
    settings.f_outx_cts_flow = false;
    settings.f_outx_dsr_flow = false;
    settings.f_dtr_control = DTR_CONTROL_DISABLE;
    settings.f_dsr_sensitivity = false;
    settings.f_tx_continue_on_xoff = false;
    settings.f_out_x = false;
    settings.f_in_x = false;
    settings.f_rts_control = RTS_CONTROL_DISABLE;
    settings.xon_char = 17; // Ctrl-Q
    settings.xoff_char = 19; // Ctrl-S
    match flow {
        SerialFlowControl::None => {}
        SerialFlowControl::Hardware => {
            settings.f_rts_control = RTS_CONTROL_HANDSHAKE;
            settings.f_outx_cts_flow = true;
        }
        SerialFlowControl::Software => {
            settings.f_out_x = true;
            settings.f_in_x = true;
        }
    }
}

// winbase.h `_bitfield` layout, LSB first:
//
//   bit 0      fBinary
//   bit 1      fParity
//   bit 2      fOutxCtsFlow
//   bit 3      fOutxDsrFlow
//   bits 4-5   fDtrControl
//   bit 6      fDsrSensitivity
//   bit 7      fTXContinueOnXoff
//   bit 8      fOutX
//   bit 9      fInX
//   bit 10     fErrorChar
//   bit 11     fNull
//   bits 12-13 fRtsControl
//   bit 14     fAbortOnError
//   bits 15-31 fDummy2 (reserved)
const BIT_F_BINARY: u32 = 1 << 0;
const BIT_F_PARITY: u32 = 1 << 1;
const BIT_F_OUTX_CTS_FLOW: u32 = 1 << 2;
const BIT_F_OUTX_DSR_FLOW: u32 = 1 << 3;
const SHIFT_F_DTR_CONTROL: u32 = 4;
const MASK_F_DTR_CONTROL: u32 = 0b11 << SHIFT_F_DTR_CONTROL;
const BIT_F_DSR_SENSITIVITY: u32 = 1 << 6;
const BIT_F_TX_CONTINUE_ON_XOFF: u32 = 1 << 7;
const BIT_F_OUT_X: u32 = 1 << 8;
const BIT_F_IN_X: u32 = 1 << 9;
const BIT_F_ERROR_CHAR: u32 = 1 << 10;
const BIT_F_NULL: u32 = 1 << 11;
const SHIFT_F_RTS_CONTROL: u32 = 12;
const MASK_F_RTS_CONTROL: u32 = 0b11 << SHIFT_F_RTS_CONTROL;
const BIT_F_ABORT_ON_ERROR: u32 = 1 << 14;

/// Pack the named members into the `DCB._bitfield` dword.
pub(crate) fn pack_flags(settings: &SerialDcbSettings) -> u32 {
    let mut bits = 0;
    if settings.f_binary {
        bits |= BIT_F_BINARY;
    }
    if settings.f_parity {
        bits |= BIT_F_PARITY;
    }
    if settings.f_outx_cts_flow {
        bits |= BIT_F_OUTX_CTS_FLOW;
    }
    if settings.f_outx_dsr_flow {
        bits |= BIT_F_OUTX_DSR_FLOW;
    }
    bits |= (settings.f_dtr_control << SHIFT_F_DTR_CONTROL) & MASK_F_DTR_CONTROL;
    if settings.f_dsr_sensitivity {
        bits |= BIT_F_DSR_SENSITIVITY;
    }
    if settings.f_tx_continue_on_xoff {
        bits |= BIT_F_TX_CONTINUE_ON_XOFF;
    }
    if settings.f_out_x {
        bits |= BIT_F_OUT_X;
    }
    if settings.f_in_x {
        bits |= BIT_F_IN_X;
    }
    if settings.f_error_char {
        bits |= BIT_F_ERROR_CHAR;
    }
    if settings.f_null {
        bits |= BIT_F_NULL;
    }
    bits |= (settings.f_rts_control << SHIFT_F_RTS_CONTROL) & MASK_F_RTS_CONTROL;
    if settings.f_abort_on_error {
        bits |= BIT_F_ABORT_ON_ERROR;
    }
    bits
}

/// Unpack a `DCB._bitfield` dword into the named members.
pub(crate) fn unpack_flags(bits: u32, settings: &mut SerialDcbSettings) {
    settings.f_binary = bits & BIT_F_BINARY != 0;
    settings.f_parity = bits & BIT_F_PARITY != 0;
    settings.f_outx_cts_flow = bits & BIT_F_OUTX_CTS_FLOW != 0;
    settings.f_outx_dsr_flow = bits & BIT_F_OUTX_DSR_FLOW != 0;
    settings.f_dtr_control = (bits & MASK_F_DTR_CONTROL) >> SHIFT_F_DTR_CONTROL;
    settings.f_dsr_sensitivity = bits & BIT_F_DSR_SENSITIVITY != 0;
    settings.f_tx_continue_on_xoff = bits & BIT_F_TX_CONTINUE_ON_XOFF != 0;
    settings.f_out_x = bits & BIT_F_OUT_X != 0;
    settings.f_in_x = bits & BIT_F_IN_X != 0;
    settings.f_error_char = bits & BIT_F_ERROR_CHAR != 0;
    settings.f_null = bits & BIT_F_NULL != 0;
    settings.f_rts_control = (bits & MASK_F_RTS_CONTROL) >> SHIFT_F_RTS_CONTROL;
    settings.f_abort_on_error = bits & BIT_F_ABORT_ON_ERROR != 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A settings copy the way GNU's looks after `GetCommState` plus the
    /// read-side defaults, before any keyword arm has run.
    fn defaulted() -> SerialDcbSettings {
        let mut settings = SerialDcbSettings {
            dcb_length: 28,
            baud_rate: 1200,
            // GetCommState can leave handshake bits on; GNU's arms clear them.
            f_outx_cts_flow: true,
            f_dtr_control: DTR_CONTROL_HANDSHAKE,
            f_rts_control: RTS_CONTROL_HANDSHAKE,
            xon_lim: 1024,
            xoff_lim: 1024,
            byte_size: 7,
            parity: ODDPARITY,
            stop_bits: TWOSTOPBITS,
            xon_char: 0,
            xoff_char: 0,
            error_char: 5,
            eof_char: 5,
            evt_char: 5,
            ..SerialDcbSettings::default()
        };
        apply_w32_read_defaults(&mut settings);
        settings
    }

    #[test]
    fn read_defaults_match_w32_c() {
        let settings = defaulted();
        assert!(settings.f_binary);
        assert!(!settings.f_null);
        assert!(!settings.f_abort_on_error);
        assert_eq!(settings.error_char, 0);
        assert_eq!(settings.eof_char, 0);
        assert_eq!(settings.evt_char, 0);
        // Untouched by the defaults: XonLim/XoffLim (w32.c:11167 comment) and
        // whatever speed the device had.
        assert_eq!(settings.xon_lim, 1024);
        assert_eq!(settings.baud_rate, 1200);
    }

    #[test]
    fn speed_is_stored_raw_without_a_speed_table() {
        let mut settings = defaulted();
        set_w32_speed(&mut settings, 9600);
        assert_eq!(settings.baud_rate, 9600);
        // A value no termios table knows still lands in BaudRate unconverted;
        // Windows accepts or rejects it at SetCommState (w32.c:11176 has no
        // validation, unlike sysdep.c's cfsetspeed arm).
        set_w32_speed(&mut settings, 12345);
        assert_eq!(settings.baud_rate, 12345);
    }

    #[test]
    fn parity_arm_sets_dcbs_even_odd_postures() {
        let mut settings = defaulted();
        set_w32_parity(&mut settings, SerialParity::None);
        assert!(!settings.f_parity);
        assert_eq!(settings.parity, NOPARITY);
        assert!(!settings.f_error_char);

        set_w32_parity(&mut settings, SerialParity::Even);
        assert!(settings.f_parity);
        assert_eq!(settings.parity, EVENPARITY);
        assert!(settings.f_error_char);

        set_w32_parity(&mut settings, SerialParity::Odd);
        assert!(settings.f_parity);
        assert_eq!(settings.parity, ODDPARITY);
        assert!(settings.f_error_char);
    }

    #[test]
    fn flowcontrol_arm_clears_then_sets_and_always_sets_xon_xoff() {
        let mut settings = defaulted();
        // The defaulted() copy carries leftover handshake bits, as a real
        // GetCommState can; the nil arm must clear them all.
        set_w32_flow_control(&mut settings, SerialFlowControl::None);
        assert!(!settings.f_outx_cts_flow);
        assert_eq!(settings.f_dtr_control, DTR_CONTROL_DISABLE);
        assert_eq!(settings.f_rts_control, RTS_CONTROL_DISABLE);
        assert!(!settings.f_out_x);
        assert!(!settings.f_in_x);
        assert_eq!(settings.xon_char, 17);
        assert_eq!(settings.xoff_char, 19);

        set_w32_flow_control(&mut settings, SerialFlowControl::Hardware);
        assert!(settings.f_outx_cts_flow);
        assert_eq!(settings.f_rts_control, RTS_CONTROL_HANDSHAKE);
        assert!(!settings.f_out_x);

        set_w32_flow_control(&mut settings, SerialFlowControl::Software);
        assert!(!settings.f_outx_cts_flow);
        assert_eq!(settings.f_rts_control, RTS_CONTROL_DISABLE);
        assert!(settings.f_out_x);
        assert!(settings.f_in_x);
    }

    #[test]
    fn byte_size_and_stopbits_arms_write_the_winbase_constants() {
        let mut settings = defaulted();
        set_w32_byte_size(&mut settings, SerialByteSize::Seven);
        assert_eq!(settings.byte_size, 7);
        set_w32_byte_size(&mut settings, SerialByteSize::Eight);
        assert_eq!(settings.byte_size, 8);
        set_w32_stop_bits(&mut settings, SerialStopBits::One);
        assert_eq!(settings.stop_bits, ONESTOPBIT);
        set_w32_stop_bits(&mut settings, SerialStopBits::Two);
        assert_eq!(settings.stop_bits, TWOSTOPBITS);
    }

    #[test]
    fn flags_round_trip_through_the_winbase_bit_layout() {
        let mut settings = defaulted();
        set_w32_flow_control(&mut settings, SerialFlowControl::Hardware);
        set_w32_parity(&mut settings, SerialParity::Even);
        let packed = pack_flags(&settings);
        let mut back = SerialDcbSettings::default();
        unpack_flags(packed, &mut back);
        assert_eq!(
            (back.f_binary, back.f_parity, back.f_error_char),
            (true, true, true)
        );
        assert_eq!(
            (back.f_outx_cts_flow, back.f_rts_control),
            (true, RTS_CONTROL_HANDSHAKE)
        );
        assert_eq!(back.f_dtr_control, DTR_CONTROL_DISABLE);
        // The bit positions themselves: parity even is bit 1, RTS handshake is
        // bits 12-13 = 0x3000.
        assert_eq!(packed & (1 << 1), 1 << 1);
        assert_eq!(packed & 0x3000, 0x2000);
    }
}
