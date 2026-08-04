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
			$h = $i(rules, key, entry);
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
function $i(self, key, value) {
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
