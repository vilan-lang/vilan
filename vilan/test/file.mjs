import { open, unlink, writeFile } from "node:fs/promises";
function __fs_close(file) {
	file.close().catch((error) => {
		console.error("vilan: closing a dropped file failed:", error);
	});
}
async function __fs_close_awaited(file) {
	await file.close();
}
function decode_utf8(bytes) {
	return new TextDecoder().decode(bytes);
}
async function open2(path) {
	return await (open(path, "r"));
}
async function read_at(self, buffer, position) {
	return (await (self.read(buffer, 0, buffer.length, position))).bytesRead;
}
async function stat(self) {
	const raw = await (self.stat());
	return [ raw.size, raw.mtimeMs, raw.isDirectory() ];
}
function drop(self) {
	__fs_close(self);
}
function $a($b) {
	drop($b);
}
async function $e(path, body) {
	const file = await (open2(path));
	try {
		const result = await (body(file));
		await (__fs_close_awaited(file));
		return result;
	} finally {
		$a(file);
	}
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
	await (unlink("file-corpus.txt"));
})().catch(($f) => {
	console.error(String($f));
	process.exit(1);
});
