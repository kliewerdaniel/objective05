use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A parsed cron schedule that determines when a job should run.
///
/// Supports a limited subset of cron syntax:
/// - `* * * * *` -- every minute
/// - `0 * * * *` -- every hour at minute 0
/// - `*/5 * * * *` -- every 5 minutes
/// - `0 3 * * *` -- daily at 3:00 AM
/// - `0 */2 * * *` -- every 2 hours at minute 0
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct CronSchedule {
    pub minute: CronField,
    pub hour: CronField,
    pub day_of_month: CronField,
    pub month: CronField,
    pub day_of_week: CronField,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CronField {
    Any,
    Fixed(u32),
    Step(u32),
}

impl CronSchedule {
    /// Parse a 5-field cron expression.
    pub fn parse(expression: &str) -> Result<Self, String> {
        let fields: Vec<&str> = expression.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(format!(
                "cron expression must have 5 fields, got {}",
                fields.len()
            ));
        }

        Ok(CronSchedule {
            minute: parse_cron_field(fields[0], 0, 59)?,
            hour: parse_cron_field(fields[1], 0, 23)?,
            day_of_month: parse_cron_field(fields[2], 1, 31)?,
            month: parse_cron_field(fields[3], 1, 12)?,
            day_of_week: parse_cron_field(fields[4], 0, 6)?,
        })
    }

    /// Returns the next trigger time after the given time.
    pub fn next_trigger_after(&self, after: DateTime<Utc>) -> DateTime<Utc> {
        let mut candidate = after + Duration::minutes(1);
        candidate = candidate
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();

        for _ in 0..366 * 24 * 60 {
            if self.matches(&candidate) {
                return candidate;
            }
            candidate = candidate + Duration::minutes(1);
        }

        after + Duration::hours(1)
    }

    fn matches(&self, dt: &DateTime<Utc>) -> bool {
        field_matches(&self.minute, dt.minute(), 0, 59)
            && field_matches(&self.hour, dt.hour(), 0, 23)
            && field_matches(&self.day_of_month, dt.day(), 1, 31)
            && field_matches(&self.month, dt.month(), 1, 12)
            && field_matches(&self.day_of_week, dt.weekday().num_days_from_sunday(), 0, 6)
    }
}

fn parse_cron_field(field: &str, min: u32, max: u32) -> Result<CronField, String> {
    if field == "*" {
        return Ok(CronField::Any);
    }

    if let Some(step) = field.strip_prefix("*/") {
        let step_val: u32 = step.parse().map_err(|_| format!("invalid step: {field}"))?;
        if step_val == 0 {
            return Err("step cannot be zero".to_string());
        }
        return Ok(CronField::Step(step_val));
    }

    let val: u32 = field.parse().map_err(|_| format!("invalid cron field: {field}"))?;
    if val < min || val > max {
        return Err(format!("value {val} out of range {min}-{max}"));
    }
    Ok(CronField::Fixed(val))
}

fn field_matches(field: &CronField, value: u32, _min: u32, _max: u32) -> bool {
    match field {
        CronField::Any => true,
        CronField::Fixed(fixed) => value == *fixed,
        CronField::Step(step) => value % step == 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_every_minute() {
        let schedule = CronSchedule::parse("* * * * *").unwrap();
        assert_eq!(schedule.minute, CronField::Any);
        assert_eq!(schedule.hour, CronField::Any);
    }

    #[test]
    fn test_parse_every_5_minutes() {
        let schedule = CronSchedule::parse("*/5 * * * *").unwrap();
        assert_eq!(schedule.minute, CronField::Step(5));
    }

    #[test]
    fn test_parse_daily_at_3am() {
        let schedule = CronSchedule::parse("0 3 * * *").unwrap();
        assert_eq!(schedule.minute, CronField::Fixed(0));
        assert_eq!(schedule.hour, CronField::Fixed(3));
    }

    #[test]
    fn test_parse_every_2_hours() {
        let schedule = CronSchedule::parse("0 */2 * * *").unwrap();
        assert_eq!(schedule.minute, CronField::Fixed(0));
        assert_eq!(schedule.hour, CronField::Step(2));
    }

    #[test]
    fn test_next_trigger_after() {
        let schedule = CronSchedule::parse("0 * * * *").unwrap();
        let after = Utc::now();
        let next = schedule.next_trigger_after(after);
        assert!(next > after);
        assert_eq!(next.minute(), 0);
    }

    #[test]
    fn test_parse_invalid_field_count() {
        assert!(CronSchedule::parse("* * *").is_err());
    }

    #[test]
    fn test_parse_invalid_value() {
        assert!(CronSchedule::parse("60 * * * *").is_err());
    }
}
