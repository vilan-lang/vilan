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
function __nmap_keys(table) {
	const out = [];
	for (const entry of table[0].values()) if (entry[1][0] === 0) out.push(entry[0]);
	return out.map(__clone);
}
function __nmap_remove(table, hash) {
	__nmap_settle(table);
	table[0].delete(hash);
}
function __nmap_settle(table) {
	const hash = table[1];
	if (hash === undefined) return;
	table[1] = undefined;
	const entry = table[0].get(hash);
	if (entry !== undefined && entry[1][0] === 1) table[0].delete(hash);
}
function __nmap_values(table) {
	const out = [];
	for (const entry of table[0].values()) if (entry[1][0] === 0) out.push(entry[1][1]);
	return out.map(__clone);
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
function slot_of(key) {
	const parts = key.split(":");
	if (parts.length !== 3) {
		(() => {
			throw __panic("this style\'s slot key is not one media:condition:property triple (got \"" + key + "\"" + ") \u{2014} every field that reaches a key is fenced against \':\' where it is written, so a key holding another one means a condition token was minted carrying the key\'s own separator; that is the bug, not this read", "std/src/web/style.vl:1305:3");
		})();
	}
	return [ __at(parts, 0, "std/src/web/style.vl:1307:17"), __at(parts, 1, "std/src/web/style.vl:1307:39"), __at(parts, 2, "std/src/web/style.vl:1307:60") ];
}
function family_longhands(property) {
	const $c = property;
	let $d = null;
	if ($c === "padding") {
		$d = ";padding-top;padding-right;padding-bottom;padding-left;";
	} else if ($c === "margin") {
		$d = ";margin-top;margin-right;margin-bottom;margin-left;";
	} else if ($c === "inset") {
		$d = ";top;right;bottom;left;";
	} else if ($c === "flex") {
		$d = ";flex-grow;flex-shrink;flex-basis;";
	} else if ($c === "background") {
		$d = ";background-color;background-image;background-position;background-size;background-repeat;background-attachment;background-origin;background-clip;";
	} else if ($c === "border") {
		$d = border_longhands();
	} else {
		$d = "";
	}
	return $d;
}
function border_longhands() {
	let out = ";border-width;border-style;border-color;";
	for (const edge of [ "top", "right", "bottom", "left" ]) {
		out = out + ("border-" + edge + ";");
		for (const part of [ "width", "style", "color" ]) {
			out = out + ("border-" + edge + "-" + part + ";");
		}
	}
	return out;
}
function without_covered(rules, media, condition, property) {
	const longhands = family_longhands(property);
	if (longhands === "") {
		return __clone(rules);
	}
	let out = __clone(rules);
	for (const key of keys(rules)) {
		const slot = slot_of(key);
		if (slot[0] === media && slot[1] === condition && longhands.includes(";" + slot[2] + ";")) {
			remove(out, key);
		}
	}
	return out;
}
function when(self, condition, delta) {
	let $e = null;
	if (condition) {
		$e = add(self, delta);
	} else {
		$e = __clone(self);
	}
	return $e;
}
function class_list(self) {
	let out = "";
	for (const entry of values(self[0])) {
		const $f = entry;
		const class2 = $f[0];
		const _declaration = $f[1];
		if (out === "") {
			out = class2;
		} else {
			out = out + " " + class2;
		}
	}
	return out;
}
function add(self, b) {
	let rules = __clone(self[0]);
	for (const key of keys(b[0])) {
		const $a = get(b[0], key);
		let $b = null;
		if ($a[0] === 0) {
			const entry = $a[1];
			const slot = slot_of(key);
			rules = without_covered(rules, slot[0], slot[1], slot[2]);
			insert(rules, key, entry);
			$b = undefined;
		} else {
			$b = undefined;
		}
		$b;
	}
	return [ rules ];
}
function chained(is_chosen2, is_muted2) {
	return class_list(when(when(base, is_chosen2, chosen), is_muted2, muted));
}
function built(is_chosen2, is_muted2) {
	let out = __clone(base);
	if (is_chosen2) {
		out = add(out, chosen);
	}
	if (is_muted2) {
		out = add(out, muted);
	}
	return class_list(out);
}
function keys(self) {
	return __nmap_keys(self[0]);
}
function get(self, key) {
	return __nmap_get(self[0], hash(key));
}
function remove(self, key) {
	__nmap_remove(self[0], hash(key));
}
function insert(self, key, value) {
	__nmap_insert(self[0], hash(key), __clone(key), __clone(value));
}
function values(self) {
	return __nmap_values(self[0]);
}
const base = [ [ [ new Map([ [ "::padding", [ "::padding", [ 0, [ "s1ufvp8", "padding:var(--space-2)" ] ] ] ], [ "::color", [ "::color", [ 0, [ "s1hbuywq", "color:var(--gray-900)" ] ] ] ], [ "::background-color", [ "::background-color", [ 0, [ "sdoeicu", "background-color:#ffffff" ] ] ] ] ]), undefined ] ] ];
const chosen = [ [ [ new Map([ [ "::color", [ "::color", [ 0, [ "s1ip1dgv", "color:var(--blue-900)" ] ] ] ], [ "::background-color", [ "::background-color", [ 0, [ "s1do7ev5", "background-color:var(--blue-100)" ] ] ] ] ]), undefined ] ] ];
const muted = [ [ [ new Map([ [ "::color", [ "::color", [ 0, [ "s1hbr49h", "color:var(--gray-400)" ] ] ] ] ]), undefined ] ] ];
let cell = 0;
while (cell < 4) {
	const is_chosen = cell % 2 === 1;
	const is_muted = Math.trunc(cell / 2) === 1;
	const chain = chained(is_chosen, is_muted);
	const sum = built(is_chosen, is_muted);
	console.log("cell " + cell + " same=" + (chain === sum) + " classes=" + chain);
	cell = cell + 1;
}
//# sourceMappingURL=style-when.mjs.map
