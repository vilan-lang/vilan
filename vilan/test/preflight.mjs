function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __map_values(map) {
	return [ ...map.values() ].map(__clone);
}
function key_is_marked(key) {
	return key.startsWith(":!:") || key.startsWith(":! ");
}
function class_list(self) {
	for (const key of $a(self[0])) {
		if (key_is_marked(key)) {
			(() => {
				throw "this style carries an unwrapped not(..): `not` marks the condition immediately outside it and emits no rule of its own, so wrap it before applying the style \u{2014} attribute(name, value, not(..)), within(name, value, not(..)), hover(not(..))";
			})();
		}
	}
	let out = "";
	for (const entry of $b(self[0])) {
		const $c = entry;
		const class2 = $c[0];
		const _declaration = $c[1];
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
		result.push(__clone(entry[0]));
	}
	return result;
}
function $b(self) {
	let result = [  ];
	for (const entry of __map_values(self[0])) {
		result.push(__clone(entry[1]));
	}
	return result;
}
const card = [ [ new Map([ [ "::display", [ "::display", [ "sbiovxm", "display:flex" ] ] ] ]) ] ];
console.log(class_list(card));
