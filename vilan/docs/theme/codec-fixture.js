// The share codec: program text carried in a URL fragment as
// base64url(deflate-raw(source)) — no server holds anything, and a fragment
// never reaches one in a request.
//
// ONE home (K15, design-language.md §2.6): the canonical file is the website
// repo's playground/codec.js — the playground's editor bundle imports it, and
// esbuild inlines it, so the editor build fails if it is missing — and the
// toolchain repo commits a byte-identical copy of it as
// vilan/docs/theme/codec-fixture.js (you may be reading either). The book's
// run/share harness (vilan/docs/theme/vilan.js) is a classic script that must
// keep working on a locally built book with no served playground beside it,
// so it cannot import this module; it carries an inline copy of the two
// functions it needs (encodeBase64Url, deflate), and the toolchain suite
// (crates/vilan-cli/tests/book_mirrors.rs) holds that copy byte-equal to the
// fixture's. When a function moves: edit the website's playground/codec.js,
// rebuild the editor bundle, re-copy this file over the fixture —
//   cp playground/codec.js ../vilan/vilan/docs/theme/codec-fixture.js
// — and the red suite walks the book's copy forward in the same change-set.

function encodeBase64Url(bytes) {
	let binary = "";
	for (const byte of bytes) {
		binary += String.fromCharCode(byte);
	}
	return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

function decodeBase64Url(text) {
	const padded = text.replaceAll("-", "+").replaceAll("_", "/");
	const binary = atob(padded + "=".repeat((4 - (padded.length % 4)) % 4));
	return Uint8Array.from(binary, (ch) => ch.charCodeAt(0));
}

async function deflate(text) {
	const stream = new Blob([new TextEncoder().encode(text)])
		.stream()
		.pipeThrough(new CompressionStream("deflate-raw"));
	return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function inflate(bytes) {
	const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
	return new Response(stream).text();
}

export { decodeBase64Url, deflate, encodeBase64Url, inflate };
