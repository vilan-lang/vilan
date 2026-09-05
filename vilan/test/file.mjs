import { mkdir, open, rm, unlink, writeFile } from "node:fs/promises";
function __fs_close(file) {
	file.close().catch((error) => {
		console.error("vilan: closing a dropped file failed:", error);
	});
}
async function __fs_close_awaited(file) {
	await file.close();
}
function __random_int(low, high) {
	return Math.floor(Math.random() * (high - low + 1)) + low;
}
function __shared_new(value) {
	return { v: value };
}
function __substring(text, start, end) {
	if (0 <= start && start <= end && end <= text.length) return text.substring(start, end);
	throw "substring out of range: the length is " + text.length + " but the range is " + start + ".." + end + " — substring requires 0 <= start <= end <= len and never clamps or swaps; to drop a known affix use strip_prefix/strip_suffix, and for the rest of the string pass s.len() as the end";
}
function encode_utf8(text) {
	return new TextEncoder().encode(text);
}
function decode_utf8(bytes) {
	return new TextDecoder().decode(bytes);
}
async function create_dir_all(path) {
	const options = Object();
	options.recursive = true;
	return await (mkdir(path, options));
}
async function remove_dir_all(path) {
	const options = Object();
	options.recursive = true;
	options.force = true;
	return await (rm(path, options));
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
function basename(path) {
	let end = path.length;
	while (end > 0 && __substring(path, end - 1, end) === "/") {
		end = end - 1;
	}
	let start = end;
	while (start > 0 && __substring(path, start - 1, start) !== "/") {
		start = start - 1;
	}
	return __substring(path, start, end);
}
function range(low, high) {
	return __random_int(low, high);
}
function $a(low, high) {
	return range(low, high);
}
function $b($c) {
	drop($c);
}
async function $g(file, body) {
	try {
		const result = await (body(file));
		await (__fs_close_awaited(file));
		return result;
	} finally {
		$b(file);
	}
}
async function $f(path, body) {
	return await ($g(await (open2(path)), body));
}
async function $h(path, body) {
	return await ($g(await (create(path)), body));
}
function $j($k) {
	$b($k[0]);
}
(async () => {
	const root = "file-corpus-" + $a(100000, 999999);
	await (create_dir_all(root));
	const scratch = "" + root + "/data.txt";
	console.log(basename(scratch));
	await (writeFile(scratch, "0123456789"));
	let file = await (open2(scratch));
	try {
		const buffer = new Uint8Array(4);
		console.log(await (read_at(file, buffer, 3)));
		console.log(decode_utf8(buffer.slice(0, 4)));
		console.log((await (stat(file)))[0]);
		$b(file);
		file = null;
		const $d = await (open2(scratch));
		try {
			console.log(await (read_at($d, buffer, 0)));
		} finally {
			$b($d);
		}
	} finally {
		if (file !== null) {
			$b(file);
		}
	}
	const $e = await (open2(scratch));
	try {
		console.log((await (stat($e)))[0]);
	} finally {
		$b($e);
	}
	const size = await ($f(scratch, async (f) => {
		return (await (stat(f)))[0];
	}));
	console.log(size);
	await ($h(scratch, async (f) => {
		await (write_at(f, encode_utf8("0123456789"), 0));
		return;
	}));
	let reader = of(await (open2(scratch)));
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
		$j(reader);
		reader = null;
	} finally {
		if (reader !== null) {
			$j(reader);
		}
	}
	await (unlink(scratch));
	await (remove_dir_all(root));
})().catch(($l) => {
	console.error(String($l));
	process.exit(1);
});
