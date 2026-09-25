//! HMAC (RFC 2104) and PBKDF2 (RFC 8018 §5.2) over the four digests this
//! runtime has — SHA-1 and SHA-256 from `vilan_rt::crypto`, SHA-384 and SHA-512
//! from [`crate::sha512`].
//!
//! PBKDF2's cost IS its iteration count, so the inner loop is written for it:
//! a [`Mac`] computes its two keyed prefixes once, and for the SHA-512 family
//! it keeps them as block-function STATES, so an iteration is two compressions
//! rather than four. The SHA-1/SHA-256 pair is reached through `vilan-rt`'s
//! one-shot digests (their block functions are private to it and this crate
//! does not reach inside), which costs the prefix block again per call — they
//! are here for node's `pbkdf2Sync(.., "sha256")`, not for a hot path anything
//! in std walks.

use crate::sha512::{BLOCK_LENGTH, Sha512Family};

/// A digest by node's name for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl Algorithm {
    /// node's digest names, case-insensitively (`pbkdf2Sync(.., "SHA512")`
    /// is accepted there too). `None` for any other name.
    pub fn named(name: &str) -> Option<Algorithm> {
        match name.to_ascii_lowercase().as_str() {
            "sha1" => Some(Algorithm::Sha1),
            "sha256" => Some(Algorithm::Sha256),
            "sha384" => Some(Algorithm::Sha384),
            "sha512" => Some(Algorithm::Sha512),
            _ => None,
        }
    }

    fn block_length(self) -> usize {
        match self {
            Algorithm::Sha1 | Algorithm::Sha256 => 64,
            Algorithm::Sha384 | Algorithm::Sha512 => BLOCK_LENGTH,
        }
    }

    /// The digest's length in bytes — PBKDF2's `hLen`.
    pub fn output_length(self) -> usize {
        match self {
            Algorithm::Sha1 => 20,
            Algorithm::Sha256 => 32,
            Algorithm::Sha384 => 48,
            Algorithm::Sha512 => 64,
        }
    }

    fn digest(self, data: &[u8]) -> Vec<u8> {
        match self {
            Algorithm::Sha1 => vilan_rt::crypto::sha1(data).to_vec(),
            Algorithm::Sha256 => vilan_rt::crypto::sha256(data).to_vec(),
            Algorithm::Sha384 => crate::sha512::sha384(data),
            Algorithm::Sha512 => crate::sha512::sha512(data),
        }
    }
}

/// An HMAC keyed once and signed with many times.
pub enum Mac {
    /// SHA-384/512: the two keyed prefixes as block-function states.
    Wide {
        family: Sha512Family,
        inner: [u64; 8],
        outer: [u64; 8],
    },
    /// SHA-1/256: the two padded keys, prepended per signature.
    Narrow {
        algorithm: Algorithm,
        inner_key: Vec<u8>,
        outer_key: Vec<u8>,
    },
}

impl Mac {
    /// RFC 2104 §2: a key longer than the block is hashed first, then every
    /// key is zero-padded to the block and XORed with `ipad`/`opad`.
    pub fn new(algorithm: Algorithm, key: &[u8]) -> Mac {
        let block_length = algorithm.block_length();
        let mut padded = if key.len() > block_length {
            algorithm.digest(key)
        } else {
            key.to_vec()
        };
        padded.resize(block_length, 0);
        let inner_key: Vec<u8> = padded.iter().map(|byte| byte ^ 0x36).collect();
        let outer_key: Vec<u8> = padded.iter().map(|byte| byte ^ 0x5c).collect();
        let family = match algorithm {
            Algorithm::Sha384 => Sha512Family::Sha384,
            Algorithm::Sha512 => Sha512Family::Sha512,
            Algorithm::Sha1 | Algorithm::Sha256 => {
                return Mac::Narrow {
                    algorithm,
                    inner_key,
                    outer_key,
                };
            }
        };
        let block = |key: &[u8]| -> [u8; BLOCK_LENGTH] {
            key.try_into().expect("the padded key is one block")
        };
        Mac::Wide {
            family,
            inner: family.state_after_block(&block(&inner_key)),
            outer: family.state_after_block(&block(&outer_key)),
        }
    }

    /// `H(K ^ opad || H(K ^ ipad || data))`.
    pub fn sign(&self, data: &[u8]) -> Vec<u8> {
        match self {
            Mac::Wide {
                family,
                inner,
                outer,
            } => {
                let inner_digest = family.render(&family.digest_after(*inner, BLOCK_LENGTH, data));
                family.render(&family.digest_after(*outer, BLOCK_LENGTH, &inner_digest))
            }
            Mac::Narrow {
                algorithm,
                inner_key,
                outer_key,
            } => {
                let mut inner_message = inner_key.clone();
                inner_message.extend_from_slice(data);
                let mut outer_message = outer_key.clone();
                outer_message.extend_from_slice(&algorithm.digest(&inner_message));
                algorithm.digest(&outer_message)
            }
        }
    }
}

/// HMAC of `data` under `key`.
pub fn hmac(algorithm: Algorithm, key: &[u8], data: &[u8]) -> Vec<u8> {
    Mac::new(algorithm, key).sign(data)
}

