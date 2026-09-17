function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __force(cell) {
	if (cell.state === 2) return cell.value;
	if (cell.state === 1) throw "lazy initialization cycle: `" + cell.name + "`";
	if (cell.state === 3) throw "lazy `" + cell.name + "` is poisoned: its initializer panicked: " + cell.value;
	cell.state = 1;
	try {
		cell.value = cell.thunk();
	} catch (failure) {
		cell.state = 3;
		cell.value = failure;
		throw failure;
	}
	cell.state = 2;
	cell.thunk = null;
	return cell.value;
}
function __lazy(name, thunk) {
	return { name: name, state: 0, value: undefined, thunk: thunk };
}
function __parse_f64(text) {
	const trimmed = text.trim();
	const value = Number(trimmed);
	return trimmed === "" || Number.isNaN(value) ? [ 1 ] : [ 0, value ];
}
function $a(self, fallback) {
	const $b = self;
	let $c = null;
	if ($b[0] === 0) {
		const x = __clone($b[1]);
		$c = x;
	} else {
		$c = __clone(__force(fallback));
	}
	return $c;
}
function $d(self) {
	const $e = self;
	return $e[0] === 0;
}
console.log($a(__parse_f64("3.14"), __lazy("fallback", () => {
	return 0;
})));
console.log($a(__parse_f64("42"), __lazy("fallback", () => {
	return 0;
})));
console.log($a(__parse_f64("-2.5"), __lazy("fallback", () => {
	return 0;
})));
console.log($a(__parse_f64("nope"), __lazy("fallback", () => {
	return -(1);
})));
console.log($d(__parse_f64("3.14")));
console.log($d(__parse_f64("abc")));
