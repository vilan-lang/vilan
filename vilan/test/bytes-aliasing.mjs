function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __substring(text, start, end) {
	if (0 <= start && start <= end && end <= text.length) return text.substring(start, end);
	throw "substring out of range: the length is " + text.length + " but the range is " + start + ".." + end + " — substring requires 0 <= start <= end <= len and never clamps or swaps; to drop a known affix use strip_prefix/strip_suffix, and for the rest of the string pass s.len() as the end";
}
function to_hex(self) {
	const digits = "0123456789abcdef";
	let out = "";
	const $b = new2(0, self.length);
	while (true) {
		const $c = next($b);
		if ($c[0] !== 0) {
			break;
		}
		const index = $c[1];
		const byte = self.at(index);
		out = out + __substring(digits, Math.trunc(byte / 16), Math.trunc(byte / 16) + 1) + __substring(digits, byte % 16, byte % 16 + 1);
	}
	return out;
}
function set(self, index, value) {
	self.fill(value, index, index + 1);
}
function set_u32(self, index, value) {
	self.fill(value, index, index + 1);
}
function concat(a2, b2) {
	const joined2 = new Uint8Array(a2.length + b2.length);
	joined2.set(a2, 0);
	joined2.set(b2, a2.length);
	return joined2;
}
function new2(start, end) {
	return [ start, end ];
}
function next(self) {
	let $a = null;
	if (self[0] < self[1]) {
		const value = self[0];
		self[0] = self[0] + 1;
		$a = [ 0, value ];
	} else {
		$a = [ 1 ];
	}
	return $a;
}
function poke(target, index, value) {
	set(target, index, value);
}
const a = new Uint8Array(4);
const b = __clone(a);
set(a, 0, 7);
set(b, 1, 300);
poke(a, 2, 9);
console.log("" + to_hex(a) + " " + to_hex(b) + " " + a.length);
const c = a.slice(0, 2);
set(c, 0, 1);
console.log("" + to_hex(a) + " " + to_hex(c));
const filled = a.fill(255, 3, 4);
set(filled, 2, 0);
console.log("" + to_hex(a) + " " + to_hex(filled));
a.set(c, 2);
console.log("" + to_hex(b) + " " + b.at(3) + " " + b.at(-(1)));
const joined = concat(a, c);
set(joined, 0, 0);
console.log("" + to_hex(a) + " " + to_hex(joined));
const first = [ "first", new Uint8Array(2) ];
let second = __clone(first);
second[0] = "second";
set(second[1], 0, 0xAB);
console.log("" + first[0] + " " + to_hex(first[1]) + " " + second[0] + " " + to_hex(second[1]));
const shared = new Uint8Array(3);
const holders = [ __clone(shared), shared.slice(0, 3), __clone(shared) ];
const write = (index) => {
	return set_u32(shared, index, 0x1FF);
};
write(1);
for (const holder of holders) {
	console.log(to_hex(holder));
}
console.log("" + shared.at(1));
