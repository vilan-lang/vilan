function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __nmap_values(table) {
	const out = [];
	for (const entry of table[0].values()) if (entry[1][0] === 0) out.push(entry[1][1]);
	return out.map(__clone);
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
	return __nmap_values(self[0]);
}
const card = [ [ [ new Map([ [ "::padding", [ "::padding", [ 0, [ "s1ufvr2", "padding:var(--space-4)" ] ] ] ], [ "::color", [ "::color", [ 0, [ "s8f74a7", "color:var(--color-ink)" ] ] ] ], [ "::background-color", [ "::background-color", [ 0, [ "s1caphfa", "background-color:var(--color-ground)" ] ] ] ], [ "::border", [ "::border", [ 0, [ "syywdmj", "border:1px solid rgb(from var(--color-ink) r g b / 0.2)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(card));
const iron_dark = [ [ "^[data-theme=\"iron-dark\"]" ] ];
const lifted = [ [ [ new Map([ [ "::padding", [ "::padding", [ 0, [ "s1ufvr2", "padding:var(--space-4)" ] ] ] ], [ "::color", [ "::color", [ 0, [ "s8f74a7", "color:var(--color-ink)" ] ] ] ], [ "::background-color", [ "::background-color", [ 0, [ "s1caphfa", "background-color:var(--color-ground)" ] ] ] ], [ "::border", [ "::border", [ 0, [ "syywdmj", "border:1px solid rgb(from var(--color-ink) r g b / 0.2)" ] ] ] ], [ "::box-shadow", [ "::box-shadow", [ 0, [ "s1qtu9wx", "box-shadow:0 1px 2px rgba(0, 0, 0, 0.08)" ] ] ] ], [ ":^[data-theme=\"iron-dark\"]:box-shadow", [ ":^[data-theme=\"iron-dark\"]:box-shadow", [ 0, [ "shy6qhb", "box-shadow:none" ] ] ] ], [ ":^[data-theme=\"iron-dark\"] hover:border-color", [ ":^[data-theme=\"iron-dark\"] hover:border-color", [ 0, [ "sl22br5", "border-color:var(--color-ink)" ] ] ] ] ]), undefined ] ] ];
console.log(class_list(lifted));
//# sourceMappingURL=theme.mjs.map
