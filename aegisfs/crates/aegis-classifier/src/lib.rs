//! Offline, experimental byte-distribution features. No detection guarantee.
pub mod chi2;
pub mod entropy;
pub mod headers;
pub mod histogram;
pub mod stride;
use histogram::Histogram;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BlockFeatures {
    pub offset: u64,
    pub bytes: usize,
    pub entropy_pre: f64,
    pub entropy_post: f64,
    pub delta_h: f64,
    pub chi2: Option<f64>,
    /// Upper-tail p-value, not the probability of encryption.
    pub chi2_p: Option<f64>,
    pub kl_post_pre_bits: f64,
    pub serial_correlation_post: Option<f64>,
    pub header_before: Option<&'static str>,
    pub header_after: Option<&'static str>,
    pub header_destroyed: bool,
}

/// Requires equally sized nonempty overlapping extents; callers handle creations,
/// truncations and short writes separately. Pre and post are not modified.
pub fn classify(pre: &[u8], post: &[u8], offset: u64) -> Result<BlockFeatures, &'static str> {
    if pre.is_empty() || pre.len() != post.len() || pre.len() > 4096 {
        return Err("pre/post must be equally sized, nonempty, and at most 4096 bytes");
    }
    let ph = Histogram::from_bytes(pre);
    let qh = Histogram::from_bytes(post);
    let a = entropy::shannon(&ph, pre.len());
    let b = entropy::shannon(&qh, post.len());
    let (chi2, chi2_p) =
        chi2::uniformity(&qh, post.len()).map_or((None, None), |(x, p)| (Some(x), Some(p)));
    let header_before = if offset == 0 {
        headers::signature(pre)
    } else {
        None
    };
    let header_after = if offset == 0 {
        headers::signature(post)
    } else {
        None
    };
    // No destruction claim if the available post prefix cannot cover the old signature.
    let required = match header_before {
        Some("PDF") => 5,
        Some("ZIP/OOXML") => 4,
        Some("PNG") => 8,
        Some("JPEG") => 3,
        Some("SQLite") => 16,
        Some("ELF") => 4,
        Some("gzip") => 3,
        Some("zstd") => 4,
        Some("GIF") => 6,
        _ => 0,
    };
    Ok(BlockFeatures {
        offset,
        bytes: pre.len(),
        entropy_pre: a,
        entropy_post: b,
        delta_h: b - a,
        chi2,
        chi2_p,
        kl_post_pre_bits: entropy::kl_post_pre(&ph, &qh, pre.len(), 0.5),
        serial_correlation_post: serial_correlation(post),
        header_before,
        header_after,
        header_destroyed: required > 0 && post.len() >= required && header_before != header_after,
    })
}

/// Pearson lag-one correlation, unavailable for constant/short samples.
pub fn serial_correlation(bytes: &[u8]) -> Option<f64> {
    if bytes.len() < 3 {
        return None;
    }
    let (a, b) = (&bytes[..bytes.len() - 1], &bytes[1..]);
    let n = a.len() as f64;
    let ma = a.iter().map(|&x| x as f64).sum::<f64>() / n;
    let mb = b.iter().map(|&x| x as f64).sum::<f64>() / n;
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for (&x, &y) in a.iter().zip(b) {
        let x = x as f64 - ma;
        let y = y as f64 - mb;
        cov += x * y;
        va += x * x;
        vb += y * y;
    }
    (va > 0.0 && vb > 0.0).then(|| cov / (va * vb).sqrt())
}

/// An intentionally uncalibrated demonstration baseline, not a security decision.
pub fn demo_flag(f: &BlockFeatures) -> bool {
    f.header_destroyed
        || (f.delta_h > 1.0
            && f.entropy_post > 7.7
            && f.chi2_p.is_some_and(|p| (0.01..=0.99).contains(&p)))
}
