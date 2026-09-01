import { open, unlink, writeFile } from "node:fs/promises";
function __fs_close(file) {
	file.close().catch((error) => {
		console.error("vilan: closing a dropped file failed:", error);
	});
}
async function __fs_close_awaited(file) {
	await file.close();
}
function __shared_new(value) {
	return { v: value };
}
function encode_utf8(text) {
	return new TextEncoder().encode(text);
}
function decode_utf8(bytes) {
	return new TextDecoder().decode(bytes);
}
async function open2(path) {
	return await (open(path, "r"));
}
async function create(path) {
	return await (open(path, "w"));
}
async function read_at(self, buffer, position2) {
	return (await (self.read(buffer, 0, buffer.length, position2))).bytesRead;
}
async function write_at(self, buffer, position2) {
	return (await (self.write(buffer, 0, buffer.length, position2))).bytesWritten;
}
async function stat(self) {
	const raw = await (self.stat());
	return [ raw.size, raw.mtimeMs, raw.isDirectory() ];
}
function drop(self) {
	__fs_close(self);
}
function of(file) {
	return [ file, __shared_new(0) ];
}
async function next(self, size) {
	const buffer = new Uint8Array(size);
	const count = await (read_at(self[0], buffer, self[1].v));
	self[1].v = self[1].v + as_i53(count);
	return buffer.slice(0, count);
}
function position(self) {
	return self[1].v;
}
function as_i53(self) {
	const widened = Number(self);
	return Number(Math.trunc(widened));
}
function $a($b) {
	drop($b);
}
async function $f(file, body) {
	try {
		const result = await (body(file));
		await (__fs_close_awaited(file));
		return result;
	} finally {
		$a(file);
	}
}
async function $e(path, body) {
	return await ($f(await (open2(path)), body));
}
async function $g(path, body) {
	return await ($f(await (create(path)), body));
}
function $i($j) {
	$a($j[0]);
}
(async () => {
	await (writeFile("file-corpus.txt", "0123456789"));
	let file = await (open2("file-corpus.txt"));
	try {
		const buffer = new Uint8Array(4);
		console.log(await (read_at(file, buffer, 3)));
		console.log(decode_utf8(buffer.slice(0, 4)));
		console.log((await (stat(file)))[0]);
		$a(file);
		file = null;
		const $c = await (open2("file-corpus.txt"));
		try {
			console.log(await (read_at($c, buffer, 0)));
		} finally {
			$a($c);
		}
	} finally {
		if (file !== null) {
			$a(file);
		}
	}
	const $d = await (open2("file-corpus.txt"));
	try {
		console.log((await (stat($d)))[0]);
	} finally {
		$a($d);
	}
	const size = await ($e("file-corpus.txt", async (f) => {
		return (await (stat(f)))[0];
	}));
	console.log(size);
	await ($g("file-corpus.txt", async (f) => {
		await (write_at(f, encode_utf8("0123456789"), 0));
		return;
	}));
	let reader = of(await (open2("file-corpus.txt")));
	try {
		let whole = "";
		while (true) {
			const chunk = await (next(reader, 4));
			if (chunk.length === 0) {
				break;
			}
			whole = whole + decode_utf8(chunk);
		}
		console.log(whole);
		console.log(position(reader));
		$i(reader);
		reader = null;
	} finally {
		if (reader !== null) {
			$i(reader);
		}
	}
	await (unlink("file-corpus.txt"));
})().catch(($k) => {
	console.error(String($k));
	process.exit(1);
});
