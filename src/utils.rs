use chrono::{DateTime, Local, TimeZone, Timelike};

pub fn adjust_datetime_digit(dt: &DateTime<Local>, pos: usize, c: char) -> Option<DateTime<Local>> {
    if let Some(digit) = c.to_digit(10) {
        if (pos == 1 && digit >= 3) || (pos % 2 == 1 && digit >= 6) {
            return None;
        }

        let old = match pos {
            1..=2 => dt.hour(),
            3..=4 => dt.minute(),
            5..=6 => dt.second(),
            _ => panic!("Unsupported pos"),
        };

        let limit = match pos {
            1..=2 => 23,
            _ => 59,
        };

        let kept_digit = if pos % 2 == 1 {
            old % 10
        } else {
            old - old % 10
        };

        let new_digit = if pos % 2 == 1 { digit * 10 } else { digit };

        let new_val = (kept_digit + new_digit).min(limit);

        return match pos {
            1..=2 => dt.with_hour(new_val),
            3..=4 => dt.with_minute(new_val),
            _ => dt.with_second(new_val),
        };
    }

    None
}

/// Panics if this DateTime cannot be zeroed, which... I think should only be
/// possible if... uhh... you have the smallest representable datetime as your
/// starting time! Or within a day of it! Because the smallest representable
/// DateTime likely does not fall on exactly 0:00:00.0, especially in whatever
/// timezone you're using. So yeah, this function will panic if you pass a
/// DateTime less than 1 full day later than the smallest representable
/// DateTime, so don't do that.
pub fn datetime_with_zeroed_time<T: TimeZone>(dt: &DateTime<T>) -> DateTime<T> {
    dt.with_hour(0)
        .unwrap()
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap()
}
