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
console.log($a(__parse_f64("3.14"), 0));
console.log($a(__parse_f64("42"), 0));
console.log($a(__parse_f64("-2.5"), 0));
console.log($a(__parse_f64("nope"), -(1)));
console.log($d(__parse_f64("3.14")));
console.log($d(__parse_f64("abc")));
