function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __map_values(map) {
	return [ ...map.values() ].map(__clone);
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
function values(self) {
	return __map_values(self[1]);
}
const card = [ [ new Map([ [ "::display", "::display" ] ]), new Map([ [ "::display", [ "sbiovxm", "display:flex" ] ] ]) ] ];
console.log(class_list(card));
//# sourceMappingURL=preflight.mjs.map
