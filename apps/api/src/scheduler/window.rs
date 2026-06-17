use chrono::{DateTime, Datelike, Days, NaiveDate, NaiveTime, Utc, Weekday};
use chrono_tz::Tz;

#[cfg(test)]
pub const SCL: Tz = chrono_tz::America::Santiago;
pub const FIRE_HOUR: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisWindow {
    pub date_from: String,
    pub date_to: String,
}

/// Ventana a analizar cuando el tick ocurre en `today` (fecha local SCL).
/// Lunes cubre viernes a domingo; martes a viernes cubren el día anterior;
/// sábado y domingo no generan ventana.
pub fn analysis_window_for(today: NaiveDate) -> Option<AnalysisWindow> {
    let days_back_from = match today.weekday() {
        Weekday::Mon => 3,
        Weekday::Tue | Weekday::Wed | Weekday::Thu | Weekday::Fri => 1,
        Weekday::Sat | Weekday::Sun => return None,
    };
    let date_from = today.checked_sub_days(Days::new(days_back_from))?;
    let date_to = today.checked_sub_days(Days::new(1))?;
    Some(AnalysisWindow {
        date_from: date_from.format("%Y-%m-%d").to_string(),
        date_to: date_to.format("%Y-%m-%d").to_string(),
    })
}

/// True si el loop interno debe intentar correr: día hábil y hora local >= 08:00.
#[cfg(test)]
pub fn is_fire_time(now_scl: DateTime<Tz>) -> bool {
    is_fire_time_local(now_scl)
}

pub fn due_window_for(now_utc: DateTime<Utc>, timezone: &str) -> Option<AnalysisWindow> {
    let tz = parse_timezone(timezone)?;
    let local_now = now_utc.with_timezone(&tz);
    if !is_fire_time_local(local_now) {
        return None;
    }
    analysis_window_for(local_now.date_naive())
}

pub fn next_fire_time_label(now_utc: DateTime<Utc>, timezone: &str) -> Option<String> {
    let tz = parse_timezone(timezone)?;
    let mut day = now_utc.with_timezone(&tz).date_naive();
    let fire_at = NaiveTime::from_hms_opt(FIRE_HOUR, 0, 0).expect("valid fire time");
    for _ in 0..10 {
        if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
            let local_fire = day.and_time(fire_at).and_local_timezone(tz).single();
            if let Some(local_fire) = local_fire
                && local_fire.with_timezone(&Utc) > now_utc
            {
                return Some(local_fire.to_rfc3339());
            }
        }
        day = day.checked_add_days(Days::new(1))?;
    }
    None
}

fn is_fire_time_local(now_local: DateTime<Tz>) -> bool {
    let fire_at = NaiveTime::from_hms_opt(FIRE_HOUR, 0, 0).expect("valid fire time");
    !matches!(now_local.weekday(), Weekday::Sat | Weekday::Sun) && now_local.time() >= fire_at
}

fn parse_timezone(timezone: &str) -> Option<Tz> {
    timezone.parse::<Tz>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn window(from: &str, to: &str) -> AnalysisWindow {
        AnalysisWindow {
            date_from: from.to_string(),
            date_to: to.to_string(),
        }
    }

    #[test]
    fn monday_covers_friday_through_sunday() {
        assert_eq!(
            analysis_window_for(date("2026-06-15")),
            Some(window("2026-06-12", "2026-06-14"))
        );
    }

    #[test]
    fn tuesday_through_friday_cover_previous_day() {
        assert_eq!(
            analysis_window_for(date("2026-06-16")),
            Some(window("2026-06-15", "2026-06-15"))
        );
        assert_eq!(
            analysis_window_for(date("2026-06-12")),
            Some(window("2026-06-11", "2026-06-11"))
        );
    }

    #[test]
    fn weekend_has_no_window() {
        assert_eq!(analysis_window_for(date("2026-06-13")), None);
        assert_eq!(analysis_window_for(date("2026-06-14")), None);
    }

    #[test]
    fn monday_window_crosses_month_boundary() {
        assert_eq!(
            analysis_window_for(date("2026-03-02")),
            Some(window("2026-02-27", "2026-03-01"))
        );
    }

    #[test]
    fn friday_window_crosses_year_boundary() {
        assert_eq!(
            analysis_window_for(date("2027-01-01")),
            Some(window("2026-12-31", "2026-12-31"))
        );
    }

    #[test]
    fn fire_time_requires_weekday_at_eight_local() {
        // Lunes 2026-06-15, Chile en UTC-4 (sin DST): 08:00 SCL = 12:00 UTC.
        let before = Utc.with_ymd_and_hms(2026, 6, 15, 11, 59, 0).unwrap();
        let at = Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0).unwrap();
        assert!(!is_fire_time(before.with_timezone(&SCL)));
        assert!(is_fire_time(at.with_timezone(&SCL)));
    }

    #[test]
    fn fire_time_never_fires_on_weekends() {
        // Sábado 2026-06-13 a mediodía local.
        let saturday = Utc.with_ymd_and_hms(2026, 6, 13, 16, 0, 0).unwrap();
        assert!(!is_fire_time(saturday.with_timezone(&SCL)));
    }

    #[test]
    fn due_window_uses_configured_timezone() {
        // 08:00 Europe/Madrid on Monday 2026-06-15 is 06:00 UTC.
        let madrid_fire = Utc.with_ymd_and_hms(2026, 6, 15, 6, 0, 0).unwrap();
        assert_eq!(
            due_window_for(madrid_fire, "Europe/Madrid"),
            Some(window("2026-06-12", "2026-06-14"))
        );
        // At that same instant it is not yet 08:00 in New York.
        assert_eq!(due_window_for(madrid_fire, "America/New_York"), None);
    }

    #[test]
    fn next_fire_time_label_skips_weekends() {
        let friday_after_fire = Utc.with_ymd_and_hms(2026, 6, 12, 14, 0, 0).unwrap();
        let next = next_fire_time_label(friday_after_fire, "America/Santiago").unwrap();
        assert!(next.starts_with("2026-06-15T08:00:00"));
    }

    #[test]
    fn fire_time_tracks_chile_dst_offsets() {
        // Lunes 2026-04-06, justo después del fin del DST: Chile en UTC-4.
        let after_dst_end = Utc.with_ymd_and_hms(2026, 4, 6, 12, 0, 0).unwrap();
        let scl = after_dst_end.with_timezone(&SCL);
        assert_eq!(scl.time(), NaiveTime::from_hms_opt(8, 0, 0).unwrap());
        assert!(is_fire_time(scl));

        // Lunes 2026-09-07, justo después del inicio del DST: Chile en UTC-3.
        let after_dst_start = Utc.with_ymd_and_hms(2026, 9, 7, 11, 0, 0).unwrap();
        let scl = after_dst_start.with_timezone(&SCL);
        assert_eq!(scl.time(), NaiveTime::from_hms_opt(8, 0, 0).unwrap());
        assert!(is_fire_time(scl));

        // A las 11:00 UTC de ese lunes de abril aún son las 07:00 en Chile.
        let too_early = Utc.with_ymd_and_hms(2026, 4, 6, 11, 0, 0).unwrap();
        assert!(!is_fire_time(too_early.with_timezone(&SCL)));
    }
}
