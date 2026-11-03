//! Small RFC 8439 ChaCha20 transform used only with fixed public lab keys.
//! This is workload generation, not a cryptographic API or secret storage.

pub fn transform(input: &[u8], stream: u32) -> Vec<u8> {
    let key = [
        0x0001_0203,
        0x0405_0607,
        0x0809_0a0b,
        0x0c0d_0e0f,
        0x1011_1213,
        0x1415_1617,
        0x1819_1a1b,
        0x1c1d_1e1f,
    ];
    let nonce = [0x0000_0000, 0x0000_004a, 0x0000_0000];
    let mut output = Vec::with_capacity(input.len());
    for (index, chunk) in input.chunks(64).enumerate() {
        let counter = 1u32
            .wrapping_add(stream.wrapping_mul(65_536))
            .wrapping_add(index as u32);
        let block = block(key, counter, nonce);
        output.extend(chunk.iter().zip(block).map(|(&byte, key)| byte ^ key));
    }
    output
}

fn block(key: [u32; 8], counter: u32, nonce: [u32; 3]) -> [u8; 64] {
    let initial = [
        0x6170_7865,
        0x3320_646e,
        0x7962_2d32,
        0x6b20_6574,
        key[0],
        key[1],
        key[2],
        key[3],
        key[4],
        key[5],
        key[6],
        key[7],
        counter,
        nonce[0],
        nonce[1],
        nonce[2],
    ];
    let mut state = initial;
    for _ in 0..10 {
        quarter(&mut state, 0, 4, 8, 12);
        quarter(&mut state, 1, 5, 9, 13);
        quarter(&mut state, 2, 6, 10, 14);
        quarter(&mut state, 3, 7, 11, 15);
        quarter(&mut state, 0, 5, 10, 15);
        quarter(&mut state, 1, 6, 11, 12);
        quarter(&mut state, 2, 7, 8, 13);
        quarter(&mut state, 3, 4, 9, 14);
    }
    let mut output = [0u8; 64];
    for (index, (word, original)) in state.into_iter().zip(initial).enumerate() {
        output[index * 4..index * 4 + 4]
            .copy_from_slice(&word.wrapping_add(original).to_le_bytes());
    }
    output
}

fn quarter(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(12);
    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(8);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(7);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_8439_block_vector() {
        // RFC 8439 section 2.3.2 uses this key, counter and nonce.
        let key = [
            0x0302_0100,
            0x0706_0504,
            0x0b0a_0908,
            0x0f0e_0d0c,
            0x1312_1110,
            0x1716_1514,
            0x1b1a_1918,
            0x1f1e_1d1c,
        ];
        let actual = block(key, 1, [0x0000_0090, 0x4a00_0000, 0x0000_0000]);
        assert_eq!(
            actual[..16],
            [
                0x10, 0xf1, 0xe7, 0xe4, 0xd1, 0x3b, 0x59, 0x15, 0x50, 0x0f, 0xdd, 0x1f,
                0xa3, 0x20, 0x71, 0xc4
            ]
        );
    }
}
