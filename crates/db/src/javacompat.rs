//! Reproductions of the `java.time` / `java.math.BigDecimal` behavior of the former JDBC bridge's
//! value reading, plus its error shape.

use chrono::{Offset, TimeZone};
use chrono_tz::{OffsetComponents, OffsetName, Tz};
use std::fmt;
use std::sync::LazyLock;

/// An error with the exact shape the former JDBC bridge reported:
/// `"{errorClass}: {message} [SQLSTATE={sqlState}; vendorCode={vendorCode}]"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbError {
    pub class: String,
    pub message: String,
    pub sql_state: String,
    pub vendor_code: i64,
}

impl DbError {
    pub fn new(class: &str, message: impl Into<String>, sql_state: &str, vendor_code: i64) -> Self {
        DbError {
            class: class.to_owned(),
            message: message.into(),
            sql_state: sql_state.to_owned(),
            vendor_code,
        }
    }

    /// A `java.sql.SQLException` with no SQLSTATE and vendor code 0.
    pub fn sql(message: impl Into<String>) -> Self {
        DbError::new("java.sql.SQLException", message, "", 0)
    }

    /// A non-SQL Java exception (SQLSTATE empty, vendor code 0).
    pub fn java(class: &str, message: impl Into<String>) -> Self {
        DbError::new(class, message, "", 0)
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} [SQLSTATE={}; vendorCode={}]",
            self.class, self.message, self.sql_state, self.vendor_code
        )
    }
}

impl std::error::Error for DbError {}

/// Error produced when `SqlSplit.splitSql` throws (an `end` token with no matching `begin`/`case`).
/// The exception has a null message, so `Throwable.toString()` was reported.
pub fn split_error(_: dqd_sqlrender::SplitError) -> DbError {
    DbError::java(
        "java.util.EmptyStackException",
        "java.util.EmptyStackException",
    )
}

/// Split as the former bridge's `query`/`execute` did.
pub fn split_statements(sql: &str) -> Result<Vec<String>, DbError> {
    dqd_sqlrender::try_split_sql(sql).map_err(split_error)
}

/// The former bridge's single-statement requirement for `query`.
pub fn single_statement(sql: &str) -> Result<String, DbError> {
    let mut statements = split_statements(sql)?;
    if statements.len() != 1 {
        return Err(DbError::java(
            "java.lang.IllegalArgumentException",
            format!(
                "A query that returns a result can only consist of one SQL statement, but {} statements were found",
                statements.len()
            ),
        ));
    }
    Ok(statements.remove(0))
}

pub const BIGINT_ERROR: &str = "The data contains integers >= 2^53, and converting those to R's numeric type leads to precision loss. Consider using smaller integers, converting the integers to doubles on the database side, or using `options(databaseConnectorInteger64AsNumeric = FALSE)`.";

/// The former bridge's `Types.BIGINT` handling.
pub fn check_bigint(value: i64) -> Result<i64, DbError> {
    if value.unsigned_abs() >= 9_007_199_254_740_992 {
        return Err(DbError::java("java.lang.ArithmeticException", BIGINT_ERROR));
    }
    Ok(value)
}

// ---------------------------------------------------------------------------------------------
// Calendar arithmetic (proleptic Gregorian)
// ---------------------------------------------------------------------------------------------

pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// ISO year as `LocalDate.toString` prints it.
fn iso_year(year: i64) -> String {
    let abs = year.abs();
    if abs < 1000 {
        if year < 0 {
            format!("-{abs:04}")
        } else {
            format!("{abs:04}")
        }
    } else if year > 9999 {
        format!("+{year}")
    } else {
        year.to_string()
    }
}

/// `LocalDate.toString()`
pub fn local_date_string(year: i64, month: i64, day: i64) -> String {
    format!("{}-{month:02}-{day:02}", iso_year(year))
}

/// `Instant.ofEpochMilli(ms).atZone(UTC).toLocalDate().toString()`
pub fn epoch_millis_utc_date(ms: i64) -> String {
    let (y, m, d) = civil_from_days(ms.div_euclid(86_400_000));
    local_date_string(y, m, d)
}

