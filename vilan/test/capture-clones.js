function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __replace(target, value) {
	if (Array.isArray(target) && Array.isArray(value)) target.length = value.length;
	return Object.assign(target, value);
}
function step(self) {
	const $n = self;
	const items = __clone($n[1]);
	const at = $n[2];
	if ($n[0] === 0) {
		__replace(self, [ 0, __clone(items), at + 1 ]);
		return __at(items, at);
	}
	return "-";
}
function width(self) {
	const $o = self;
	if ($o[0] === 0) {
		return $o[1].length + $o[2];
	}
	return 0;
}
function viewed_guarded(pair2) {
	const $p = pair2;
	let $q = null;
	let $r = false;
	const cells = __clone($p[0]);
	const weight = $p[1];
	if (weight > 0) {
		$r = true;
		pair2[1] = 9;
		$q = cells.length + weight;
	}
	if (!($r)) {
		$q = 0;
	}
	return $q;
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
let feed = [ 0, [ "a", "b", "c" ], 0 ];
console.log(step(feed));
console.log(step(feed));
console.log(width(feed));
let viewed = [ [ 1, 2 ], 3 ];
console.log(viewed_guarded(viewed));
console.log(viewed[1]);
