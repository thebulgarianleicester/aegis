use crate::histogram::Histogram;
use statrs::function::gamma::gamma_ur;

/// Returns the statistic and its *upper-tail* p-value for 255 degrees of freedom.
/// Only report the asymptotic test when expected counts are at least five.
pub fn uniformity(hist: &Histogram, n: usize) -> Option<(f64, f64)> {
    if n < 1280 {
        return None;
    }
    let expected = n as f64 / 256.0;
    let statistic = hist
        .0
        .iter()
        .map(|&c| {
            let d = c as f64 - expected;
            d * d / expected
        })
        .sum::<f64>();
    // statrs rejects x=0 even though the limiting survival probability is 1.
    let p = if statistic == 0.0 {
        1.0
    } else {
        gamma_ur(255.0 / 2.0, statistic / 2.0)
    };
    Some((statistic, p))
}
