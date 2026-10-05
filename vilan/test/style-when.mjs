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
function __map_get(map, key) {
	return map.has(key) ? [ 0, __clone(map.get(key)) ] : [ 1 ];
}
function __map_values(map) {
	return [ ...map.values() ].map(__clone);
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
			throw __panic("this style\'s slot key is not one media:condition:property triple (got \"" + key + "\"" + ") \u{2014} every field that reaches a key is fenced against \':\' where it is written, so a key holding another one means a condition token was minted carrying the key\'s own separator; that is the bug, not this read", "std/src/web/style.vl:873:3");
		})();
	}
	return [ __at(parts, 0, "std/src/web/style.vl:875:17"), __at(parts, 1, "std/src/web/style.vl:875:39"), __at(parts, 2, "std/src/web/style.vl:875:60") ];
}
function family_longhands(property) {
	const $e = property;
	let $f = null;
	if ($e === "padding") {
		$f = ";padding-top;padding-right;padding-bottom;padding-left;";
	} else if ($e === "margin") {
		$f = ";margin-top;margin-right;margin-bottom;margin-left;";
	} else if ($e === "inset") {
		$f = ";top;right;bottom;left;";
	} else if ($e === "flex") {
		$f = ";flex-grow;flex-shrink;flex-basis;";
	} else if ($e === "background") {
		$f = ";background-color;background-image;background-position;background-size;background-repeat;background-attachment;background-origin;background-clip;";
	} else if ($e === "border") {
		$f = border_longhands();
	} else {
		$f = "";
	}
	return $f;
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
	let $g = null;
	if (condition) {
		$g = add(self, delta);
	} else {
		$g = __clone(self);
	}
	return $g;
}
function class_list(self) {
	let out = "";
	for (const entry of values(self[0])) {
		const $h = entry;
		const class2 = $h[0];
		const _declaration = $h[1];
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
		const $c = get(b[0], key);
		let $d = null;
		if ($c[0] === 0) {
			const entry = $c[1];
			const slot = slot_of(key);
			rules = without_covered(rules, slot[0], slot[1], slot[2]);
			insert(rules, key, entry);
			$d = undefined;
		} else {
			$d = undefined;
		}
		$d;
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
	let result = [  ];
	for (const entry of __map_values(self[0])) {
		result.push(__clone(entry[0]));
	}
	return result;
}
function get(self, key) {
	const $a = __map_get(self[0], hash(key));
	let $b = null;
	if ($a[0] === 0) {
		const entry = $a[1];
		$b = [ 0, __clone(entry.slice(1, 3)) ];
	} else {
		$b = [ 1 ];
	}
	return $b;
}
function remove(self, key) {
	self[0].delete(hash(key));
}
function insert(self, key, value) {
	self[0].set(hash(key), [ __clone(key), ...__clone(value) ]);
}
function values(self) {
	let result = [  ];
	for (const entry of __map_values(self[0])) {
		result.push(__clone(entry.slice(1, 3)));
	}
	return result;
}
const base = [ [ new Map([ [ "::padding", [ "::padding", "s1ufvp8", "padding:var(--space-2)" ] ], [ "::color", [ "::color", "s1hbuywq", "color:var(--gray-900)" ] ], [ "::background-color", [ "::background-color", "sdoeicu", "background-color:#ffffff" ] ] ]) ] ];
const chosen = [ [ new Map([ [ "::color", [ "::color", "s1ip1dgv", "color:var(--blue-900)" ] ], [ "::background-color", [ "::background-color", "s1do7ev5", "background-color:var(--blue-100)" ] ] ]) ] ];
const muted = [ [ new Map([ [ "::color", [ "::color", "s1hbr49h", "color:var(--gray-400)" ] ] ]) ] ];
let cell = 0;
while (cell < 4) {
	const is_chosen = cell % 2 === 1;
	const is_muted = Math.trunc(cell / 2) === 1;
	const chain = chained(is_chosen, is_muted);
	const sum = built(is_chosen, is_muted);
	console.log("cell " + cell + " same=" + (chain === sum) + " classes=" + chain);
	cell = cell + 1;
}
