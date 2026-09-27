//! `vilan-rt::crypto` — the digests `std::crypto` and `std::rpc_server` bind
//! (tracker F18 slices 2 and 3).
//!
//! # What this is a twin OF
//!
//! `vilan/std/src/crypto.vl` declares `sha256`, `sha384` and `sha512` over the
//! `__sha256`/`__sha384`/`__sha512` helpers, which on the JS backend reach
//! WebCrypto's `subtle.digest`. Only SHA-256 is here, because only SHA-256 is
//! on the path slice 2 had to open: `std::http`'s `etag_of` mints a
//! validator over a response body, which is what `Server::builder()`'s
//! `cache_build` arm reaches. The other two are refused by name, which is the
//! standing preference over a wrong answer — and adding them is the SHA-512
//! block function and nothing else.
//!
//! Slice 3 adds the one other digest std reaches: `std::rpc_server`'s
//! WebSocket handshake binds node:crypto's `createHash("sha1")` →
//! `update(text)` → `digest("base64")` for RFC 6455's `Sec-WebSocket-Accept`
//! proof. [`NodeHash`] is that host object; [`sha1`] and [`base64`] are the
//! arithmetic under it.
//!
//! # Why by hand
//!
//! Order 37's R8 and Order 39's R1: no crates.io dependencies. FIPS 180-4's
//! SHA-256 and SHA-1 are each sixty lines of arithmetic with a published test
//! vector set, and RFC 4648's base64 is twenty, so they fit inside that rule
//! the way HTTP/1.1 and JSON did.
//!
//! **These are content digests, not password primitives** — the same sentence
//! `crypto.vl` writes at the binding. Nothing here is constant-time and
//! nothing here is slow, which is what a password hash needs. SHA-1 is here
//! because RFC 6455 names it, not as a digest anyone should choose.

use std::cell::RefCell;
use std::rc::Rc;

use crate::{Js, Str, str_new};

/// FIPS 180-4 §4.2.2 — the first 32 bits of the fractional parts of the cube
/// roots of the first 64 primes.
const ROUND_CONSTANTS: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// FIPS 180-4 §5.3.3 — the first 32 bits of the fractional parts of the square
/// roots of the first 8 primes.
const INITIAL_STATE: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// SHA-256 of `data` — the 32 bytes of the digest, in order.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state = INITIAL_STATE;
    // §5.1.1: append `0x80`, then zeros up to 56 mod 64, then the length in
    // BITS as a big-endian u64.
    let mut message = data.to_vec();
    let bit_length = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        compress(&mut state, block);
    }
    let mut digest = [0u8; 32];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// One 64-byte block, §6.2.2.
fn compress(state: &mut [u32; 8], block: &[u8]) {
    let mut schedule = [0u32; 64];
    for (index, word) in schedule.iter_mut().take(16).enumerate() {
        let bytes = &block[index * 4..index * 4 + 4];
        *word = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    }
    for index in 16..64 {
        let previous = schedule[index - 15];
        let ahead = schedule[index - 2];
        let sigma0 = previous.rotate_right(7) ^ previous.rotate_right(18) ^ (previous >> 3);
        let sigma1 = ahead.rotate_right(17) ^ ahead.rotate_right(19) ^ (ahead >> 10);
        schedule[index] = schedule[index - 16]
            .wrapping_add(sigma0)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(sigma1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for index in 0..64 {
        let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ (!e & g);
        let first = h
            .wrapping_add(sum1)
            .wrapping_add(choose)
            .wrapping_add(ROUND_CONSTANTS[index])
            .wrapping_add(schedule[index]);
        let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
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

/// `__sha256(data)` — the shape `std::crypto` binds, which is `async` because
/// WebCrypto's `subtle.digest` is a promise on the other backend. There is
/// nothing to await here; the `async fn` exists so the emitted call site is
/// the same `.await` on both.
pub async fn sha256_bytes(data: crate::bytes::Bytes) -> crate::bytes::Bytes {
    crate::bytes::Bytes::from_vec(sha256(&data.as_slice()).to_vec())
}

/// FIPS 180-4 §5.3.1 — SHA-1's initial hash value.
const SHA1_INITIAL_STATE: [u32; 5] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];

/// SHA-1 of `data` — the 20 bytes of the digest, in order (FIPS 180-4 §6.1).
///
/// The padding is SHA-256's (§5.1.1 is shared by both): `0x80`, zeros up to
/// 56 mod 64, the length in bits as a big-endian u64.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut state = SHA1_INITIAL_STATE;
    let mut message = data.to_vec();
    let bit_length = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        sha1_compress(&mut state, block);
    }
    let mut digest = [0u8; 20];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// One 64-byte block, §6.1.2: an 80-word schedule, four rounds of twenty.
fn sha1_compress(state: &mut [u32; 5], block: &[u8]) {
    let mut schedule = [0u32; 80];
    for (index, word) in schedule.iter_mut().take(16).enumerate() {
        let bytes = &block[index * 4..index * 4 + 4];
        *word = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    }
    for index in 16..80 {
        schedule[index] = (schedule[index - 3]
            ^ schedule[index - 8]
            ^ schedule[index - 14]
            ^ schedule[index - 16])
            .rotate_left(1);
    }
    let [mut a, mut b, mut c, mut d, mut e] = *state;
    for (index, word) in schedule.iter().enumerate() {
        // §4.1.1's three functions and §4.2.1's four constants, by round.
        let (mixed, constant) = match index {
            0..=19 => ((b & c) | (!b & d), 0x5a827999),
            20..=39 => (b ^ c ^ d, 0x6ed9eba1),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
            _ => (b ^ c ^ d, 0xca62c1d6),
        };
        let next = a
            .rotate_left(5)
            .wrapping_add(mixed)
            .wrapping_add(e)
            .wrapping_add(constant)
            .wrapping_add(*word);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = next;
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e]) {
        *slot = slot.wrapping_add(value);
    }
}

/// RFC 4648 §4's alphabet.
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// RFC 4648 §5's URL-safe alphabet — node's `base64url`.
const BASE64URL_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// `bytes` in RFC 4648 §4 base64, `=`-padded — node's `digest("base64")`.
pub fn base64(bytes: &[u8]) -> String {
    encode_base64(bytes, BASE64_ALPHABET, true)
}

/// Three bytes to four characters; a short last group is padded with `=`
/// when `padded` (node's `base64`) and left short when not (node's
/// `base64url`, which omits the padding).
fn encode_base64(bytes: &[u8], alphabet: &[u8; 64], padded: bool) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let joined = (u32::from(group[0]) << 16)
            | (u32::from(*group.get(1).unwrap_or(&0)) << 8)
            | u32::from(*group.get(2).unwrap_or(&0));
        let characters = group.len() + 1;
        for position in 0..4 {
            if position < characters {
                let index = (joined >> (18 - 6 * position)) & 0x3f;
                out.push(alphabet[index as usize] as char);
            } else if padded {
                out.push('=');
            }
        }
    }
    out
}