/// PBKDF2 with HMAC-`algorithm` as the PRF (RFC 8018 §5.2): `key_length`
/// bytes derived from `password` and `salt` over `iterations` rounds.
///
/// The caller has validated the parameters — each host twin refuses its own
/// out-of-range values in its own words before reaching here.
pub fn pbkdf2(
    algorithm: Algorithm,
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    key_length: usize,
) -> Vec<u8> {
    let mac = Mac::new(algorithm, password);
    let mut derived = Vec::with_capacity(key_length);
    let mut block_index: u32 = 1;
    while derived.len() < key_length {
        // U_1 = PRF(P, S || INT(i)); T_i = U_1 ^ U_2 ^ … ^ U_c.
        let mut first = salt.to_vec();
        first.extend_from_slice(&block_index.to_be_bytes());
        let mut previous = mac.sign(&first);
        let mut block = previous.clone();
        for _ in 1..iterations {
            previous = mac.sign(&previous);
            for (accumulated, byte) in block.iter_mut().zip(&previous) {
                *accumulated ^= byte;
            }
        }
        let wanted = (key_length - derived.len()).min(block.len());
        derived.extend_from_slice(&block[..wanted]);
        block_index += 1;
    }
    derived
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex;

    /// RFC 4231's HMAC-SHA-512 cases 1, 2 and 6 (6 is a key LONGER than the
    /// block, so it is hashed first), and case 2 at the other three digests —
    /// every expected value produced by node's `createHmac`.
    #[test]
    fn hmac_signs_rfc_4231s_vectors() {
        assert_eq!(
            hex(&hmac(Algorithm::Sha512, &[0x0b; 20], b"Hi There")),
            "87aa7cdea5ef619d4ff0b4241a1d6cb02379f4e2ce4ec2787ad0b30545e17cdedaa833b7d6b8a702038b274eaea3f4e4be9d914eeb61f1702e696c203a126854"
        );
        let jefe = b"what do ya want for nothing?";
        assert_eq!(
            hex(&hmac(Algorithm::Sha512, b"Jefe", jefe)),
            "164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea2505549758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737"
        );
        assert_eq!(
            hex(&hmac(
                Algorithm::Sha512,
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "80b24263c7c1a3ebb71493c1dd7be8b49b46d1f41b4aeec1121b013783f8f3526b56d037e05f2598bd0fd2215d6a1e5295e64f73f63f0aec8b915a985d786598"
        );
        assert_eq!(
            hex(&hmac(Algorithm::Sha384, b"Jefe", jefe)),
            "af45d2e376484031617f78d2b58a6b1b9c7ef464f5a01b47e42ec3736322445e8e2240ca5e69e2c78b3239ecfab21649"
        );
        assert_eq!(
            hex(&hmac(Algorithm::Sha256, b"Jefe", jefe)),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(
            hex(&hmac(Algorithm::Sha1, b"Jefe", jefe)),
            "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"
        );
    }

    /// PBKDF2-HMAC-SHA-512 at 1, 2 and 4096 iterations, a derived key LONGER
    /// than one digest (100 bytes: two blocks, the second truncated), and
    /// the other three digests at 2 — node's `pbkdf2Sync` produced every one.
    #[test]
    fn pbkdf2_derives_nodes_keys() {
        for (iterations, expected) in [
            (
                1,
                "867f70cf1ade02cff3752599a3a53dc4af34c7a669815ae5d513554e1c8cf252c02d470a285a0501bad999bfe943c08f050235d7d68b1da55e63f73b60a57fce",
            ),
            (
                2,
                "e1d9c16aa681708a45f5c7c4e215ceb66e011a2e9f0040713f18aefdb866d53cf76cab2868a39b9f7840edce4fef5a82be67335c77a6068e04112754f27ccf4e",
            ),
            (
                4096,
                "d197b1b33db0143e018b12f3d1d1479e6cdebdcc97c5c0f87f6902e072f457b5143f30602641b3d55cd335988cb36b84376060ecd532e039b742a239434af2d5",
            ),
        ] {
            assert_eq!(
                hex(&pbkdf2(
                    Algorithm::Sha512,
                    b"password",
                    b"salt",
                    iterations,
                    64
                )),
                expected,
                "{iterations} iterations"
            );
        }
        assert_eq!(
            hex(&pbkdf2(
                Algorithm::Sha512,
                b"passwordPASSWORDpassword",
                b"saltSALTsaltSALTsaltSALTsaltSALTsalt",
                4096,
                100
            )),
            "8c0511f4c6e597c6ac6315d8f0362e225f3c501495ba23b868c005174dc4ee71115b59f9e60cd9532fa33e0f75aefe30225c583a186cd82bd4daea9724a3d3b804f75bdd41494fa324cab24bcc680fb3b96a30cf5d21fac3c2875913919f3399b1d9ce7e"
        );
        for (algorithm, length, expected) in [
            (
                Algorithm::Sha384,
                48,
                "54f775c6d790f21930459162fc535dbf04a939185127016a04176a0730c6f1f4fb48832ad1261baadd2cedd50814b1c8",
            ),
            (
                Algorithm::Sha256,
                32,
                "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43",
            ),
            (
                Algorithm::Sha1,
                20,
                "ea6c014dc72d6f8ccd1ed92ace1d41f0d8de8957",
            ),
        ] {
            assert_eq!(
                hex(&pbkdf2(algorithm, b"password", b"salt", 2, length)),
                expected,
                "{algorithm:?}"
            );
        }
    }

    #[test]
    fn a_digest_is_named_as_node_names_it() {
        assert_eq!(Algorithm::named("sha512"), Some(Algorithm::Sha512));
        assert_eq!(Algorithm::named("SHA512"), Some(Algorithm::Sha512));
        assert_eq!(Algorithm::named("sha1"), Some(Algorithm::Sha1));
        assert_eq!(Algorithm::named("md4x"), None);
    }
}
