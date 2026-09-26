//! SHA-512 and SHA-384 (FIPS 180-4 §6.4 and §6.5).
//!
//! One block function, two initial states: SHA-384 is SHA-512 started from a
//! different IV and truncated to six words. The digest is exposed at the
//! BLOCK level as well as whole ([`Sha512Family::digest_after`]), because
//! HMAC's two keyed prefixes are exactly one block each and PBKDF2 re-hashes
//! under them hundreds of thousands of times — precomputing the two keyed
//! states once is what keeps an iteration at two compressions instead of four.

/// FIPS 180-4 §4.2.3 — the first 64 bits of the fractional parts of the cube
/// roots of the first 80 primes.
const ROUND_CONSTANTS: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

/// The block size both digests share, in bytes.
pub const BLOCK_LENGTH: usize = 128;

/// Which of the two digests — the IV and the output length are all that
/// differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sha512Family {
    Sha384,
    Sha512,
}

impl Sha512Family {
    /// §5.3.4 and §5.3.5 — the two initial hash values.
    pub fn initial_state(self) -> [u64; 8] {
        match self {
            Sha512Family::Sha512 => [
                0x6a09e667f3bcc908,
                0xbb67ae8584caa73b,
                0x3c6ef372fe94f82b,
                0xa54ff53a5f1d36f1,
                0x510e527fade682d1,
                0x9b05688c2b3e6c1f,
                0x1f83d9abfb41bd6b,
                0x5be0cd19137e2179,
            ],
            Sha512Family::Sha384 => [
                0xcbbb9d5dc1059ed8,
                0x629a292a367cd507,
                0x9159015a3070dd17,
                0x152fecd8f70e5939,
                0x67332667ffc00b31,
                0x8eb44a8768581511,
                0xdb0c2e0d64f98fa7,
                0x47b5481dbefa4fa4,
            ],
        }
    }

    /// The digest's length in bytes: 64 or 48.
    pub fn output_length(self) -> usize {
        match self {
            Sha512Family::Sha512 => 64,
            Sha512Family::Sha384 => 48,
        }
    }

    /// The digest of `data` from the start.
    pub fn digest(self, data: &[u8]) -> Vec<u8> {
        let state = self.digest_after(self.initial_state(), 0, data);
        self.render(&state)
    }

    /// The final state of a message whose first `consumed` bytes (a whole
    /// number of blocks) already produced `state`, and whose remaining bytes
    /// are `data` — §5.1.2's padding written over the TOTAL length.
    pub fn digest_after(self, mut state: [u64; 8], consumed: usize, data: &[u8]) -> [u64; 8] {
        debug_assert_eq!(consumed % BLOCK_LENGTH, 0, "a whole number of blocks");
        let (blocks, tail) = data.as_chunks::<BLOCK_LENGTH>();
        for block in blocks {
            compress(&mut state, block);
        }
        // §5.1.2: `0x80`, then zeros up to 112 mod 128, then the length in
        // BITS as a big-endian 128-bit integer.
        let bit_length = ((consumed + data.len()) as u128).wrapping_mul(8);
        let mut last = [0u8; BLOCK_LENGTH * 2];
        last[..tail.len()].copy_from_slice(tail);
        last[tail.len()] = 0x80;
        let padded = if tail.len() < BLOCK_LENGTH - 16 {
            BLOCK_LENGTH
        } else {
            BLOCK_LENGTH * 2
        };
        last[padded - 16..padded].copy_from_slice(&bit_length.to_be_bytes());
        for block in last[..padded].as_chunks::<BLOCK_LENGTH>().0 {
            compress(&mut state, block);
        }
        state
    }

    /// The state after one whole block from the start — HMAC's keyed prefix.
    pub fn state_after_block(self, block: &[u8; BLOCK_LENGTH]) -> [u64; 8] {
        let mut state = self.initial_state();
        compress(&mut state, block);
        state
    }

