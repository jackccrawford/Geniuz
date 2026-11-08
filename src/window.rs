//! Time windows for recall: `--since` / `--until`.
//!
//! Relevance with no date is a popularity contest across all time: a week-old
//! answer loses to months of older neighbours (Pumpkin's test, 2026-09-30).
//! A window narrows the corpus BEFORE ranking, so every reader — recent,
//! keyword, semantic — answers inside it.
//!
//! Bounds are stored as UTC in SQLite's `YYYY-MM-DD HH:MM:SS` form, the shape
//! of `created_at`. `since` is inclusive, `until` exclusive. A bound given as
//! a date or a zone-less time is read in LOCAL time — that is the clock the
//! results are shown in — and an `until` date means through the end of that
//! day.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

const SQL_TS: &str = "%Y-%m-%d %H:%M:%S";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Window {
    /// Inclusive lower bound, UTC `YYYY-MM-DD HH:MM:SS`.
    pub since: Option<String>,
    /// Exclusive upper bound, UTC `YYYY-MM-DD HH:MM:SS`.
    pub until: Option<String>,
    /// The bounds as the caller wrote them, for saying what was searched.
    pub since_label: Option<String>,
    pub until_label: Option<String>,
}

impl Window {
    /// No bounds: all of time.
    pub fn all() -> Self {
        Self::default()
    }

    pub fn is_all(&self) -> bool {
        self.since.is_none() && self.until.is_none()
    }

    /// Parse `--since` / `--until` as given on a command line or in a tool call.
    /// Accepts `24h`, `7d`, `2w`, a date (`2026-09-23`), a local time
    /// (`2026-09-23 14:00`), or RFC 3339 (`2026-09-23T14:00:00Z`).
    pub fn parse(since: Option<&str>, until: Option<&str>) -> Result<Self, String> {
        Self::parse_at(since, until, Utc::now())
    }

    /// [`Window::parse`] against a fixed "now", for tests.
    pub fn parse_at(
        since: Option<&str>,
        until: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Self, String> {
        let since = since.map(str::trim).filter(|s| !s.is_empty());
        let until = until.map(str::trim).filter(|s| !s.is_empty());
        let lo = since.map(|s| bound(s, now, false).map_err(|e| format!("--since {e}"))).transpose()?;
        let hi = until.map(|s| bound(s, now, true).map_err(|e| format!("--until {e}"))).transpose()?;
        if let (Some(lo), Some(hi)) = (lo, hi) {
            if lo >= hi {
                return Err(format!(
                    "--since {} is not before --until {}: that window holds nothing",
                    since.unwrap_or_default(),
                    until.unwrap_or_default()
                ));
            }
        }
        Ok(Window {
            since: lo.map(|t| t.format(SQL_TS).to_string()),
            until: hi.map(|t| t.format(SQL_TS).to_string()),
            since_label: since.map(String::from),
            until_label: until.map(String::from),
        })
    }

    /// Whether a stored `created_at` falls inside the window. A timestamp that
    /// cannot be read is outside any bounded window (and inside the unbounded one).
    pub fn contains(&self, created_at: &str) -> bool {
        if self.is_all() {
            return true;
        }
        let Some(ts) = normalize(created_at) else { return false };
        self.since.as_deref().map_or(true, |lo| ts.as_str() >= lo)
            && self.until.as_deref().map_or(true, |hi| ts.as_str() < hi)
    }

    /// A SQL condition on `col` with its parameters, numbered from `first`
    /// (`?first`, `?first+1`). `("1", [])` when unbounded.
    pub fn sql(&self, col: &str, first: usize) -> (String, Vec<String>) {
        let mut parts = Vec::new();
        let mut params = Vec::new();
        if let Some(ref lo) = self.since {
            parts.push(format!("datetime({col}) >= ?{}", first + params.len()));
            params.push(lo.clone());
        }
        if let Some(ref hi) = self.until {
            parts.push(format!("datetime({col}) < ?{}", first + params.len()));
            params.push(hi.clone());
        }
        if parts.is_empty() {
            ("1".to_string(), params)
        } else {
            (parts.join(" AND "), params)
        }
    }

    /// How the window reads to a person: `since 7d`, `2026-09-01 – 2026-09-30`.
    /// Empty when unbounded.
    pub fn describe(&self) -> String {
        match (&self.since_label, &self.until_label) {
            (Some(s), Some(u)) => format!("since {s} until {u}"),
            (Some(s), None) => format!("since {s}"),
            (None, Some(u)) => format!("until {u}"),
            (None, None) => String::new(),
        }
    }
}

/// The closing line of every recall answer: what was searched, so that
/// "nothing found" carries its scope and a short list says how big the pool
/// was (search feature 5). For example
/// `searched: PILOT · 312 memories · since 7d · semantic "reviewers"`.
pub fn scope_line(station: Option<&str>, memories: usize, window: &Window, mode: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(s) = station.filter(|s| !s.is_empty()) {
        parts.push(s.to_string());
    }
    parts.push(format!("{memories} {}", if memories == 1 { "memory" } else { "memories" }));
    parts.push(if window.is_all() { "all time".to_string() } else { window.describe() });
    parts.push(mode.to_string());
    format!("searched: {}", parts.join(" · "))
}

/// When a memory happened, for `keep --at` and the MCP's `at` (Jack, 7 Nov 2026: an
/// agent bringing its history into Geniuz keeps each memory's own date). Same forms
/// as `--since`: `2026-03-21`, `2026-03-21 14:00` (local), RFC 3339, or `3d` ago. A
/// date alone is the start of that local day. Never in the future. Returns the stored
/// form, UTC `YYYY-MM-DD HH:MM:SS`.
pub fn moment(raw: &str) -> Result<String, String> {
    moment_at(raw, Utc::now())
}

/// [`moment`] against a fixed "now", for tests.
pub fn moment_at(raw: &str, now: DateTime<Utc>) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("--at needs a time: a date (2026-03-21), a local time (2026-03-21 14:00) or RFC 3339".into());
    }
    let t = bound(raw, now, false).map_err(|e| format!("--at {e}"))?;
    if t > now {
        return Err(format!("--at {raw} is in the future; a memory can only be dated when it happened"));
    }
    // Before 1970 is a typo, not a memory, and SQLite's date functions misread
    // years outside 0000-9999 (cubic, PR #30).
    if t.year() < 1970 {
        return Err(format!("--at {raw} is before 1970; give the date the memory happened"));
    }
    Ok(t.format(SQL_TS).to_string())
}

