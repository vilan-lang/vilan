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
function __nmap_get(table, hash) {
	const entry = table[0].get(hash);
	return entry !== undefined && entry[1][0] === 0 ? [ 0, __clone(entry[1][1]) ] : [ 1 ];
}
function __nmap_insert(table, hash, key, value) {
	__nmap_settle(table);
	const entry = table[0].get(hash);
	if (entry === undefined) table[0].set(hash, [ key, [ 0, value ] ]);
	else {
		entry[0] = key;
		if (entry[1][0] !== 0 || entry[1][1] !== value) entry[1] = [ 0, value ];
	}
}
function __nmap_len(table) {
	const hash = table[1];
	if (hash !== undefined) {
		const entry = table[0].get(hash);
		if (entry !== undefined && entry[1][0] === 1) return table[0].size - 1;
	}
	return table[0].size;
}
function __nmap_settle(table) {
	const hash = table[1];
	if (hash === undefined) return;
	table[1] = undefined;
	const entry = table[0].get(hash);
	if (entry !== undefined && entry[1][0] === 1) table[0].delete(hash);
}
function bump(x) {
	x[0][x[1]] = x[0][x[1]] + 100;
}
function at(t2, k) {
	return [ t2, k[0] ];
}
function at2(t2, k) {
	return t2[k[0]];
}
function peek(t2, k) {
	return [ t2, k[0] ];
}
function eq(self, other) {
	return self[0] === other[0];
}
function debug(self) {
	return "TupleKey(" + self[0] + ")";
}
function new2() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function hash(self) {
	return __hash(self[0]);
}
function insert(self, key, value) {
	__nmap_insert(self[0], hash(key), __clone(key), __clone(value));
}
function get(self, key) {
	return __nmap_get(self[0], hash(key));
}
function len(self) {
	return __nmap_len(self[0]);
}
function new3() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function insert2(self, value) {
	__nmap_insert(self[0], hash(value), __clone(value), undefined);
}
function len2(self) {
	return __nmap_len(self[0]);
}
function __show_i32(value) {
	return "" + value;
}
function __show_str(value) {
	return __dbg_str(value);
}
function __show_P(value) {
	return __dbg_group("P {", "}", true, [ [ "x = ", __show_i32(value[0]) ], [ "y = ", __show_i32(value[1]) ] ]);
}
function __show_i32_str_P(value) {
	return __dbg_group("(", ")", false, [ [ "", __show_i32(value[0]) ], [ "", __show_str(value[1]) ], [ "", __show_P(value[2]) ] ]);
}
function __show_usize(value) {
	return "" + value;
}
function __show_usize_usize_usize(value) {
	return __dbg_group("(", ")", false, [ [ "", __show_usize(value[0]) ], [ "", __show_usize(value[1]) ], [ "", __show_usize(value[2]) ] ]);
}
function __show_bool(value) {
	return "" + value;
}
function __show_bool_str(value) {
	return __dbg_group("(", ")", false, [ [ "", __show_bool(value[0]) ], [ "", __show_str(value[1]) ] ]);
}
function __show_Option_str(value) {
	if (value[0] === 0) {
		return __dbg_group("Some(", ")", false, [ [ "", __show_str(value[1]) ] ]);
	}
	return "None";
}
function __show_Option_str_usize_bool(value) {
	return __dbg_group("(", ")", false, [ [ "", __show_Option_str(value[0]) ], [ "", __show_usize(value[1]) ], [ "", __show_bool(value[2]) ] ]);
}
function __show_i32_i32(value) {
	return __dbg_group("(", ")", false, [ [ "", __show_i32(value[0]) ], [ "", __show_i32(value[1]) ] ]);
}
function __show_i32_i32_i32(value) {
	return __dbg_group("(", ")", false, [ [ "", __show_i32_i32(value.slice(0, 2)) ], [ "", __show_i32(value[2]) ] ]);
}
function __show_i32_i32_i32_i32(value) {
	return __dbg_group("(", ")", false, [ [ "", __show_i32(value[0]) ], [ "", __show_i32_i32(value.slice(1, 3)) ], [ "", __show_i32(value[3]) ] ]);
}
let t = [ 1, "a", [ 1, 2 ] ];
const $a = [ [ 0 ], [ 1 ], [ 2 ] ];
const a = $a[0];
const b = $a[1];
const c = $a[2];
console.log(String(t[a[0]]));
console.log(t[b[0]]);
t[a[0]] = 5;
t[a[0]] = t[a[0]] + 2;
t[b[0]] = t[b[0]] + "z";
t[c[0]][0] = 40;
bump([ t, a[0] ]);
console.log(__dbg_flat(__show_i32_str_P(t)));
const r = t[c[0]];
console.log(String(r[1]));
const $b = at(t, a);
const $c = at(t, a);
$c[0][$c[1]] = $b[0][$b[1]] + 1;
at2(t, c)[1] = 9;
const $d = peek(t, b);
console.log($d[0][$d[1]]);
console.log(__dbg_flat(__show_i32_str_P(t)));
console.log(__dbg_flat(__show_usize_usize_usize([ a[0], b[0], c[0] ])));
console.log(__dbg_flat(__show_bool_str([ eq(a, a), debug(a) ])));
let seen = new2();
const $e = ([ 3, 4 ], [ [ 0 ], [ 1 ] ]);
const p = $e[0];
const q = $e[1];
insert(seen, p, "p");
insert(seen, q, "q");
console.log(__dbg_flat(__show_Option_str_usize_bool([ get(seen, p), len(seen), eq(p, q) ])));
let set = new3();
insert2(set, p);
insert2(set, p);
console.log(String(len2(set)));
let n = [ 1, ...[ 2, 3 ], 4 ];
const $f = [ [ 0 ], [ 1 ], [ 2 ] ];
const n0 = $f[0];
const n1 = $f[1];
const n2 = $f[2];
console.log(__dbg_flat(__show_i32_i32_i32([ ...__clone((($tuple, $offset) => {
	return $tuple.slice($offset, $offset + 2);
})(n, [ 0, 1, 3 ][n1[0]])), n[[ 0, 1, 3 ][n2[0]]] ])));
n[[ 0, 1, 3 ][n2[0]]] = 9;
(($tuple, $offset, $value) => {
	$tuple[$offset + 0] = $value[0];
	$tuple[$offset + 1] = $value[1];
	return $value;
})(n, [ 0, 1, 3 ][n1[0]], [ 7, 8 ]);
n[[ 0, 1, 3 ][n0[0]]] = n[[ 0, 1, 3 ][n0[0]]] + 1;
console.log(__dbg_flat(__show_i32_i32_i32_i32(n)));
//# sourceMappingURL=tuple-keys.mjs.map
