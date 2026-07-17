//! Tests for the cron parser + `next_after`. Split into this sibling file to keep
//! `cron.rs` (the algorithm) under the file-size threshold.

use super::*;
use time::macros::datetime;

fn next(expr: &str, from: OffsetDateTime) -> OffsetDateTime {
    Cron::parse(expr).unwrap().next_after(from).unwrap()
}

#[test]
fn daily_at_4am() {
    let from = datetime!(2026-06-29 02:30:00 UTC);
    assert_eq!(next("0 4 * * *", from), datetime!(2026-06-29 04:00:00 UTC));
    // After 4am → tomorrow.
    let from = datetime!(2026-06-29 09:00:00 UTC);
    assert_eq!(next("0 4 * * *", from), datetime!(2026-06-30 04:00:00 UTC));
}

#[test]
fn every_15_minutes() {
    let from = datetime!(2026-06-29 10:07:00 UTC);
    assert_eq!(next("*/15 * * * *", from), datetime!(2026-06-29 10:15:00 UTC));
    let from = datetime!(2026-06-29 10:55:30 UTC);
    assert_eq!(next("*/15 * * * *", from), datetime!(2026-06-29 11:00:00 UTC));
}

#[test]
fn strictly_after() {
    // Exactly on a fire time → returns the *next* one, not the same instant.
    let from = datetime!(2026-06-29 04:00:00 UTC);
    assert_eq!(next("0 4 * * *", from), datetime!(2026-06-30 04:00:00 UTC));
}

#[test]
fn weekday_names_and_or_rule() {
    // Mondays at 09:00. 2026-06-29 is a Monday.
    let from = datetime!(2026-06-28 12:00:00 UTC); // Sunday
    assert_eq!(next("0 9 * * mon", from), datetime!(2026-06-29 09:00:00 UTC));
    // Both dom and dow restricted → either matches. dom=1 OR Friday.
    // 2026-07-01 is a Wednesday (dom match); next Friday is 2026-07-03.
    let from = datetime!(2026-06-30 00:00:00 UTC);
    assert_eq!(next("0 0 1 * fri", from), datetime!(2026-07-01 00:00:00 UTC));
}

#[test]
fn sunday_as_7_equals_0() {
    // Day-of-week 7 must normalize to Sunday (0) both fire on the same instant.
    let from = datetime!(2026-06-29 12:00:00 UTC); // Monday
    assert_eq!(next("0 0 * * 7", from), next("0 0 * * 0", from));
    assert_eq!(next("0 0 * * 7", from), datetime!(2026-07-05 00:00:00 UTC)); // next Sunday
}

#[test]
fn month_rollover_and_names() {
    // 1st of next month at midnight.
    let from = datetime!(2026-06-29 00:00:00 UTC);
    assert_eq!(next("@monthly", from), datetime!(2026-07-01 00:00:00 UTC));
    // Specific month by name (January).
    let from = datetime!(2026-06-29 00:00:00 UTC);
    assert_eq!(next("0 0 1 jan *", from), datetime!(2027-01-01 00:00:00 UTC));
}

#[test]
fn satisfiable_leap_day_skips_non_leap_years() {
    // Feb 29 exists only in leap years from 2026 the next is 2028-02-29 (the
    // month-skipping search must step over 2026/2027's missing Feb 29).
    let from = datetime!(2026-01-01 00:00:00 UTC);
    assert_eq!(next("0 0 29 2 *", from), datetime!(2028-02-29 00:00:00 UTC));
}

#[test]
fn seconds_field() {
    let from = datetime!(2026-06-29 10:00:00 UTC);
    assert_eq!(next("30 0 10 * * *", from), datetime!(2026-06-29 10:00:30 UTC));
}

#[test]
fn ranges_and_lists() {
    let from = datetime!(2026-06-29 11:30:00 UTC);
    // Hours 9-17 only, on the hour → next is 12:00.
    assert_eq!(next("0 9-17 * * *", from), datetime!(2026-06-29 12:00:00 UTC));
    let from = datetime!(2026-06-29 11:00:00 UTC);
    assert_eq!(next("0,30 * * * *", from), datetime!(2026-06-29 11:30:00 UTC));
}

#[test]
fn open_ended_step_form() {
    // `a/n` (no range) means a..=max stepping by n → minutes 5,15,25,…
    let from = datetime!(2026-06-29 10:00:00 UTC);
    assert_eq!(next("5/10 * * * *", from), datetime!(2026-06-29 10:05:00 UTC));
    let from = datetime!(2026-06-29 10:05:00 UTC);
    assert_eq!(next("5/10 * * * *", from), datetime!(2026-06-29 10:15:00 UTC));
}

#[test]
fn rejects_garbage() {
    assert!(Cron::parse("").is_err());
    assert!(Cron::parse("* * *").is_err());
    assert!(Cron::parse("60 * * * *").is_err());
    assert!(Cron::parse("* 25 * * *").is_err());
    assert!(Cron::parse("*/0 * * * *").is_err());
    assert!(Cron::parse("nope * * * *").is_err());
}

#[test]
fn unsatisfiable_returns_none() {
    // Feb 30th never happens.
    let from = datetime!(2026-01-01 00:00:00 UTC);
    assert!(Cron::parse("0 0 30 2 *").unwrap().next_after(from).is_none());
}
