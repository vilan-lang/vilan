function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function $a(self) {
	return __clone(self[0]);
}
function $b(self) {
	return [ 0, __clone(self[0]) ];
}
const b = [ [ 5 ] ];
console.log($a(b)[0]);
const $c = $b(b);
let $d = null;
if ($c[0] === 0) {
	const n = $c[1];
	$d = console.log(n[0]);
} else {
	$d = console.log(-(1));
}
process.exit($d);
