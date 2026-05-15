use crate::config::AppConfig;
use crate::domain::{FraudRequest, FraudResponse};
use crate::index::{validate_index_path, QuantizedIndex};
use crate::ivf::IvfIndex;
use crate::vectorize::Vectorizer;
use anyhow::{bail, Result};

pub enum SearchAlgorithm {
    Flat(QuantizedIndex),
    FlatPruned(QuantizedIndex),
    Ivf { index: IvfIndex, nprobe: usize },
    IvfAdaptive {
        index: IvfIndex,
        low_nprobe: usize,
        high_nprobe: usize,
        min_margin_ratio: f64,
    },
    IvfPureGate {
        index: IvfIndex,
        nprobe: usize,
        min_margin_ratio: f64,
    },
}

pub struct Detector {
    vectorizer: Vectorizer,
    algorithm: SearchAlgorithm,
}

impl Detector {
    pub fn load(config: &AppConfig) -> Result<Self> {
        let vectorizer =
            Vectorizer::load(&config.normalization_path, &config.mcc_risk_path)?;
        let algorithm = match config.algorithm.as_str() {
            "flat" => {
                validate_index_path(&config.index_path)?;
                SearchAlgorithm::Flat(QuantizedIndex::load(&config.index_path)?)
            }
            "flat-pruned" => {
                validate_index_path(&config.index_path)?;
                SearchAlgorithm::FlatPruned(QuantizedIndex::load(&config.index_path)?)
            }
            "ivf" => SearchAlgorithm::Ivf {
                index: IvfIndex::load(&config.ivf_index_path)?,
                nprobe: config.ivf_nprobe,
            },
            "ivf-adaptive" => SearchAlgorithm::IvfAdaptive {
                index: IvfIndex::load(&config.ivf_index_path)?,
                low_nprobe: config.ivf_low_nprobe,
                high_nprobe: config.ivf_nprobe,
                min_margin_ratio: config.ivf_adaptive_margin,
            },
            "ivf-pure-gate" => SearchAlgorithm::IvfPureGate {
                index: IvfIndex::load(&config.ivf_index_path)?,
                nprobe: config.ivf_nprobe,
                min_margin_ratio: config.ivf_gate_margin,
            },
            other => bail!("unsupported algorithm: {other}"),
        };

        Ok(Self { vectorizer, algorithm })
    }

    pub fn score(&self, request: &FraudRequest) -> Result<FraudResponse> {
        let fraud_neighbors = self.fraud_neighbors(request)?;
        let fraud_score = fraud_neighbors as f64 / 5.0;

        Ok(FraudResponse {
            approved: fraud_score < 0.6,
            fraud_score,
        })
    }

    pub fn fraud_neighbors(&self, request: &FraudRequest) -> Result<u8> {
        let query = self.vectorizer.quantize(request)?;
        let fraud_neighbors = match &self.algorithm {
            SearchAlgorithm::Flat(index) => index.fraud_count_top5_flat(&query),
            SearchAlgorithm::FlatPruned(index) => index.fraud_count_top5_pruned(&query),
            SearchAlgorithm::Ivf { index, nprobe } => index.fraud_count_top5(&query, *nprobe),
            SearchAlgorithm::IvfAdaptive {
                index,
                low_nprobe,
                high_nprobe,
                min_margin_ratio,
            } => index.fraud_count_top5_adaptive(&query, *low_nprobe, *high_nprobe, *min_margin_ratio),
            SearchAlgorithm::IvfPureGate {
                index,
                nprobe,
                min_margin_ratio,
            } => index.fraud_count_top5_pure_gate(&query, *nprobe, *min_margin_ratio),
        };

        Ok(fraud_neighbors)
    }

    pub fn reference_count(&self) -> usize {
        match &self.algorithm {
            SearchAlgorithm::Flat(index) => index.len(),
            SearchAlgorithm::FlatPruned(index) => index.len(),
            SearchAlgorithm::Ivf { index, .. } => index.len(),
            SearchAlgorithm::IvfAdaptive { index, .. } => index.len(),
            SearchAlgorithm::IvfPureGate { index, .. } => index.len(),
        }
    }
}
