//! Typing text as key usages on a US keyboard layout.

use gearwise_core::Result;

use crate::InputSink;

const LEFT_SHIFT: u8 = 0xe1;

/// The US-layout key usage that produces `c`, and whether Shift is needed.
fn ascii_key(c: char) -> Option<(u8, bool)> {
    const SHIFTED_DIGITS: &str = "!@#$%^&*()";
    // (unshifted, shifted) characters on the punctuation keys, by usage.
    const PUNCTUATION: [(u8, char, char); 11] = [
        (0x2d, '-', '_'),
        (0x2e, '=', '+'),
        (0x2f, '[', '{'),
        (0x30, ']', '}'),
        (0x31, '\\', '|'),
        (0x33, ';', ':'),
        (0x34, '\'', '"'),
        (0x35, '`', '~'),
        (0x36, ',', '<'),
        (0x37, '.', '>'),
        (0x38, '/', '?'),
    ];

    match c {
        'a'..='z' => Some((0x04 + (c as u8 - b'a'), false)),
        'A'..='Z' => Some((0x04 + (c as u8 - b'A'), true)),
        '1'..='9' => Some((0x1e + (c as u8 - b'1'), false)),
        '0' => Some((0x27, false)),
        '\n' => Some((0x28, false)),
        '\t' => Some((0x2b, false)),
        ' ' => Some((0x2c, false)),
        _ => {
            if let Some(i) = SHIFTED_DIGITS.find(c) {
                // "!" is on the "1" key (0x1e), ")" on the "0" key (0x27).
                return Some((if i == 9 { 0x27 } else { 0x1e + i as u8 }, true));
            }
            PUNCTUATION.iter().find_map(|&(usage, plain, shifted)| {
                if c == plain {
                    Some((usage, false))
                } else if c == shifted {
                    Some((usage, true))
                } else {
                    None
                }
            })
        }
    }
}

/// Type `text` through `sink`, one character at a time. Carriage returns
/// are ignored and characters without a US-layout key are skipped.
pub(crate) fn type_text<S: InputSink + ?Sized>(sink: &mut S, text: &str) -> Result<()> {
    for c in text.chars() {
        if c == '\r' {
            continue;
        }
        let Some((usage, shift)) = ascii_key(c) else {
            log::warn!("text: no US-layout key for {c:?}, skipped");
            continue;
        };
        if shift {
            sink.key_down(LEFT_SHIFT)?;
        }
        let tapped = sink.key_down(usage).and_then(|()| sink.key_up(usage));
        if shift {
            // Always let go of Shift, even if the tap failed.
            let released = sink.key_up(LEFT_SHIFT);
            tapped.and(released)?;
        } else {
            tapped?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{Event, Recorder};
    use gearwise_core::keys::KEYS;

    #[test]
    fn every_printable_ascii_char_has_a_known_key() {
        for c in (0x20u8..=0x7e).map(char::from).chain(['\n', '\t']) {
            let (usage, _) = ascii_key(c).unwrap_or_else(|| panic!("no key for {c:?}"));
            assert!(
                KEYS.iter().any(|(u, _)| *u == usage),
                "{c:?} -> unknown usage {usage:#x}"
            );
        }
    }

    #[test]
    fn shifted_and_plain_characters_share_keys() {
        assert_eq!(ascii_key('a'), Some((0x04, false)));
        assert_eq!(ascii_key('Z'), Some((0x1d, true)));
        assert_eq!(ascii_key('1'), Some((0x1e, false)));
        assert_eq!(ascii_key('!'), Some((0x1e, true)));
        assert_eq!(ascii_key('0'), Some((0x27, false)));
        assert_eq!(ascii_key(')'), Some((0x27, true)));
        assert_eq!(ascii_key('?'), Some((0x38, true)));
        assert_eq!(ascii_key('é'), None);
    }

    #[test]
    fn typing_wraps_shifted_keys_in_shift() {
        let rec = Recorder::default();
        let mut sink = rec.clone();
        type_text(&mut sink, "aB\r\né").unwrap();
        assert_eq!(
            rec.events(),
            vec![
                Event::KeyDown(0x04),
                Event::KeyUp(0x04),
                Event::KeyDown(LEFT_SHIFT),
                Event::KeyDown(0x05),
                Event::KeyUp(0x05),
                Event::KeyUp(LEFT_SHIFT),
                Event::KeyDown(0x28),
                Event::KeyUp(0x28),
            ]
        );
    }
}
