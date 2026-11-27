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
function __dbg_map(open, table, showKey, showValue) {
	const items = [];
	for (const entry of table[0].values()) if (entry[1][0] === 0) items.push([ entry[0], entry[1][1] ]);
	return __dbg_members(open, items, (entry) => [ __dbg_flat(showKey(entry[0])) + " => ", showValue(entry[1]) ]);
}
function __dbg_set(open, table, show, fill) {
	const items = [];
	for (const entry of table[0].values()) if (entry[1][0] === 0) items.push(entry[0]);
	return __dbg_members(open, items, (item) => [ "", show(item) ], fill);
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
		else if (character < " " || character === "\x7f") out += "\\u{" + character.charCodeAt(0).toString(16) + "}";
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
function __mkey_get(value, key, copy) {
	const place = __mkey_reach(value, key);
	if (place === null) return [ 1 ];
	const field = place[0][place[1]];
	return [ 0, copy ? __clone(field) : field ];
}
function __mkey_lend(value, key, body, scalar) {
	const place = __mkey_reach(value, key);
	if (place === null) return false;
	body(scalar ? place : place[0][place[1]]);
	return true;
}
function __mkey_reach(value, key) {
	const parts = key[0][0];
	const last = parts.length - 1;
	for (let at = 0; ; at++) {
		const part = parts[at];
		let slot = part[1];
		if (part[3][0] === 1) {
			if (value[0] !== slot) return null;
			slot = 1 + part[3][1];
		}
		if (at === last) return [ value, slot ];
		value = value[slot];
	}
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function eq(self, b) {
	return self === b;
}
function assert(condition, message, caller) {
	let $h = null;
	if (!(condition)) {
		$h = (() => {
			throw __panic(message, caller);
		})();
	}
	return $h;
}
function name(self) {
	let name3 = "";
	for (const step of self[0]) {
		name3 = name3 + spelled(step, name3 === "");
	}
	return name3;
}
function starts_with(self, prefix) {
	if (prefix[0].length > self[0].length) {
		return false;
	}
	let at = 0;
	for (const step of prefix[0]) {
		const own = __at(self[0], at, "std/src/key.vl:141:14");
		if (step[1] !== own[1] || step[0] !== own[0] || !(same_kind(step[3], own[3]))) {
			return false;
		}
		at = at + 1;
	}
	return true;
}
function then(self, next) {
	let parts = [  ];
	for (const step of self[0]) {
		parts.push(__clone(step));
	}
	for (const step2 of next[0]) {
		parts.push(__clone(step2));
	}
	return [ parts ];
}
function eq2(self, other) {
	return eq(__hash(self[0]), __hash(other[0]));
}
function written(path) {
	if (path[0].length === 0) {
		return "";
	}
	let out = owner_name(__at(path[0], 0, "std/src/key.vl:245:23"));
	for (const step of path[0]) {
		out = out + spelled(step, false);
	}
	return out;
}
function owner_name(step) {
	const segments = step[0].split("::");
	return __at(__at(segments, segments.length - 1, "std/src/key.vl:256:2").split("#"), 0, "std/src/key.vl:256:2");
}
function spelled(step, leads) {
	const $D = step[3];
	let $E = null;
	if ($D[0] === 0) {
		let $F = null;
		if (leads) {
			$F = step[2];
		} else {
			$F = "." + step[2];
		}
		$E = $F;
	} else {
		const position = $D[1];
		const width = $D[2];
		let $G = null;
		if (width > 1) {
			$G = "::" + step[2] + "." + position;
		} else {
			$G = "::" + step[2];
		}
		$E = $G;
	}
	return $E;
}
function same_kind(left, right) {
	const $y = left;
	let $z = null;
	if ($y[0] === 0) {
		const $A = right;
		$z = $A[0] === 0;
	} else {
		const position = $y[1];
		const $B = right;
		let $C = null;
		if ($B[0] === 1) {
			const other = $B[1];
			$C = position === other;
		} else {
			$C = false;
		}
		$z = $C;
	}
	return $z;
}
function eq3(self, b) {
	const $c = self;
	let $f = null;
	if ($c[0] === 0) {
		const $d = b;
		let $e = null;
		if ($d[0] === 0) {
			$e = $c[1] === $d[1];
		} else {
			$e = false;
		}
		$f = $e;
	} else {
		const $g = b;
		$f = $g[0] === 1;
	}
	return $f;
}
function grow(d2, key) {
	return __mkey_lend(d2, key, (x) => {
		x[0][x[1]] = x[0][x[1]] * 2.0;
		return;
	}, 1);
}
const $a = [ [ [ [ "maybe-keys.vl::Doc", 0, "title", [ 0 ] ], [ "std/src/option.vl::Option", 0, "Some", [ 1, 0, 1 ] ] ] ] ];
function $b(value) {
	const v1 = value[0];
	if (v1[0] !== 0) {
		return [ 1 ];
	}
	return [ 0, v1[1] ];
}
const $i = [ [ [ [ "maybe-keys.vl::Doc", 1, "owner", [ 0 ] ], [ "std/src/option.vl::Option", 0, "Some", [ 1, 0, 1 ] ], [ "maybe-keys.vl::User", 0, "name", [ 0 ] ] ] ] ];
function $j(value) {
	const v1 = value[1];
	if (v1[0] !== 0) {
		return [ 1 ];
	}
	return [ 0, v1[1][0] ];
}
const $k = [ [ [ [ "maybe-keys.vl::Doc", 2, "shape", [ 0 ] ], [ "maybe-keys.vl::Shape", 0, "Circle", [ 1, 0, 1 ] ] ] ] ];
function $l(value) {
	const v1 = value[2];
	if (v1[0] !== 0) {
		return [ 1 ];
	}
	return [ 0, v1[1] ];
}
const $m = [ [ [ [ "maybe-keys.vl::Doc", 2, "shape", [ 0 ] ], [ "maybe-keys.vl::Shape", 1, "Rect", [ 1, 1, 2 ] ] ] ] ];
function $n(value, field) {
	const v1 = value[2];
	if (v1[0] !== 1) {
		return false;
	}
	v1[2] = field;
	return true;
}
const $o = [ [ [ [ "maybe-keys.vl::Doc", 2, "shape", [ 0 ] ], [ "maybe-keys.vl::Shape", 1, "Rect", [ 1, 0, 2 ] ] ] ] ];
const $p = [ [ [ [ "maybe-keys.vl::Doc", 1, "owner", [ 0 ] ], [ "std/src/option.vl::Option", 0, "Some", [ 1, 0, 1 ] ] ] ] ];
function $q(value, body) {
	const v1 = value[1];
	if (v1[0] !== 0) {
		return false;
	}
	body(v1[1]);
	return true;
}
const $r = [ [ [ [ "maybe-keys.vl::Doc", 1, "owner", [ 0 ] ], [ "std/src/option.vl::Option", 0, "Some", [ 1, 0, 1 ] ], [ "maybe-keys.vl::User", 1, "age", [ 0 ] ] ] ] ];
function $s(value, body) {
	const v1 = value[1];
	if (v1[0] !== 0) {
		return false;
	}
	body([ v1[1], 1 ]);
	return true;
}
const $v = [ [ [ [ "maybe-keys.vl::Doc", 1, "owner", [ 0 ] ] ] ] ];
function maybe(self) {
	return [ __clone(self[0]) ];
}
const $w = [ [ [ [ "std/src/option.vl::Option", 0, "Some", [ 1, 0, 1 ] ] ] ] ];
function then2(self, next) {
	return [ then(self[0], next[0]) ];
}
function some(self) {
	return then2(maybe(self), $w);
}
const $x = [ [ [ [ "maybe-keys.vl::User", 1, "age", [ 0 ] ] ] ] ];
function eq4(self, other) {
	return eq2(self[0], other[0]);
}
function starts_with2(self, prefix) {
	return starts_with(self[0], prefix[0]);
}
function name2(self) {
	return name(self[0]);
}
function debug(self) {
	return written(self[0]);
}
function $H(value, field) {
	const v1 = value[0];
	if (v1[0] !== 0) {
		return false;
	}
	v1[1] = field;
	return true;
}
function __show_i32(value) {
	return "" + value;
}
function __show_Option_i32(value) {
	if (value[0] === 0) {
		return __dbg_group("Some(", ")", false, [ [ "", __show_i32(value[1]) ] ]);
	}
	return "None";
}
function __show_str(value) {
	return __dbg_str(value);
}
function __show_Option_str(value) {
	if (value[0] === 0) {
		return __dbg_group("Some(", ")", false, [ [ "", __show_str(value[1]) ] ]);
	}
	return "None";
}
let d = [ [ 0, "a" ], [ 0, [ "ada", 3 ] ], [ 1, 1.0, 2.0 ] ];
assert(eq3($b(d), [ 0, "a" ]), "an option\'s payload", "maybe-keys.vl:39:2");
assert(eq3($j(d), [ 0, "ada" ]), "a field past a variant step", "maybe-keys.vl:40:2");
assert(eq3($l(d), [ 1 ]), "another variant misses", "maybe-keys.vl:41:2");
assert($n(d, 5.0), "a put where the place is", "maybe-keys.vl:42:2");
assert(grow(d, $o), "a held key lends its place", "maybe-keys.vl:43:2");
assert($q(d, (u) => {
	return console.log(String(u[1]));
}), "a view lends a struct", "maybe-keys.vl:44:2");
assert($s(d, (a) => {
	a[0][a[1]] = a[0][a[1]] + 1;
	return;
}), "a change writes in place", "maybe-keys.vl:45:2");
const $t = d[2];
let $u = null;
if ($t[0] === 1) {
	const w = $t[1];
	const h = $t[2];
	$u = console.log("" + w + " " + h);
} else {
	$u = console.log("moved");
}
$u;
const age = then2(some($v), $x);
assert(eq4(age, $r), "equal paths, however written", "maybe-keys.vl:54:2");
assert(starts_with2(age, $v), "a variant step starts with its field", "maybe-keys.vl:55:2");
assert(!(starts_with2($o, $m)), "two slots are disjoint", "maybe-keys.vl:56:2");
console.log(__dbg_flat(__show_Option_i32(__mkey_get(d, age, 0))));
console.log(name2($o));
console.log(debug(age));
d[0] = [ 1 ];
d[2] = [ 2 ];
assert(!($H(d, "b")), "a put never creates a place", "maybe-keys.vl:63:2");
assert(!(grow(d, $o)), "a miss lends nothing", "maybe-keys.vl:64:2");
console.log(__dbg_flat(__show_Option_str(d[0])));
//# sourceMappingURL=maybe-keys.mjs.map
