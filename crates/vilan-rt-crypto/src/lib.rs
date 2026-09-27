//! `vilan-rt-crypto` — `std::crypto`'s randomness and key derivation on the
//! native backend (tracker F40, RULED (a) 2026-09-25).
//!
//! # What this is a twin OF
//!
//! Two host surfaces, and every public item names the binding it exists for:
//!
//! - `vilan/std/src/crypto.vl`'s helpers — `__random_bytes`
//!   (`crypto.getRandomValues`), `crypto.randomUUID`, and the WebCrypto four:
//!   `__sha384`, `__sha512`, `__hmac_sha512`, `__pbkdf2_sha512`. SHA-256 is
//!   `vilan-rt`'s already and stays there.
//! - node:crypto's `pbkdf2Sync`, which a program binds ITSELF when the path
//!   that hashes a password cannot suspend (kolt's `store.vl` does: its `[rpc]`
//!   bodies are synchronous and WebCrypto is not), and the `Buffer` it answers,
//!   read back through `toString(encoding)` — [`pbkdf2_sync`] and
//!   [`NodeBuffer`].
//!
//! # Why a crate of its own
//!
//! `vilan-rt` takes no crates.io dependencies and forbids `unsafe`; OS
//! randomness needs one or the other (the manifest says why at length). So
//! `getrandom` lives here, and the cargo project `vilan build --backend rust`
//! writes names this crate ONLY when the program reaches one of the items
//! below. This crate's own code is `forbid(unsafe_code)` like the runtime's.
//!
//! # Why the digests are hand-written
//!
//! SHA-512 is SHA-256's block function at twice the word width (FIPS 180-4
//! §6.4), HMAC is two digests (RFC 2104), and PBKDF2 is a loop of HMACs (RFC
//! 8018 §5.2) — together under three hundred lines, each pinned to published
//! vectors and to node's own answers, which is how `vilan-rt` already carries
//! SHA-1 and SHA-256. The RustCrypto crates (`sha2` + `hmac` + `pbkdf2`) would
//! add six more to every build that reaches this crate (`digest`,
//! `crypto-common`, `block-buffer`, `generic-array`, `typenum`, `cpufeatures`),
//! and the ruling's point was ONE dependency, at the syscall, where there is no
//! alternative. The arithmetic here is data-independent in its control flow
//! (rotations, shifts, additions, no table indexed by a secret), which is the
//! property a password hash needs from its PRF; comparing a derived hash is
//! the caller's (`std::crypto::equals_constant_time`).
//!
//! # Failure
//!
//! Every host twin here THROWS on a bad parameter, and a throw nothing catches
//! ends the program with its message — so a refusal is [`vilan_rt::panic_with`]
//! carrying the host's own sentence, as `vilan-rt-sqlite`'s are.

#![forbid(unsafe_code)]

pub mod mac;
pub mod sha512;

use vilan_rt::bytes::Bytes;
use vilan_rt::{Js, Json, Str, panic_with, str_new};

use crate::mac::Algorithm;

/// Lowercase hex, two digits a byte.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `length` bytes from the operating system's CSPRNG.
///
/// A failure of the OS source is not something a program can act on (and on
/// every platform `getrandom` supports it does not happen once the process is
/// running), so it ends the program like any host throw.
fn os_random(length: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; length];
    if let Err(error) = getrandom::fill(&mut bytes) {
        panic_with(&format!(
            "Error: the operating system's random source failed: {error}"
        ));
    }
    bytes
}

/// `__random_bytes(length)` — `crypto.getRandomValues(new Uint8Array(length))`,
/// `std::crypto::random_bytes`.
///
/// The host's two refusals, in its own words: a negative length is
/// `new Uint8Array`'s `RangeError`, and more than 65,536 bytes in one call is
/// `getRandomValues`' quota. The length is widened to `i64` at the call so the
/// binding's declared width is the caller's business, not this signature's.
pub fn random_bytes(length: i64) -> Bytes {
    if length < 0 {
        panic_with(&format!("RangeError: Invalid typed array length: {length}"));
    }
    if length > 65_536 {
        panic_with("QuotaExceededError: The requested length exceeds 65,536 bytes");
    }
    Bytes::from_vec(os_random(length as usize))
}

