use crate::domain::FraudRequest;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
#[cfg(test)]
use time::Weekday;

pub const DIMENSIONS: usize = 14;
pub const QUANT_SCALE: f64 = 10_000.0;
pub const SENTINEL_MISSING: i16 = -10_000;

struct ParsedTimestamp {
    unix_seconds: i64,
    hour: u8,
    weekday: u8,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NormalizationConfig {
    pub max_amount: f64,
    pub max_installments: f64,
    pub amount_vs_avg_ratio: f64,
    pub max_minutes: f64,
    pub max_km: f64,
    pub max_tx_count_24h: f64,
    pub max_merchant_avg_amount: f64,
}

pub struct Vectorizer {
    normalization: NormalizationConfig,
    mcc_risk: HashMap<String, f64>,
}

impl Vectorizer {
    pub fn load(normalization_path: &Path, mcc_risk_path: &Path) -> Result<Self> {
        let normalization: NormalizationConfig =
            serde_json::from_slice(&fs::read(normalization_path).with_context(|| {
                format!("failed to read normalization config at {}", normalization_path.display())
            })?)
            .with_context(|| {
                format!(
                    "failed to parse normalization config at {}",
                    normalization_path.display()
                )
            })?;

        let mcc_risk: HashMap<String, f64> =
            serde_json::from_slice(&fs::read(mcc_risk_path).with_context(|| {
                format!("failed to read mcc risk config at {}", mcc_risk_path.display())
            })?)
            .with_context(|| {
                format!("failed to parse mcc risk config at {}", mcc_risk_path.display())
            })?;

        Ok(Self {
            normalization,
            mcc_risk,
        })
    }

    pub fn normalize(&self, request: &FraudRequest) -> Result<[f64; DIMENSIONS]> {
        let requested_at = parse_rfc3339(&request.transaction.requested_at)?;
        let day_of_week = requested_at.weekday as f64 / 6.0;
        let hour_of_day = requested_at.hour as f64 / 23.0;

        let amount_vs_avg = if request.customer.avg_amount <= f64::EPSILON {
            1.0
        } else {
            (request.transaction.amount / request.customer.avg_amount)
                / self.normalization.amount_vs_avg_ratio
        };

        let mut vector = [0.0_f64; DIMENSIONS];
        vector[0] = clamp01(request.transaction.amount / self.normalization.max_amount);
        vector[1] =
            clamp01(request.transaction.installments as f64 / self.normalization.max_installments);
        vector[2] = clamp01(amount_vs_avg);
        vector[3] = hour_of_day;
        vector[4] = day_of_week;

        if let Some(last_transaction) = &request.last_transaction {
            let last_timestamp = parse_rfc3339(&last_transaction.timestamp)?;
            let minutes_since_last =
                (requested_at.unix_seconds - last_timestamp.unix_seconds) as f64 / 60.0;
            vector[5] = clamp01(minutes_since_last / self.normalization.max_minutes);
            vector[6] = clamp01(last_transaction.km_from_current / self.normalization.max_km);
        } else {
            vector[5] = -1.0;
            vector[6] = -1.0;
        }

        vector[7] = clamp01(request.terminal.km_from_home / self.normalization.max_km);
        vector[8] =
            clamp01(request.customer.tx_count_24h as f64 / self.normalization.max_tx_count_24h);
        vector[9] = if request.terminal.is_online { 1.0 } else { 0.0 };
        vector[10] = if request.terminal.card_present { 1.0 } else { 0.0 };
        vector[11] = if request
            .customer
            .known_merchants
            .iter()
            .any(|merchant| merchant == &request.merchant.id)
        {
            0.0
        } else {
            1.0
        };
        vector[12] = self
            .mcc_risk
            .get(&request.merchant.mcc)
            .copied()
            .unwrap_or(0.5);
        vector[13] =
            clamp01(request.merchant.avg_amount / self.normalization.max_merchant_avg_amount);

        Ok(vector.map(round4))
    }

    pub fn quantize(&self, request: &FraudRequest) -> Result<[i16; DIMENSIONS]> {
        Ok(self.normalize(request)?.map(quantize_value))
    }
}

pub fn quantize_reference(vector: [f64; DIMENSIONS]) -> [i16; DIMENSIONS] {
    vector.map(quantize_value)
}

fn quantize_value(value: f64) -> i16 {
    if value <= -1.0 {
        SENTINEL_MISSING
    } else {
        (value * QUANT_SCALE).round() as i16
    }
}

fn parse_rfc3339(timestamp: &str) -> Result<ParsedTimestamp> {
    if let Some(parsed) = parse_utc_zulu_timestamp(timestamp) {
        return Ok(parsed);
    }

    let parsed = OffsetDateTime::parse(timestamp, &Rfc3339)
        .with_context(|| format!("invalid rfc3339 timestamp: {timestamp}"))?;
    let unix_seconds = parsed.unix_timestamp();
    let days_since_epoch = unix_seconds.div_euclid(86_400);

    Ok(ParsedTimestamp {
        unix_seconds,
        hour: parsed.hour(),
        weekday: weekday_index_from_days(days_since_epoch),
    })
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn round4(value: f64) -> f64 {
    (value * QUANT_SCALE).round() / QUANT_SCALE
}

fn weekday_index_from_days(days_since_epoch: i64) -> u8 {
    ((days_since_epoch + 3).rem_euclid(7)) as u8
}

#[cfg(test)]
fn weekday_index(weekday: Weekday) -> u8 {
    match weekday {
        Weekday::Monday => 0,
        Weekday::Tuesday => 1,
        Weekday::Wednesday => 2,
        Weekday::Thursday => 3,
        Weekday::Friday => 4,
        Weekday::Saturday => 5,
        Weekday::Sunday => 6,
    }
}

fn parse_utc_zulu_timestamp(timestamp: &str) -> Option<ParsedTimestamp> {
    if timestamp.len() != 20 || !timestamp.ends_with('Z') {
        return None;
    }

    let bytes = timestamp.as_bytes();
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }

    let year = parse_4(bytes, 0)?;
    let month = parse_2(bytes, 5)?;
    let day = parse_2(bytes, 8)?;
    let hour = parse_2(bytes, 11)?;
    let minute = parse_2(bytes, 14)?;
    let second = parse_2(bytes, 17)?;

    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }

