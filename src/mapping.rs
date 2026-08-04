//! The aural-coding musical mapping, ported verbatim (DESIGN.md §3).
//!
//! Pure functions over virtual-key codes — no platform dependencies, fully unit-tested.
//! `vk` values are Windows VK codes, which v1 uses as the cross-platform key identity.

/// Which sample bank a note plays from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instrument {
    Piano,
    Drums,
}

/// A key press resolved to a musical note.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MappedNote {
    pub instrument: Instrument,
    pub midi: u8,
    pub velocity: f32,
}

// Original aural-coding range: firstKey = 0x15, lastKey = 0x6C.
pub const FIRST_KEY: u8 = 0x15; // MIDI 21 (A0)
pub const LAST_KEY: u8 = 0x6C; // MIDI 108 (C8)

/// `majorScaleNotes` from aural-coding: MIDI 21..=108 filtered by
/// `((index + 4) % 12) in {0,2,4,5,7,9,11}`.
///
/// The original author commented "C Major Scale. (I think?)" — by pitch class the filter
/// actually yields C Mixolydian (C D E F G A Bb). The formula is the spec; ported verbatim.
pub const MAJOR_SCALE_NOTES: [u8; 52] = {
    let mut out = [0u8; 52];
    let mut n = 0usize;
    let mut midi = FIRST_KEY;
    while midi <= LAST_KEY {
        let i = midi - FIRST_KEY;
        if matches!((i + 4) % 12, 0 | 2 | 4 | 5 | 7 | 9 | 11) {
            out[n] = midi;
            n += 1;
        }
        midi += 1;
    }
    assert!(n == 52, "MAJOR_SCALE_NOTES length drifted");
    out
};

// --- Virtual-key codes used by the v1 mapping (US-layout for symbol keys) ---

pub const VK_BACK: u8 = 0x08;
pub const VK_TAB: u8 = 0x09;
pub const VK_SHIFT: u8 = 0x10;
pub const VK_CONTROL: u8 = 0x11;
pub const VK_MENU: u8 = 0x12; // Alt
pub const VK_SPACE: u8 = 0x20;
pub const VK_DELETE: u8 = 0x2E;
pub const VK_0: u8 = 0x30;
pub const VK_1: u8 = 0x31;
pub const VK_9: u8 = 0x39;
pub const VK_A: u8 = 0x41;
pub const VK_Z: u8 = 0x5A;
pub const VK_LWIN: u8 = 0x5B;
pub const VK_RWIN: u8 = 0x5C;
pub const VK_APPS: u8 = 0x5D;
pub const VK_LSHIFT: u8 = 0xA0;
pub const VK_RSHIFT: u8 = 0xA1;
pub const VK_LCONTROL: u8 = 0xA2;
pub const VK_RCONTROL: u8 = 0xA3;
pub const VK_LMENU: u8 = 0xA4;
pub const VK_RMENU: u8 = 0xA5;
pub const VK_OEM_PLUS: u8 = 0xBB; // =/+ key
pub const VK_OEM_4: u8 = 0xDB; // [/{
pub const VK_OEM_6: u8 = 0xDD; // ]/}
pub const VK_OEM_7: u8 = 0xDE; // '/"
pub const VK_OEM_PERIOD: u8 = 0xBE; // ./>

/// Modifier keys are silent in the original (`key = null if key in ['meta','shift','control','alt']`).
pub fn is_modifier(vk: u8) -> bool {
    matches!(
        vk,
        VK_SHIFT
            | VK_CONTROL
            | VK_MENU
            | VK_LWIN
            | VK_RWIN
            | VK_APPS
            | VK_LSHIFT
            | VK_RSHIFT
            | VK_LCONTROL
            | VK_RCONTROL
            | VK_LMENU
            | VK_RMENU
    )
}

fn drum(midi: u8, velocity: f32) -> Option<MappedNote> {
    Some(MappedNote {
        instrument: Instrument::Drums,
        midi,
        velocity,
    })
}

