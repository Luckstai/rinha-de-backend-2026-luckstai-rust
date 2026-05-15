use anyhow::{Context, Result};
use rinha_backend_2026_luckstai_rust::config::AppConfig;
use rinha_backend_2026_luckstai_rust::detector::Detector;
use rinha_backend_2026_luckstai_rust::domain::FraudRequest;
use serde::Deserialize;
use std::env;
use std::fs;
use std::time::Instant;

#[derive(Deserialize)]
struct BenchFixture {
    entries: Vec<BenchEntry>,
}

#[derive(Deserialize)]
struct BenchEntry {
    request: FraudRequest,
}

fn main() -> Result<()> {
    let dataset_path = env::var("BENCH_DATASET")
        .unwrap_or_else(|_| "test/test-data.json".to_string());
    let limit = env::var("BENCH_LIMIT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(10_000);

    let fixture: BenchFixture = serde_json::from_slice(
        &fs::read(&dataset_path)
            .with_context(|| format!("failed to read benchmark dataset {}", dataset_path))?,
    )
    .with_context(|| format!("failed to parse benchmark dataset {}", dataset_path))?;

    let raw_requests = fixture
        .entries
        .into_iter()
        .take(limit)
        .map(|entry| serde_json::to_vec(&entry.request))
        .collect::<serde_json::Result<Vec<_>>>()
        .with_context(|| "failed to serialize benchmark requests")?;

    let detector = Detector::load(&AppConfig::from_env())?;
    let mut parse_elapsed = Vec::with_capacity(raw_requests.len());
    let mut score_elapsed = Vec::with_capacity(raw_requests.len());
    let mut encode_elapsed = Vec::with_capacity(raw_requests.len());
    let mut total_elapsed = Vec::with_capacity(raw_requests.len());

    for payload in &raw_requests {
        let started = Instant::now();

        let parse_started = Instant::now();
        let request: FraudRequest = serde_json::from_slice(payload)
            .with_context(|| "failed to parse serialized request")?;
        parse_elapsed.push(parse_started.elapsed().as_micros() as u64);

        let score_started = Instant::now();
        let fraud_neighbors = detector.fraud_neighbors(&request)?;
        score_elapsed.push(score_started.elapsed().as_micros() as u64);

        let encode_started = Instant::now();
        let _body = response_bytes(fraud_neighbors);
        encode_elapsed.push(encode_started.elapsed().as_micros() as u64);

        total_elapsed.push(started.elapsed().as_micros() as u64);
    }

    report("parse_us", &parse_elapsed);
    report("score_us", &score_elapsed);
    report("encode_us", &encode_elapsed);
    report("total_us", &total_elapsed);

    Ok(())
}

fn response_bytes(fraud_neighbors: u8) -> &'static [u8] {
    match fraud_neighbors {
        0 => br#"{"approved":true,"fraud_score":0.0}"#,
        1 => br#"{"approved":true,"fraud_score":0.2}"#,
        2 => br#"{"approved":true,"fraud_score":0.4}"#,
        3 => br#"{"approved":false,"fraud_score":0.6}"#,
        4 => br#"{"approved":false,"fraud_score":0.8}"#,
        _ => br#"{"approved":false,"fraud_score":1.0}"#,
    }
}

fn report(label: &str, values: &[u64]) {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let avg = sorted.iter().copied().sum::<u64>() as f64 / sorted.len() as f64;

    println!(
        "{} avg={:.1} p50={} p95={} p99={}",
        label,
        avg,
        percentile(&sorted, 0.50),
        percentile(&sorted, 0.95),
        percentile(&sorted, 0.99)
    );
}

fn percentile(values: &[u64], ratio: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let index = ((values.len() - 1) as f64 * ratio).round() as usize;
    values[index]
}
