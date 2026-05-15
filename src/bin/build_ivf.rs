use anyhow::{Context, Result};
use rinha_backend_2026_luckstai_rust::ivf::{build_ivf_from_flat, IvfBuildConfig};
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    let flat_index = env::args()
        .nth(1)
        .map(PathBuf::from)
        .context(
            "usage: build-ivf <flat.idx> <output.ivf> [nlist] [sample_size] [iterations] [partition_bits]",
        )?;
    let output_path = env::args()
        .nth(2)
        .map(PathBuf::from)
        .context(
            "usage: build-ivf <flat.idx> <output.ivf> [nlist] [sample_size] [iterations] [partition_bits]",
        )?;

    let nlist = env::args()
        .nth(3)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(512);
    let sample_size = env::args()
        .nth(4)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(32_768);
    let iterations = env::args()
        .nth(5)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(8);
    let partition_bits = env::args()
        .nth(6)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);

    let config = IvfBuildConfig {
        nlist,
        sample_size,
        iterations,
        partition_bits,
    };

    build_ivf_from_flat(&flat_index, &output_path, &config)?;
    eprintln!(
        "wrote ivf index from {} to {} with nlist={}, sample_size={}, iterations={}, partition_bits={}",
        flat_index.display(),
        output_path.display(),
        nlist,
        sample_size,
        iterations,
        partition_bits
    );
    Ok(())
}