    let days_since_epoch = days_from_civil(year, month, day)?;
    let unix_seconds =
        days_since_epoch * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second);

    Some(ParsedTimestamp {
        unix_seconds,
        hour,
        weekday: weekday_index_from_days(days_since_epoch),
    })
}

fn parse_2(bytes: &[u8], start: usize) -> Option<u8> {
    let high = bytes.get(start)?.checked_sub(b'0')?;
    let low = bytes.get(start + 1)?.checked_sub(b'0')?;
    if high > 9 || low > 9 {
        return None;
    }
    Some(high * 10 + low)
}

fn parse_4(bytes: &[u8], start: usize) -> Option<i32> {
    let a = i32::from(bytes.get(start)?.checked_sub(b'0')?);
    let b = i32::from(bytes.get(start + 1)?.checked_sub(b'0')?);
    let c = i32::from(bytes.get(start + 2)?.checked_sub(b'0')?);
    let d = i32::from(bytes.get(start + 3)?.checked_sub(b'0')?);
    if a > 9 || b > 9 || c > 9 || d > 9 {
        return None;
    }
    Some(a * 1000 + b * 100 + c * 10 + d)
}

fn days_from_civil(year: i32, month: u8, day: u8) -> Option<i64> {
    let month = i32::from(month);
    let day = i32::from(day);
    let adjusted_year = year - if month <= 2 { 1 } else { 0 };
    let era = if adjusted_year >= 0 {
        adjusted_year / 400
    } else {
        (adjusted_year - 399) / 400
    };
    let year_of_era = adjusted_year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;

    Some(i64::from(days))
}

#[cfg(test)]
mod tests {
    use super::{parse_rfc3339, weekday_index, NormalizationConfig, Vectorizer};
    use crate::domain::FraudRequest;
    use std::collections::HashMap;
    use time::OffsetDateTime;

    fn vectorizer() -> Vectorizer {
        let mut mcc_risk = HashMap::new();
        mcc_risk.insert("5411".to_string(), 0.15);
        mcc_risk.insert("7802".to_string(), 0.75);

        Vectorizer {
            normalization: NormalizationConfig {
                max_amount: 10_000.0,
                max_installments: 12.0,
                amount_vs_avg_ratio: 10.0,
                max_minutes: 1440.0,
                max_km: 1000.0,
                max_tx_count_24h: 20.0,
                max_merchant_avg_amount: 10_000.0,
            },
            mcc_risk,
        }
    }

    #[test]
    fn normalizes_legit_example_from_docs() {
        let payload = r#"{
          "id":"tx-1329056812",
          "transaction":{"amount":41.12,"installments":2,"requested_at":"2026-03-11T18:45:53Z"},
          "customer":{"avg_amount":82.24,"tx_count_24h":3,"known_merchants":["MERC-003","MERC-016"]},
          "merchant":{"id":"MERC-016","mcc":"5411","avg_amount":60.25},
          "terminal":{"is_online":false,"card_present":true,"km_from_home":29.23},
          "last_transaction":null
        }"#;

        let request: FraudRequest = serde_json::from_str(payload).unwrap();
        let vector = vectorizer().quantize(&request).unwrap();

        assert_eq!(
            vector,
            [
                41, 1667, 500, 7826, 3333, -10_000, -10_000, 292, 1500, 0, 10_000, 0, 1500, 60
            ]
        );
    }

    #[test]
    fn normalizes_fraud_example_from_docs() {
        let payload = r#"{
          "id":"tx-3330991687",
          "transaction":{"amount":9505.97,"installments":10,"requested_at":"2026-03-14T05:15:12Z"},
          "customer":{"avg_amount":81.28,"tx_count_24h":20,"known_merchants":["MERC-008","MERC-007","MERC-005"]},
          "merchant":{"id":"MERC-068","mcc":"7802","avg_amount":54.86},
          "terminal":{"is_online":false,"card_present":true,"km_from_home":952.27},
          "last_transaction":null
        }"#;

        let request: FraudRequest = serde_json::from_str(payload).unwrap();
        let vector = vectorizer().quantize(&request).unwrap();

        assert_eq!(
            vector,
            [
                9506, 8333, 10_000, 2174, 8333, -10_000, -10_000, 9523, 10_000, 0, 10_000,
                10_000, 7500, 55
            ]
        );
    }

    #[test]
    fn fast_timestamp_parser_matches_time_crate_for_zulu_timestamps() {
        let timestamp = "2026-03-11T18:45:53Z";
        let fast = parse_rfc3339(timestamp).unwrap();
        let slow = OffsetDateTime::parse(timestamp, &time::format_description::well_known::Rfc3339)
            .unwrap();

        assert_eq!(fast.unix_seconds, slow.unix_timestamp());
        assert_eq!(fast.hour, slow.hour());
        assert_eq!(fast.weekday, weekday_index(slow.weekday()));
    }
}
