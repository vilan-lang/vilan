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
const block = [ [ new Map([ [ "::display", [ "::display", [ "sbiovxm", "display:flex" ] ] ], [ "::flex-direction", [ "::flex-direction", [ "s1atdsbb", "flex-direction:column" ] ] ], [ "::gap", [ "::gap", [ "s8myyrk", "gap:var(--space-4)" ] ] ], [ "::padding", [ "::padding", [ "s1ufvr2", "padding:var(--space-4)" ] ] ], [ "::background-color", [ "::background-color", [ "siolu0w", "background-color:var(--gray-50)" ] ] ], [ "::border-radius", [ "::border-radius", [ "s94jklx", "border-radius:8px" ] ] ], [ "768px::padding", [ "768px::padding", [ "s1wyflm5", "padding:var(--space-6)" ] ] ], [ ":hover:background-color", [ ":hover:background-color", [ "s1c7l5ao", "background-color:var(--gray-100)" ] ] ] ]) ] ];
const chain = [ [ new Map([ [ "::display", [ "::display", [ "sbiovxm", "display:flex" ] ] ], [ "::flex-direction", [ "::flex-direction", [ "s1atdsbb", "flex-direction:column" ] ] ], [ "::gap", [ "::gap", [ "s8myyrk", "gap:var(--space-4)" ] ] ], [ "::padding", [ "::padding", [ "s1ufvr2", "padding:var(--space-4)" ] ] ], [ "::background-color", [ "::background-color", [ "siolu0w", "background-color:var(--gray-50)" ] ] ], [ "::border-radius", [ "::border-radius", [ "s94jklx", "border-radius:8px" ] ] ], [ "768px::padding", [ "768px::padding", [ "s1wyflm5", "padding:var(--space-6)" ] ] ], [ ":hover:background-color", [ ":hover:background-color", [ "s1c7l5ao", "background-color:var(--gray-100)" ] ] ] ]) ] ];
console.log(class_list(block));
console.log(class_list(chain));
console.log("s9bu6v3 sgdl28p sw0ajwn sflnbwj s16sw83c s1e7dqf5 s17s8g64");
console.log("s1hbuywq s3s9k3d scur295 sxzag36 skr9oll s1dwvy7w");
const wider = [ [ new Map([ [ "::padding", [ "::padding", [ "s1ufvsw", "padding:var(--space-6)" ] ] ] ]) ] ];
console.log(class_list(add(block, wider)));
