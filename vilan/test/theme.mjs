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
function $a(self) {
	let result = [  ];
	for (const entry of __map_values(self[0])) {
		result.push(__clone(entry[1]));
	}
	return result;
}
const card = [ [ new Map([ [ "::padding", [ "::padding", [ "s1ufvr2", "padding:var(--space-4)" ] ] ], [ "::color", [ "::color", [ "s8f74a7", "color:var(--color-ink)" ] ] ], [ "::background-color", [ "::background-color", [ "s1caphfa", "background-color:var(--color-ground)" ] ] ], [ "::border", [ "::border", [ "syywdmj", "border:1px solid rgb(from var(--color-ink) r g b / 0.2)" ] ] ] ]) ] ];
console.log(class_list(card));
const lifted = [ [ new Map([ [ "::padding", [ "::padding", [ "s1ufvr2", "padding:var(--space-4)" ] ] ], [ "::color", [ "::color", [ "s8f74a7", "color:var(--color-ink)" ] ] ], [ "::background-color", [ "::background-color", [ "s1caphfa", "background-color:var(--color-ground)" ] ] ], [ "::border", [ "::border", [ "syywdmj", "border:1px solid rgb(from var(--color-ink) r g b / 0.2)" ] ] ], [ "::box-shadow", [ "::box-shadow", [ "s1qtu9wx", "box-shadow:0 1px 2px rgba(0, 0, 0, 0.08)" ] ] ], [ ":^[data-theme=\"iron-dark\"]:box-shadow", [ ":^[data-theme=\"iron-dark\"]:box-shadow", [ "shy6qhb", "box-shadow:none" ] ] ], [ ":^[data-theme=\"iron-dark\"] hover:border-color", [ ":^[data-theme=\"iron-dark\"] hover:border-color", [ "sl22br5", "border-color:var(--color-ink)" ] ] ] ]) ] ];
console.log(class_list(lifted));