/// One bound as a UTC instant. An `until` that names a day or a minute covers
/// all of it, so the exclusive bound is the start of the next one.
fn bound(raw: &str, now: DateTime<Utc>, is_until: bool) -> Result<DateTime<Utc>, String> {
    if let Some(d) = relative(raw) {
        // A huge offset is an error, not a panic (cubic, PR #30).
        return now.checked_sub_signed(d).ok_or_else(|| format!("`{raw}` reaches before any date there is"));
    }
    if let Ok(t) = DateTime::parse_from_rfc3339(raw) {
        let t = t.with_timezone(&Utc);
        return Ok(if is_until { t + Duration::seconds(1) } else { t });
    }
    if let Ok(d) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        let day = if is_until { d.succ_opt().ok_or("is past the end of the calendar")? } else { d };
        return local(day.and_hms_opt(0, 0, 0).unwrap());
    }
    let norm = raw.replacen('T', " ", 1);
    if let Ok(t) = NaiveDateTime::parse_from_str(&norm, "%Y-%m-%d %H:%M:%S") {
        return local(t).map(|t| if is_until { t + Duration::seconds(1) } else { t });
    }
    if let Ok(t) = NaiveDateTime::parse_from_str(&norm, "%Y-%m-%d %H:%M") {
        return local(t).map(|t| if is_until { t + Duration::minutes(1) } else { t });
    }
    Err(format!(
        "`{raw}`: expected 24h, 7d, 2w, a date (2026-09-23), a time (2026-09-23 14:00) or RFC 3339"
    ))
}

/// `24h`, `7d`, `2w`.
fn relative(raw: &str) -> Option<Duration> {
    // Split before the last character, not the last byte: input can end in any
    // character, and a byte index inside one panics (cubic, PR #30).
    let (cut, _) = raw.char_indices().last()?;
    let (n, unit) = raw.split_at(cut);
    // Digits only: "+3d" or " 3d" is not a relative time (Firn's review, PR #30).
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: i64 = n.parse().ok()?;
    match unit {
        "h" => Duration::try_hours(n),
        "d" => Duration::try_days(n),
        "w" => Duration::try_weeks(n),
        _ => None,
    }
}

fn local(t: NaiveDateTime) -> Result<DateTime<Utc>, String> {
    // A clock-change gap has no such local time; take the earliest reading.
    Local
        .from_local_datetime(&t)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .ok_or_else(|| format!("`{t}` does not exist in local time"))
}

