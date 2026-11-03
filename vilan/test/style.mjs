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
	for (const entry of table[0].values()) if (entry[1][0] === 0) out.push(__clone(entry[0]));
	return out;
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
	for (const entry of table[0].values()) if (entry[1][0] === 0) out.push(__clone(entry[1][1]));
	return out;
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
			throw __panic("this style\'s slot key is not one media:condition:property triple (got \"" + key + "\"" + ") \u{2014} every field that reaches a key is fenced against \':\' where it is written, so a key holding another one means a condition token was minted carrying the key\'s own separator; that is the bug, not this read", "std/src/web/style.vl:1243:3");
		})();
	}
	return [ __at(parts, 0, "std/src/web/style.vl:1245:17"), __at(parts, 1, "std/src/web/style.vl:1245:39"), __at(parts, 2, "std/src/web/style.vl:1245:60") ];
}
function family_longhands(property) {
	const $d = property;
	let $e = null;
	if ($d === "padding") {
		$e = ";padding-top;padding-right;padding-bottom;padding-left;";
	} else if ($d === "margin") {
		$e = ";margin-top;margin-right;margin-bottom;margin-left;";
	} else if ($d === "inset") {
		$e = ";top;right;bottom;left;";
	} else if ($d === "flex") {
		$e = ";flex-grow;flex-shrink;flex-basis;";
	} else if ($d === "background") {
		$e = ";background-color;background-image;background-position;background-size;background-repeat;background-attachment;background-origin;background-clip;";
	} else if ($d === "border") {
		$e = border_longhands();
	} else {
		$e = "";
	}
	return $e;
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
function class_list(self) {
	let out = "";
	for (const entry of values(self[0])) {
		const $a = entry;
		const class2 = $a[0];
		const _declaration = $a[1];
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
		const $b = get(b[0], key);
		let $c = null;
		if ($b[0] === 0) {
			const entry = $b[1];
			const slot = slot_of(key);
			rules = without_covered(rules, slot[0], slot[1], slot[2]);
			insert(rules, key, entry);
			$c = undefined;
		} else {
			$c = undefined;
		}
		$c;
	}
	return [ rules ];
}
function values(self) {
	return __nmap_values(self[0]);
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
const card = [ [ [ new Map([ [ "::display", [ "::display", [ 0, [ "sbiovxm", "display:flex" ] ] ] ], [ "::padding", [ "::padding", [ 0, [ "s1ufvr2", "padding:var(--space-4)" ] ] ] ], [ "::background-color", [ "::background-color", [ 0, [ "siolu0w", "background-color:var(--gray-50)" ] ] ] ], [ ":hover:background-color", [ ":hover:background-color", [ 0, [ "s1c7l5ao", "background-color:var(--gray-100)" ] ] ] ] ]), undefined ] ] ];
const active = [ [ [ new Map([ [ "::padding", [ "::padding", [ 0, [ "s1ufvsw", "padding:var(--space-6)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(card));
console.log(class_list(add(card, active)));
const wide = [ [ [ new Map([ [ "::width", [ "::width", [ 0, [ "s178hckh", "width:37px" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(wide));
const responsive = [ [ [ new Map([ [ "640px::padding", [ "640px::padding", [ 0, [ "sl8ru5a", "padding:var(--space-2)" ] ] ] ], [ "1024px::padding", [ "1024px::padding", [ 0, [ "s4x9b8s", "padding:var(--space-3)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(responsive));
const dark = [ [ "^[data-theme=\"dark\"]" ] ];
const themed = [ [ [ new Map([ [ "768px:^[data-theme=\"dark\"] hover:background-color", [ "768px:^[data-theme=\"dark\"] hover:background-color", [ 0, [ "sayz5ok", "background-color:var(--gray-50)" ] ] ] ], [ ":^[data-theme=\"dark\"]:background-color", [ ":^[data-theme=\"dark\"]:background-color", [ 0, [ "s120e6x3", "background-color:var(--gray-900)" ] ] ] ], [ ":^[data-theme=\"dark\"] hover:background-color", [ ":^[data-theme=\"dark\"] hover:background-color", [ 0, [ "ss3fj9l", "background-color:var(--gray-700)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(themed));
const translucent = [ [ [ new Map([ [ "::color", [ "::color", [ 0, [ "s1kwp696", "color:rgb(from var(--gray-900) r g b / 0.08)" ] ] ] ], [ "::background-color", [ "::background-color", [ 0, [ "s12ne3o2", "background-color:rgba(27, 6, 13, 0.9)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(translucent));
const painted = [ [ [ new Map([ [ "::background-color", [ "::background-color", [ 0, [ "siolu0w", "background-color:var(--gray-50)" ] ] ] ], [ "::background-image", [ "::background-image", [ 0, [ "s1fek3dv", "background-image:linear-gradient(90deg, var(--blue-600) 0%, transparent 100%)" ] ] ] ], [ ":hover:background-image", [ ":hover:background-image", [ 0, [ "s1lsiu5q", "background-image:radial-gradient(closest-side, rgba(178, 48, 86, 0.5) 0%, transparent 100%)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(painted));
const framed = [ [ [ new Map([ [ "::display", [ "::display", [ 0, [ "s2m9jw6", "display:inline-flex" ] ] ] ], [ "::padding-top", [ "::padding-top", [ 0, [ "stbzxoc", "padding-top:var(--space-2)" ] ] ] ], [ "::margin-left", [ "::margin-left", [ 0, [ "s10oplpw", "margin-left:auto" ] ] ] ], [ "::border-top", [ "::border-top", [ 0, [ "s1sb4lgm", "border-top:1px solid var(--gray-300)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(framed));
console.log("s1mnphwb");
const zeroed = [ [ [ new Map([ [ "::inset", [ "::inset", [ 0, [ "s1ucbaf9", "inset:0" ] ] ] ], [ "::left", [ "::left", [ 0, [ "s1ypvw5g", "left:clamp(120px, 30%, 185px)" ] ] ] ], [ "::min-width", [ "::min-width", [ 0, [ "sitgfdt", "min-width:0" ] ] ] ], [ "::max-width", [ "::max-width", [ 0, [ "s63dg6q", "max-width:calc(100% - 2rem)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(zeroed));
const accented = [ [ [ new Map([ [ "::background-color", [ "::background-color", [ 0, [ "sumjtxl", "background-color:var(--accent)" ] ] ] ], [ "::border-color", [ "::border-color", [ 0, [ "s1rhkqzm", "border-color:rgb(from var(--accent) r g b / 0.5)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(accented));
const perceptual = [ [ [ new Map([ [ "::color", [ "::color", [ 0, [ "s1ouskes", "color:rgb(from oklch(0.97 0.02 340) r g b / 0.8)" ] ] ] ], [ "::background-color", [ "::background-color", [ 0, [ "s1yxmxe6", "background-color:oklch(0.62 0.19 313)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(perceptual));
const squared = [ [ [ new Map([ [ "::width", [ "::width", [ 0, [ "s178h6ec", "width:1rem" ] ] ] ], [ "::height", [ "::height", [ 0, [ "s22ylrq", "height:1rem" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(squared));
console.log("s178h6ec s22zdhz");
const open = [ [ "[data-open=\"true\"]" ] ];
const disclosed = [ [ [ new Map([ [ "768px:^[data-theme=\"dark\"] [data-open=\"true\"] hover:color", [ "768px:^[data-theme=\"dark\"] [data-open=\"true\"] hover:color", [ 0, [ "s1dwvy7w", "color:var(--gray-50)" ] ] ] ], [ ":^[data-theme=\"dark\"] [data-open=\"true\"]:background-color", [ ":^[data-theme=\"dark\"] [data-open=\"true\"]:background-color", [ 0, [ "s1jqpl5k", "background-color:var(--gray-800)" ] ] ] ], [ ":[data-open=\"true\"]:display", [ ":[data-open=\"true\"]:display", [ 0, [ "s11vlq4c", "display:flex" ] ] ] ], [ ":[data-open=\"true\"] hover:background-color", [ ":[data-open=\"true\"] hover:background-color", [ 0, [ "s1j5l6ea", "background-color:var(--gray-100)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(disclosed));
const gated = [ [ [ new Map([ [ ":!^[data-theme=\"dark\"]:background-color", [ ":!^[data-theme=\"dark\"]:background-color", [ 0, [ "s1jayb7s", "background-color:var(--gray-900)" ] ] ] ], [ ":^[data-collapsed]:display", [ ":^[data-collapsed]:display", [ 0, [ "s4ss8fz", "display:none" ] ] ] ], [ ":[data-selected]:color", [ ":[data-selected]:color", [ 0, [ "st2ig1a", "color:var(--gray-50)" ] ] ] ], [ ":![disabled] hover:background-color", [ ":![disabled] hover:background-color", [ 0, [ "s1pk9f1d", "background-color:var(--gray-100)" ] ] ] ], [ ":!hover:color", [ ":!hover:color", [ 0, [ "s6t6iu5", "color:var(--gray-700)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(gated));
const stacked = [ [ [ new Map([ [ "768px:>*:gap", [ "768px:>*:gap", [ 0, [ "svmscer", "gap:var(--space-2)" ] ] ] ], [ ":>*:margin-top", [ ":>*:margin-top", [ 0, [ "sxzag36", "margin-top:var(--space-2)" ] ] ] ], [ ":>*+*:margin-top", [ ":>*+*:margin-top", [ 0, [ "skr9oll", "margin-top:var(--space-4)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(stacked));
const tiled = [ [ [ new Map([ [ "::line-height", [ "::line-height", [ 0, [ "snq90yh", "line-height:24px" ] ] ] ], [ "::background-image", [ "::background-image", [ 0, [ "s5hidsk", "background-image:url(tile.png)" ] ] ] ], [ "::background-size", [ "::background-size", [ 0, [ "skugn91", "background-size:120px 120px" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(tiled));
//# sourceMappingURL=style.mjs.map
