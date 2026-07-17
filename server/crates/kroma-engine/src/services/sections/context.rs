//! The signals the generator selects sections off: *when* it is, plus a light
//! read of the viewer. Built once per `/api/home` request.

use time::{OffsetDateTime, Weekday};

use crate::db::{self, Pool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartOfDay {
    Morning,
    Afternoon,
    Evening,
    LateNight,
}

pub struct Context {
    /// 1–12 (UTC; good enough for season/holiday gating).
    pub month: u8,
    pub weekday: Weekday,
    pub part_of_day: PartOfDay,
    /// The viewer's most recent finished item, if any ("Because you watched …").
    pub last_played: Option<String>,
    /// Recent distinct watched ids the taste window for the "For You" centroid.
    pub watched: Vec<String>,
}

impl Context {
    /// Build from the current time + the user's recent history.
    pub fn build(pool: &Pool, user_id: &str) -> Self {
        let now = OffsetDateTime::now_utc();
        let part_of_day = match now.hour() {
            5..=11 => PartOfDay::Morning,
            12..=17 => PartOfDay::Afternoon,
            18..=22 => PartOfDay::Evening,
            _ => PartOfDay::LateNight,
        };
        Self {
            month: u8::from(now.month()),
            weekday: now.weekday(),
            part_of_day,
            last_played: db::last_played(pool, user_id).ok().flatten(),
            watched: db::recent_watched_ids(pool, user_id).unwrap_or_default(),
        }
    }

    pub fn is_weekend(&self) -> bool {
        matches!(self.weekday, Weekday::Saturday | Weekday::Sunday)
    }

    pub fn is_evening(&self) -> bool {
        matches!(self.part_of_day, PartOfDay::Evening | PartOfDay::LateNight)
    }
}
