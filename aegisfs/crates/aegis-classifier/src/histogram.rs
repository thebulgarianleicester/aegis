/// Byte-frequency histogram. Each input byte is counted once.
#[repr(align(64))]
pub struct Histogram(pub [u32; 256]);

impl Histogram {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut bins = [0u32; 256];
        for &b in bytes {
            bins[b as usize] += 1;
        }
        Self(bins)
    }
}