    /// A final state as the digest's bytes, truncated to the family's length.
    pub fn render(self, state: &[u64; 8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        for word in state {
            out.extend_from_slice(&word.to_be_bytes());
        }
        out.truncate(self.output_length());
        out
    }
}

/// SHA-512 of `data` — the 64 bytes of the digest, in order.
pub fn sha512(data: &[u8]) -> Vec<u8> {
    Sha512Family::Sha512.digest(data)
}

/// SHA-384 of `data` — the 48 bytes of the digest, in order.
pub fn sha384(data: &[u8]) -> Vec<u8> {
    Sha512Family::Sha384.digest(data)
}

/// One 128-byte block, §6.4.2: an 80-word schedule and 80 rounds.
fn compress(state: &mut [u64; 8], block: &[u8; BLOCK_LENGTH]) {
    let mut schedule = [0u64; 80];
    for (word, bytes) in schedule.iter_mut().zip(block.as_chunks::<8>().0) {
        *word = u64::from_be_bytes(*bytes);
    }
    for index in 16..80 {
        let previous = schedule[index - 15];
        let ahead = schedule[index - 2];
        let sigma0 = previous.rotate_right(1) ^ previous.rotate_right(8) ^ (previous >> 7);
        let sigma1 = ahead.rotate_right(19) ^ ahead.rotate_right(61) ^ (ahead >> 6);
        schedule[index] = schedule[index - 16]
            .wrapping_add(sigma0)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(sigma1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for (constant, word) in ROUND_CONSTANTS.iter().zip(schedule) {
        let sum1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
        let choose = (e & f) ^ (!e & g);
        let first = h
            .wrapping_add(sum1)
            .wrapping_add(choose)
            .wrapping_add(*constant)
            .wrapping_add(word);
        let sum0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let second = sum0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(first);
        d = c;
        c = b;
        b = a;
        a = first.wrapping_add(second);
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex;

    /// FIPS 180-4's example vectors (the empty message, `abc`, the 896-bit
    /// two-block message) and the million-`a` one, for both digests. Every
    /// expected digest here was produced by node's own `createHash`.
    #[test]
    fn the_published_vectors_hash_to_their_published_digests() {
        let two_block = b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu";
        let million = vec![b'a'; 1_000_000];
        for (data, sha512_expected, sha384_expected) in [
            (
                &b""[..],
                "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
                "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b",
            ),
            (
                &b"abc"[..],
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
                "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7",
            ),
            (
                &two_block[..],
                "8e959b75dae313da8cf4f72814fc143f8f7779c6eb9f7fa17299aeadb6889018501d289e4900f7e4331b99dec4b5433ac7d329eeb6dd26545e96e55b874be909",
                "09330c33f71147e83d192fc782cd1b4753111b173b3b05d22fa08086e3b0f712fcc7c71a557e2db966c3e9fa91746039",
            ),
            (
                &million[..],
                "e718483d0ce769644e2e42c7bc15b4638e1f98b13b2044285632a803afa973ebde0ff244877ea60a4cb0432ce577c31beb009c5c2c49aa2e4eadb217ad8cc09b",
                "9d0e1809716474cb086e834e310a4a1ced149e9c00f248527972cec5704c2a5b07b8b3dc38ecc4ebae97ddd87f3d8985",
            ),
        ] {
            assert_eq!(hex(&sha512(data)), sha512_expected, "{} bytes", data.len());
            assert_eq!(hex(&sha384(data)), sha384_expected, "{} bytes", data.len());
        }
    }

    /// The boundaries §5.1.2's padding turns on: 111 bytes still fits one
    /// block (the `0x80` plus the 16-byte length exactly fill it), 112 needs a
    /// second, 128 is a whole block of message, and 239/240 are the same step
    /// one block later. A padding bug invisible on `abc` shows here.
    #[test]
    fn the_padding_boundaries_hash_to_their_published_digests() {
        for (length, sha512_expected, sha384_expected) in [
            (
                111,
                "9a2a120825c2319867758ec277924f6faa254968bf752046dacdd948d8ad299b10359fd04bfd7d3810b5fa1b16a294236138baff981cbb85248478053ac4d3dd",
                "dfec6588f894c2b089119e1884a23941e5aa69ed38702839cf7352ac6d155d315693bb5d26b7468d8b69ecf4631ef419",
            ),
            (
                112,
                "a3722b515ef40c910f2419f6e0da8ca51d410114ce6272faae64045f9e9f630e7fa8dd5a3243c9860b899d148c3da4bc0f9e07454542604d030bb55531fe0d5b",
                "d8489693bc428374931aedf508740398d9d4a92887116fecb2fcdd91c68b9db329fc4a474e05e78c3eb34e649a6b5c77",
            ),
            (
                127,
                "1d5a8893e7b7ed83d485d26f88cfb846f3760279916976fe538e539fc16f7cd19ba3e1c2cd5fda78749a74205755cdf694e8fa90b2bfed8815f406af76c1d7bf",
                "7f90015607d4c124f9b89bea8aa5b246ebfc792cda7ddf8bd6dcf69f9ff8bbdccbcbb32cebaa0b03fc93f10b61bde2a5",
            ),
            (
                128,
                "e2e22f8422b54b06e35c3ea30a383d1de7a8fbc27992923074103117020d8dd7024c3ecf7d6d1a15a6de5a75ff32fb486b9e8ced4c02ffe05822bf2cb734d0e0",
                "e660584956c8b1df44c92acb7c8eccfe0dca5255627c9fb44637c15363b772e5709edcf35b07bf43531951ab2fd51130",
            ),
            (
                239,
                "805c40517b44cef414a06c632b4ee9c21d34c85a84c3f9530274db86e2a3e38cb0e54025666b925426bd6860f81538abbfab13011e85767d814a245d232a6044",
                "831b8db491e9f1ce281f814dbfaa770446f3d2286b51395f14ea02cb34222dd94f6d5003d359ca575c55064a7bf8b1d7",
            ),
            (
                240,
                "3e05caa7e0fddab685e6052bd902c316505180504344d00b1217e1349a532d1d0e5ce4b24d69eea3aca369dc16d8bbdf872b2fe1770e0e1aad22f84e0519bc6c",
                "2c12b66c79b060be97e4949a0529db684f46d7949866f6d1096a22e621c60a9c1fbc1959d69dcfb08e88d869037eb048",
            ),
        ] {
            let data = vec![b'x'; length];
            assert_eq!(hex(&sha512(&data)), sha512_expected, "{length} bytes");
            assert_eq!(hex(&sha384(&data)), sha384_expected, "{length} bytes");
        }
    }

    /// Hashing a prefix block by block and the rest through `digest_after`
    /// is the whole-message digest — the seam HMAC's precomputed states use.
    #[test]
    fn a_digest_resumed_after_a_block_is_the_whole_messages_digest() {
        let message: Vec<u8> = (0..300u32).map(|index| (index * 7) as u8).collect();
        let family = Sha512Family::Sha512;
        let first: &[u8; BLOCK_LENGTH] = message[..BLOCK_LENGTH].try_into().unwrap();
        let state = family.digest_after(
            family.state_after_block(first),
            BLOCK_LENGTH,
            &message[BLOCK_LENGTH..],
        );
        assert_eq!(family.render(&state), sha512(&message));
    }
}
