function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function unwrap(self) {
	return __clone(self[0]);
}
function peek(self) {
	return [ 0, __clone(self[0]) ];
}
const b = [ [ 5 ] ];
console.log(String(unwrap(b)[0]));
const $a = peek(b);
let $b = null;
if ($a[0] === 0) {
	const n = $a[1];
	$b = console.log(String(n[0]));
} else {
	$b = console.log(String(-(1)));
}
process.exit($b);
