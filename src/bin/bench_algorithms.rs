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
    expected_approved: bool,
}

fn main() -> Result<()> {
    let dataset_path = env::var("BENCH_DATASET")
        .unwrap_or_else(|_| "test/test-data.json".to_string());
    let limit = env::var("BENCH_LIMIT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(2_000);

    let fixture: BenchFixture = serde_json::from_slice(
        &fs::read(&dataset_path)
            .with_context(|| format!("failed to read benchmark dataset {}", dataset_path))?,
    )
    .with_context(|| format!("failed to parse benchmark dataset {}", dataset_path))?;

    let mut config = AppConfig::from_env();
    let algorithms = selected_algorithms();
    let selected = fixture.entries.into_iter().take(limit).collect::<Vec<_>>();

    for algorithm in algorithms {
        if algorithm.starts_with("ivf") && !config.ivf_index_path.exists() {
            println!("algorithm={algorithm} skipped (missing {})", config.ivf_index_path.display());
            continue;
        }

        config.algorithm = algorithm.to_string();
        let detector = Detector::load(&config)?;
        let mut fp = 0_u64;
        let mut fn_count = 0_u64;
        let mut elapsed = Vec::with_capacity(selected.len());

        for entry in &selected {
            let started = Instant::now();
            let response = detector.score(&entry.request)?;
            elapsed.push(started.elapsed().as_micros() as u64);

            if response.approved != entry.expected_approved {
                if response.approved {
                    fn_count += 1;
                } else {
                    fp += 1;
                }
            }
        }

        elapsed.sort_unstable();
        let p50 = percentile(&elapsed, 0.50);
        let p95 = percentile(&elapsed, 0.95);
        let p99 = percentile(&elapsed, 0.99);
        let avg = elapsed.iter().copied().sum::<u64>() as f64 / elapsed.len() as f64;

        println!(
            "algorithm={algorithm} checked={} fp={} fn={} avg_us={:.1} p50_us={} p95_us={} p99_us={}",
            elapsed.len(),
            fp,
            fn_count,
            avg,
            p50,
            p95,
            p99
        );
    }

    Ok(())
}

fn selected_algorithms() -> Vec<String> {
    match env::var("BENCH_ALGORITHMS") {
        Ok(value) => value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        Err(_) => vec![
            "flat".to_string(),
            "flat-pruned".to_string(),
            "ivf".to_string(),
            "ivf-adaptive".to_string(),
            "ivf-pure-gate".to_string(),
        ],
    }
}

fn percentile(values: &[u64], ratio: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let index = ((values.len() - 1) as f64 * ratio).round() as usize;
    values[index]
}
