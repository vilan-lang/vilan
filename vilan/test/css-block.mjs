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
const block = [ [ [ new Map([ [ "::display", [ "::display", [ 0, [ "sbiovxm", "display:flex" ] ] ] ], [ "::flex-direction", [ "::flex-direction", [ 0, [ "s1atdsbb", "flex-direction:column" ] ] ] ], [ "::gap", [ "::gap", [ 0, [ "s8myyrk", "gap:var(--space-4)" ] ] ] ], [ "::padding", [ "::padding", [ 0, [ "s1ufvr2", "padding:var(--space-4)" ] ] ] ], [ "::background-color", [ "::background-color", [ 0, [ "siolu0w", "background-color:var(--gray-50)" ] ] ] ], [ "::border-radius", [ "::border-radius", [ 0, [ "s94jklx", "border-radius:8px" ] ] ] ], [ "768px::padding", [ "768px::padding", [ 0, [ "s1wyflm5", "padding:var(--space-6)" ] ] ] ], [ ":hover:background-color", [ ":hover:background-color", [ 0, [ "s1c7l5ao", "background-color:var(--gray-100)" ] ] ] ] ]), undefined ] ] ];
const chain = [ [ [ new Map([ [ "::display", [ "::display", [ 0, [ "sbiovxm", "display:flex" ] ] ] ], [ "::flex-direction", [ "::flex-direction", [ 0, [ "s1atdsbb", "flex-direction:column" ] ] ] ], [ "::gap", [ "::gap", [ 0, [ "s8myyrk", "gap:var(--space-4)" ] ] ] ], [ "::padding", [ "::padding", [ 0, [ "s1ufvr2", "padding:var(--space-4)" ] ] ] ], [ "::background-color", [ "::background-color", [ 0, [ "siolu0w", "background-color:var(--gray-50)" ] ] ] ], [ "::border-radius", [ "::border-radius", [ 0, [ "s94jklx", "border-radius:8px" ] ] ] ], [ "768px::padding", [ "768px::padding", [ 0, [ "s1wyflm5", "padding:var(--space-6)" ] ] ] ], [ ":hover:background-color", [ ":hover:background-color", [ 0, [ "s1c7l5ao", "background-color:var(--gray-100)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(block));
console.log(class_list(chain));
console.log("sflnbwj sgdl28p sw0ajwn s9bu6v3 s16sw83c s1e7dqf5 s17s8g64");
console.log("s1hbuywq s1dwvy7w s3s9k3d scur295 sxzag36 skr9oll");
const wider = [ [ [ new Map([ [ "::padding", [ "::padding", [ 0, [ "s1ufvsw", "padding:var(--space-6)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(add(block, wider)));
//# sourceMappingURL=css-block.mjs.map
