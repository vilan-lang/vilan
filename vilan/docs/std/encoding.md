# Encoding reference

JSON (`std::json`), the codec-agnostic wire layer (`std::wire`), the binary
codec (`std::binary`), raw bytes (`std::bytes`), and base64
(`std::base64`).

The short version: derive `Json` for JSON in/out at app boundaries, derive
`Wire` for rpc payloads, and let the codecs do the rest. Everything below
"Derives" here is plumbing you only meet when building custom transports or
parsers.

## JSON

```vilan,fragment
trait Json { fun to_json(self): str; }        // encode
trait FromJson {                              // decode
	fun from_json(text: str): FromJson;
	fun from_json_value(value: JsonValue): FromJson;
}
```

`[derive(Json)]` implements both from a struct/enum's shape; scalars,
`List`, and `Option` nest.

Encoding (`to_json`) is total, but **decoding is fallible**: the input is
untrusted, so a missing field, a wrong-shaped value, or text that isn't
JSON is a decode error rather than silent garbage or a crash. Both
`from_json(text)` and `from_json_value(value)` return `Result<Self, str>`;
handle it with `!`, `match`, or `is Ok(..)`:

```vilan
import std::print;
import std::json::{ Json, FromJson };
import std::result::Result::{ self, Ok, Err };

[derive(Json)]
struct Point {
	x: i32,
	y: i32,
}

fun main() {
	let point = Point { x = 1, y = 2 };
	let text = point.to_json();
	print(text); // {"x":1,"y":2}

	match Point::from_json(text) {
		Ok(let back) => print(back.x), // 1
		Err(let reason) => print(reason),
	}

	// A missing field is a decode error naming the field.
	match Point::from_json("{\"x\":1}") {
		Ok(_) => print("decoded"),
		Err(let reason) => print(reason), // missing field y
	}
}
```

Untyped inspection, when the shape isn't known up front:

```vilan,fragment
external struct JsonValue;
fun parse_json_value(text: str): JsonValue  // throws on malformed text
str.try_parse_json(): Option<JsonValue>     // the safe form

value.kind(): str        // "object"|"array"|"string"|"number"|"boolean"|"null"
value.is_number(): bool  // and is_string / is_bool / is_array, off kind()
value.is_null(): bool
value.field(name: str): JsonValue
value.has_field(name: str): bool
value.elements(): List<JsonValue>
value.tag(): str         // an enum discriminator, NOT a type — see below
```

`kind()` is the value's JSON type, normalized: the host's `typeof` calls
both an array and `null` an `"object"`, so the four `is_*` predicates read
`kind()` instead. `is_null()` is the exception — it tests the value against
`null` directly.

`tag()` answers a different question and is not a spelling of `kind()`: it
reads an **externally-tagged enum's discriminator** — the string itself for a
bare `"Variant"`, the single key for a `{"Variant":…}` object. That is what a
derived decoder calls to pick a variant, so it only means anything on those
two shapes: on a number or a bool it yields `"undefined"`, and on `null` it
throws. Reach for `kind()` unless you are decoding an enum by hand.

```vilan
import std::print;
import std::json::parse_json_value;

fun main() {
	let value = parse_json_value("{\"name\":\"ada\",\"tags\":[\"x\",\"y\"]}");
	print(value.kind());               // object
	print(value.field("name").kind()); // string
	print(value.has_field("age"));     // false

	for element in value.field("tags").elements() {
		if element.is_string() {
			print(element.kind()); // string, twice
		}
	}

	// `tag()` is the other question: an externally-tagged enum's discriminator.
	print(parse_json_value("\"Start\"").tag());         // Start
	print(parse_json_value("{\"Text\":\"hi\"}").tag()); // Text
}
```

`json_codec(): Codec` is the JSON wire codec for rpc (see below).

## The wire layer (`std::wire`)

The codec-agnostic serialization protocol under `derive(Wire)` and rpc:

- `trait Serialize` / `trait Deserialize`: visitor-style value
  description (`begin_struct`/`field`/`str_value`/`i53_value`/…). The
  wire scalars: `str`, `bool`, `i32`, `u32`, `i53`, `f64` (+ lists,
  options, structs, enum variants).
- `Frame`: one encoded message.
- `Codec`: a matched writer/reader pair, `json_codec()` (`std::json`,
  readable) or `binary_codec()` (`std::binary`, compact). Client and
  server must agree.

`[derive(Wire)]` requires every field to be Wire, recursively, checked at
the derive site. You implement `Serialize`/`Deserialize` by hand only for
types with a custom encoding.

### Backed enums on the wire

A **backed enum** — one whose variants carry an explicit value, `enum
Align { Start = "flex-start", … }` — encodes as that value rather than as
its variant name, for both `Json` and `Wire`. `Align::Start` is
`"flex-start"` on the wire, not `"Start"`, and it decodes through
`Align::parse`, so a peer sending a value outside the set is a decode
error rather than a confidently-wrong variant.

**Adding a backing value to an existing derived enum is a wire-format
break, and so is removing one.** An enum with no backing value keeps the
externally-tagged form (`"Start"`, or `{"Text":…}` with a payload), so
the two shapes are not interchangeable across a version.

## Bytes

An immutable-length byte array (`Uint8Array` underneath), the currency of
the binary codec, crypto, and websockets:

```vilan,fragment
impl Bytes {
	fun alloc(size: i32): Bytes
	fun len(self): i32
	fun get(self, index: i32): i32
	fun set(self, index: i32, value: i32)
	fun slice(self, from: i32, to: i32): Bytes
	fun fill(self, value: i32, from: i32, to: i32): Bytes
	fun copy_into(self, source: Bytes, offset: i32)
	fun concat(a: Bytes, b: Bytes): Bytes     // static
	fun to_hex(self): str
}

// UTF-8 text ↔ bytes
fun encode_utf(text: str): Bytes
fun decode_utf(bytes: Bytes): str
```

Lower still: `ByteBuffer`/`DataView` (host ArrayBuffer access,
`read_f64`/`write_f64`), the binary codec's float channel.

## Binary codec (`std::binary`)

```vilan,fragment
fun binary_codec(): Codec
fun encode_binary<T: Wire>(value: T): Bytes
fun decode_binary<T: Wire>(bytes: Bytes): T
struct BinaryWriter { … }   // write_byte / write_i32 / write_str / finish(): Bytes
```

Same model as JSON, compact layout. `i53` values ride as f64 bit patterns,
exact to 2^53.

## Base64 (`std::base64`)

URL-safe alphabet, no padding (the JWT flavor):

```vilan,fragment
fun encode_url(bytes: Bytes): str
fun decode_url(text: str): Option<Bytes>
```
