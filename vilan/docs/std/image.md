# Images

`std::image` decodes PNG, JPEG, WebP and GIF into pixels, scales and crops
them, and encodes PNG, JPEG and WebP — the server half of "someone uploaded a
picture, keep a few smaller copies". It is a process module, like
[`std::db`](process.md#stddb-sqlite): a browser program that imports it is
refused at build time (the browser has `createImageBitmap` and a canvas for
its own side).

```vilan,fragment
external struct Image;                  // RGBA pixels, 8 bits each — a value

enum Format { Png, Jpeg, Webp, Gif }
enum Fit { Contain, Cover, Fill }

impl Image {
	fun decode(bytes: Bytes): Result<Image, str>              // limit: 50,000,000 pixels
	fun decode_within(bytes: Bytes, max_pixels: usize): Result<Image, str>
	fun sniff(bytes: Bytes): Option<Format>                   // from the first bytes alone
	fun from_rgba(width: usize, height: usize, pixels: Bytes): Result<Image, str>

	fun width(self): usize
	fun height(self): usize
	fun rgba(self): Bytes                                     // a copy, row-major RGBA

	fun resized(self, width: usize, height: usize, fit: Fit): Image  // panics past 50,000,000 px
	fun cropped(self, x: usize, y: usize, width: usize, height: usize): Image
	fun encode(self, format: Format, quality: i32): Result<Bytes, str>
}
```

## Avatars: one upload, three sizes

```vilan,norun
import std::bytes::Bytes;
import std::fs::{ read_bytes, write_bytes };
import std::image::{ Fit, Format, Image };
import std::io::print;

fun keep_avatar(id: str, upload: Bytes): Result<void, str> {
	let picture = Image::decode(upload)!;    // an Err returns early, carrying the sentence
	for size in [32usize, 64, 256] {
		let copy = picture.resized(size, size, Fit::Cover).encode(Format::Webp, 80)!;
		write_bytes(i"avatars/{id}-{size}.webp", copy);
	}
	Ok(void)
}

fun main() {
	match keep_avatar("ada", read_bytes("upload.jpg")) {
		Ok(_) => print("saved"),
		Err(let reason) => print(i"refused: {reason}"),
	}
}
main();
```

`Cover` is the avatar fit: the picture fills the square and whatever
overflows it is cropped away, centred. `Contain` keeps the whole picture
inside the box instead (the result may be narrower or shorter than the box,
never below one pixel a side), and `Fill` stretches to exactly the box.
Scaling is Lanczos3 — a downscale averages every source pixel it covers, so
a 4,000-pixel photo shown at 300 does not shimmer — and upscaling is allowed.
A picture with transparency is scaled with its colours weighted by their
alpha, so a fully transparent pixel's colour never bleeds into an edge.
`resized` PANICS when its result would exceed 50,000,000 pixels — `a 100000
x 100000 result exceeds the 50,000,000-pixel limit` — the same ceiling
`decode` puts on an upload. The box is your program's choice rather than the
upload's, so an oversized one is a bug to fix, not an `Err` to handle; the
RESULT is what counts, so a `Contain` into a huge box that comes out small
is fine.

`cropped(x, y, width, height)` CLAMPS rather than refuses: a window reaching
past an edge stops at it, and one starting past an edge is empty.

## What decode does for you

- **The format is sniffed** from the magic bytes (`\x89PNG`, `\xFF\xD8`,
  `RIFF…WEBP`, `GIF8`); anything else is `Err("not a PNG, JPEG, WebP or GIF")`.
  `Image::sniff` answers the same question without decoding.
- **Orientation is applied.** A phone stores a portrait photo sideways and
  writes an EXIF orientation beside it; `decode` turns the pixels upright, and
  `width()`/`height()` are the upright size.
- **The size is checked first.** An upload can claim to be 60,000 x 60,000
  pixels in a few hundred bytes; `decode` reads the size from the header and
  refuses anything over 50,000,000 pixels (a 12-megapixel photo is well under)
  before allocating a byte for it: `Err("60000 x 60000 exceeds the
  50,000,000-pixel limit")`. `decode_within` takes your own limit.
- **A GIF is its first frame.**

## Encoding

`encode(format, quality)` writes PNG, JPEG or WebP. `quality` (1-100, clamped)
is JPEG's; PNG and WebP are written **lossless** and ignore it — so a WebP
avatar is exact, and a photo you want small is a JPEG. JPEG has no alpha
channel: transparent pixels are flattened onto white. `Format::Gif` is
decode-only, and `encode` answers `Err("GIF cannot be encoded")`.

`rgba()` and `from_rgba(width, height, pixels)` are the raw way in and out:
four bytes a pixel, row by row, and `from_rgba` refuses a buffer that is not
exactly `width * height * 4` bytes. Both copy, so writing the `Bytes` you got
(or gave) never changes an `Image`.

## Cost, and what runs it

Every call is **synchronous**. Decoding a 12-megapixel JPEG and scaling it
takes a few hundred milliseconds on node and a fraction of that in a native
build, all of it on the thread that called — fine for one upload at a time,
and worth moving off the request path when a server takes many.

Both backends run the **same codec**: the `vilan-image` crate, over Rust's
`image` crate. A native build links it (and builds it optimized even in a
debug build); on node, the compiler writes the same code, compiled to
WebAssembly, into the bundle — about 800 KB of it, and only into a program
that decodes, scales, crops or encodes. A program that never does carries
none of it. (The web playground's compiler leaves the codec out to stay small, so in
its server mode such a program is refused with a sentence saying so; build
it with the `vilan` CLI.) One implementation is why the two backends answer the same
pixels and the same error sentences for the same input. The codec's crates
are MIT- and Apache-2.0-licensed; the toolchain's `THIRD-PARTY-NOTICES.txt`
lists them, and a program that ships the codec ships them too.