/// `DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss").withZone(UTC).format(Instant.ofEpochMilli(ms))`
/// (`yyyy` is year-of-era with EXCEEDS_PAD sign style).
pub fn epoch_millis_utc_datetime(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let sod = secs.rem_euclid(86_400);
    let yoe = if y > 0 { y } else { 1 - y };
    let year = if yoe > 9999 {
        format!("+{yoe}")
    } else {
        format!("{yoe:04}")
    };
    format!(
        "{year}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        sod / 3600,
        sod / 60 % 60,
        sod % 60
    )
}

/// `Instant.toString()` for seconds since the epoch plus nanoseconds.
pub fn instant_string(secs: i64, nanos: u32) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let sod = secs.rem_euclid(86_400);
    let mut out = format!(
        "{}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        iso_year(y),
        sod / 3600,
        sod / 60 % 60,
        sod % 60
    );
    if nanos != 0 {
        if nanos % 1_000_000 == 0 {
            out.push_str(&format!(".{:03}", nanos / 1_000_000));
        } else if nanos % 1000 == 0 {
            out.push_str(&format!(".{:06}", nanos / 1000));
        } else {
            out.push_str(&format!(".{nanos:09}"));
        }
    }
    out.push('Z');
    out
}

// ---------------------------------------------------------------------------------------------
// java.math.BigDecimal(String) then `multiply(1000).longValue()`
// ---------------------------------------------------------------------------------------------

struct Decimal {
    negative: bool,
    digits: Vec<u8>,
    /// value = digits * 10^exp10
    exp10: i64,
}

/// `Character.digit(c, 10)`: ASCII and the other Unicode decimal digits (category Nd, Unicode 15).
fn java_digit(c: char) -> Option<u8> {
    if let Some(d) = c.to_digit(10) {
        return Some(d as u8);
    }
    const ZEROS: &[u32] = &[
        0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66, 0xDE6,
        0xE50, 0xED0, 0xF20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90,
        0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0,
        0xFF10, 0x104A0, 0x10D30, 0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0,
        0x11650, 0x116C0, 0x11730, 0x118E0, 0x11950, 0x11C50, 0x11D50, 0x11DA0, 0x11F50, 0x16A60,
        0x16AC0, 0x16B50, 0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0, 0x1E4F0,
        0x1E950, 0x1FBF0,
    ];
    let code = c as u32;
    ZEROS
        .iter()
        .find(|&&zero| code >= zero && code < zero + 10)
        .map(|&zero| (code - zero) as u8)
}

fn parse_big_decimal(text: &str) -> Option<Decimal> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut negative = false;
    if let Some(&c) = chars.first() {
        if c == '+' || c == '-' {
            negative = c == '-';
            i = 1;
        }
    }
    let mut digits = Vec::new();
    let mut frac = 0i64;
    let mut seen_digit = false;
    let mut seen_point = false;
    while i < chars.len() {
        if let Some(digit) = java_digit(chars[i]) {
            digits.push(digit);
            seen_digit = true;
            if seen_point {
                frac += 1;
            }
        } else if chars[i] == '.' && !seen_point {
            seen_point = true;
        } else {
            break;
        }
        i += 1;
    }
    if !seen_digit {
        return None;
    }
    let mut exp = 0i64;
    if i < chars.len() {
        if chars[i] != 'e' && chars[i] != 'E' {
            return None;
        }
        i += 1;
        let mut exp_negative = false;
        if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
            exp_negative = chars[i] == '-';
            i += 1;
        }
        if i >= chars.len() {
            return None;
        }
        let mut value = 0i64;
        while i < chars.len() {
            let digit = java_digit(chars[i])?;
            value = value * 10 + digit as i64;
            if value > i32::MAX as i64 {
                return None;
            }
            i += 1;
        }
        exp = if exp_negative { -value } else { value };
    }
    Some(Decimal {
        negative,
        digits,
        exp10: exp - frac,
    })
}

