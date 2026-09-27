//! `vilan-rt::bytes` — `std::bytes`'s host types: the byte buffer and the two
//! text codecs (tracker F33, RULED (a) 2026-09-22).
//!
//! # What this is a twin OF
//!
//! `vilan/std/src/bytes.vl` binds the host `Uint8Array`, `TextEncoder` and
//! `TextDecoder`. The first is the one with a representation question: a
//! `Uint8Array` is a MUTABLE REFERENCE type — `new Uint8Array(n)`, `fill` and
//! `set` write into the one buffer every binding that names it sees, and
//! `__clone` passes it through untouched (it is not an array), so `let b = a`
//! is an alias on the JS backend.
//!
//! # The ruling, and what it rules out
//!
//! F33 weighed three answers: (a) a shared, mutable buffer — the host's own
//! semantics; (b) copy-on-write, with the three mutating bindings answering a
//! new buffer, which silently changes what an ALIASING program prints; (c)
//! refusing the three natively for good, which keeps `std::ws`'s frame codec,
//! and with it the rpc server, off the native backend. The owner ruled (a). So
//! [`Bytes`] is an `Rc<RefCell<Vec<u8>>>`: a rule-1 copy is a refcount bump, and
//! it is an ALIAS, exactly as the other backend's is.
//!
//! # Why it was here and not in `http`
//!
//! It began life in `vilan_rt::http`, the first surface that needed one. It
//! has four customers now (http, crypto, fs, and the `std::ws` codec the
//! emitter reaches through `std::bytes` itself), which is what the old comment
//! said would justify a module of its own.

use std::cell::{Ref, RefCell};
use std::rc::Rc;

use crate::{Js, Str, str_new};

/// `Bytes` — `std::bytes`'s host type, which on the JS backend is a
/// `Uint8Array`: a fixed-length buffer that every holder SHARES.
///
/// Equality is IDENTITY (`===`), which is the only equality a `Uint8Array` has
/// on the other backend — vilan refuses `==` on a `Bytes` outright (it has no
/// `PartialEq` impl), so this impl is reached only by an emitted aggregate's
/// derived comparison, and there it must mean what the JS field comparison
/// means.
#[derive(Clone, Debug, Default)]
pub struct Bytes(Rc<RefCell<Vec<u8>>>);

impl PartialEq for Bytes {
    fn eq(&self, other: &Bytes) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Bytes {}

impl Bytes {
    pub fn from_vec(bytes: Vec<u8>) -> Bytes {
        Bytes(Rc::new(RefCell::new(bytes)))
    }

    /// `new Uint8Array(size)` — `std::bytes`'s `alloc`: `size` zero bytes.
    ///
    /// The size is a `usize`, the index type (I5), so the host's `RangeError`
    /// for a negative one has nothing left to catch.
    pub fn alloc(size: usize) -> Bytes {
        Bytes::from_vec(vec![0; size])
    }

    /// The bytes, borrowed for as long as the caller holds the guard.
    ///
    /// A guard rather than a `&[u8]`, because the buffer is shared and mutable:
    /// the borrow is what keeps a write from landing under a reader. Nothing in
    /// this runtime holds one across a call back into emitted code.
    pub fn as_slice(&self) -> Ref<'_, [u8]> {
        Ref::map(self.0.borrow(), |bytes| bytes.as_slice())
    }

