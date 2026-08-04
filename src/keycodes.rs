//! macOS hardware keycode (CGKeyCode) → Windows VK translation, so the rest of
//! the engine keeps using VK codes as the cross-platform key identity
//! (`mapping.rs`). CGKeyCodes are positional (US ANSI layout); v1 therefore
//! assumes a US layout for *letters* too on macOS — on Windows letters follow
//! the active layout. Layout-true characters (`UCKeyTranslate`) are deferred,
//! mirroring the planned `ToUnicodeEx` refinement on Windows (DESIGN.md §8).

/// VK for "unknown/unmapped macOS keycode". Plays the default drum like any
/// other unmapped key; all unknowns share one voice key for NoteOff pairing
/// (acceptable: rare keys, and Windows lumps them into the same drum anyway).
pub const VK_UNKNOWN: u8 = 0xFF;

/// `(CGKeyCode, Windows VK)` pairs. Keycodes are the `kVK_*` constants from
/// HIToolbox `Events.h` (ANSI positional); VKs match `mapping.rs`.
const PAIRS: &[(u16, u8)] = &[
    // Letters (positional, US ANSI)
    (0x00, 0x41), // kVK_ANSI_A → VK_A
    (0x01, 0x53), // S
    (0x02, 0x44), // D
    (0x03, 0x46), // F
    (0x04, 0x48), // H
    (0x05, 0x47), // G
    (0x06, 0x5A), // Z
    (0x07, 0x58), // X
    (0x08, 0x43), // C
    (0x09, 0x56), // V
    (0x0B, 0x42), // B
    (0x0C, 0x51), // Q
    (0x0D, 0x57), // W
    (0x0E, 0x45), // E
    (0x0F, 0x52), // R
    (0x10, 0x59), // Y
    (0x11, 0x54), // T
    (0x1F, 0x4F), // O
    (0x20, 0x55), // U
    (0x22, 0x49), // I
    (0x23, 0x50), // P
    (0x25, 0x4C), // L
    (0x26, 0x4A), // J
    (0x28, 0x4B), // K
    (0x2D, 0x4E), // N
    (0x2E, 0x4D), // M
    // Digits
    (0x12, 0x31), // 1
    (0x13, 0x32), // 2
    (0x14, 0x33), // 3
    (0x15, 0x34), // 4
    (0x17, 0x35), // 5
    (0x16, 0x36), // 6
    (0x1A, 0x37), // 7
    (0x1C, 0x38), // 8
    (0x19, 0x39), // 9
    (0x1D, 0x30), // 0
    // OEM punctuation (US)
    (0x18, 0xBB), // = → VK_OEM_PLUS
    (0x1B, 0xBD), // - → VK_OEM_MINUS
    (0x1E, 0xDD), // ] → VK_OEM_6
    (0x21, 0xDB), // [ → VK_OEM_4
    (0x27, 0xDE), // ' → VK_OEM_7
    (0x29, 0xBA), // ; → VK_OEM_1
    (0x2A, 0xDC), // \ → VK_OEM_5
    (0x2B, 0xBC), // , → VK_OEM_COMMA
    (0x2C, 0xBF), // / → VK_OEM_2
    (0x2F, 0xBE), // . → VK_OEM_PERIOD
    (0x32, 0xC0), // ` → VK_OEM_3
    // Whitespace / editing / system
    (0x24, 0x0D), // Return → VK_RETURN
    (0x30, 0x09), // Tab → VK_TAB
    (0x31, 0x20), // Space → VK_SPACE
    (0x33, 0x08), // Backspace → VK_BACK
    (0x35, 0x1B), // Escape → VK_ESCAPE
    (0x39, 0x14), // CapsLock → VK_CAPITAL
    (0x75, 0x2E), // ForwardDelete → VK_DELETE
    (0x72, 0x2D), // Help (Insert position) → VK_INSERT
    // Navigation
    (0x73, 0x24), // Home → VK_HOME
    (0x77, 0x23), // End → VK_END
    (0x74, 0x21), // PageUp → VK_PRIOR
    (0x79, 0x22), // PageDown → VK_NEXT
    (0x7B, 0x25), // LeftArrow → VK_LEFT
    (0x7C, 0x27), // RightArrow → VK_RIGHT
    (0x7D, 0x28), // DownArrow → VK_DOWN
    (0x7E, 0x26), // UpArrow → VK_UP
    // Modifiers (arrive via flagsChanged)
    (0x38, 0xA0), // Shift → VK_LSHIFT
    (0x3C, 0xA1), // RightShift → VK_RSHIFT
    (0x3B, 0xA2), // Control → VK_LCONTROL
    (0x3E, 0xA3), // RightControl → VK_RCONTROL
    (0x3A, 0xA4), // Option → VK_LMENU
    (0x3D, 0xA5), // RightOption → VK_RMENU
    (0x37, 0x5B), // Command → VK_LWIN
    (0x36, 0x5C), // RightCommand → VK_RWIN
    // Function keys (F1..=F20 → VK_F1..=VK_F20)
    (0x7A, 0x70), // F1
    (0x78, 0x71), // F2
    (0x63, 0x72), // F3
    (0x76, 0x73), // F4
    (0x60, 0x74), // F5
    (0x61, 0x75), // F6
    (0x62, 0x76), // F7
    (0x64, 0x77), // F8
    (0x65, 0x78), // F9
    (0x6D, 0x79), // F10
    (0x6F, 0x7B), // F12
    (0x67, 0x7A), // F11
    (0x69, 0x7C), // F13
    (0x6B, 0x7D), // F14
    (0x71, 0x7E), // F15
    (0x6A, 0x7F), // F16
    (0x40, 0x80), // F17
    (0x4F, 0x81), // F18
    (0x50, 0x82), // F19
    (0x5A, 0x83), // F20
    // Keypad (distinct VK_NUMPAD identity; all play the default drum)
    (0x52, 0x60), // Keypad0 → VK_NUMPAD0
    (0x53, 0x61), // Keypad1
    (0x54, 0x62), // Keypad2
    (0x55, 0x63), // Keypad3
    (0x56, 0x64), // Keypad4
    (0x57, 0x65), // Keypad5
    (0x58, 0x66), // Keypad6
    (0x59, 0x67), // Keypad7
    (0x5B, 0x68), // Keypad8
    (0x5C, 0x69), // Keypad9
    (0x41, 0x6E), // KeypadDecimal → VK_DECIMAL
    (0x43, 0x6A), // KeypadMultiply → VK_MULTIPLY
    (0x45, 0x6B), // KeypadPlus → VK_ADD
    (0x4E, 0x6D), // KeypadMinus → VK_SUBTRACT
    (0x4B, 0x6F), // KeypadDivide → VK_DIVIDE
    (0x4C, 0x0D), // KeypadEnter → VK_RETURN (same voice key as Return)
    (0x47, 0x90), // KeypadClear (NumLock position) → VK_NUMLOCK
    (0x51, 0x92), // KeypadEquals → VK_OEM_NEC_EQUAL
    // Volume keys (older Apple keyboards; distinct identity, default drum)
    (0x4A, 0xAD), // VolumeMute → VK_VOLUME_MUTE
    (0x49, 0xAE), // VolumeDown → VK_VOLUME_DOWN
    (0x48, 0xAF), // VolumeUp → VK_VOLUME_UP
];

