function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __force(cell) {
	if (cell.state === 2) return cell.value;
	if (cell.state === 1) throw "lazy initialization cycle: `" + cell.name + "`";
	if (cell.state === 3) throw "lazy `" + cell.name + "` is poisoned: its initializer panicked: " + (cell.value && cell.value.location !== undefined ? cell.value.message : cell.value);
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
function __parse_i32(text) {
	const trimmed = text.trim();
	const value = Number(trimmed);
	return /^[+-]?[0-9]+$/.test(trimmed) && value >= -2147483648 && value <= 2147483647 ? [ 0, value ] : [ 1 ];
}
function unwrap_or(self, fallback) {
	const $a = self;
	let $b = null;
	if ($a[0] === 0) {
		const x = __clone($a[1]);
		$b = x;
	} else {
		$b = __clone(__force(fallback));
	}
	return $b;
}
function is_some(self) {
	const $c = self;
	return $c[0] === 0;
}
console.log(String(unwrap_or(__parse_i32("42"), __lazy("fallback", () => {
	return 0 - 1;
}))));
console.log(String(unwrap_or(__parse_i32("-7"), __lazy("fallback", () => {
	return 0;
}))));
console.log(String(unwrap_or(__parse_i32("+9"), __lazy("fallback", () => {
	return 0;
}))));
console.log(String(unwrap_or(__parse_i32(" 42 "), __lazy("fallback", () => {
	return 0 - 1;
}))));
console.log(is_some(__parse_i32("")));
console.log(is_some(__parse_i32("abc")));
console.log(is_some(__parse_i32("1.5")));
console.log(is_some(__parse_i32("12x")));
console.log(String(unwrap_or(__parse_i32("2147483647"), __lazy("fallback", () => {
	return 0;
}))));
console.log(is_some(__parse_i32("2147483648")));
console.log(String(unwrap_or(__parse_i32("-2147483648"), __lazy("fallback", () => {
	return 0;
}))));
console.log(is_some(__parse_i32("-2147483649")));
