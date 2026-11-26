//! Conversions d'horodatage par navigateur. Toutes renvoient un Option<i64>
//! en millisecondes Unix UTC, ou None si invalide / hors plage.

const WEBKIT_TO_UNIX_US: i64 = 11_644_473_600_000_000; // us entre 1601 et 1970
const CF_OFFSET_S: i64 = 978_307_200; // s entre 1970 et 2001
const MIN_MS: i64 = -11_644_473_600_000; // ~ 1601-01-01
const MAX_MS: i64 = 253_402_300_799_000; // ~ 9999-12-31

fn guard(ms: i64) -> Option<i64> {
    if (MIN_MS..=MAX_MS).contains(&ms) {
        Some(ms)
    } else {
        None
    }
}

/// Microsecondes depuis 1601 (Chrome/Edge/Chromium, Login Data).
pub fn from_webkit(us: i64) -> Option<i64> {
    if us == 0 {
        return None;
    }
    guard((us - WEBKIT_TO_UNIX_US) / 1000)
}

/// Microsecondes depuis 1970 (Firefox PRTime).
pub fn from_prtime(us: i64) -> Option<i64> {
    if us == 0 {
        return None;
    }
    guard(us / 1000)
}

/// Secondes Unix (autofill Chromium, expiry cookies Firefox).
pub fn from_unix_s(s: i64) -> Option<i64> {
    if s == 0 {
        return None;
    }
    guard(s.saturating_mul(1000))
}

/// Millisecondes Unix (logins.json Firefox).
pub fn from_unix_ms(ms: i64) -> Option<i64> {
    if ms == 0 {
        return None;
    }
    guard(ms)
}

/// Secondes depuis 2001 (Safari / CFAbsoluteTime).
pub fn from_safari(s: f64) -> Option<i64> {
    if s == 0.0 {
        return None;
    }
    let ms = ((s + CF_OFFSET_S as f64) * 1000.0) as i64;
    guard(ms)
}

/// FILETIME : intervalles de 100 ns depuis 1601 (WebCache IE/Edge).
pub fn from_filetime(ticks: i64) -> Option<i64> {
    if ticks == 0 {
        return None;
    }
    guard((ticks / 10 - WEBKIT_TO_UNIX_US) / 1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn ms(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, s)
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn webkit() {
        // 2026-03-15 16:00:00 UTC en us depuis 1601
        let target = ms(2026, 3, 15, 16, 0, 0);
        let us = (target + WEBKIT_TO_UNIX_US / 1000) * 1000;
        assert_eq!(from_webkit(us), Some(target));
        assert_eq!(from_webkit(0), None);
    }

    #[test]
    fn prtime_and_safari_and_filetime() {
        let target = ms(2026, 4, 1, 9, 0, 0);
        assert_eq!(from_prtime((target) * 1000), Some(target));
        let t2 = ms(2026, 6, 1, 10, 0, 0);
        let cf = (t2 / 1000 - CF_OFFSET_S) as f64;
        assert_eq!(from_safari(cf), Some(t2));
        let t3 = ms(2026, 1, 5, 0, 0, 0);
        let ticks = (t3 + WEBKIT_TO_UNIX_US / 1000) * 10_000;
        assert_eq!(from_filetime(ticks), Some(t3));
    }
}
