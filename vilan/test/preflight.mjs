function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __map_values(map) {
	return [ ...map.values() ].map(__clone);
}
function token_is_negated(token) {
	return token.startsWith("!");
}
function class_list(self) {
	let out = "";
	for (const entry of $a(self[0])) {
		const $b = entry;
		const class2 = $b[0];
		const _declaration = $b[1];
		if (token_is_negated(class2)) {
			(() => {
				throw "this style carries an unwrapped not(..): `not` marks the condition immediately outside it and emits no rule of its own, so wrap it before applying the style \u{2014} attribute(name, value, not(..)), within(name, value, not(..)), hover(not(..))";
			})();
		}
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
const card = [ [ new Map([ [ "::display", [ "::display", [ "sbiovxm", "display:flex" ] ] ] ]) ] ];
console.log(class_list(card));
