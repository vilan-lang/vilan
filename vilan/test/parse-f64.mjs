function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __parse_f64(text) {
	const trimmed = text.trim();
	const value = Number(trimmed);
	return trimmed === "" || Number.isNaN(value) ? [ 1 ] : [ 0, value ];
}
function unwrap_or(self, fallback) {
	const $a = self;
	let $b = null;
	if ($a[0] === 0) {
		const x = __clone($a[1]);
		$b = x;
	} else {
		$b = __clone(fallback);
	}
	return $b;
}
function is_some(self) {
	const $c = self;
	return $c[0] === 0;
}
console.log(unwrap_or(__parse_f64("3.14"), 0));
console.log(unwrap_or(__parse_f64("42"), 0));
console.log(unwrap_or(__parse_f64("-2.5"), 0));
console.log(unwrap_or(__parse_f64("nope"), -(1)));
console.log(is_some(__parse_f64("3.14")));
console.log(is_some(__parse_f64("abc")));