/// Lookup table built from `PAIRS` at compile time.
static TABLE: [u8; 256] = {
    let mut t = [VK_UNKNOWN; 256];
    let mut i = 0;
    while i < PAIRS.len() {
        t[PAIRS[i].0 as usize] = PAIRS[i].1;
        i += 1;
    }
    t
};

/// Translate a macOS CGKeyCode to the Windows VK key identity.
/// Unknown keycodes (e.g. JIS-only keys) map to [`VK_UNKNOWN`].
pub fn vk_for_keycode(keycode: u16) -> u8 {
    TABLE.get(keycode as usize).copied().unwrap_or(VK_UNKNOWN)
}

/// Reverse lookup (config hotkey VK → CGKeyCode). Returns `None` for VKs with
/// no macOS counterpart; the one ambiguous pair (Return/KeypadEnter) resolves
/// to Return.
pub fn keycode_for_vk(vk: u8) -> Option<u16> {
    PAIRS.iter().find(|&&(_, v)| v == vk).map(|&(k, _)| k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn keycodes_are_unique() {
        let mut seen = HashSet::new();
        for &(kc, _) in PAIRS {
            assert!(seen.insert(kc), "duplicate keycode {kc:#04x}");
        }
    }

    #[test]
    fn vks_are_unique_except_documented_aliases() {
        let mut seen = HashSet::new();
        for &(kc, vk) in PAIRS {
            if vk == 0x0D {
                continue; // Return and KeypadEnter intentionally share VK_RETURN
            }
            assert!(
                seen.insert(vk),
                "duplicate vk {vk:#04x} (keycode {kc:#04x})"
            );
        }
    }

    #[test]
    fn us_layout_spot_checks() {
        assert_eq!(vk_for_keycode(0x00), 0x41); // A → VK_A
        assert_eq!(vk_for_keycode(0x06), 0x5A); // Z → VK_Z
        assert_eq!(vk_for_keycode(0x12), 0x31); // 1 → VK_1
        assert_eq!(vk_for_keycode(0x1D), 0x30); // 0 → VK_0
        assert_eq!(vk_for_keycode(0x33), 0x08); // Backspace → VK_BACK
        assert_eq!(vk_for_keycode(0x31), 0x20); // Space → VK_SPACE
        assert_eq!(vk_for_keycode(0x2F), 0xBE); // . → VK_OEM_PERIOD
        assert_eq!(vk_for_keycode(0x27), 0xDE); // ' → VK_OEM_7
        assert_eq!(vk_for_keycode(0x38), 0xA0); // Shift → VK_LSHIFT
        assert_eq!(vk_for_keycode(0x37), 0x5B); // Command → VK_LWIN
    }

    #[test]
    fn function_keys() {
        assert_eq!(vk_for_keycode(0x7A), 0x70); // F1 → VK_F1
        assert_eq!(vk_for_keycode(0x6F), 0x7B); // F12 → VK_F12 (the mute hotkey)
        assert_eq!(vk_for_keycode(0x5A), 0x83); // F20 → VK_F20
    }

    #[test]
    fn unknown_keycodes_fall_back() {
        assert_eq!(vk_for_keycode(0x0A), VK_UNKNOWN); // ISO § key (not mapped)
        assert_eq!(vk_for_keycode(0x3F), VK_UNKNOWN); // Fn
        assert_eq!(vk_for_keycode(0x7F), VK_UNKNOWN);
        assert_eq!(vk_for_keycode(0xFFFF), VK_UNKNOWN);
    }

    #[test]
    fn reverse_lookup() {
        assert_eq!(keycode_for_vk(0x7B), Some(0x6F)); // VK_F12 → macOS F12
        assert_eq!(keycode_for_vk(0x41), Some(0x00)); // VK_A
        assert_eq!(keycode_for_vk(0x0D), Some(0x24)); // VK_RETURN → Return (not keypad)
        assert_eq!(keycode_for_vk(0x87), None); // VK_F24: no macOS keycode
    }
}