/// True when `new BigDecimal(text)` would succeed.
pub fn is_big_decimal(text: &str) -> bool {
    parse_big_decimal(text).is_some()
}

/// `new BigDecimal(text).multiply(BigDecimal.valueOf(1000)).longValue()`; `None` on `NumberFormatException`.
pub fn big_decimal_millis(text: &str) -> Option<i64> {
    let d = parse_big_decimal(text)?;
    let shift = d.exp10 + 3;
    let mut digits: &[u8] = &d.digits;
    let mut pow = 0u64;
    let mut multiply = false;
    if shift >= 0 {
        multiply = true;
        pow = if shift > u32::MAX as i64 {
            0
        } else {
            10u64.wrapping_pow(shift as u32)
        };
    } else {
        let drop = (-shift) as usize;
        digits = if drop >= digits.len() {
            &[]
        } else {
            &digits[..digits.len() - drop]
        };
    }
    let mut acc = 0u64;
    for &digit in digits {
        acc = acc.wrapping_mul(10).wrapping_add(digit as u64);
    }
    if multiply {
        if digits.iter().all(|&x| x == 0) {
            acc = 0;
        } else {
            acc = acc.wrapping_mul(pow);
        }
    }
    let value = if d.negative { acc.wrapping_neg() } else { acc };
    Some(value as i64)
}

/// `Double.toString(x)` digits are the shortest round-trip digits; BigDecimal only needs the value.
pub fn double_decimal_text(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    Some(format!("{value:e}"))
}

// ---------------------------------------------------------------------------------------------
// JVM default time zone
// ---------------------------------------------------------------------------------------------

fn detect_zone() -> Tz {
    if let Ok(name) = std::env::var("DQD_DB_TIMEZONE") {
        if let Ok(tz) = name.parse::<Tz>() {
            return tz;
        }
    }
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|name| name.parse::<Tz>().ok())
        .unwrap_or(Tz::UTC)
}

/// The zone the JVM would use as `TimeZone.getDefault()`.
pub fn default_zone() -> Tz {
    static ZONE: LazyLock<Tz> = LazyLock::new(detect_zone);
    *ZONE
}

/// IANA id of the default zone, as pgjdbc sends in the `TimeZone` startup parameter.
pub fn default_zone_id() -> String {
    default_zone().name().to_owned()
}

fn naive_of(days: i64, second_of_day: i64) -> Option<chrono::NaiveDateTime> {
    let (y, m, d) = civil_from_days(days);
    let date = chrono::NaiveDate::from_ymd_opt(i32::try_from(y).ok()?, m as u32, d as u32)?;
    date.and_hms_opt(
        (second_of_day / 3600) as u32,
        (second_of_day / 60 % 60) as u32,
        (second_of_day % 60) as u32,
    )
}

/// The JDK's legacy `TimeZone` drops local-mean-time: before a zone's first transition it uses the
/// standard offset that follows it. Returns that offset (seconds east).
fn first_standard_offset() -> i64 {
    static OFFSET: LazyLock<i64> = LazyLock::new(|| {
        for year in 1700..2100 {
            if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, 1, 1) {
                let utc = date.and_hms_opt(0, 0, 0).expect("midnight");
                let offset = default_zone().offset_from_utc_datetime(&utc);
                if offset.abbreviation() != Some("LMT") {
                    return offset.base_utc_offset().num_seconds();
                }
            }
        }
        0
    });
    *OFFSET
}

/// UTC offset (seconds east) of the default zone at `epoch_secs`. Instants beyond chrono's range fold
/// into range by whole 400-year Gregorian cycles (the calendar and zone rules repeat).
pub fn zone_offset_at(epoch_secs: i64) -> i64 {
    const CYCLE: i64 = 12_622_780_800;
    let mut secs = epoch_secs;
    if secs.abs() > 4_000_000_000_000 {
        secs = secs.rem_euclid(CYCLE) + 2 * CYCLE;
    }
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|utc| {
            let offset = default_zone().offset_from_utc_datetime(&utc.naive_utc());
            if offset.abbreviation() == Some("LMT") {
                first_standard_offset()
            } else {
                offset.fix().local_minus_utc() as i64
            }
        })
        .unwrap_or(0)
}

