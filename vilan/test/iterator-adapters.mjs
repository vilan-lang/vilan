function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __dbg(write, location, entries) {
	if (entries.length === 0) {
		write("[" + location + "]");
		return;
	}
	for (const entry of entries) {
		const head = "[" + location + "] " + entry[0] + " = ";
		write(head + __dbg_layout(entry[1], __dbg_width(head), 0));
	}
}
function __dbg_value(write, location, text, show, value) {
	__dbg(write, location, [ [ text, show(value) ] ]);
	return value;
}
function __dbg_values(write, location, texts, shows, values, spread) {
	__dbg(write, location, values.map((value, index) => [ texts[index], shows[index](value) ]));
	return spread ? values.flatMap((value, index) => spread[index] ? value : [ value ]) : values;
}
function __dbg_group(open, close, padded, entries, fill) {
	return { o: open, c: close, p: padded, e: entries, f: fill === true };
}
function __dbg_list(items, show, fill) {
	const entries = [];
	const shown = Math.min(items.length, 100);
	for (let index = 0; index < shown; index++) entries.push([ "", show(items[index]) ]);
	if (items.length > shown) entries.push([ "", "… " + (items.length - shown) + " more" ]);
	return __dbg_group("[", "]", false, entries, fill);
}
const __dbg_seen = [];
function __dbg_shared(cell, show) {
	if (__dbg_seen.includes(cell)) return "<cycle>";
	__dbg_seen.push(cell);
	try {
		return __dbg_group("Shared(", ")", false, [ [ "", show(cell.v) ] ]);
	} finally {
		__dbg_seen.pop();
	}
}
function __dbg_members(open, items, show, fill) {
	const entries = [];
	for (const item of items) {
		if (entries.length === 100) {
			entries.push([ "", "… " + (items.length - 100) + " more" ]);
			break;
		}
		entries.push(show(item));
	}
	return __dbg_group(open, "}", true, entries, fill);
}
function __dbg_map(open, table, showKey, showValue, keyWidth, valueWidth) {
	const items = Array.from(table.values());
	return __dbg_members(open, items, (pair) => {
		const key = keyWidth === 1 ? pair[0] : pair.slice(0, keyWidth);
		const value = valueWidth === 1 ? pair[keyWidth] : pair.slice(keyWidth, keyWidth + valueWidth);
		return [ __dbg_flat(showKey(key)) + " => ", showValue(value) ];
	});
}
function __dbg_set(open, table, show, fill) {
	return __dbg_members(open, Array.from(table.values()), (item) => [ "", show(item) ], fill);
}
function __dbg_str(text) {
	let out = "\"";
	for (const character of text) {
		if (character === "\\") out += "\\\\";
		else if (character === "\"") out += "\\\"";
		else if (character === "\n") out += "\\n";
		else if (character === "\t") out += "\\t";
		else if (character === "\r") out += "\\r";
		else if (character === "\0") out += "\\0";
		else out += character;
	}
	return out + "\"";
}
function __dbg_float(value) {
	if (Object.is(value, -0)) return "-0.0";
	const text = String(value);
	return Number.isInteger(value) && !text.includes("e") ? text + ".0" : text;
}
function __dbg_width(text) {
	let width = 0;
	for (const _ of text) width++;
	return width;
}
function __dbg_flat(document) {
	if (typeof document === "string") return document;
	if (document.e.length === 0) return document.o + document.c;
	const inner = document.e.map((entry) => entry[0] + __dbg_flat(entry[1])).join(", ");
	return document.p ? document.o + " " + inner + " " + document.c : document.o + inner + document.c;
}
function __dbg_layout(document, column, indent) {
	const flat = __dbg_flat(document);
	if (typeof document === "string" || document.e.length === 0 || column + __dbg_width(flat) <= 80) return flat;
	const pad = " ".repeat(indent + 2);
	let out = document.o + "\n";
	if (document.f) {
		let line = "";
		for (const entry of document.e) {
			const text = entry[0] + __dbg_flat(entry[1]) + ",";
			if (line === "") line = pad + text;
			else if (__dbg_width(line) + 1 + __dbg_width(text) > 80) {
				out += line + "\n";
				line = pad + text;
			} else line += " " + text;
		}
		return out + line + "\n" + " ".repeat(indent) + document.c;
	}
	for (const entry of document.e) out += pad + entry[0] + __dbg_layout(entry[1], __dbg_width(pad + entry[0]), indent + 2) + ",\n";
	return out + " ".repeat(indent) + document.c;
}
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
}
function __map_get(map, key) {
	return map.has(key) ? [ 0, __clone(map.get(key)) ] : [ 1 ];
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function hash(self) {
	return __hash(self);
}
function hash2(self) {
	return __hash(self);
}
function new2(start, end) {
	return [ start, end ];
}
function next(self) {
	let $m = null;
	if (self[0] < self[1]) {
		const value = self[0];
		self[0] = self[0] + 1;
		$m = [ 0, value ];
	} else {
		$m = [ 1 ];
	}
	return $m;
}
function next2(self) {
	self[0] = self[0] + 1;
	return [ 0, self[0] ];
}
function iter(self) {
	return [ __clone(self), 0 ];
}
function filter(self, predicate) {
	return [ self, predicate ];
}
function map(self, fn) {
	return [ self, fn ];
}
function skip(self, count3) {
	return [ self, count3 ];
}
function take(self, count3) {
	return [ self, count3 ];
}
function next3(self) {
	let $a = null;
	if (self[1] < self[0].length) {
		const value = __clone(__at(self[0], self[1], "std/src/iterator.vl:168:16"));
		self[1] = self[1] + 1;
		$a = [ 0, value ];
	} else {
		$a = [ 1 ];
	}
	return $a;
}
function next4(self) {
	let found = [ 1 ];
	let searching = true;
	while (searching) {
		const $b = next3(self[0]);
		let $c = null;
		if ($b[0] === 0) {
			if (self[1]($b[1])) {
				found = [ 0, $b[1] ];
				searching = false;
			}
			$c = undefined;
		} else {
			searching = false;
		}
		$c;
	}
	return found;
}
function next5(self) {
	const $d = next4(self[0]);
	if ($d[0] === 0) {
		return [ 0, self[1]($d[1]) ];
	}
	return [ 1 ];
}
function next6(self) {
	while (self[1] > 0) {
		self[1] = self[1] - 1;
		const $e = next5(self[0]);
		if ($e[0] === 1) {
			self[1] = 0;
			return [ 1 ];
		}
	}
	return next5(self[0]);
}
function next7(self) {
	if (self[1] <= 0) {
		return [ 1 ];
	}
	self[1] = self[1] - 1;
	return next6(self[0]);
}
function to_list(self) {
	let result = [  ];
	const $f = self;
	while (true) {
		const $g = next7($f);
		if ($g[0] !== 0) {
			break;
		}
		const value = $g[1];
		result.push(__clone(value));
	}
	return result;
}
function next8(self) {
	const $h = next2(self[0]);
	if ($h[0] === 0) {
		return [ 0, self[1]($h[1]) ];
	}
	return [ 1 ];
}
function next9(self) {
	if (self[1] <= 0) {
		return [ 1 ];
	}
	self[1] = self[1] - 1;
	return next8(self[0]);
}
function to_list2(self) {
	let result = [  ];
	const $i = self;
	while (true) {
		const $j = next9($i);
		if ($j[0] !== 0) {
			break;
		}
		const value = $j[1];
		result.push(__clone(value));
	}
	return result;
}
function any(self, predicate) {
	const $k = self;
	while (true) {
		const $l = next2($k);
		if ($l[0] !== 0) {
			break;
		}
		const value = $l[1];
		if (predicate(value)) {
			return true;
		}
	}
	return false;
}
function zip(self, other) {
	return [ self, __clone(other) ];
}
function next10(self) {
	const $n = next(self[0]);
	let $q = null;
	if ($n[0] === 0) {
		const $p = next3(self[1]);
		if ($p[0] === 0) {
			return [ 0, [ $n[1], $p[1] ] ];
		}
		$q = undefined;
	}
	$q;
	return [ 1 ];
}
function chain(self, other) {
	return [ self, __clone(other), true ];
}
function next11(self) {
	if (self[2]) {
		const $t = next3(self[0]);
		if ($t[0] === 0) {
			return [ 0, $t[1] ];
		}
		self[2] = false;
	}
	return next3(self[1]);
}
function count(self) {
	let seen = 0;
	const $u = self;
	while (true) {
		const $v = next11($u);
		if ($v[0] !== 0) {
			break;
		}
		const _value = $v[1];
		seen = seen + 1;
	}
	return seen;
}
function enumerate(self) {
	return [ self, 0 ];
}
function next12(self) {
	const $w = next3(self[0]);
	if ($w[0] === 0) {
		const index = self[1];
		self[1] = index + 1;
		return [ 0, [ index, $w[1] ] ];
	}
	return [ 1 ];
}
function fold(self, init, fn) {
	let accumulator = __clone(init);
	const $z = self;
	while (true) {
		const $A = next3($z);
		if ($A[0] !== 0) {
			break;
		}
		const value = $A[1];
		accumulator = fn(accumulator, value);
	}
	return accumulator;
}
function all(self, predicate) {
	const $B = self;
	while (true) {
		const $C = next3($B);
		if ($C[0] !== 0) {
			break;
		}
		const value = $C[1];
		if (!(predicate(value))) {
			return false;
		}
	}
	return true;
}
function to_list3(self) {
	let result = [  ];
	const $D = self;
	while (true) {
		const $E = next3($D);
		if ($E[0] !== 0) {
			break;
		}
		const value = $E[1];
		result.push(__clone(value));
	}
	return result;
}
function reverse(self) {
	let result = [  ];
	let index = self.length;
	while (index > 0) {
		index = index - 1;
		result.push(__clone(__at(self, index, "std/src/list.vl:98:16")));
	}
	return result;
}
function rev(self) {
	return [ reverse(to_list3(self)), 0 ];
}
function for_each(self, fn) {
	const $F = self;
	while (true) {
		const $G = next3($F);
		if ($G[0] !== 0) {
			break;
		}
		const value = $G[1];
		fn(value);
	}
}
function to_list4(self) {
	let result = [  ];
	const $H = self;
	while (true) {
		const $I = next4($H);
		if ($I[0] !== 0) {
			break;
		}
		const value = $I[1];
		result.push(__clone(value));
	}
	return result;
}
function new3() {
	const table = new Map();
	return [ table ];
}
function insert(self, value) {
	self[0].set(hash2(value), value);
}
function to_set(self) {
	let result = new3();
	for (const value of self) {
		insert(result, value);
	}
	return result;
}
function len(self) {
	return self[0].size;
}
function next13(self) {
	const $J = next3(self[0]);
	if ($J[0] === 0) {
		return [ 0, self[1]($J[1]) ];
	}
	return [ 1 ];
}
function to_list5(self) {
	let result = [  ];
	const $K = self;
	while (true) {
		const $L = next13($K);
		if ($L[0] !== 0) {
			break;
		}
		const value = $L[1];
		result.push(__clone(value));
	}
	return result;
}
function new4() {
	const table = new Map();
	return [ table ];
}
function insert2(self, key, value) {
	self[0].set(hash(key), [ __clone(key), __clone(value) ]);
}
function to_map(self) {
	let result = new4();
	for (const entry2 of self) {
		insert2(result, entry2[0], entry2[1]);
	}
	return result;
}
function get(self, key) {
	const $M = __map_get(self[0], hash(key));
	let $N = null;
	if ($M[0] === 0) {
		const entry2 = $M[1];
		$N = [ 0, __clone(entry2[1]) ];
	} else {
		$N = [ 1 ];
	}
	return $N;
}
function unwrap_or(self, fallback) {
	const $O = self;
	let $P = null;
	if ($O[0] === 0) {
		const x = __clone($O[1]);
		$P = x;
	} else {
		$P = __clone(fallback);
	}
	return $P;
}
function count2(self) {
	let seen = 0;
	const $Q = self;
	while (true) {
		const $R = next3($Q);
		if ($R[0] !== 0) {
			break;
		}
		const _value = $R[1];
		seen = seen + 1;
	}
	return seen;
}
function __show_i32(value) {
	return "" + value;
}
function __show_List_i32(value) {
	return __dbg_list(value, __show_i32, true);
}
console.log(__dbg_flat(__show_List_i32(to_list(take(skip(map(filter(iter([ 1, 2, 3, 4, 5, 6 ]), (n) => {
	return n % 2 === 0;
}), (n) => {
	return n * 10;
}), 1), 2)))));
console.log(__dbg_flat(__show_List_i32(to_list2(take(map([ 0 ], (n) => {
	return n * n;
}), 4)))));
console.log(any([ 0 ], (n) => {
	return n === 3;
}));
let zipped = zip(new2(0, 9), iter([ "a", "b" ]));
const $r = zipped;
while (true) {
	const $s = next10($r);
	if ($s[0] !== 0) {
		break;
	}
	const pair = $s[1];
	console.log("" + pair[0] + pair[1]);
}
console.log(String(count(chain(iter([ 1, 2 ]), iter([ 3 ])))));
let numbered = enumerate(iter([ "x", "y" ]));
const $x = numbered;
while (true) {
	const $y = next12($x);
	if ($y[0] !== 0) {
		break;
	}
	const entry = $y[1];
	console.log("" + entry[0] + "=" + entry[1]);
}
console.log(String(fold(iter([ 1, 2, 3 ]), 0, (total, n) => {
	return total + n;
})));
console.log(all(iter([ 1, 2, 3 ]), (n) => {
	return n > 0;
}));
console.log(__dbg_flat(__show_List_i32(to_list3(rev(iter([ 1, 2, 3 ]))))));
for_each(iter([ 1, 2 ]), (n) => {
	return console.log(String(n));
});
console.log(String(len(to_set(to_list4(filter(iter([ 1, 2, 2, 3 ]), (n) => {
	return n > 1;
}))))));
const lengths = to_map(to_list5(map(iter([ "alpha", "hi" ]), (word) => {
	return [ word, word.length ];
})));
console.log(String(unwrap_or(get(lengths, "hi"), 0)));
let live = [ 1, 2 ];
let cursor = iter(live);
live.push(3);
console.log(String(count2(cursor)));
console.log(String(live.length));
