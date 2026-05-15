use anyhow::{bail, Context, Result};
use rinha_backend_2026_luckstai_rust::index::build_index_from_json;
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let input_path = PathBuf::from(args.next().context("missing input references path")?);
    let output_path = PathBuf::from(args.next().context("missing output index path")?);

    if args.next().is_some() {
        bail!("usage: build-index <references.json.gz> <references.idx>");
    }

    let count = build_index_from_json(&input_path, &output_path)?;
    eprintln!(
        "wrote {count} quantized references from {} to {}",
        input_path.display(),
        output_path.display()
    );

    Ok(())
}