/// `crypto.randomUUID()` — an RFC 9562 version-4 UUID, lowercase, from 122
/// random bits: the version nibble is `4` and the variant's two high bits are
/// `10`, as the host's are.
pub fn random_uuid() -> Str {
    let mut bytes = os_random(16);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let digits = hex(&bytes);
    str_new(&format!(
        "{}-{}-{}-{}-{}",
        &digits[0..8],
        &digits[8..12],
        &digits[12..16],
        &digits[16..20],
        &digits[20..32]
    ))
}

/// `__sha384(data)` — `crypto.subtle.digest("SHA-384", data)`. `async` because
/// WebCrypto's digest is a promise on the other backend; there is nothing to
/// await here, and the `async fn` keeps the emitted call site the same
/// `.await` on both (`vilan_rt::crypto::sha256_bytes`'s reason).
pub async fn sha384_bytes(data: Bytes) -> Bytes {
    Bytes::from_vec(sha512::sha384(&data.as_slice()))
}

/// `__sha512(data)` — `crypto.subtle.digest("SHA-512", data)`.
pub async fn sha512_bytes(data: Bytes) -> Bytes {
    Bytes::from_vec(sha512::sha512(&data.as_slice()))
}

/// `__hmac_sha512(key, data)` — WebCrypto's `sign("HMAC", ..)` under a raw
/// SHA-512 key.
pub async fn hmac_sha512_bytes(key: Bytes, data: Bytes) -> Bytes {
    Bytes::from_vec(mac::hmac(
        Algorithm::Sha512,
        &key.as_slice(),
        &data.as_slice(),
    ))
}

/// `__pbkdf2_sha512(password, salt, iterations, bits)` — WebCrypto's
/// `deriveBits({ name: "PBKDF2", hash: "SHA-512", .. }, key, bits)`, the
/// `std::crypto::pbkdf2_sha512` primitive. `bits`, not bytes, as there.
///
/// WebCrypto's refusals, in its words: zero iterations, a length that is not
/// a whole number of bytes, and (its WebIDL conversions) a negative count of
/// either. Zero bits is an empty key, which WebCrypto answers without error.
pub async fn pbkdf2_sha512_bytes(
    password: Bytes,
    salt: Bytes,
    iterations: i64,
    bits: i64,
) -> Bytes {
    if iterations < 0 {
        panic_with(
            "TypeError: Failed to normalize algorithm: 'iterations' of 'Pbkdf2Params' (passed \
             algorithm) is outside the expected range of 0 to 4294967295.",
        );
    }
    if iterations == 0 {
        panic_with("OperationError: iterations cannot be zero");
    }
    if bits < 0 {
        panic_with("OperationError: The operation failed for an operation-specific reason");
    }
    if bits % 8 != 0 {
        panic_with("OperationError: length must be a multiple of 8");
    }
    Bytes::from_vec(mac::pbkdf2(
        Algorithm::Sha512,
        &password.as_slice(),
        &salt.as_slice(),
        iterations as u32,
        (bits / 8) as usize,
    ))
}

/// What node's key-material parameters take: a string (fed as its UTF-8
/// bytes, node's default encoding) or bytes. A program declares
/// `pbkdf2Sync`'s `password`/`salt` at whichever it holds.
pub trait KeyMaterial {
    fn key_bytes(&self) -> Vec<u8>;
}