/// The verbatim `bufferForEvent` port.
///
/// Letters always play piano at velocity 1.0: `index = 24 + (code - 'A') % 12`,
/// `+12` when shifted (uppercase in the original). Everything else plays drums;
/// the drum MIDI numbers are the original's General-MIDI-percussion choices.
pub fn map_key(vk: u8, shift: bool) -> Option<MappedNote> {
    if is_modifier(vk) {
        return None;
    }
    match vk {
        VK_A..=VK_Z => {
            let mut idx = 24 + (vk - VK_A) as usize % 12;
            if shift {
                idx += 12;
            }
            Some(MappedNote {
                instrument: Instrument::Piano,
                midi: MAJOR_SCALE_NOTES[idx],
                velocity: 1.0,
            })
        }
        VK_BACK => drum(50, 1.0),
        VK_DELETE => drum(49, 1.0),
        VK_SPACE => drum(41, 0.025),
        VK_TAB => drum(41, 0.2),
        VK_OEM_PERIOD => drum(56, 0.2),                     // '.'
        VK_OEM_7 => drum(if shift { 57 } else { 58 }, 0.2), // '"' / '\''
        VK_OEM_PLUS => drum(61, 0.2), // '+' ('=' unshifted also lands here in v1)
        VK_OEM_4 => drum(36, 0.2),    // '['
        VK_OEM_6 => drum(37, 0.2),    // ']'
        VK_1 if shift => drum(54, 2.0), // '!'
        VK_9 if shift => drum(38, 0.2), // '('
        VK_0 if shift => drum(39, 0.2), // ')'
        _ => drum(45, 0.2),           // the original `else [45]`
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(vk: u8, shift: bool) -> MappedNote {
        map_key(vk, shift).expect("expected a mapped note")
    }

    #[test]
    fn scale_matches_original_formula() {
        assert_eq!(MAJOR_SCALE_NOTES[0], 21);
        assert_eq!(MAJOR_SCALE_NOTES[1], 22);
        assert_eq!(MAJOR_SCALE_NOTES[2], 24);
        assert_eq!(MAJOR_SCALE_NOTES[23], 60); // boundary note
        assert_eq!(MAJOR_SCALE_NOTES[24], 62); // first letter note
        assert_eq!(MAJOR_SCALE_NOTES[36], 82); // first shifted letter note
        assert_eq!(MAJOR_SCALE_NOTES[47], 101); // last note used by letters
        assert_eq!(MAJOR_SCALE_NOTES.len(), 52);
    }

    #[test]
    fn letters_play_piano_at_full_velocity() {
        let a = note(VK_A, false);
        assert_eq!(a.instrument, Instrument::Piano);
        assert_eq!(a.midi, 62);
        assert_eq!(a.velocity, 1.0);

        // Shift raises by 12 scale steps (the original's uppercase behavior).
        assert_eq!(note(VK_A, true).midi, 82);

        // The original wraps letters around 12 scale slots: 'm' == 'a', 'z' == 'b'.
        assert_eq!(note(0x4D, false).midi, 62);
        assert_eq!(note(VK_Z, false).midi, note(0x42, false).midi);
    }

    #[test]
    fn special_keys_match_original_drums() {
        assert_eq!(
            (note(VK_BACK, false).midi, note(VK_BACK, false).velocity),
            (50, 1.0)
        );
        assert_eq!(
            (note(VK_DELETE, false).midi, note(VK_DELETE, false).velocity),
            (49, 1.0)
        );
        assert_eq!(
            (note(VK_SPACE, false).midi, note(VK_SPACE, false).velocity),
            (41, 0.025)
        );
        assert_eq!(
            (note(VK_TAB, false).midi, note(VK_TAB, false).velocity),
            (41, 0.2)
        );
        assert_eq!(note(VK_OEM_PERIOD, false).midi, 56);
        assert_eq!(note(VK_OEM_7, true).midi, 57);
        assert_eq!(note(VK_OEM_7, false).midi, 58);
        assert_eq!(note(VK_OEM_PLUS, false).midi, 61);
        assert_eq!(note(VK_OEM_4, false).midi, 36);
        assert_eq!(note(VK_OEM_6, false).midi, 37);
        assert_eq!(
            (note(VK_1, true).midi, note(VK_1, true).velocity),
            (54, 2.0)
        );
        assert_eq!(note(VK_9, true).midi, 38);
        assert_eq!(note(VK_0, true).midi, 39);
    }

    #[test]
    fn unmapped_keys_fall_to_default_drum() {
        let enter = note(0x0D, false);
        assert_eq!(enter.instrument, Instrument::Drums);
        assert_eq!((enter.midi, enter.velocity), (45, 0.2));
        // CapsLock plays the default drum in the original too.
        assert_eq!(note(0x14, false).midi, 45);
    }

    #[test]
    fn modifiers_are_silent() {
        for vk in [
            VK_SHIFT,
            VK_CONTROL,
            VK_MENU,
            VK_LWIN,
            VK_RWIN,
            VK_LSHIFT,
            VK_RSHIFT,
            VK_LCONTROL,
            VK_RCONTROL,
            VK_LMENU,
            VK_RMENU,
        ] {
            assert!(map_key(vk, false).is_none(), "vk {vk:#x} should be silent");
        }
    }
}
