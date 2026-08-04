function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function sum_over(entries2) {
	let total = 0;
	for (const entry of entries2) {
		const $a = entry;
		let $b = null;
		const a = $a[0];
		const b = $a[1];
		total = total + a + b;
		$b = undefined;
		$b;
	}
	return total;
}
function first_or(held2, fallback) {
	const $c = held2;
	let $d = null;
	if ($c[0] === 0) {
		const inner2 = __clone($c[1]);
		$d = inner2;
	} else {
		$d = fallback;
	}
	return $d;
}
let entries = [  ];
entries.push([ 1, 2 ]);
entries.push([ 10, 20 ]);
console.log(sum_over(entries));
const held = [ 0, [ 1, 2 ] ];
let got = first_or(held, [  ]);
got.push(9);
console.log(got.length);
const $e = held;
let $f = null;
if ($e[0] === 0) {
	const inner = $e[1];
	$f = console.log(inner.length);
} else {
	$f = console.log(0);
}
process.exit($f);