impl KeyMaterial for Str {
    fn key_bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

impl KeyMaterial for Bytes {
    fn key_bytes(&self) -> Vec<u8> {
        self.to_vec()
    }
}

impl KeyMaterial for NodeBuffer {
    fn key_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

/// `pbkdf2Sync(password, salt, iterations, keylen, digest)` from node:crypto —
/// the SYNCHRONOUS twin, answering a [`NodeBuffer`].
///
/// node's refusals, in its words: an iteration count outside `1..=2^31-1`, a
/// negative key length, a digest it does not know (this runtime knows the four
/// in [`Algorithm::named`]; node knows more, and any other name here is
/// refused with node's sentence plus which four this runtime has), and a key
/// length of zero, which node 24 answers with `Deriving bits failed`.
pub fn pbkdf2_sync(
    password: &impl KeyMaterial,
    salt: &impl KeyMaterial,
    iterations: i64,
    key_length: i64,
    digest: &str,
) -> NodeBuffer {
    if !(1..=i64::from(i32::MAX)).contains(&iterations) {
        panic_with(&format!(
            "RangeError [ERR_OUT_OF_RANGE]: The value of \"iterations\" is out of range. It must \
             be >= 1 && <= 2147483647. Received {iterations}"
        ));
    }
    if !(0..=i64::from(i32::MAX)).contains(&key_length) {
        panic_with(&format!(
            "RangeError [ERR_OUT_OF_RANGE]: The value of \"keylen\" is out of range. It must be \
             >= 0 && <= 2147483647. Received {key_length}"
        ));
    }
    let Some(algorithm) = Algorithm::named(digest) else {
        panic_with(&format!(
            "TypeError: Invalid digest: {digest} (the native runtime has sha1, sha256, sha384 \
             and sha512)"
        ));
    };
    if key_length == 0 {
        panic_with("Error: Deriving bits failed");
    }
    NodeBuffer(Bytes::from_vec(mac::pbkdf2(
        algorithm,
        &password.key_bytes(),
        &salt.key_bytes(),
        iterations as u32,
        key_length as usize,
    )))
}

/// node's `Buffer` — what `pbkdf2Sync` answers, and a program declares as an
/// `external struct` of its own naming to read it back through
/// `toString(encoding)`.
///
/// A `Buffer` IS a `Uint8Array` on the other backend, so the bytes are one
/// shared [`Bytes`] and the identity is its identity. It is a type of its own
/// rather than a bare `Bytes` because the two host classes answer `toString`
/// differently: a `Buffer` takes an encoding, a plain `Uint8Array` prints its
/// elements joined by commas and ignores any argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeBuffer(pub Bytes);

impl NodeBuffer {
    /// `buffer.toString(encoding)` — `hex`, `base64`, `base64url` and `utf8`
    /// (lossily, as node decodes: a malformed sequence is one U+FFFD per
    /// maximal subpart, which is also what `String::from_utf8_lossy` writes).
    /// node's other encodings (`latin1`, `ucs2`, `ascii`) are refused by name.
    pub fn to_string_encoded(&self, encoding: &str) -> Str {
        let bytes = self.0.to_vec();
        let rendered = match encoding.to_ascii_lowercase().as_str() {
            "hex" => hex(&bytes),
            "base64" => vilan_rt::crypto::base64(&bytes),
            // RFC 4648 §5: the URL-safe alphabet, unpadded (node's `base64url`).
            "base64url" => vilan_rt::crypto::base64(&bytes)
                .trim_end_matches('=')
                .replace('+', "-")
                .replace('/', "_"),
            "utf8" | "utf-8" => String::from_utf8_lossy(&bytes).into_owned(),
            other => panic_with(&format!(
                "the native runtime does not render a Buffer as `{other}` (it has `hex`, \
                 `base64`, `base64url` and `utf8`)"
            )),
        };
        str_new(&rendered)
    }
}

impl Js for NodeBuffer {
    /// Node prints a `Buffer` as `<Buffer 12 34 …>`, its own object inspection;
    /// printing one is refused, as printing a `Bytes` is.
    fn js(&self) -> String {
        panic_with(
            "printing a node `Buffer` is a host object's own inspection, which the native \
             backend does not reproduce",
        )
    }
}

/// `JSON.stringify(buffer)` is `{"type":"Buffer","data":[…]}` — `Buffer`
/// defines `toJSON`, which is where it parts from a plain `Uint8Array`.
impl Json for NodeBuffer {
    fn json(&self) -> String {
        let data: Vec<String> = self.0.to_vec().iter().map(u8::to_string).collect();
        format!("{{\"type\":\"Buffer\",\"data\":[{}]}}", data.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// kolt's own call, at kolt's own parameters: 100,000 iterations of
    /// HMAC-SHA-512, a 64-byte key, rendered as hex — the value node answers.
    #[test]
    fn pbkdf2_sync_at_kolts_parameters_is_nodes_answer() {
        let derived = pbkdf2_sync(
            &str_new("lovelace1"),
            &str_new("0123456789abcdef"),
            100_000,
            64,
            "sha512",
        );
        assert_eq!(
            &*derived.to_string_encoded("hex"),
            "9c549ce63c45f8df93229c0fac3d6457dc31b241409e21ef1b4e45c97c11001333ddb86821b04cb42fdfa9e3cb9996f4cee97ff6a7e62be799a5b23a83fc5a7f"
        );
    }

    /// `toString`'s four encodings over bytes that exercise each: `+`/`/`
    /// in base64 (and their URL-safe spellings), a NUL, and malformed UTF-8
    /// (a lone continuation lead, `0xff`, and a truncated three-byte
    /// sequence) — node's answers, byte for byte.
    #[test]
    fn a_buffer_renders_as_node_renders_it() {
        let buffer = NodeBuffer(Bytes::from_vec(vec![0xfb, 0xff, 0x00, 0x41, 0xe2, 0x82]));
        assert_eq!(&*buffer.to_string_encoded("hex"), "fbff0041e282");
        assert_eq!(&*buffer.to_string_encoded("base64"), "+/8AQeKC");
        assert_eq!(&*buffer.to_string_encoded("base64url"), "-_8AQeKC");
        assert_eq!(
            &*buffer.to_string_encoded("utf8"),
            "\u{fffd}\u{fffd}\u{0}A\u{fffd}"
        );
        assert_eq!(
            buffer.json(),
            "{\"type\":\"Buffer\",\"data\":[251,255,0,65,226,130]}"
        );
    }

    #[test]
    #[should_panic(expected = "The value of \"iterations\" is out of range")]
    fn zero_iterations_is_nodes_range_error() {
        pbkdf2_sync(&str_new("p"), &str_new("s"), 0, 64, "sha512");
    }

    #[test]
    #[should_panic(expected = "Invalid digest: md4x")]
    fn an_unknown_digest_is_nodes_type_error() {
        pbkdf2_sync(&str_new("p"), &str_new("s"), 1, 64, "md4x");
    }

    #[test]
    #[should_panic(expected = "Deriving bits failed")]
    fn a_zero_length_key_is_nodes_error() {
        pbkdf2_sync(&str_new("p"), &str_new("s"), 1, 0, "sha512");
    }

    /// The OS source answers the length asked for, and two draws differ — a
    /// constant (the failure a stubbed source would have) is 2^-256 likely.
    #[test]
    fn random_bytes_answers_fresh_bytes_of_the_length_asked() {
        let first = random_bytes(32);
        let second = random_bytes(32);
        assert_eq!(first.len(), 32);
        assert_ne!(first.to_vec(), second.to_vec());
        assert_eq!(random_bytes(0).len(), 0);
        assert_eq!(random_bytes(65_536).len(), 65_536);
    }

    #[test]
    #[should_panic(expected = "QuotaExceededError")]
    fn more_than_the_quota_is_the_hosts_refusal() {
        random_bytes(65_537);
    }

    #[test]
    #[should_panic(expected = "RangeError: Invalid typed array length: -1")]
    fn a_negative_length_is_the_hosts_range_error() {
        random_bytes(-1);
    }

    /// Version 4, variant `10`, lowercase, 8-4-4-4-12 — the shape
    /// `crypto.randomUUID` answers — and fresh per call.
    #[test]
    fn a_random_uuid_is_a_version_four_uuid() {
        let uuid = random_uuid();
        let groups: Vec<&str> = uuid.split('-').collect();
        assert_eq!(
            groups.iter().map(|group| group.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert!(
            uuid.chars()
                .all(|character| character == '-' || matches!(character, '0'..='9' | 'a'..='f'))
        );
        assert!(groups[2].starts_with('4'), "{uuid}");
        assert!(
            matches!(groups[3].as_bytes()[0], b'8' | b'9' | b'a' | b'b'),
            "{uuid}"
        );
        assert_ne!(uuid, random_uuid());
    }

    /// The four WebCrypto twins answer what `std::crypto` documents; each is
    /// `async` for the call site's sake and completes on its first poll.
    #[test]
    fn the_webcrypto_twins_answer_through_their_futures() {
        let run = |future: std::pin::Pin<Box<dyn std::future::Future<Output = Bytes>>>| {
            let waker = std::task::Waker::noop();
            let mut context = std::task::Context::from_waker(waker);
            let mut future = future;
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(bytes) => hex(&bytes.to_vec()),
                std::task::Poll::Pending => panic!("nothing here suspends"),
            }
        };
        let text = |value: &str| Bytes::from_vec(value.as_bytes().to_vec());
        assert!(run(Box::pin(sha512_bytes(text("abc")))).starts_with("ddaf35a1"));
        assert!(run(Box::pin(sha384_bytes(text("abc")))).starts_with("cb00753f"));
        assert!(
            run(Box::pin(hmac_sha512_bytes(
                text("Jefe"),
                text("what do ya want for nothing?")
            )))
            .starts_with("164b7a7b")
        );
        assert!(
            run(Box::pin(pbkdf2_sha512_bytes(
                text("password"),
                text("salt"),
                2,
                512
            )))
            .starts_with("e1d9c16a")
        );
        assert_eq!(
            run(Box::pin(pbkdf2_sha512_bytes(
                text("password"),
                text("salt"),
                2,
                0
            ))),
            ""
        );
    }
}
