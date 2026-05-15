use std::env;
use std::path::PathBuf;

pub struct AppConfig {
    pub algorithm: String,
    pub bind_addr: String,
    pub workers: usize,
    pub normalization_path: PathBuf,
    pub mcc_risk_path: PathBuf,
    pub index_path: PathBuf,
    pub ivf_index_path: PathBuf,
    pub ivf_nprobe: usize,
    pub ivf_gate_margin: f64,
    pub ivf_low_nprobe: usize,
    pub ivf_adaptive_margin: f64,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            algorithm: env::var("RINHA_ALGORITHM").unwrap_or_else(|_| "ivf".to_string()),
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:9999".to_string()),
            workers: env::var("APP_WORKERS")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(1),
            normalization_path: env_path(
                "RINHA_NORMALIZATION_PATH",
                "/app/fixtures/resources/normalization.json",
            ),
            mcc_risk_path: env_path("RINHA_MCC_RISK_PATH", "/app/fixtures/resources/mcc_risk.json"),
            index_path: env_path("RINHA_INDEX_PATH", "/app/fixtures/resources/references.idx"),
            ivf_index_path: env_path(
                "RINHA_IVF_INDEX_PATH",
                "/app/fixtures/resources/references.ivf",
            ),
            ivf_nprobe: env::var("RINHA_IVF_NPROBE")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(8),
            ivf_gate_margin: env::var("RINHA_IVF_GATE_MARGIN")
                .ok()
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite() && *value >= 1.0)
                .unwrap_or(1.02),
            ivf_low_nprobe: env::var("RINHA_IVF_LOW_NPROBE")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(4),
            ivf_adaptive_margin: env::var("RINHA_IVF_ADAPTIVE_MARGIN")
                .ok()
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite() && *value >= 1.0)
                .unwrap_or(1.10),
        }
    }
}

fn env_path(key: &str, default: &str) -> PathBuf {
    PathBuf::from(env::var(key).unwrap_or_else(|_| default.to_string()))
}
