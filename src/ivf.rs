use anyhow::{bail, Context, Result};
use bytemuck::{bytes_of, cast_slice, Pod, Zeroable};
use memmap2::Mmap;
use std::cmp::min;
use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::mem::size_of;
use std::path::Path;

use crate::index::{squared_distance_pruned, PackedRecord, QuantizedIndex};
use crate::vectorize::{DIMENSIONS, SENTINEL_MISSING};

const MAGIC_V1: [u8; 8] = *b"R26IVF01";
const MAGIC_V2: [u8; 8] = *b"R26IVF02";
const VERSION: u32 = 1;
const KNN_K: usize = 5;
const LIST_PURITY_ALL_LEGIT: u8 = 0;
const LIST_PURITY_ALL_FRAUD: u8 = 1;
const LIST_PURITY_MIXED: u8 = 2;

const PARTITION_LAST_TX_MISSING: usize = 0;
const PARTITION_IS_ONLINE: usize = 1;
const PARTITION_CARD_PRESENT: usize = 2;
const PARTITION_UNKNOWN_MERCHANT: usize = 3;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct IvfHeaderV1 {
    magic: [u8; 8],
    version: u32,
    count: u32,
    nlist: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct IvfHeaderV2 {
    magic: [u8; 8],
    version: u32,
    count: u32,
    nlist: u32,
    partition_bits: u32,
    partition_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct IvfPartitionMeta {
    centroid_start: u32,
    centroid_count: u32,
    record_start: u32,
    record_count: u32,
}

pub struct IvfBuildConfig {
    pub nlist: usize,
    pub sample_size: usize,
    pub iterations: usize,
    pub partition_bits: usize,
}

pub struct IvfIndex {
    mmap: Mmap,
    count: usize,
    nlist: usize,
    partition_bits: usize,
    partition_offset: usize,
    centroids_offset: usize,
    offsets_offset: usize,
    records_offset: usize,
    list_purity: Vec<u8>,
}

impl IvfIndex {
    pub fn load(path: &Path) -> Result<Self> {
        let file =
            File::open(path).with_context(|| format!("failed to open ivf index {}", path.display()))?;
        let mmap = unsafe { Mmap::map(&file) }
            .with_context(|| format!("failed to mmap ivf index {}", path.display()))?;

        if mmap.len() < size_of::<IvfHeaderV1>() {
            bail!("ivf file too small: {}", path.display());
        }

        let magic = *bytemuck::from_bytes::<[u8; 8]>(&mmap[..8]);

        match magic {
            MAGIC_V1 => Self::load_v1(mmap, path),
            MAGIC_V2 => Self::load_v2(mmap, path),
            _ => bail!("invalid ivf magic at {}", path.display()),
        }
    }

    fn load_v1(mmap: Mmap, path: &Path) -> Result<Self> {
        let header = *bytemuck::from_bytes::<IvfHeaderV1>(&mmap[..size_of::<IvfHeaderV1>()]);
        if header.version != VERSION {
            bail!("unsupported ivf version {} at {}", header.version, path.display());
        }

        let nlist = header.nlist as usize;
        let count = header.count as usize;
        let partition_offset = size_of::<IvfHeaderV1>();
        let centroids_offset = partition_offset;
        let centroids_bytes = nlist * size_of::<[i16; DIMENSIONS]>();
        let offsets_offset = centroids_offset + centroids_bytes;
        let offsets_bytes = (nlist + 1) * size_of::<u32>();
        let records_offset = offsets_offset + offsets_bytes;
        let records_bytes = count * size_of::<PackedRecord>();

        if mmap.len() < records_offset + records_bytes {
            bail!("corrupt ivf file layout: {}", path.display());
        }

        let list_purity = build_list_purity_from_slices(
            nlist,
            cast_slice(&mmap[offsets_offset..records_offset]),
            cast_slice(&mmap[records_offset..records_offset + records_bytes]),
        );

        Ok(Self {
            mmap,
            count,
            nlist,
            partition_bits: 0,
            partition_offset,
            centroids_offset,
            offsets_offset,
            records_offset,
            list_purity,
        })
    }

    fn load_v2(mmap: Mmap, path: &Path) -> Result<Self> {
        let header = *bytemuck::from_bytes::<IvfHeaderV2>(&mmap[..size_of::<IvfHeaderV2>()]);
        if header.version != VERSION {
            bail!("unsupported ivf version {} at {}", header.version, path.display());
        }

        let nlist = header.nlist as usize;
        let count = header.count as usize;
        let partition_bits = header.partition_bits as usize;
        let partition_count = header.partition_count as usize;
        let expected_partition_count = 1_usize
            .checked_shl(partition_bits as u32)
            .unwrap_or(0);

        if partition_bits == 0 || partition_count != expected_partition_count {
            bail!("invalid ivf partition metadata at {}", path.display());
        }

        let partition_offset = size_of::<IvfHeaderV2>();
        let partition_bytes = partition_count * size_of::<IvfPartitionMeta>();
        let centroids_offset = partition_offset + partition_bytes;
        let centroids_bytes = nlist * size_of::<[i16; DIMENSIONS]>();
        let offsets_offset = centroids_offset + centroids_bytes;
        let offsets_bytes = (nlist + 1) * size_of::<u32>();
        let records_offset = offsets_offset + offsets_bytes;
        let records_bytes = count * size_of::<PackedRecord>();

        if mmap.len() < records_offset + records_bytes {
            bail!("corrupt partitioned ivf layout: {}", path.display());
        }

        let list_purity = build_list_purity_from_slices(
            nlist,
            cast_slice(&mmap[offsets_offset..records_offset]),
            cast_slice(&mmap[records_offset..records_offset + records_bytes]),
        );

        Ok(Self {
            mmap,
            count,
            nlist,
            partition_bits,
            partition_offset,
            centroids_offset,
            offsets_offset,
            records_offset,
            list_purity,
        })
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn fraud_count_top5(&self, query: &[i16; DIMENSIONS], nprobe: usize) -> u8 {
        if self.partition_bits == 0 {
            return self.fraud_count_top5_in_partition(query, nprobe, 0, self.nlist);
        }

        let partition_key = partition_key(query, self.partition_bits);
        let partition = self.partitions()[partition_key];
        if partition.record_count == 0 || partition.centroid_count == 0 {
            return self.fraud_count_top5_in_partition(query, nprobe, 0, self.nlist);
        }

        self.fraud_count_top5_in_partition(
            query,
            nprobe,
            partition.centroid_start as usize,
            partition.centroid_count as usize,
        )
    }

    pub fn fraud_count_top5_adaptive(
        &self,
        query: &[i16; DIMENSIONS],
        low_nprobe: usize,
        high_nprobe: usize,
        min_margin_ratio: f64,
    ) -> u8 {
        if self.partition_bits == 0 {
            return self.fraud_count_top5_adaptive_in_partition(
                query,
                low_nprobe,
                high_nprobe,
                min_margin_ratio,
                0,
                self.nlist,
            );
        }

        let partition_key = partition_key(query, self.partition_bits);
        let partition = self.partitions()[partition_key];
        if partition.record_count == 0 || partition.centroid_count == 0 {
            return self.fraud_count_top5_adaptive_in_partition(
                query,
                low_nprobe,
                high_nprobe,
                min_margin_ratio,
                0,
                self.nlist,
            );
        }

        self.fraud_count_top5_adaptive_in_partition(
            query,
            low_nprobe,
            high_nprobe,
            min_margin_ratio,
            partition.centroid_start as usize,
            partition.centroid_count as usize,
        )
    }

    pub fn fraud_count_top5_pure_gate(
        &self,
        query: &[i16; DIMENSIONS],
        nprobe: usize,
        min_margin_ratio: f64,
    ) -> u8 {
        if self.partition_bits == 0 {
            return self.fraud_count_top5_pure_gate_in_partition(
                query,
                nprobe,
                min_margin_ratio,
                0,
                self.nlist,
            );
        }

        let partition_key = partition_key(query, self.partition_bits);
        let partition = self.partitions()[partition_key];
        if partition.record_count == 0 || partition.centroid_count == 0 {
            return self.fraud_count_top5_pure_gate_in_partition(
                query,
                nprobe,
                min_margin_ratio,
                0,
                self.nlist,
            );
        }

        self.fraud_count_top5_pure_gate_in_partition(
            query,
            nprobe,
            min_margin_ratio,
            partition.centroid_start as usize,
            partition.centroid_count as usize,
        )
    }

    fn fraud_count_top5_in_partition(
        &self,
        query: &[i16; DIMENSIONS],
        nprobe: usize,
        centroid_start: usize,
        centroid_count: usize,
    ) -> u8 {
        let best_lists = self.select_best_lists(query, nprobe, centroid_start, centroid_count);
        self.score_records(query, &best_lists)
    }

    fn fraud_count_top5_adaptive_in_partition(
        &self,
        query: &[i16; DIMENSIONS],
        low_nprobe: usize,
        high_nprobe: usize,
        min_margin_ratio: f64,
        centroid_start: usize,
        centroid_count: usize,
    ) -> u8 {
        let high_nprobe = min(high_nprobe.max(1), centroid_count.max(1));
        let low_nprobe = min(low_nprobe.max(1), high_nprobe);
        let best_lists = self.select_best_lists(query, high_nprobe, centroid_start, centroid_count);
        let selected_len = if best_lists.len() >= 2 {
            let margin_ratio = best_lists[1].0 as f64 / (best_lists[0].0.max(1) as f64);
            if margin_ratio >= min_margin_ratio {
                low_nprobe
            } else {
                high_nprobe
            }
        } else {
            low_nprobe
        };

        self.score_records(query, &best_lists[..selected_len])
    }

    fn fraud_count_top5_pure_gate_in_partition(
        &self,
        query: &[i16; DIMENSIONS],
        nprobe: usize,
        min_margin_ratio: f64,
        centroid_start: usize,
        centroid_count: usize,
    ) -> u8 {
        let best_lists = self.select_best_lists(query, nprobe, centroid_start, centroid_count);

        if best_lists.len() >= 2 {
            let top_list_index = best_lists[0].1;
            let top_purity = self.list_purity[top_list_index];
            let margin_ratio = best_lists[1].0 as f64 / (best_lists[0].0.max(1) as f64);
            if margin_ratio >= min_margin_ratio {
                if top_purity == LIST_PURITY_ALL_LEGIT {
                    return 0;
                }
                if top_purity == LIST_PURITY_ALL_FRAUD {
                    return 5;
                }
            }
        }

        self.score_records(query, &best_lists)
    }

    fn partitions(&self) -> &[IvfPartitionMeta] {
        if self.partition_bits == 0 {
            return &[];
        }

        let end = self.centroids_offset;
        cast_slice(&self.mmap[self.partition_offset..end])
    }

    fn centroids(&self) -> &[[i16; DIMENSIONS]] {
        let end = self.offsets_offset;
        cast_slice(&self.mmap[self.centroids_offset..end])
    }

    fn offsets(&self) -> &[u32] {
        let end = self.records_offset;
        cast_slice(&self.mmap[self.offsets_offset..end])
    }

    fn records(&self) -> &[PackedRecord] {
        cast_slice(&self.mmap[self.records_offset..])
    }

    fn select_best_lists(
        &self,
        query: &[i16; DIMENSIONS],
        nprobe: usize,
        centroid_start: usize,
        centroid_count: usize,
    ) -> Vec<(i64, usize)> {
        let centroids = self.centroids();
        let centroid_end = centroid_start + centroid_count;
        let nprobe = min(nprobe.max(1), centroid_count.max(1));
        let mut best_lists = vec![(i64::MAX, centroid_start); nprobe];
        let mut centroid_index = centroid_start;
        while centroid_index < centroid_end {
            let distance = squared_distance_i16(query, &centroids[centroid_index]);
            if distance < best_lists[nprobe - 1].0 {
                let mut slot = nprobe - 1;
                while slot > 0 && distance < best_lists[slot - 1].0 {
                    best_lists[slot] = best_lists[slot - 1];
                    slot -= 1;
                }
                best_lists[slot] = (distance, centroid_index);
            }
            centroid_index += 1;
        }

        best_lists
    }

    fn score_records(
        &self,
        query: &[i16; DIMENSIONS],
        best_lists: &[(i64, usize)],
    ) -> u8 {
        let offsets = self.offsets();
        let records = self.records();
        let mut best_distances = [i64::MAX; KNN_K];
        let mut best_labels = [0_u8; KNN_K];

        for &(_, list_index) in best_lists {
            let start = offsets[list_index] as usize;
            let end = offsets[list_index + 1] as usize;

            for record in &records[start..end] {
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
        }

        best_labels.iter().copied().sum()
    }
}

pub fn build_ivf_from_flat(
    flat_index_path: &Path,
    output_path: &Path,
    config: &IvfBuildConfig,
) -> Result<()> {
    if config.nlist == 0 {
        bail!("nlist must be > 0");
    }
    if config.iterations == 0 {
        bail!("iterations must be > 0");
    }
    if config.partition_bits > 4 {
        bail!("partition_bits must be between 0 and 4");
    }

    let flat = QuantizedIndex::load(flat_index_path)?;
    let records = flat.records_for_build();
    let count = records.len();
    if count == 0 {
        bail!("flat index is empty");
    }

    if config.partition_bits == 0 {
        return build_standard_ivf(records, output_path, config);
    }

    build_partitioned_ivf(records, output_path, config)
}

fn build_standard_ivf(
    records: &[PackedRecord],
    output_path: &Path,
    config: &IvfBuildConfig,
) -> Result<()> {
    let count = records.len();
    let sample_size = min(config.sample_size.max(config.nlist), count);
    let sample = sample_records(records, sample_size);
    let centroids = train_centroids(&sample, config.nlist, config.iterations);

    let mut counts = vec![0_u32; config.nlist];
    for record in records {
        let centroid = nearest_centroid(&record.vector, &centroids);
        counts[centroid] += 1;
    }

    let offsets = build_offsets(&counts);
    let mut grouped_records = vec![PackedRecord::zeroed(); count];
    let mut positions = offsets[..config.nlist].to_vec();

    for record in records {
        let centroid = nearest_centroid(&record.vector, &centroids);
        let position = positions[centroid] as usize;
        grouped_records[position] = *record;
        positions[centroid] += 1;
    }

    write_standard_ivf(output_path, records.len(), &centroids, &offsets, &grouped_records)
}

fn build_partitioned_ivf(
    records: &[PackedRecord],
    output_path: &Path,
    config: &IvfBuildConfig,
) -> Result<()> {
    let partition_count = 1_usize << config.partition_bits;
    let mut partition_indices = vec![Vec::<usize>::new(); partition_count];
    for (index, record) in records.iter().enumerate() {
        partition_indices[partition_key(&record.vector, config.partition_bits)].push(index);
    }

    let partition_sizes = partition_indices.iter().map(Vec::len).collect::<Vec<_>>();
    let nlists_per_partition = allocate_partition_nlists(&partition_sizes, config.nlist)?;
    let mut sample_sizes = allocate_partition_samples(&partition_sizes, config.sample_size);

    let mut partitions = Vec::with_capacity(partition_count);
    let mut all_centroids = Vec::with_capacity(config.nlist);
    for partition_index in 0..partition_count {
        let indices = &partition_indices[partition_index];
        let assigned_nlist = nlists_per_partition[partition_index];
        if indices.is_empty() {
            partitions.push(BuildPartition {
                centroid_start: all_centroids.len(),
                centroids: Vec::new(),
                indices,
            });
            continue;
        }

        let sample_size = sample_sizes[partition_index].max(assigned_nlist);
        let sample = sample_partition_records(records, indices, sample_size);
        let centroids = train_centroids(&sample, assigned_nlist, config.iterations);
        let centroid_start = all_centroids.len();
        all_centroids.extend(centroids.iter().copied());
        partitions.push(BuildPartition {
            centroid_start,
            centroids,
            indices,
        });
    }

    let total_nlist = all_centroids.len();
    if total_nlist != config.nlist {
        bail!(
            "partitioned ivf centroid count mismatch: expected {}, got {}",
            config.nlist,
            total_nlist
        );
    }

    let mut counts = vec![0_u32; total_nlist];
    for partition in &partitions {
        for &record_index in partition.indices {
            let record = &records[record_index];
            let local_centroid = nearest_centroid(&record.vector, &partition.centroids);
            counts[partition.centroid_start + local_centroid] += 1;
        }
    }

    let offsets = build_offsets(&counts);
    let mut grouped_records = vec![PackedRecord::zeroed(); records.len()];
    let mut positions = offsets[..total_nlist].to_vec();
    let mut partition_meta = vec![IvfPartitionMeta::zeroed(); partition_count];

    for (partition_index, partition) in partitions.iter().enumerate() {
        let record_start = partition
            .centroids
            .first()
            .map(|_| offsets[partition.centroid_start] as usize)
            .unwrap_or(0);
        let record_end = partition
            .centroids
            .last()
            .map(|_| offsets[partition.centroid_start + partition.centroids.len()] as usize)
            .unwrap_or(record_start);

        partition_meta[partition_index] = IvfPartitionMeta {
            centroid_start: partition.centroid_start as u32,
            centroid_count: partition.centroids.len() as u32,
            record_start: record_start as u32,
            record_count: (record_end - record_start) as u32,
        };

        for &record_index in partition.indices {
            let record = &records[record_index];
            let local_centroid = nearest_centroid(&record.vector, &partition.centroids);
            let global_centroid = partition.centroid_start + local_centroid;
            let position = positions[global_centroid] as usize;
            grouped_records[position] = *record;
            positions[global_centroid] += 1;
        }
    }

    sample_sizes.clear();

    write_partitioned_ivf(
        output_path,
        records.len(),
        config.partition_bits,
        &partition_meta,
        &all_centroids,
        &offsets,
        &grouped_records,
    )
}

fn write_standard_ivf(
    output_path: &Path,
    count: usize,
    centroids: &[[i16; DIMENSIONS]],
    offsets: &[u32],
    grouped_records: &[PackedRecord],
) -> Result<()> {
    let output = File::create(output_path)
        .with_context(|| format!("failed to create ivf index {}", output_path.display()))?;
    let mut writer = BufWriter::new(output);

    writer.write_all(bytes_of(&IvfHeaderV1 {
        magic: MAGIC_V1,
        version: VERSION,
        count: count as u32,
        nlist: centroids.len() as u32,
    }))?;
    writer.write_all(cast_slice(centroids))?;
    writer.write_all(cast_slice(offsets))?;
    writer.write_all(cast_slice(grouped_records))?;
    writer.flush()?;
    writer.seek(SeekFrom::Start(0))?;
    writer.flush()?;

    Ok(())
}

fn write_partitioned_ivf(
    output_path: &Path,
    count: usize,
    partition_bits: usize,
    partition_meta: &[IvfPartitionMeta],
    centroids: &[[i16; DIMENSIONS]],
    offsets: &[u32],
    grouped_records: &[PackedRecord],
) -> Result<()> {
    let output = File::create(output_path)
        .with_context(|| format!("failed to create ivf index {}", output_path.display()))?;
    let mut writer = BufWriter::new(output);

    writer.write_all(bytes_of(&IvfHeaderV2 {
        magic: MAGIC_V2,
        version: VERSION,
        count: count as u32,
        nlist: centroids.len() as u32,
        partition_bits: partition_bits as u32,
        partition_count: partition_meta.len() as u32,
    }))?;
    writer.write_all(cast_slice(partition_meta))?;
    writer.write_all(cast_slice(centroids))?;
    writer.write_all(cast_slice(offsets))?;
    writer.write_all(cast_slice(grouped_records))?;
    writer.flush()?;
    writer.seek(SeekFrom::Start(0))?;
    writer.flush()?;

    Ok(())
}

fn build_offsets(counts: &[u32]) -> Vec<u32> {
    let mut offsets = vec![0_u32; counts.len() + 1];
    let mut index = 0;
    while index < counts.len() {
        offsets[index + 1] = offsets[index] + counts[index];
        index += 1;
    }
    offsets
}

fn sample_records(records: &[PackedRecord], sample_size: usize) -> Vec<[i16; DIMENSIONS]> {
    let stride = (records.len() / sample_size).max(1);
    let mut sample = Vec::with_capacity(sample_size);
    let mut index = 0;
    while index < records.len() && sample.len() < sample_size {
        sample.push(records[index].vector);
        index += stride;
    }
    while sample.len() < sample_size {
        sample.push(records[sample.len() % records.len()].vector);
    }
    sample
}

fn sample_partition_records(
    records: &[PackedRecord],
    indices: &[usize],
    sample_size: usize,
) -> Vec<[i16; DIMENSIONS]> {
    let sample_size = min(sample_size, indices.len()).max(1);
    let stride = (indices.len() / sample_size).max(1);
    let mut sample = Vec::with_capacity(sample_size);
    let mut index = 0;
    while index < indices.len() && sample.len() < sample_size {
        sample.push(records[indices[index]].vector);
        index += stride;
    }
    while sample.len() < sample_size {
        sample.push(records[indices[sample.len() % indices.len()]].vector);
    }
    sample
}

fn train_centroids(
    sample: &[[i16; DIMENSIONS]],
    nlist: usize,
    iterations: usize,
) -> Vec<[i16; DIMENSIONS]> {
    let mut centroids = initial_centroids(sample, nlist);

    for _ in 0..iterations {
        let mut sums = vec![[0_i64; DIMENSIONS]; nlist];
        let mut counts = vec![0_u32; nlist];

        for vector in sample {
            let centroid = nearest_centroid(vector, &centroids);
            counts[centroid] += 1;
            let mut dim = 0;
            while dim < DIMENSIONS {
                sums[centroid][dim] += vector[dim] as i64;
                dim += 1;
            }
        }

        for centroid_index in 0..nlist {
            if counts[centroid_index] == 0 {
                centroids[centroid_index] = sample[centroid_index % sample.len()];
                continue;
            }

            let mut dim = 0;
            while dim < DIMENSIONS {
                centroids[centroid_index][dim] =
                    (sums[centroid_index][dim] / counts[centroid_index] as i64) as i16;
                dim += 1;
            }
        }
    }

    centroids
}

fn initial_centroids(sample: &[[i16; DIMENSIONS]], nlist: usize) -> Vec<[i16; DIMENSIONS]> {
    let mut centroids = Vec::with_capacity(nlist);
    let stride = (sample.len() / nlist).max(1);
    let mut index = 0;
    while centroids.len() < nlist {
        centroids.push(sample[index % sample.len()]);
        index += stride;
    }
    centroids
}

fn nearest_centroid(vector: &[i16; DIMENSIONS], centroids: &[[i16; DIMENSIONS]]) -> usize {
    let mut best_index = 0;
    let mut best_distance = i64::MAX;
    let mut index = 0;

    while index < centroids.len() {
        let distance = squared_distance_i16(vector, &centroids[index]);
        if distance < best_distance {
            best_distance = distance;
            best_index = index;
        }
        index += 1;
    }

    best_index
}

fn squared_distance_i16(left: &[i16; DIMENSIONS], right: &[i16; DIMENSIONS]) -> i64 {
    let mut sum = 0_i64;
    let mut index = 0;
    while index < DIMENSIONS {
        let diff = left[index] as i32 - right[index] as i32;
        sum += (diff * diff) as i64;
        index += 1;
    }
    sum
}

fn build_list_purity_from_slices(
    nlist: usize,
    offsets: &[u32],
    records: &[PackedRecord],
) -> Vec<u8> {
    let mut purity = vec![LIST_PURITY_MIXED; nlist];

    for list_index in 0..nlist {
        let start = offsets[list_index] as usize;
        let end = offsets[list_index + 1] as usize;
        if start >= end {
            continue;
        }

        let first_label = records[start].label;
        let mut only_one_label = true;
        let mut record_index = start + 1;
        while record_index < end {
            if records[record_index].label != first_label {
                only_one_label = false;
                break;
            }
            record_index += 1;
        }

        if only_one_label {
            purity[list_index] = if first_label == 0 {
                LIST_PURITY_ALL_LEGIT
            } else {
                LIST_PURITY_ALL_FRAUD
            };
        }
    }

    purity
}

fn partition_key(vector: &[i16; DIMENSIONS], partition_bits: usize) -> usize {
    let mut key = 0_usize;

    if partition_bits > PARTITION_LAST_TX_MISSING && vector[5] == SENTINEL_MISSING {
        key |= 1 << PARTITION_LAST_TX_MISSING;
    }
    if partition_bits > PARTITION_IS_ONLINE && vector[9] > 0 {
        key |= 1 << PARTITION_IS_ONLINE;
    }
    if partition_bits > PARTITION_CARD_PRESENT && vector[10] > 0 {
        key |= 1 << PARTITION_CARD_PRESENT;
    }
    if partition_bits > PARTITION_UNKNOWN_MERCHANT && vector[11] > 0 {
        key |= 1 << PARTITION_UNKNOWN_MERCHANT;
    }

    key
}

fn allocate_partition_nlists(partition_sizes: &[usize], total_nlist: usize) -> Result<Vec<usize>> {
    let total_records = partition_sizes.iter().sum::<usize>();
    let non_empty = partition_sizes.iter().filter(|size| **size > 0).count();

    if total_records == 0 {
        bail!("cannot allocate nlists for empty partitions");
    }
    if total_nlist < non_empty {
        bail!("nlist {} is smaller than non-empty partitions {}", total_nlist, non_empty);
    }

    let mut allocation = vec![0_usize; partition_sizes.len()];
    let mut remaining = total_nlist;
    for (index, size) in partition_sizes.iter().enumerate() {
        if *size > 0 {
            allocation[index] = 1;
            remaining -= 1;
        }
    }

    if remaining == 0 {
        return Ok(allocation);
    }

    let total_records_f64 = total_records as f64;
    let mut fractional = Vec::with_capacity(partition_sizes.len());
    for (index, size) in partition_sizes.iter().enumerate() {
        if *size == 0 {
            fractional.push((0.0_f64, index));
            continue;
        }

        let exact_extra = *size as f64 * remaining as f64 / total_records_f64;
        let base = exact_extra.floor() as usize;
        allocation[index] += base;
        fractional.push((exact_extra - base as f64, index));
    }

    let assigned = allocation.iter().sum::<usize>();
    let leftovers = total_nlist.saturating_sub(assigned);
    fractional.sort_by(|left, right| right.0.total_cmp(&left.0));
    for (_, index) in fractional.into_iter().take(leftovers) {
        if partition_sizes[index] > 0 {
            allocation[index] += 1;
        }
    }

    Ok(allocation)
}

fn allocate_partition_samples(partition_sizes: &[usize], total_sample_size: usize) -> Vec<usize> {
    let total_records = partition_sizes.iter().sum::<usize>().max(1);
    let mut allocation = vec![0_usize; partition_sizes.len()];

    for (index, size) in partition_sizes.iter().enumerate() {
        if *size == 0 {
            continue;
        }

        let proportional = (*size * total_sample_size / total_records).max(1);
        allocation[index] = min(*size, proportional);
    }

    allocation
}

struct BuildPartition<'a> {
    centroid_start: usize,
    centroids: Vec<[i16; DIMENSIONS]>,
    indices: &'a [usize],
}

#[cfg(test)]
mod tests {
    use super::{allocate_partition_nlists, partition_key};
    use crate::vectorize::{DIMENSIONS, SENTINEL_MISSING};

    #[test]
    fn partition_key_uses_expected_bits() {
        let mut vector = [0_i16; DIMENSIONS];
        vector[5] = SENTINEL_MISSING;
        vector[9] = 10_000;
        vector[10] = 0;
        vector[11] = 10_000;

        assert_eq!(partition_key(&vector, 1), 0b0001);
        assert_eq!(partition_key(&vector, 2), 0b0011);
        assert_eq!(partition_key(&vector, 3), 0b0011);
        assert_eq!(partition_key(&vector, 4), 0b1011);
    }

    #[test]
    fn nlist_allocation_preserves_total_and_non_empty_partitions() {
        let allocation = allocate_partition_nlists(&[100, 200, 0, 700], 16).unwrap();

        assert_eq!(allocation.iter().sum::<usize>(), 16);
        assert_eq!(allocation[2], 0);
        assert!(allocation[0] >= 1);
        assert!(allocation[1] >= 1);
        assert!(allocation[3] >= 1);
        assert!(allocation[3] > allocation[0]);
    }
}
