use anyhow::{anyhow, bail, Context, Result};
use bytemuck::{bytes_of, cast_slice, Pod, Zeroable};
use flate2::read::GzDecoder;
use memmap2::Mmap;
use serde::de::{DeserializeSeed, SeqAccess, Visitor};
use serde::Deserialize;
use std::fmt;
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::mem::size_of;
use std::path::Path;

use crate::vectorize::{quantize_reference, DIMENSIONS};

const MAGIC: [u8; 8] = *b"R26IDX01";
const VERSION: u32 = 1;
const KNN_K: usize = 5;
const SEARCH_DIM_ORDER: [usize; DIMENSIONS] = [5, 6, 2, 7, 0, 8, 11, 12, 1, 13, 3, 4, 9, 10];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct IndexHeader {
    magic: [u8; 8],
    version: u32,
    count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PackedRecord {
    pub vector: [i16; DIMENSIONS],
    pub label: u8,
    pub _padding: u8,
}

#[derive(Deserialize)]
struct ReferenceJson {
    vector: [f64; DIMENSIONS],
    label: String,
}

pub struct QuantizedIndex {
    mmap: Mmap,
    count: usize,
}

impl QuantizedIndex {
    pub fn load(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("failed to open quantized index at {}", path.display()))?;
        let mmap = unsafe { Mmap::map(&file) }
            .with_context(|| format!("failed to mmap quantized index at {}", path.display()))?;

        if mmap.len() < size_of::<IndexHeader>() {
            bail!("index file too small: {}", path.display());
        }

        let header = *bytemuck::from_bytes::<IndexHeader>(&mmap[..size_of::<IndexHeader>()]);
        if header.magic != MAGIC {
            bail!("invalid index magic at {}", path.display());
        }
        if header.version != VERSION {
            bail!("unsupported index version {} at {}", header.version, path.display());
        }

        let records_bytes = &mmap[size_of::<IndexHeader>()..];
        if records_bytes.len() % size_of::<PackedRecord>() != 0 {
            bail!("corrupt index record region at {}", path.display());
        }

        let count = records_bytes.len() / size_of::<PackedRecord>();
        if count != header.count as usize {
            bail!(
                "index header count mismatch at {}: header={}, computed={count}",
                path.display(),
                header.count
            );
        }

        Ok(Self { mmap, count })
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn records_for_build(&self) -> &[PackedRecord] {
        self.records()
    }

    pub fn fraud_count_top5_flat(&self, query: &[i16; DIMENSIONS]) -> u8 {
        let mut best_distances = [i64::MAX; KNN_K];
        let mut best_labels = [0_u8; KNN_K];

        for record in self.records() {
            let distance = squared_distance(query, &record.vector);

            if distance >= best_distances[KNN_K - 1] {
                continue;
            }

            let mut slot = KNN_K - 1;
            while slot > 0 && distance < best_distances[slot - 1] {
                best_distances[slot] = best_distances[slot - 1];
                best_labels[slot] = best_labels[slot - 1];
                slot -= 1;
            }

            best_distances[slot] = distance;
            best_labels[slot] = record.label;
        }

        best_labels.iter().copied().sum()
    }

    pub fn fraud_count_top5_pruned(&self, query: &[i16; DIMENSIONS]) -> u8 {
        let mut best_distances = [i64::MAX; KNN_K];
        let mut best_labels = [0_u8; KNN_K];

        for record in self.records() {
            let current_limit = best_distances[KNN_K - 1];
            let distance = squared_distance_pruned(query, &record.vector, current_limit);

            if distance >= best_distances[KNN_K - 1] {
                continue;
            }

            let mut slot = KNN_K - 1;
            while slot > 0 && distance < best_distances[slot - 1] {
                best_distances[slot] = best_distances[slot - 1];
                best_labels[slot] = best_labels[slot - 1];
                slot -= 1;
            }

            best_distances[slot] = distance;
            best_labels[slot] = record.label;
        }

        best_labels.iter().copied().sum()
    }

    fn records(&self) -> &[PackedRecord] {
        cast_slice(&self.mmap[size_of::<IndexHeader>()..])
    }
}

pub fn build_index_from_json(input_path: &Path, output_path: &Path) -> Result<u32> {
    let reader = open_reference_reader(input_path)?;

    let output = File::create(output_path)
        .with_context(|| format!("failed to create index file at {}", output_path.display()))?;
    let mut writer = BufWriter::new(output);

    writer
        .write_all(bytes_of(&IndexHeader {
            magic: MAGIC,
            version: VERSION,
            count: 0,
        }))
        .context("failed to write index header")?;

    let mut sink = ReferenceArrayWriter {
        writer: &mut writer,
        count: 0,
    };
    let mut deserializer = serde_json::Deserializer::from_reader(reader);
    let count = (&mut sink)
        .deserialize(&mut deserializer)
        .context("failed to stream reference json array")?;

    writer.flush().context("failed to flush index body")?;
    writer
        .seek(SeekFrom::Start(0))
        .context("failed to seek index header")?;
    writer
        .write_all(bytes_of(&IndexHeader {
            magic: MAGIC,
            version: VERSION,
            count,
        }))
        .context("failed to rewrite final index header")?;
    writer.flush().context("failed to flush final index header")?;

    Ok(count)
}

fn open_reference_reader(path: &Path) -> Result<Box<dyn Read>> {
    let input = File::open(path)
        .with_context(|| format!("failed to open references file at {}", path.display()))?;

    match path.extension().and_then(|value| value.to_str()) {
        Some("gz") => Ok(Box::new(GzDecoder::new(input))),
        _ => Ok(Box::new(input)),
    }
}

fn squared_distance(left: &[i16; DIMENSIONS], right: &[i16; DIMENSIONS]) -> i64 {
    let mut sum = 0_i64;
    let mut index = 0;
    while index < DIMENSIONS {
        let diff = left[index] as i32 - right[index] as i32;
        sum += (diff * diff) as i64;
        index += 1;
    }
    sum
}

pub fn squared_distance_pruned(
    left: &[i16; DIMENSIONS],
    right: &[i16; DIMENSIONS],
    limit: i64,
) -> i64 {
    let mut sum = 0_i64;
    let mut step = 0;

    while step < DIMENSIONS {
        let index = SEARCH_DIM_ORDER[step];
        let diff = left[index] as i32 - right[index] as i32;
        sum += (diff * diff) as i64;
        if sum >= limit {
            return sum;
        }
        step += 1;
    }

    sum
}

struct ReferenceArrayWriter<'a, W> {
    writer: &'a mut W,
    count: u32,
}

impl<'de, 'a, W: Write + Seek> DeserializeSeed<'de> for &mut ReferenceArrayWriter<'a, W> {
    type Value = u32;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(self)
    }
}

impl<'de, 'a, W: Write + Seek> Visitor<'de> for &mut ReferenceArrayWriter<'a, W> {
    type Value = u32;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a json array of reference vectors")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while let Some(reference) = seq.next_element::<ReferenceJson>()? {
            if self.count == u32::MAX {
                return Err(serde::de::Error::custom("too many references for u32 header"));
            }

            let label = match reference.label.as_str() {
                "legit" => 0_u8,
                "fraud" => 1_u8,
                other => {
                    return Err(serde::de::Error::custom(format!(
                        "unexpected reference label: {other}"
                    )))
                }
            };

            let record = PackedRecord {
                vector: quantize_reference(reference.vector),
                label,
                _padding: 0,
            };

            self.writer
                .write_all(bytes_of(&record))
                .map_err(serde::de::Error::custom)?;
            self.count += 1;
        }

        Ok(self.count)
    }
}

pub fn validate_index_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!("index file not found at {}", path.display()));
    }

    Ok(())
}
