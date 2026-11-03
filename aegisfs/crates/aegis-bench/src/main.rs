use aegis_classifier::{classify, demo_flag, BlockFeatures};
use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Parser)]
#[command(about = "Offline paired-file feature evaluation; does not monitor or protect files")]
struct Args {
    /// CSV with pre,post,label columns; paths relative to the CSV directory.
    #[arg(long)]
    manifest: PathBuf,
    /// JSON output path (must not be the manifest or an input file).
    #[arg(long)]
    report: PathBuf,
}
#[derive(Deserialize)]
struct Pair {
    pre: PathBuf,
    post: PathBuf,
    label: String,
    /// Optional semicolon-separated zero-based block indexes for intermittent changes.
    #[serde(default)]
    positive_blocks: Option<String>,
    #[serde(default)]
    workload: Option<String>,
}
#[derive(Serialize, Default, Clone)]
struct Confusion {
    true_positive: u64,
    false_positive: u64,
    true_negative: u64,
    false_negative: u64,
}
#[derive(Serialize)]
struct Observation {
    pair: usize,
    label: String,
    workload: String,
    flagged: bool,
    features: BlockFeatures,
}
#[derive(Serialize)]
struct Report {
    schema_version: u8,
    warning: &'static str,
    elapsed_seconds: f64,
    bytes_compared: u64,
    pairs: usize,
    blocks: u64,
    skipped_short_blocks: u64,
    confusion: Confusion,
    /// Block-level results; row labels or explicit positive block indices are ground truth.
    baselines: BTreeMap<String, Confusion>,
    workloads: BTreeMap<String, BTreeMap<String, Confusion>>,
    observations: Vec<Observation>,
    observations_truncated: bool,
}

fn add(confusion: &mut Confusion, positive: bool, flag: bool) {
    match (positive, flag) {
        (true, true) => confusion.true_positive += 1,
        (true, false) => confusion.false_negative += 1,
        (false, true) => confusion.false_positive += 1,
        (false, false) => confusion.true_negative += 1,
    }
}

fn block_labels(pair: &Pair, bytes: u64) -> Result<Option<HashSet<u64>>> {
    let Some(spec) = pair.positive_blocks.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if pair.label != "suspicious" {
        bail!("positive_blocks is only valid for suspicious pairs");
    }
    let mut selected = HashSet::new();
    for part in spec.split(';') {
        let index: u64 = part
            .parse()
            .context("positive_blocks must contain semicolon-separated zero-based integers")?;
        if index >= bytes.div_ceil(4096) {
            bail!("positive block index {index} is outside the file");
        }
        if !selected.insert(index) {
            bail!("duplicate positive block index {index}");
        }
    }
    Ok(Some(selected))
}

fn read_block(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match r.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(total)
}
fn canonical_existing(p: &Path) -> Result<PathBuf> {
    p.canonicalize()
        .with_context(|| format!("cannot resolve {}", p.display()))
}
fn main() -> Result<()> {
    let args = Args::parse();
    let manifest = canonical_existing(&args.manifest)?;
    let base = manifest.parent().context("manifest has no parent")?;
    let pairs: Vec<Pair> = csv::Reader::from_path(&manifest)?
        .deserialize()
        .collect::<std::result::Result<_, _>>()?;
    if pairs.is_empty() {
        bail!("manifest contains no pairs");
    }
    // Validate before output creation, including aliasing through symlinks/hard links.
    let mut resolved = Vec::new();
    let mut positive_blocks = Vec::new();
    for p in &pairs {
        if p.label != "benign" && p.label != "suspicious" {
            bail!("label must be benign or suspicious");
        }
        let a = canonical_existing(&base.join(&p.pre))?;
        let b = canonical_existing(&base.join(&p.post))?;
        if !a.is_file() || !b.is_file() {
            bail!("inputs must be regular files");
        }
        if a.metadata()?.len() != b.metadata()?.len() {
            bail!("input lengths differ: {} and {}", a.display(), b.display());
        }
        if a.metadata()?.len() == 0 {
            bail!("empty file pairs are not supported");
        }
        positive_blocks.push(block_labels(p, a.metadata()?.len())?);
        resolved.push((a, b));
    }
    use std::os::unix::fs::MetadataExt;
    if args.report.exists() {
        let out = args.report.metadata()?;
        let sources = std::iter::once(&manifest).chain(resolved.iter().flat_map(|(a, b)| [a, b]));
        for source in sources {
            let m = source.metadata()?;
            if m.dev() == out.dev() && m.ino() == out.ino() {
                bail!("report aliases an input file");
            }
        }
    }
    let start = Instant::now();
    let (mut confusion, mut observations) = (Confusion::default(), Vec::new());
    let mut baselines: BTreeMap<String, Confusion> = BTreeMap::new();
    let mut workloads: BTreeMap<String, BTreeMap<String, Confusion>> = BTreeMap::new();
    let (mut blocks, mut bytes, mut short) = (0, 0, 0);
    for (index, (a, b)) in resolved.iter().enumerate() {
        let (mut pre, mut post) = (
            BufReader::new(File::open(a)?),
            BufReader::new(File::open(b)?),
        );
        let (mut ab, mut bb) = ([0u8; 4096], [0u8; 4096]);
        let mut offset = 0u64;
        loop {
            let x = read_block(&mut pre, &mut ab)?;
            let y = read_block(&mut post, &mut bb)?;
            if x != y {
                bail!("input changed during reading");
            }
            if x == 0 {
                break;
            }
            let f = classify(&ab[..x], &bb[..x], offset).map_err(anyhow::Error::msg)?;
            let flag = demo_flag(&f);
            let positive = pairs[index].label == "suspicious"
                && positive_blocks[index]
                    .as_ref()
                    .is_none_or(|set| set.contains(&(offset / 4096)));
            let workload = pairs[index]
                .workload
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("unspecified");
            let methods = [
                ("demo_heuristic", flag),
                ("static_entropy_gt_7_9", f.entropy_post > 7.9),
                ("delta_entropy_gt_1_5", f.delta_h > 1.5),
            ];
            add(&mut confusion, positive, flag);
            for (name, prediction) in methods {
                add(
                    baselines.entry(name.to_owned()).or_default(),
                    positive,
                    prediction,
                );
                add(
                    workloads
                        .entry(workload.to_owned())
                        .or_default()
                        .entry(name.to_owned())
                        .or_default(),
                    positive,
                    prediction,
                );
            }
            if f.chi2_p.is_none() {
                short += 1;
            }
            if observations.len() < 10_000 {
                observations.push(Observation {
                    pair: index,
                    label: if positive { "suspicious" } else { "benign" }.to_string(),
                    workload: workload.to_string(),
                    flagged: flag,
                    features: f,
                });
            }
            blocks += 1;
            bytes += x as u64;
            offset += x as u64;
        }
    }
    let report=Report { schema_version:1, warning:"Experimental uncalibrated block-level heuristic; not a ransomware detector or protection system.", elapsed_seconds:start.elapsed().as_secs_f64(), bytes_compared:bytes, pairs:pairs.len(), blocks, skipped_short_blocks:short, confusion, baselines, workloads, observations, observations_truncated:blocks>10_000 };
    let json = serde_json::to_vec_pretty(&report)?;
    let mut output = File::create(&args.report)?;
    output.write_all(&json)?;
    output.write_all(b"\n")?;
    println!(
        "Compared {} pairs, {} blocks, {} bytes; wrote {}",
        report.pairs,
        report.blocks,
        report.bytes_compared,
        args.report.display()
    );
    Ok(())
}