/// The algorithm a [`NodeHash`] was created for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Algorithm {
    Sha1,
    Sha256,
}

/// What a [`NodeHash`] holds: the algorithm, the data `update` has fed it,
/// and whether `digest` has been called — node refuses a second one.
#[derive(Debug)]
struct HashState {
    algorithm: Algorithm,
    data: Vec<u8>,
    finalized: bool,
}

/// node:crypto's `Hash` — `std::rpc_server`'s `external struct NodeHash`, the
/// object `createHash(algorithm)` answers (F18 slice 3).
///
/// A handle, because the host object is one: `update` answers `this` and the
/// std binding chains through it, so every copy is the same hash. The data is
/// buffered rather than streamed into the block function — the one caller
/// hashes a sixty-byte handshake key, and buffering keeps [`sha1`] and
/// [`crate::crypto::sha256`] the only arithmetic in the module.
#[derive(Clone, Debug)]
pub struct NodeHash(Rc<RefCell<HashState>>);

impl PartialEq for NodeHash {
    fn eq(&self, other: &NodeHash) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// `createHash(algorithm)`. The two digests this runtime has are the two it
/// answers for; any other name is node's own refusal, which natively is the
/// abort every host throw takes.
pub fn create_hash(algorithm: &str) -> NodeHash {
    let algorithm = match algorithm {
        "sha1" => Algorithm::Sha1,
        "sha256" => Algorithm::Sha256,
        other => crate::panic_with(&format!(
            "Error: Digest method not supported (`{other}`: the native runtime has `sha1` and \
             `sha256`)"
        )),
    };
    NodeHash(Rc::new(RefCell::new(HashState {
        algorithm,
        data: Vec::new(),
        finalized: false,
    })))
}

impl NodeHash {
    /// `hash.update(text)` — a string is fed as its UTF-8 bytes (node's
    /// default input encoding), and the answer is the same hash.
    pub fn update(&self, text: &str) -> NodeHash {
        {
            let mut state = self.0.borrow_mut();
            if state.finalized {
                drop(state);
                crate::panic_with("Error [ERR_CRYPTO_HASH_FINALIZED]: Digest already called");
            }
            state.data.extend_from_slice(text.as_bytes());
        }
        self.clone()
    }