    /// The bytes, copied out.
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.borrow().clone()
    }

    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    /// `bytes.at(index)` — `std::bytes`'s `get`/`get_u32`.
    ///
    /// JavaScript's `at` answers `undefined` out of range, and the vilan
    /// signature types the result an integer, so there is no `undefined` to
    /// hand back: `0` is the answer, and `std::bytes` documents the index as
    /// the caller's contract exactly as `str::code_at` does. The index is a
    /// `usize` (I5), so the host's count-from-the-end reading of a negative one
    /// has no spelling left.
    pub fn at(&self, index: usize) -> i32 {
        self.0.borrow().get(index).map_or(0, |byte| *byte as i32)
    }

    /// `bytes.slice(from, to)` — a COPY of the half-open range, with
    /// JavaScript's clamping: an out-of-range bound is pulled to the nearest
    /// end and a reversed pair answers empty (which is where `slice` differs
    /// from `str::substring`, whose host swaps them). A copy on both backends:
    /// `slice` is the one reader that makes a NEW buffer.
    pub fn slice(&self, from: usize, to: usize) -> Bytes {
        let bytes = self.0.borrow();
        let start = from.min(bytes.len());
        let end = to.min(bytes.len());
        if end <= start {
            return Bytes::from_vec(Vec::new());
        }
        Bytes::from_vec(bytes[start..end].to_vec())
    }

    /// `bytes.fill(value, from, to)` — `std::bytes`'s `fill`/`fill_u32`, and
    /// through them `set`/`set_u32`: IN PLACE, answering the same buffer (the
    /// host returns `this`, so the answer is an alias, not a copy).
    ///
    /// The stored byte is the host's `ToUint8`, which for an integer is its low
    /// eight bits — `value as u8` for every width the emitter passes (it widens
    /// to `i64` first, so `i32` and `u32` arrive the same). The range resolves
    /// as [`Bytes::slice`]'s does, and a reversed one writes nothing.
    pub fn fill(&self, value: i64, from: usize, to: usize) -> Bytes {
        {
            let mut bytes = self.0.borrow_mut();
            let start = from.min(bytes.len());
            let end = to.min(bytes.len());
            if start < end {
                bytes[start..end].fill(value as u8);
            }
        }
        self.clone()
    }

    /// `target.set(source, offset)` — `std::bytes`'s `copy_into`: all of
    /// `source` written into this buffer starting at `offset`, in place.
    ///
    /// The host throws a `RangeError` for a source that would run past the end,
    /// and so does this (the abort every host throw takes natively); the
    /// offset is a `usize` (I5), so a negative one has no spelling left. The source is read OUT before the write, which is
    /// what the host specifies for two views of one buffer and what makes
    /// `a.copy_into(a, 0)` a no-op here rather than a double borrow.
    pub fn copy_into(&self, source: &Bytes, offset: usize) {
        let data = source.to_vec();
        let mut bytes = self.0.borrow_mut();
        if offset + data.len() > bytes.len() {
            drop(bytes);
            crate::panic_with("RangeError: offset is out of bounds");
        }
        bytes[offset..offset + data.len()].copy_from_slice(&data);
    }
}

impl Js for Bytes {
    /// Node renders a `Uint8Array` as `Uint8Array(3) [ 1, 2, 3 ]`, which is its
    /// own object inspection rather than anything the language defines. Printing
    /// one is refused at run time by name, as printing a `Task` is.
    fn js(&self) -> String {
        crate::panic_with(
            "printing a `Bytes` is a host object's own inspection, which the native backend \
             does not reproduce",
        )
    }
}

/// `JSON.stringify(new Uint8Array([1, 2]))` is `{"0":1,"1":2}` — a typed array
/// has its indices as own enumerable properties, so it stringifies as an OBJECT
/// and not as an array. Reproduced rather than refused, because unlike the
/// executor's handles a `Bytes` really does have a JSON rendering on the other
/// backend.
impl crate::Json for Bytes {
    fn json(&self) -> String {
        let mut out = String::from("{");
        for (index, byte) in self.0.borrow().iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&format!("\"{index}\":{byte}"));
        }
        out.push('}');
        out
    }
}

/// `TextDecoder` / `TextEncoder` — `std::bytes`'s two host classes.
///
/// Both are unit structs because both host classes are stateless for the one
/// encoding vilan has — a vilan `str` is UTF-8, so `new TextDecoder()` carries
/// nothing a native twin has to keep.
///
/// **Decoding is LOSSY, as the host's is.** `new TextDecoder()` without
/// `{ fatal: true }` replaces malformed input with U+FFFD rather than
/// throwing, and `std::bytes` constructs it exactly that way — so
/// `from_utf8_lossy` is the same function, not a shortcut past an error.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct TextDecoder;

