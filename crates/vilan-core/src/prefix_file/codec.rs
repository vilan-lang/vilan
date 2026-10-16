//! The persisted std prefix's binary encoding (std-prefix P3, `std-prefix.md`
//! §5, Q12): a serde `Serializer`/`Deserializer` pair over flat bytes, in tree.
//!
//! Not self-describing — a value is read by the type that wrote it, which is
//! what the file's header (the format version, the analyzer's build id) is for
//! — so every integer is a LEB128 varint (signed ones zigzagged), every
//! sequence and map carries its length, an `Option` is a presence byte, an
//! enum is its variant index, and a struct is its fields in declaration order
//! with no names. Floats are their little-endian bits.
//!
//! **Strings borrow.** The world holds tens of thousands of `&'src str` rows —
//! names, the std sources' texts — and a loaded world must hold them without
//! copying: the writer interns every string into one TABLE ([`Writer::strings`])
//! and writes its index in place; the reader, handed the whole file as a
//! `&'de [u8]` that the loader leaked, resolves an index to a slice of the
//! table and hands it to `visit_borrowed_str`, so `#[serde(borrow)]`-shaped
//! fields point into the file buffer (`#[serde(bound(deserialize = "'de:
//! 'static"))]` on the world's types is what names that lifetime). The header
//! is written without a table (`Writer::inline`): it is read before the table
//! exists.
//!
//! Byte-stable: the same value writes the same bytes, which is what makes the
//! trailer's checksum worth computing.

use std::fmt;

use serde::de::{self, DeserializeSeed, Visitor};
use serde::ser::{self, Serialize};

/// An encoding or decoding failure, as text. A decoding failure is a MISS
/// to the loader, never a diagnostic.
#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Error(message.to_string())
    }
}

impl de::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Error(message.to_string())
    }
}

type Result<T> = std::result::Result<T, Error>;

// --- varints ---------------------------------------------------------------

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

fn unzigzag(value: u64) -> i64 {
    ((value >> 1) as i64) ^ -((value & 1) as i64)
}

// --- the writer ------------------------------------------------------------

/// Serializes into `out`; with a string table when `strings` is `Some`.
pub struct Writer {
    pub out: Vec<u8>,
    strings: Option<StringTable>,
}

#[derive(Default)]
struct StringTable {
    index: std::collections::HashMap<String, u32>,
    rows: Vec<String>,
}

impl Writer {
    /// A writer whose strings are interned into a table — the body's.
    pub fn interning() -> Writer {
        Writer {
            out: Vec::new(),
            strings: Some(StringTable::default()),
        }
    }

    /// A writer whose strings are written in place — the header's.
    pub fn inline() -> Writer {
        Writer {
            out: Vec::new(),
            strings: None,
        }
    }

    /// The finished bytes, and the string table encoded as its own block
    /// (empty for an inline writer): a count, then each row as a length and
    /// its bytes.
    pub fn finish(self) -> (Vec<u8>, Vec<u8>) {
        let mut table = Vec::new();
        if let Some(strings) = self.strings {
            put_varint(&mut table, strings.rows.len() as u64);
            for row in &strings.rows {
                put_varint(&mut table, row.len() as u64);
                table.extend_from_slice(row.as_bytes());
            }
        }
        (self.out, table)
    }

    fn put_str(&mut self, value: &str) {
        match &mut self.strings {
            Some(strings) => {
                let index = match strings.index.get(value) {
                    Some(index) => *index,
                    None => {
                        let index = strings.rows.len() as u32;
                        strings.rows.push(value.to_string());
                        strings.index.insert(value.to_string(), index);
                        index
                    }
                };
                put_varint(&mut self.out, u64::from(index));
            }
            None => {
                put_varint(&mut self.out, value.len() as u64);
                self.out.extend_from_slice(value.as_bytes());
            }
        }
    }
}

impl ser::Serializer for &mut Writer {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn serialize_bool(self, value: bool) -> Result<()> {
        self.out.push(u8::from(value));
        Ok(())
    }

    fn serialize_i8(self, value: i8) -> Result<()> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i16(self, value: i16) -> Result<()> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i32(self, value: i32) -> Result<()> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i64(self, value: i64) -> Result<()> {
        put_varint(&mut self.out, zigzag(value));
        Ok(())
    }

    fn serialize_u8(self, value: u8) -> Result<()> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u16(self, value: u16) -> Result<()> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u32(self, value: u32) -> Result<()> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u64(self, value: u64) -> Result<()> {
        put_varint(&mut self.out, value);
        Ok(())
    }

