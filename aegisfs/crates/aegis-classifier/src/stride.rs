use std::collections::VecDeque;

/// Tracks positive gaps between successive writes, in caller-supplied order.
/// It does not establish maliciousness or identify file descriptors on its own.
#[derive(Debug)]
pub struct StrideTracker {
    previous_end: Option<u64>,
    gaps: VecDeque<u64>,
    capacity: usize,
}
impl StrideTracker {
    pub fn new(capacity: usize) -> Self {
        Self {
            previous_end: None,
            gaps: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }
    pub fn observe(&mut self, offset: u64, length: u64) {
        if let Some(end) = self.previous_end {
            if offset > end {
                if self.gaps.len() == self.capacity {
                    self.gaps.pop_front();
                }
                self.gaps.push_back(offset - end);
            } else if offset < end {
                // Reordering or overlap breaks the current sequential pattern.
                self.gaps.clear();
            }
        }
        self.previous_end = offset.checked_add(length);
    }
    pub fn coefficient_of_variation(&self) -> Option<f64> {
        if self.gaps.len() < 3 {
            return None;
        }
        let n = self.gaps.len() as f64;
        let mean = self.gaps.iter().map(|&x| x as f64).sum::<f64>() / n;
        let variance = self
            .gaps
            .iter()
            .map(|&x| (x as f64 - mean).powi(2))
            .sum::<f64>()
            / n;
        Some(variance.sqrt() / mean)
    }
}