    /// `hash.digest(encoding)` — `hex`, `base64` or `base64url`; node's other
    /// encodings (`latin1`, a `Buffer` with none) answer something a vilan
    /// `str` cannot hold faithfully and are refused by name.
    pub fn digest(&self, encoding: &str) -> Str {
        let mut state = self.0.borrow_mut();
        if state.finalized {
            drop(state);
            crate::panic_with("Error [ERR_CRYPTO_HASH_FINALIZED]: Digest already called");
        }
        state.finalized = true;
        let digest = match state.algorithm {
            Algorithm::Sha1 => sha1(&state.data).to_vec(),
            Algorithm::Sha256 => sha256(&state.data).to_vec(),
        };
        drop(state);
        let rendered = match encoding {
            "hex" => digest.iter().map(|byte| format!("{byte:02x}")).collect(),
            "base64" => base64(&digest),
            "base64url" => encode_base64(&digest, BASE64URL_ALPHABET, false),
            other => crate::panic_with(&format!(
                "the native runtime does not render a digest as `{other}` (it has `hex`, \
                 `base64` and `base64url`)"
            )),
        };
        str_new(&rendered)
    }
}

impl Js for NodeHash {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a node:crypto `Hash` is a host object's own inspection, which the native \
             backend does not reproduce",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(digest: &[u8]) -> String {
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// FIPS 180-4's own vectors, plus the two-block case (a message longer than
    /// 55 bytes needs a second block for its length field, which is the padding
    /// rule most hand-written SHA-256s get wrong).
    #[test]
    fn the_published_vectors_hash_to_their_published_digests() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            hex(&sha256(&[b'a'; 1_000_000])),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    /// The two boundaries the padding rule turns on: 55 bytes still fits one
    /// block (the `0x80` plus the 8-byte length exactly fill it), 56 needs a
    /// second, and 119/120 are the same step one block later. A padding bug
    /// that is invisible on `"abc"` shows here.
    #[test]
    fn the_padding_boundaries_hash_to_their_published_digests() {
        for (length, expected) in [
            (
                55,
                "d5e285683cd4efc02d021a5c62014694958901005d6f71e89e0989fac77e4072",
            ),
            (
                56,
                "04c26261370ee7541549d16dee320c723e3fd14671e66a099afe0a377c16888e",
            ),
            (
                64,
                "7ce100971f64e7001e8fe5a51973ecdfe1ced42befe7ee8d5fd6219506b5393c",
            ),
            (
                119,
                "000b48d4edf0fa7bee3c6236ecd2785baa5db4eeb8bb54341b029e0d9fa5fb0c",
            ),
            (
                120,
                "13f05a0b594787f5ecd315edc96141bd3243203d1b7d4f0836f37308b276ba98",
            ),
        ] {
            assert_eq!(
                hex(&sha256(&vec![b'x'; length])),
                expected,
                "{length} bytes"
            );
        }
    }

    /// FIPS 180-2's SHA-1 vectors (appendix A), plus the empty message and the
    /// padding boundaries SHA-256's pin walks — the padding is shared, and so
    /// is the bug a hand-written one hides at 55/56 bytes.
    #[test]
    fn sha1_hashes_the_published_vectors_to_their_published_digests() {
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&sha1(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        assert_eq!(
            hex(&sha1(&[b'a'; 1_000_000])),
            "34aa973cd4c4daa4f61eeb2bdbad27316534016f"
        );
        for (length, expected) in [
            (55, "c1c8bbdc22796e28c0e15163d20899b65621d65a"),
            (56, "c2db330f6083854c99d4b5bfb6e8f29f201be699"),
            (64, "0098ba824b5c16427bd7a1122a5a442a25ec644d"),
        ] {
            assert_eq!(hex(&sha1(&vec![b'a'; length])), expected, "{length} bytes");
        }
    }

    /// RFC 4648 §10's test vectors — every padding case (none, one `=`, two).
    #[test]
    fn base64_encodes_rfc_4648s_vectors() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), expected, "{input:?}");
        }
        assert_eq!(
            encode_base64(&[0xfb, 0xff], BASE64URL_ALPHABET, false),
            "-_8"
        );
    }

    /// **The seam's pin: RFC 6455 §1.3's own handshake.** A client key of
    /// `dGhlIHNhbXBsZSBub25jZQ==` is answered with `s3pPLMBiTxaQ9kYGzzhZRbK+xOo=`,
    /// computed exactly as `std::rpc_server`'s `ws_accept_key` spells it:
    /// `createHash("sha1").update(key + GUID).digest("base64")`.
    #[test]
    fn the_rfc_6455_accept_key_is_the_rfcs_own_answer() {
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let accept = create_hash("sha1")
            .update(&format!("{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11"))
            .digest("base64");
        assert_eq!(&*accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    /// `update` answers the SAME hash (node's `this`), so feeding it through
    /// the answer and through the original is one message; and `sha256` is
    /// the other algorithm the handle answers for.
    #[test]
    fn update_answers_the_same_hash_and_both_algorithms_digest() {
        let hash = create_hash("sha256");
        let answered = hash.update("a");
        answered.update("bc");
        assert_eq!(
            &*hash.digest("hex"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            &*create_hash("sha1").update("abc").digest("hex"),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    #[should_panic(expected = "Digest already called")]
    fn a_second_digest_is_nodes_refusal() {
        let hash = create_hash("sha1");
        let _ = hash.digest("hex");
        let _ = hash.digest("hex");
    }
}