    fn serialize_f32(self, value: f32) -> Result<()> {
        self.out.extend_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn serialize_f64(self, value: f64) -> Result<()> {
        self.out.extend_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn serialize_char(self, value: char) -> Result<()> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_str(self, value: &str) -> Result<()> {
        self.put_str(value);
        Ok(())
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<()> {
        put_varint(&mut self.out, value.len() as u64);
        self.out.extend_from_slice(value);
        Ok(())
    }

    fn serialize_none(self) -> Result<()> {
        self.out.push(0);
        Ok(())
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<()> {
        self.out.push(1);
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
    ) -> Result<()> {
        self.serialize_u32(variant_index)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<()> {
        put_varint(&mut self.out, u64::from(variant_index));
        value.serialize(self)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self> {
        let len = len.ok_or_else(|| Error("a sequence of unknown length".to_string()))?;
        put_varint(&mut self.out, len as u64);
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self> {
        Ok(self)
    }

    fn serialize_tuple_struct(self, _name: &'static str, _len: usize) -> Result<Self> {
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self> {
        put_varint(&mut self.out, u64::from(variant_index));
        Ok(self)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self> {
        let len = len.ok_or_else(|| Error("a map of unknown length".to_string()))?;
        put_varint(&mut self.out, len as u64);
        Ok(self)
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self> {
        put_varint(&mut self.out, u64::from(variant_index));
        Ok(self)
    }
}

impl ser::SerializeSeq for &mut Writer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl ser::SerializeTuple for &mut Writer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl ser::SerializeTupleStruct for &mut Writer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl ser::SerializeTupleVariant for &mut Writer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl ser::SerializeMap for &mut Writer {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<()> {
        key.serialize(&mut **self)
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl ser::SerializeStruct for &mut Writer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl ser::SerializeStructVariant for &mut Writer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

/// Encodes `value` with an interning writer: `(body, string table)`.
pub fn encode<T: Serialize>(value: &T) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut writer = Writer::interning();
    value.serialize(&mut writer)?;
    Ok(writer.finish())
}

/// Encodes `value` with strings in place (the header).
pub fn encode_inline<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut writer = Writer::inline();
    value.serialize(&mut writer)?;
    Ok(writer.finish().0)
}

// --- the reader ------------------------------------------------------------

/// Deserializes from `bytes`, resolving string indexes through `table` when
/// one is given; every borrowed string is a slice of `'de`.
pub struct Reader<'de> {
    bytes: &'de [u8],
    at: usize,
    table: Option<Vec<&'de str>>,
}

impl<'de> Reader<'de> {
    /// A reader over `body` whose strings are the rows of `table` (as
    /// [`Writer::finish`] wrote it).
    pub fn with_table(body: &'de [u8], table: &'de [u8]) -> Result<Reader<'de>> {
        let mut rows = Reader {
            bytes: table,
            at: 0,
            table: None,
        };
        let count = rows.varint()?;
        let mut strings: Vec<&'de str> = Vec::with_capacity(count.min(1 << 20) as usize);
        for _ in 0..count {
            strings.push(rows.inline_str()?);
        }
        if rows.at != table.len() {
            return Err(Error("the string table carries trailing bytes".to_string()));
        }
        Ok(Reader {
            bytes: body,
            at: 0,
            table: Some(strings),
        })
    }

    /// A reader whose strings are in place (the header).
    pub fn inline(bytes: &'de [u8]) -> Reader<'de> {
        Reader {
            bytes,
            at: 0,
            table: None,
        }
    }

    /// How many bytes have been read.
    pub fn position(&self) -> usize {
        self.at
    }

    fn take(&mut self, count: usize) -> Result<&'de [u8]> {
        let end = self
            .at
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| Error(format!("truncated at byte {}", self.at)))?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn varint(&mut self) -> Result<u64> {
        let mut value: u64 = 0;
        let mut shift = 0;
        loop {
            let byte = self.byte()?;
            if shift >= 64 {
                return Err(Error(format!("a varint overflows at byte {}", self.at)));
            }
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
        }
    }

    fn inline_str(&mut self) -> Result<&'de str> {
        let len = self.varint()? as usize;
        let bytes = self.take(len)?;
        std::str::from_utf8(bytes).map_err(|_| Error(format!("not UTF-8 at byte {}", self.at)))
    }

    fn str(&mut self) -> Result<&'de str> {
        if self.table.is_none() {
            return self.inline_str();
        }
        let index = self.varint()? as usize;
        self.table
            .as_ref()
            .and_then(|table| table.get(index).copied())
            .ok_or_else(|| Error(format!("string {index} is past the table")))
    }

    fn u64_checked<T: TryFrom<u64>>(&mut self, what: &str) -> Result<T> {
        let value = self.varint()?;
        T::try_from(value).map_err(|_| Error(format!("{value} does not fit {what}")))
    }

    fn i64_checked<T: TryFrom<i64>>(&mut self, what: &str) -> Result<T> {
        let value = unzigzag(self.varint()?);
        T::try_from(value).map_err(|_| Error(format!("{value} does not fit {what}")))
    }
}

impl<'de> de::Deserializer<'de> for &mut Reader<'de> {
    type Error = Error;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        Err(Error(
            "the prefix encoding is not self-describing".to_string(),
        ))
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.byte()? {
            0 => visitor.visit_bool(false),
            1 => visitor.visit_bool(true),
            other => Err(Error(format!("{other} is not a bool"))),
        }
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i8(self.i64_checked("i8")?)
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i16(self.i64_checked("i16")?)
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i32(self.i64_checked("i32")?)
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i64(unzigzag(self.varint()?))
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u8(self.u64_checked("u8")?)
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u16(self.u64_checked("u16")?)
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u32(self.u64_checked("u32")?)
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u64(self.varint()?)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("four bytes");
        visitor.visit_f32(f32::from_le_bytes(bytes))
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let bytes: [u8; 8] = self.take(8)?.try_into().expect("eight bytes");
        visitor.visit_f64(f64::from_le_bytes(bytes))
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let value: u32 = self.u64_checked("char")?;
        let value = char::from_u32(value).ok_or_else(|| Error(format!("{value} is not a char")))?;
        visitor.visit_char(value)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_borrowed_str(self.str()?)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_borrowed_str(self.str()?)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let len = self.varint()? as usize;
        visitor.visit_borrowed_bytes(self.take(len)?)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.byte()? {
            0 => visitor.visit_none(),
            1 => visitor.visit_some(self),
            other => Err(Error(format!("{other} is not an option tag"))),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let len = self.varint()? as usize;
        visitor.visit_seq(Counted {
            reader: self,
            remaining: len,
        })
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value> {
        visitor.visit_seq(Counted {
            reader: self,
            remaining: len,
        })
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let len = self.varint()? as usize;
        visitor.visit_map(Counted {
            reader: self,
            remaining: len,
        })
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_tuple(fields.len(), visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_enum(self)
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        // Only a variant index is ever asked for: struct fields are positional.
        visitor.visit_u32(self.u64_checked("a variant index")?)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        Err(Error(
            "nothing in the prefix encoding can be skipped".to_string(),
        ))
    }
}

/// A sequence or map of a known length.
struct Counted<'a, 'de> {
    reader: &'a mut Reader<'de>,
    remaining: usize,
}

impl<'de> de::SeqAccess<'de> for Counted<'_, 'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        seed.deserialize(&mut *self.reader).map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.remaining)
    }
}

impl<'de> de::MapAccess<'de> for Counted<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        seed.deserialize(&mut *self.reader).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        seed.deserialize(&mut *self.reader)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.remaining)
    }
}

impl<'de> de::EnumAccess<'de> for &mut Reader<'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self)> {
        let variant = seed.deserialize(&mut *self)?;
        Ok((variant, self))
    }
}

