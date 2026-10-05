function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function unzip_pair(self) {
	const $a = self;
	let $b = null;
	if ($a[0] === 0) {
		const x = $a[1][0];
		const y = $a[1][1];
		$b = [ [ 0, __clone(x) ], [ 0, __clone(y) ] ];
	} else {
		$b = [ [ 1 ], [ 1 ] ];
	}
	return $b;
}
const pair = [ 0, [ 3, 7 ] ];
console.log(unzip_pair(pair));
const empty = [ 1 ];
console.log(unzip_pair(empty));
