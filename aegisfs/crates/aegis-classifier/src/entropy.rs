use crate::histogram::Histogram;

pub fn shannon(hist: &Histogram, n: usize) -> f64 {
    if n == 0 {
        return 0.0;
    }
    hist.0
        .iter()
        .filter(|&&v| v != 0)
        .map(|&v| {
            let p = v as f64 / n as f64;
            -p * p.log2()
        })
        .sum()
}

/// Add-alpha smoothing produces proper distributions with no zero bins.
pub fn kl_post_pre(pre: &Histogram, post: &Histogram, n: usize, alpha: f64) -> f64 {
    let denominator = n as f64 + 256.0 * alpha;
    post.0
        .iter()
        .zip(pre.0.iter())
        .map(|(&p, &q)| {
            let p = (p as f64 + alpha) / denominator;
            let q = (q as f64 + alpha) / denominator;
            p * (p / q).log2()
        })
        .sum::<f64>()
        .max(0.0)
}