impl<'de> de::VariantAccess<'de> for &mut Reader<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        seed.deserialize(self)
    }

    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value> {
        de::Deserializer::deserialize_tuple(self, len, visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        de::Deserializer::deserialize_tuple(self, fields.len(), visitor)
    }
}

/// Decodes a `T` from `body` with `table`, refusing trailing bytes.
pub fn decode<'de, T: serde::Deserialize<'de>>(body: &'de [u8], table: &'de [u8]) -> Result<T> {
    let mut reader = Reader::with_table(body, table)?;
    let value = T::deserialize(&mut reader)?;
    if reader.at != body.len() {
        return Err(Error(format!(
            "{} trailing bytes after the value",
            body.len() - reader.at
        )));
    }
    Ok(value)
}

/// Decodes a `T` written inline (the header) from the front of `bytes`,
/// returning it with the bytes consumed.
pub fn decode_inline<'de, T: serde::Deserialize<'de>>(bytes: &'de [u8]) -> Result<(T, usize)> {
    let mut reader = Reader::inline(bytes);
    let value = T::deserialize(&mut reader)?;
    Ok((value, reader.at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    enum Kind<'a> {
        Unit,
        Newtype(u32),
        Tuple(i64, #[serde(borrow)] &'a str),
        Struct {
            flag: bool,
            #[serde(borrow)]
            names: Vec<&'a str>,
        },
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    #[serde(bound(deserialize = "'de: 'a"))]
    struct Row<'a> {
        id: u32,
        name: &'a str,
        maybe: Option<&'a str>,
        kinds: Vec<Kind<'a>>,
        // The analyzer's maps hash with the constant-seed fx hasher, which is what
        // makes their iteration order — and so these bytes — a function of the
        // insertion sequence; std's `RandomState` would not be.
        by_name: crate::fx::FxHashMap<&'a str, (u8, i32)>,
        ordered: BTreeMap<u64, String>,
        bytes: Vec<u8>,
        float: f64,
        cell: std::cell::Cell<u64>,
        shared: std::sync::Arc<[Box<str>]>,
        index: indexmap::IndexMap<u32, &'a str>,
        wide: usize,
        signed: i8,
    }

    fn row() -> Row<'static> {
        Row {
            id: 7,
            name: "seven",
            maybe: None,
            kinds: vec![
                Kind::Unit,
                Kind::Newtype(300),
                Kind::Tuple(-5, "seven"),
                Kind::Struct {
                    flag: true,
                    names: vec!["a", "b", "a"],
                },
            ],
            by_name: [("x", (1, -1)), ("y", (2, i32::MIN))].into_iter().collect(),
            ordered: BTreeMap::from([(u64::MAX, "max".to_string()), (0, String::new())]),
            bytes: vec![0, 255, 128],
            float: -0.0,
            cell: std::cell::Cell::new(42),
            shared: std::sync::Arc::from(vec!["l".into(), "r".into()].into_boxed_slice()),
            index: indexmap::IndexMap::from([(9, "nine"), (1, "one")]),
            wide: usize::MAX,
            signed: -128,
        }
    }

    #[test]
    fn a_value_round_trips_through_the_table_and_borrows_its_strings() {
        let value = row();
        let (body, table) = encode(&value).expect("encode");
        let decoded: Row<'_> = decode(&body, &table).expect("decode");
        assert_eq!(decoded.id, value.id);
        assert_eq!(decoded.name, value.name);
        assert_eq!(decoded.kinds, value.kinds);
        assert_eq!(decoded.by_name, value.by_name);
        assert_eq!(decoded.ordered, value.ordered);
        assert_eq!(decoded.bytes, value.bytes);
        assert_eq!(decoded.float.to_bits(), value.float.to_bits());
        assert_eq!(decoded.cell.get(), 42);
        assert_eq!(&*decoded.shared, &*value.shared);
        assert_eq!(decoded.index, value.index);
        assert_eq!(decoded.wide, usize::MAX);
        assert_eq!(decoded.signed, -128);
        // The string table dedupes: "seven" and "a" appear once each.
        let mut reader = Reader::inline(&table);
        let count = reader.varint().expect("count");
        let mut rows = Vec::new();
        for _ in 0..count {
            rows.push(reader.inline_str().expect("row"));
        }
        assert_eq!(rows.iter().filter(|row| **row == "seven").count(), 1);
        assert_eq!(rows.iter().filter(|row| **row == "a").count(), 1);
        // And a borrowed string is a slice of the table, not a copy.
        let table_range = table.as_ptr_range();
        assert!(table_range.contains(&decoded.name.as_ptr()));
    }

    #[test]
    fn the_inline_form_round_trips_and_reports_its_length() {
        let value = row();
        let bytes = encode_inline(&value).expect("encode");
        let mut longer = bytes.clone();
        longer.extend_from_slice(b"tail");
        let (decoded, used): (Row<'_>, usize) = decode_inline(&longer).expect("decode");
        assert_eq!(used, bytes.len());
        assert_eq!(decoded.kinds, value.kinds);
        assert_eq!(decoded.name, "seven");
    }

    #[test]
    fn the_same_value_writes_the_same_bytes() {
        let (a, ta) = encode(&row()).expect("encode");
        let (b, tb) = encode(&row()).expect("encode");
        assert_eq!((a, ta), (b, tb));
    }

    #[test]
    fn a_truncated_body_is_an_error_not_a_panic() {
        let (body, table) = encode(&row()).expect("encode");
        for cut in [0, 1, body.len() / 2, body.len() - 1] {
            let result: Result<Row<'_>> = decode(&body[..cut], &table);
            assert!(result.is_err(), "a body cut at {cut} must not decode");
        }
        let result: Result<Row<'_>> = decode(&body, &table[..table.len() / 2]);
        assert!(result.is_err(), "a cut table must not decode");
        let mut trailing = body.clone();
        trailing.push(0);
        let result: Result<Row<'_>> = decode(&trailing, &table);
        assert!(result.is_err(), "trailing bytes must not decode");
    }

    #[test]
    fn a_string_index_past_the_table_is_an_error() {
        let (mut body, table) = encode(&row()).expect("encode");
        // The first string written is `name` — at offset 1 (after `id`'s varint 7).
        body[1] = 0x7f;
        let result: Result<Row<'_>> = decode(&body, &table);
        assert!(result.is_err());
    }
}