/// A stored `created_at` in the window's form, whatever shape it was written in.
fn normalize(created_at: &str) -> Option<String> {
    let s = created_at.trim();
    if let Ok(t) = NaiveDateTime::parse_from_str(s, SQL_TS) {
        return Some(t.format(SQL_TS).to_string());
    }
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Some(t.with_timezone(&Utc).format(SQL_TS).to_string());
    }
    NaiveDateTime::parse_from_str(&s.replacen('T', " ", 1), SQL_TS)
        .ok()
        .map(|t| t.format(SQL_TS).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 30, 12, 0, 0).unwrap()
    }

    #[test]
    fn moment_dates_a_memory_and_refuses_the_future() {
        let now = Utc.with_ymd_and_hms(2026, 11, 8, 5, 0, 0).unwrap();
        // RFC 3339 is exact; the stored form is UTC with a space.
        assert_eq!(moment_at("2026-03-21T14:00:00Z", now).unwrap(), "2026-03-21 14:00:00");
        // A date alone is the start of that LOCAL day, so it stays on its date locally.
        let d = moment_at("2026-03-21", now).unwrap();
        let local = NaiveDateTime::parse_from_str(&d, SQL_TS).unwrap().and_utc().with_timezone(&Local);
        assert_eq!(local.format("%Y-%m-%d %H:%M").to_string(), "2026-03-21 00:00");
        assert_eq!(moment_at("3d", now).unwrap(), "2026-11-05 05:00:00");
        assert!(moment_at("2026-12-01", now).unwrap_err().contains("future"));
        assert!(moment_at("", now).is_err());
        assert!(moment_at("next tuesday", now).is_err());
        // Input that once panicked is an error now.
        assert!(moment_at("昨日", now).is_err());
        assert!(moment_at("99999999999d", now).is_err());
        assert!(moment_at("1066-10-14", now).unwrap_err().contains("1970"));
        assert!(Window::parse_at(Some("9999999999w"), None, now).is_err());
    }

    #[test]
    fn relative_bounds_count_back_from_now() {
        let w = Window::parse_at(Some("7d"), Some("24h"), now()).unwrap();
        assert_eq!(w.since.as_deref(), Some("2026-09-23 12:00:00"));
        assert_eq!(w.until.as_deref(), Some("2026-09-29 12:00:00"));
        assert_eq!(Window::parse_at(Some("2w"), None, now()).unwrap().since.as_deref(), Some("2026-09-16 12:00:00"));
    }

    #[test]
    fn rfc3339_until_is_inclusive_to_the_second() {
        let w = Window::parse_at(None, Some("2026-09-30T10:00:00Z"), now()).unwrap();
        assert!(w.contains("2026-09-30 10:00:00"));
        assert!(!w.contains("2026-09-30 10:00:01"));
    }

    #[test]
    fn a_date_is_a_local_day_and_until_covers_all_of_it() {
        let w = Window::parse_at(Some("2026-09-23"), Some("2026-09-23"), now()).unwrap();
        let start = Local.with_ymd_and_hms(2026, 9, 23, 0, 0, 0).unwrap().with_timezone(&Utc);
        let last = Local.with_ymd_and_hms(2026, 9, 23, 23, 59, 59).unwrap().with_timezone(&Utc);
        assert!(w.contains(&start.format(SQL_TS).to_string()));
        assert!(w.contains(&last.format(SQL_TS).to_string()));
        assert!(!w.contains(&(start - Duration::seconds(1)).format(SQL_TS).to_string()));
        assert!(!w.contains(&(last + Duration::seconds(1)).format(SQL_TS).to_string()));
    }

    #[test]
    fn stored_timestamps_in_other_shapes_still_compare() {
        let w = Window::parse_at(Some("2026-09-30T00:00:00Z"), None, now()).unwrap();
        assert!(w.contains("2026-09-30T08:00:00Z"));
        assert!(w.contains("2026-09-30T08:00:00"));
        assert!(!w.contains("2026-09-29T23:59:59Z"));
        assert!(!w.contains("not a time"));
        assert!(Window::all().contains("not a time"));
    }

    #[test]
    fn an_empty_or_backwards_window_is_refused_with_its_reason() {
        let e = Window::parse_at(Some("1d"), Some("7d"), now()).unwrap_err();
        assert!(e.contains("holds nothing"), "{e}");
        let e = Window::parse_at(Some("last tuesday"), None, now()).unwrap_err();
        assert!(e.starts_with("--since `last tuesday`"), "{e}");
    }

    #[test]
    fn the_scope_line_names_pool_window_and_mode() {
        let w = Window::parse_at(Some("7d"), None, now()).unwrap();
        assert_eq!(
            scope_line(Some("PILOT"), 312, &w, "semantic \"reviewers\""),
            "searched: PILOT · 312 memories · since 7d · semantic \"reviewers\""
        );
        assert_eq!(scope_line(None, 1, &Window::all(), "recent"), "searched: 1 memory · all time · recent");
    }

    #[test]
    fn sql_numbers_its_parameters_from_the_given_slot() {
        let w = Window::parse_at(Some("7d"), Some("1d"), now()).unwrap();
        let (cond, params) = w.sql("m.created_at", 3);
        assert_eq!(cond, "datetime(m.created_at) >= ?3 AND datetime(m.created_at) < ?4");
        assert_eq!(params.len(), 2);
        assert_eq!(Window::all().sql("created_at", 1), ("1".to_string(), vec![]));
    }
}
