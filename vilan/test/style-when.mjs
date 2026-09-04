function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
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
function hash(self) {
	return __hash(self);
}
function family_longhands(property) {
	const $g = property;
	let $h = null;
	if ($g === "padding") {
		$h = ";padding-top;padding-right;padding-bottom;padding-left;";
	} else if ($g === "margin") {
		$h = ";margin-top;margin-right;margin-bottom;margin-left;";
	} else if ($g === "inset") {
		$h = ";top;right;bottom;left;";
	} else if ($g === "flex") {
		$h = ";flex-grow;flex-shrink;flex-basis;";
	} else if ($g === "background") {
		$h = ";background-color;background-image;background-position;background-size;background-repeat;background-attachment;background-origin;background-clip;";
	} else if ($g === "border") {
		$h = border_longhands();
	} else {
		$h = "";
	}
	return $h;
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
	for (const key of $a(rules)) {
		const parts = key.split(":");
		if (__at(parts, 0) === media && __at(parts, 1) === condition && longhands.includes(";" + __at(parts, 2) + ";")) {
			$i(out, key);
		}
	}
	return out;
}
function when(self, condition, delta) {
	let $k = null;
	if (condition) {
		$k = add(self, delta);
	} else {
		$k = __clone(self);
	}
	return $k;
}
function class_list(self) {
	let out = "";
	for (const entry of $l(self[0])) {
		const $m = entry;
		const class2 = $m[0];
		const _declaration = $m[1];
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
	for (const key of $a(b[0])) {
		const $e = $b(b[0], key);
		let $f = null;
		if ($e[0] === 0) {
			const entry = $e[1];
			const parts = key.split(":");
			rules = without_covered(rules, __at(parts, 0), __at(parts, 1), __at(parts, 2));
			$j(rules, key, entry);
			$f = undefined;
		} else {
			$f = undefined;
		}
		$f;
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
function $a(self) {
	let result = [  ];
	for (const entry of __map_values(self[0])) {
		result.push(__clone(entry[0]));
	}
	return result;
}
function $b(self, key) {
	const $c = __map_get(self[0], hash(key));
	let $d = null;
	if ($c[0] === 0) {
		const entry = $c[1];
		$d = [ 0, __clone(entry[1]) ];
	} else {
		$d = [ 1 ];
	}
	return $d;
}
function $i(self, key) {
	self[0].delete(hash(key));
}
function $j(self, key, value) {
	self[0].set(hash(key), [ __clone(key), __clone(value) ]);
}
function $l(self) {
	let result = [  ];
	for (const entry of __map_values(self[0])) {
		result.push(__clone(entry[1]));
	}
	return result;
}
const base = [ [ new Map([ [ "::padding", [ "::padding", [ "s1ufvp8", "padding:var(--space-2)" ] ] ], [ "::color", [ "::color", [ "s1hbuywq", "color:var(--gray-900)" ] ] ], [ "::background-color", [ "::background-color", [ "sdoeicu", "background-color:#ffffff" ] ] ] ]) ] ];
const chosen = [ [ new Map([ [ "::color", [ "::color", [ "s1ip1dgv", "color:var(--blue-900)" ] ] ], [ "::background-color", [ "::background-color", [ "s1do7ev5", "background-color:var(--blue-100)" ] ] ] ]) ] ];
const muted = [ [ new Map([ [ "::color", [ "::color", [ "s1hbr49h", "color:var(--gray-400)" ] ] ] ]) ] ];
let cell = 0;
while (cell < 4) {
	const is_chosen = cell % 2 === 1;
	const is_muted = Math.trunc(cell / 2) === 1;
	const chain = chained(is_chosen, is_muted);
	const sum = built(is_chosen, is_muted);
	console.log("cell " + cell + " same=" + (chain === sum) + " classes=" + chain);
	cell = cell + 1;
}
