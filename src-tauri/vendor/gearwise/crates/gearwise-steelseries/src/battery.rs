//! Battery queries for the wireless models.

use gearwise_core::model::Battery;

use crate::link::Packet;
use crate::models::BatteryProto;

/// Set in the status byte while the mouse charges.
const CHARGING: u8 = 0x80;
/// Dongle replies echo the command with this flag set.
const DONGLE_FLAG: u8 = 0x40;
const STATUS_COMMAND: u8 = 0x92;

pub fn request(proto: BatteryProto) -> Packet {
    Packet::output(match proto {
        BatteryProto::Status => vec![STATUS_COMMAND],
        BatteryProto::Level => vec![0xaa, 0x01],
    })
}

/// Read a reply to [`request`]. `None` for anything that is not one.
pub fn parse(proto: BatteryProto, reply: &[u8]) -> Option<Battery> {
    match proto {
        BatteryProto::Status => {
            let [echo, status, ..] = *reply else {
                return None;
            };
            if echo & !DONGLE_FLAG != STATUS_COMMAND {
                return None;
            }
            // 1 is 0 %, then 5 % per step; 0 means the mouse has no reading.
            let steps = (status & !CHARGING).checked_sub(1)?;
            let percent = u16::from(steps) * 5;
            (percent <= 100).then_some(Battery {
                percent: Some(percent as u8),
                charging: status & CHARGING != 0,
            })
        }
        BatteryProto::Level => {
            let [level, _, charging, ..] = *reply else {
                return None;
            };
            (level <= 100).then_some(Battery {
                percent: Some(level),
                charging: charging != 0,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn battery(percent: u8, charging: bool) -> Option<Battery> {
        Some(Battery {
            percent: Some(percent),
            charging,
        })
    }

    #[test]
    fn status_replies_count_in_steps_of_five() {
        assert_eq!(
            parse(BatteryProto::Status, &[0x92, 0x01]),
            battery(0, false)
        );
        assert_eq!(
            parse(BatteryProto::Status, &[0x92, 0x0b]),
            battery(50, false)
        );
        assert_eq!(
            parse(BatteryProto::Status, &[0x92, 0x15]),
            battery(100, false)
        );
        assert_eq!(
            parse(BatteryProto::Status, &[0x92, 0x80 | 0x0b]),
            battery(50, true)
        );
        // Dongle replies carry the flag in the echo.
        assert_eq!(
            parse(BatteryProto::Status, &[0xd2, 0x0b]),
            battery(50, false)
        );
    }

    #[test]
    fn status_replies_must_make_sense() {
        assert_eq!(parse(BatteryProto::Status, &[0x92, 0x00]), None);
        assert_eq!(parse(BatteryProto::Status, &[0x92, 0x16]), None);
        assert_eq!(parse(BatteryProto::Status, &[0x91, 0x0b]), None);
        assert_eq!(parse(BatteryProto::Status, &[0x92]), None);
        assert_eq!(parse(BatteryProto::Status, &[]), None);
    }

    #[test]
    fn level_replies_carry_percent_and_charging() {
        assert_eq!(parse(BatteryProto::Level, &[80, 0, 0]), battery(80, false));
        assert_eq!(
            parse(BatteryProto::Level, &[7, 1, 1, 0, 0]),
            battery(7, true)
        );
        assert_eq!(parse(BatteryProto::Level, &[101, 0, 0]), None);
        assert_eq!(parse(BatteryProto::Level, &[80, 0]), None);
    }

    #[test]
    fn requests_use_the_documented_commands() {
        assert_eq!(request(BatteryProto::Status).bytes, [0x92]);
        assert_eq!(request(BatteryProto::Level).bytes, [0xaa, 0x01]);
    }
}
