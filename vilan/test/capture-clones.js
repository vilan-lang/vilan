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
function total_width(rows2) {
	let total = 0;
	for (const row of rows2) {
		const $c = row;
		let $d = null;
		const cells = $c[0];
		const weight = $c[1];
		total = total + cells.length * weight;
		$d = undefined;
		$d;
	}
	return total;
}
function guarded_width(rows2) {
	let total = 0;
	for (const row of rows2) {
		const $e = row;
		let $f = null;
		if ($e[1] > 1) {
			total = total + $e[0].length;
			$f = undefined;
		} else {
			$f = undefined;
		}
		$f;
	}
	return total;
}
function first_or(held2, fallback) {
	const $g = held2;
	let $h = null;
	if ($g[0] === 0) {
		const inner2 = __clone($g[1]);
		$h = inner2;
	} else {
		$h = __clone(fallback);
	}
	return $h;
}
function first_or_guarded(held2, limit, fallback) {
	const $i = held2;
	let $j = null;
	if ($i[0] === 0 && limit > 0) {
		const inner2 = __clone($i[1]);
		$j = inner2;
	} else {
		$j = __clone(fallback);
	}
	return $j;
}
function grow_first(pair2) {
	const $m = pair2;
	let cells = __clone($m[0]);
	if (true) {
		cells.push($m[1]);
		return cells.length;
	}
	return 0;
}
let entries = [  ];
entries.push([ 1, 2 ]);
entries.push([ 10, 20 ]);
console.log(sum_over(entries));
let rows = [  ];
rows.push([ [ 1, 2 ], 3 ]);
rows.push([ [ 4 ], 1 ]);
console.log(total_width(rows));
console.log(guarded_width(rows));
const held = [ 0, [ 1, 2 ] ];
let got = first_or(held, [  ]);
got.push(9);
console.log(got.length);
let guarded = first_or_guarded(held, 1, [  ]);
guarded.push(9);
console.log(guarded.length);
const $k = held;
let $l = null;
if ($k[0] === 0) {
	const inner = $k[1];
	$l = console.log(inner.length);
} else {
	$l = console.log(0);
}
$l;
const pair = [ [ 1, 2 ], 3 ];
console.log(grow_first(pair));
console.log(pair[0].length);