/// Local wall time (`days` since epoch, `second_of_day`) in the default zone to epoch seconds,
/// resolving gaps and overlaps the way a lenient `java.util.GregorianCalendar` does.
pub fn local_to_epoch_secs(days: i64, second_of_day: i64) -> i64 {
    let wall = days * 86_400 + second_of_day;
    let Some(naive) = naive_of(days, second_of_day) else {
        return wall;
    };
    let zone = default_zone();
    match zone.from_local_datetime(&naive) {
        chrono::LocalResult::Single(t) => {
            if t.offset().abbreviation() == Some("LMT") {
                wall - first_standard_offset()
            } else {
                t.timestamp()
            }
        }
        chrono::LocalResult::Ambiguous(_, last) => last.timestamp(),
        chrono::LocalResult::None => {
            // Gap: interpret with the offset in force before the transition.
            let before = naive - chrono::Duration::hours(24);
            let offset = zone
                .offset_from_utc_datetime(&before)
                .fix()
                .local_minus_utc() as i64;
            wall - offset
        }
    }
}

/// Julian-calendar date (astronomical year) of a day number counted from 1970-01-01.
fn julian_from_days(days: i64) -> (i64, i64, i64) {
    let f = days + 2_440_588 + 1401;
    let e = 4 * f + 3;
    let g = e.rem_euclid(1461) / 4;
    let h = 5 * g + 2;
    let day = h.rem_euclid(153) / 5 + 1;
    let month = (h.div_euclid(153) + 2).rem_euclid(12) + 1;
    let year = e.div_euclid(1461) - 4716 + (14 - month).div_euclid(12);
    (year, month, day)
}

/// `java.sql.Date#toLocalDate` for an instant: `LocalDate.of(getYear() + 1900, getMonth() + 1, getDate())`
/// on the legacy hybrid Julian/Gregorian calendar in the default zone. `Date.getYear()` is the year of the
/// era, so BC dates print as positive years.
pub fn local_date_of_epoch_millis(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let local_days = (secs + zone_offset_at(secs)).div_euclid(86_400);
    // 1582-10-15, the Gregorian cutover.
    let (year, month, day) = if local_days >= -141_427 {
        civil_from_days(local_days)
    } else {
        julian_from_days(local_days)
    };
    let year_of_era = if year > 0 { year } else { 1 - year };
    local_date_string(year_of_era, month, day)
}

/// `new java.sql.Timestamp(ms).toInstant()` as (epoch seconds, nanos), including `Timestamp`'s
/// overflow for `Long.MIN_VALUE`-sized values.
pub fn timestamp_new(ms: i64) -> (i64, u32) {
    let mut base = (ms / 1000).wrapping_mul(1000);
    let mut nanos = ((ms % 1000) * 1_000_000) as i32;
    if nanos < 0 {
        nanos += 1_000_000_000;
        base = (ms / 1000 - 1).wrapping_mul(1000);
    }
    (base / 1000, nanos as u32)
}

/// Day number (since 1970-01-01) of a Julian-calendar date.
fn days_from_julian(year: i64, month: i64, day: i64) -> i64 {
    let a = (14 - month).div_euclid(12);
    let y = year + 4800 - a;
    let m = month + 12 * a - 3;
    day + (153 * m + 2).div_euclid(5) + 365 * y + y.div_euclid(4) - 32_083 - 2_440_588
}

/// Day number of wall-clock fields in a lenient `GregorianCalendar` with the default 1582-10-15 cutover:
/// fields before the cutover are Julian-calendar fields. `year` is astronomical (1 BC = 0).
pub fn hybrid_days(year: i64, month: i64, day: i64) -> i64 {
    let month0 = month - 1;
    let y = year + month0.div_euclid(12);
    let m = month0.rem_euclid(12) + 1;
    if (y, m, day) >= (1582, 10, 15) {
        days_from_civil(y, m, 1) + day - 1
    } else {
        days_from_julian(y, m, 1) + day - 1
    }
}
