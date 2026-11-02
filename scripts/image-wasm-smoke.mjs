// The node smoke for `std::image`'s wasm codec (tracker M185): instantiate the
// module a build just produced, decode a 2x2 PNG through it, scale it to one
// pixel, and encode it back — the protocol `transformer.rs`'s `__image_*`
// helpers drive (vilan-image-wasm's crate header), under the host they drive it
// on. N134's rule: a wasm leg RUNS what it builds.
//
//   node scripts/image-wasm-smoke.mjs path/to/vilan_image_wasm.wasm
import { readFileSync } from "node:fs";

const path = process.argv[2];
if (!path) {
    console.error("usage: node scripts/image-wasm-smoke.mjs <module.wasm>");
    process.exit(2);
}
const { instance } = await WebAssembly.instantiate(readFileSync(path), {});
const exports = instance.exports;

function fail(message) {
    console.error("image wasm smoke: " + message);
    process.exit(1);
}

function put(bytes) {
    const pointer = exports.image_alloc(bytes.length);
    new Uint8Array(exports.memory.buffer, pointer, bytes.length).set(bytes);
    return pointer;
}

function output() {
    return new Uint8Array(exports.memory.buffer, exports.image_output(), exports.image_output_len()).slice();
}

function pixels() {
    return new Uint8Array(exports.memory.buffer, exports.image_pixels(), exports.image_pixels_len()).slice();
}

// Red, green / blue, half-transparent white.
const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAE0lEQVR42mP4z8DwHwyBNAg0AABJSQl4nFEXkgAAAABJRU5ErkJggg==", "base64");

if (exports.image_sniff(put(png), png.length) !== 0) fail("the PNG does not sniff as a PNG");
if (exports.image_decode(put(png), png.length, 50000000) !== 0) {
    fail("decode refused: " + new TextDecoder().decode(output()));
}
if (exports.image_width() !== 2 || exports.image_height() !== 2) {
    fail("decoded " + exports.image_width() + " x " + exports.image_height() + ", not 2 x 2");
}
const expected = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128];
if (pixels().join() !== expected.join()) fail("decoded pixels " + pixels().join());

// The limit refuses with its sentence.
if (exports.image_decode(put(png), png.length, 3) !== 1) fail("the pixel limit did not refuse");
const refusal = new TextDecoder().decode(output());
if (refusal !== "2 x 2 exceeds the 3-pixel limit") fail("the refusal read " + JSON.stringify(refusal));

// Scale to one pixel, encode, and decode what came out.
exports.image_decode(put(png), png.length, 50000000);
exports.image_resize(1, 1, 2);
if (exports.image_width() !== 1 || exports.image_height() !== 1) fail("the resize is not 1 x 1");
if (exports.image_encode(0, 0) !== 0) fail("encode refused: " + new TextDecoder().decode(output()));
const encoded = output();
if (exports.image_decode(put(encoded), encoded.length, 1) !== 0) fail("the re-encoded PNG did not decode");
console.log("image wasm smoke: decode, limit, resize and encode under node - ok (" + pixels().join() + ")");
