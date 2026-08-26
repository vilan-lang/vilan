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
	const $i = property;
	let $j = null;
	if ($i === "padding") {
		$j = ";padding-top;padding-right;padding-bottom;padding-left;";
	} else if ($i === "margin") {
		$j = ";margin-top;margin-right;margin-bottom;margin-left;";
	} else if ($i === "inset") {
		$j = ";top;right;bottom;left;";
	} else if ($i === "flex") {
		$j = ";flex-grow;flex-shrink;flex-basis;";
	} else if ($i === "background") {
		$j = ";background-color;background-image;background-position;background-size;background-repeat;background-attachment;background-origin;background-clip;";
	} else if ($i === "border") {
		$j = border_longhands();
	} else {
		$j = "";
	}
	return $j;
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
	for (const key of $c(rules)) {
		const parts = key.split(":");
		if (__at(parts, 0) === media && __at(parts, 1) === condition && longhands.includes(";" + __at(parts, 2) + ";")) {
			$k(out, key);
		}
	}
	return out;
}
function class_list(self) {
	let out = "";
	for (const entry of $a(self[0])) {
		const $b = entry;
		const class2 = $b[0];
		const _declaration = $b[1];
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
	for (const key of $c(b[0])) {
		const $g = $d(b[0], key);
		let $h = null;
		if ($g[0] === 0) {
			const entry = $g[1];
			const parts = key.split(":");
			rules = without_covered(rules, __at(parts, 0), __at(parts, 1), __at(parts, 2));
			$l(rules, key, entry);
			$h = undefined;
		} else {
			$h = undefined;
		}
		$h;
	}
	return [ __clone(rules) ];
}
function $a(self) {
	let result = [  ];
	for (const entry of __map_values(self[0])) {
		result.push(__clone(entry[1]));
	}
	return result;
}
function $c(self) {
	let result = [  ];
	for (const entry of __map_values(self[0])) {
		result.push(__clone(entry[0]));
	}
	return result;
}
function $d(self, key) {
	const $e = __map_get(self[0], hash(key));
	let $f = null;
	if ($e[0] === 0) {
		const entry = $e[1];
		$f = [ 0, __clone(entry[1]) ];
	} else {
		$f = [ 1 ];
	}
	return $f;
}
function $k(self, key) {
	self[0].delete(hash(key));
}
function $l(self, key, value) {
	self[0].set(hash(key), [ __clone(key), __clone(value) ]);
}
const card = [ [ new Map([ [ "::display", [ "::display", [ "sbiovxm", "display:flex" ] ] ], [ "::padding", [ "::padding", [ "s1ufvr2", "padding:var(--space-4)" ] ] ], [ "::background-color", [ "::background-color", [ "siolu0w", "background-color:var(--gray-50)" ] ] ], [ ":hover:background-color", [ ":hover:background-color", [ "s1c7l5ao", "background-color:var(--gray-100)" ] ] ] ]) ] ];
const active = [ [ new Map([ [ "::padding", [ "::padding", [ "s1ufvsw", "padding:var(--space-6)" ] ] ] ]) ] ];
console.log(class_list(card));
console.log(class_list(add(card, active)));
const wide = [ [ new Map([ [ "::width", [ "::width", [ "s178hckh", "width:37px" ] ] ] ]) ] ];
console.log(class_list(wide));
const responsive = [ [ new Map([ [ "640px::padding", [ "640px::padding", [ "sl8ru5a", "padding:var(--space-2)" ] ] ], [ "1024px::padding", [ "1024px::padding", [ "s4x9b8s", "padding:var(--space-3)" ] ] ] ]) ] ];
console.log(class_list(responsive));
const themed = [ [ new Map([ [ ":dark:background-color", [ ":dark:background-color", [ "suuxkdy", "background-color:var(--gray-900)" ] ] ], [ ":dark hover:background-color", [ ":dark hover:background-color", [ "s8ww588", "background-color:var(--gray-700)" ] ] ], [ "768px:dark hover:background-color", [ "768px:dark hover:background-color", [ "s8ahr6b", "background-color:var(--gray-50)" ] ] ] ]) ] ];
console.log(class_list(themed));
const translucent = [ [ new Map([ [ "::background-color", [ "::background-color", [ "s12ne3o2", "background-color:rgba(27, 6, 13, 0.9)" ] ] ], [ "::color", [ "::color", [ "s1kwp696", "color:rgb(from var(--gray-900) r g b / 0.08)" ] ] ] ]) ] ];
console.log(class_list(translucent));
const painted = [ [ new Map([ [ "::background-color", [ "::background-color", [ "siolu0w", "background-color:var(--gray-50)" ] ] ], [ "::background-image", [ "::background-image", [ "s1fek3dv", "background-image:linear-gradient(90deg, var(--blue-600) 0%, transparent 100%)" ] ] ], [ ":hover:background-image", [ ":hover:background-image", [ "s1lsiu5q", "background-image:radial-gradient(closest-side, rgba(178, 48, 86, 0.5) 0%, transparent 100%)" ] ] ] ]) ] ];
console.log(class_list(painted));
const framed = [ [ new Map([ [ "::display", [ "::display", [ "s2m9jw6", "display:inline-flex" ] ] ], [ "::border-top", [ "::border-top", [ "s1sb4lgm", "border-top:1px solid var(--gray-300)" ] ] ], [ "::padding-top", [ "::padding-top", [ "stbzxoc", "padding-top:var(--space-2)" ] ] ], [ "::margin-left", [ "::margin-left", [ "s10oplpw", "margin-left:auto" ] ] ] ]) ] ];
console.log(class_list(framed));
console.log("s1mnphwb");
const zeroed = [ [ new Map([ [ "::inset", [ "::inset", [ "s1ucbaf9", "inset:0" ] ] ], [ "::min-width", [ "::min-width", [ "sitgfdt", "min-width:0" ] ] ], [ "::left", [ "::left", [ "s1ypvw5g", "left:clamp(120px, 30%, 185px)" ] ] ], [ "::max-width", [ "::max-width", [ "s63dg6q", "max-width:calc(100% - 2rem)" ] ] ] ]) ] ];
console.log(class_list(zeroed));
const squared = [ [ new Map([ [ "::width", [ "::width", [ "s178h6ec", "width:1rem" ] ] ], [ "::height", [ "::height", [ "s22ylrq", "height:1rem" ] ] ] ]) ] ];
console.log(class_list(squared));
console.log("s178h6ec s22zdhz");
const tiled = [ [ new Map([ [ "::background-image", [ "::background-image", [ "s5hidsk", "background-image:url(tile.png)" ] ] ], [ "::background-size", [ "::background-size", [ "skugn91", "background-size:120px 120px" ] ] ], [ "::line-height", [ "::line-height", [ "snq90yh", "line-height:24px" ] ] ] ]) ] ];
console.log(class_list(tiled));