impl TextDecoder {
    pub fn decode(&self, bytes: &Bytes) -> Str {
        str_new(&String::from_utf8_lossy(&bytes.as_slice()))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct TextEncoder;

impl TextEncoder {
    pub fn encode(&self, text: &str) -> Bytes {
        Bytes::from_vec(text.as_bytes().to_vec())
    }
}

impl Js for TextDecoder {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a `TextDecoder` is a host object's own inspection, which the native backend \
             does not reproduce",
        )
    }
}

impl Js for TextEncoder {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a `TextEncoder` is a host object's own inspection, which the native backend \
             does not reproduce",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ruling's whole claim: two holders, ONE buffer. A write through
    /// either — `fill`, `copy_into`, or through the handle `fill` answers — is
    /// seen through both, and `slice` is the one reader that copies.
    #[test]
    fn a_write_through_one_holder_is_seen_through_every_other() {
        let first = Bytes::alloc(4);
        let second = first.clone();
        first.fill(7, 0, 1);
        second.fill(300, 1, 2);
        let answered = first.fill(9, 2, 3);
        answered.fill(1, 3, 4);
        assert_eq!(first.to_vec(), vec![7, 44, 9, 1]);
        assert_eq!(second.to_vec(), vec![7, 44, 9, 1]);
        assert_eq!(first, answered, "`fill` answers the same buffer");

        let copied = first.slice(0, 2);
        copied.fill(0, 0, 2);
        assert_eq!(first.to_vec(), vec![7, 44, 9, 1], "a slice is a copy");
        assert_ne!(first, copied);

        second.copy_into(&copied, 2);
        assert_eq!(first.to_vec(), vec![7, 44, 0, 0]);
    }

    /// `fill`'s range is `slice`'s: bounds clamp to the length, a reversed
    /// range writes nothing; the stored byte is the low eight bits.
    #[test]
    fn fill_resolves_its_range_and_its_byte_as_the_host_does() {
        let bytes = Bytes::alloc(5);
        bytes.fill(-1, 3, 99);
        assert_eq!(bytes.to_vec(), vec![0, 0, 0, 255, 255]);
        bytes.fill(0x1_02, 3, 1);
        assert_eq!(bytes.to_vec(), vec![0, 0, 0, 255, 255], "reversed: nothing");
        bytes.fill(0x1_02, 0, 1);
        assert_eq!(bytes.to_vec(), vec![2, 0, 0, 255, 255]);
    }

    /// `at` past the end answers `0`, and `slice` clamps both bounds.
    #[test]
    fn at_and_slice_clamp_past_the_end() {
        let bytes = Bytes::from_vec(vec![1, 2, 3]);
        assert_eq!(bytes.at(2), 3);
        assert_eq!(bytes.at(3), 0);
        assert_eq!(bytes.slice(1, 99).to_vec(), vec![2, 3]);
        assert_eq!(bytes.slice(9, 99).to_vec(), Vec::<u8>::new());
        assert_eq!(bytes.len(), 3);
    }

    /// A buffer copied into ITSELF reads its source out first, so it is the
    /// host's no-op rather than a `RefCell` double borrow.
    #[test]
    fn a_buffer_copied_into_itself_is_unchanged() {
        let bytes = Bytes::from_vec(vec![1, 2, 3]);
        bytes.copy_into(&bytes.clone(), 0);
        assert_eq!(bytes.to_vec(), vec![1, 2, 3]);
    }

    #[test]
    #[should_panic(expected = "offset is out of bounds")]
    fn a_copy_past_the_end_is_the_hosts_range_error() {
        Bytes::alloc(2).copy_into(&Bytes::alloc(2), 1);
    }
}
