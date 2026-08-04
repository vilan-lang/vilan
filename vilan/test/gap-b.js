function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function $a(self) {
	const $b = self;
	let $c = null;
	if ($b[0] === 0) {
		const x = $b[1][0];
		const y = $b[1][1];
		$c = [ [ 0, __clone(x) ], [ 0, __clone(y) ] ];
	} else {
		$c = [ [ 1 ], [ 1 ] ];
	}
	return $c;
}
const pair = [ 0, [ 3, 7 ] ];
console.log($a(pair));
const empty = [ 1 ];
console.log($a(empty));
