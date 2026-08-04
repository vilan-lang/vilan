function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __parse_i32(text) {
	const trimmed = text.trim();
	const value = Number(trimmed);
	return /^[+-]?[0-9]+$/.test(trimmed) && value >= -2147483648 && value <= 2147483647 ? [ 0, value ] : [ 1 ];
}
function $a(self, fallback) {
	const $b = self;
	let $c = null;
	if ($b[0] === 0) {
		const x = __clone($b[1]);
		$c = x;
	} else {
		$c = fallback;
	}
	return $c;
}
function $d(self) {
	const $e = self;
	return $e[0] === 0;
}
console.log($a(__parse_i32("42"), 0 - 1));
console.log($a(__parse_i32("-7"), 0));
console.log($a(__parse_i32("+9"), 0));
console.log($a(__parse_i32(" 42 "), 0 - 1));
console.log($d(__parse_i32("")));
console.log($d(__parse_i32("abc")));
console.log($d(__parse_i32("1.5")));
console.log($d(__parse_i32("12x")));
console.log($a(__parse_i32("2147483647"), 0));
console.log($d(__parse_i32("2147483648")));
console.log($a(__parse_i32("-2147483648"), 0));
console.log($d(__parse_i32("-2147483649")));
