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
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $e = null;
	if (wrapped < 0) {
		$e = wrapped + modulus;
	} else {
		$e = wrapped;
	}
	return $e;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $q = null;
	if (truncated > 0) {
		$q = truncated;
	} else {
		$q = 0;
	}
	return $q;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $f = null;
	if (wrapped >= half) {
		$f = wrapped - modulus;
	} else {
		$f = wrapped;
	}
	return $f;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function step(self) {
	const $p = self;
	const items = __clone($p[1]);
	const at = $p[2];
	if ($p[0] === 0) {
		__replace(self, [ 0, __clone(items), at + 1 ]);
		return __at(items, as_usize(at));
	}
	return "-";
}
function width(self) {
	const $r = self;
	if ($r[0] === 0) {
		return as_i32($r[1].length + as_usize($r[2]));
	}
	return 0;
}
function viewed_guarded(pair2) {
	const $s = pair2;
	let $t = null;
	let $u = false;
	const cells = __clone($s[0]);
	const weight = $s[1];
	if (weight > 0) {
		$u = true;
		pair2[1] = 9;
		$t = as_i32(cells.length) + weight;
	}
	if (!($u)) {
		$t = 0;
	}
	return $t;
}
function place_component() {
	let cell = [ [ 1, 2 ], 3 ];
	const $v = cell;
	const cells = __clone($v[0]);
	const weight = $v[1];
	if (true) {
		cell[1] = 9;
		return as_i32(cells.length) + weight;
	}
	return 0;
}
function place_rebound() {
	let cell = [ [ 1, 2 ], 3 ];
	const $w = cell;
	const cells = __clone($w[0]);
	if (true) {
		cell = [ [ 4 ], 5 ];
		return as_i32(cells.length) + $w[1];
	}
	return 0;
}
function place_guarded() {
	let cell = [ [ 1, 2 ], 3 ];
	const $x = cell;
	let $y = null;
	let $z = false;
	const cells = __clone($x[0]);
	const weight = $x[1];
	if (weight > 0) {
		$z = true;
		cell[1] = 9;
		$y = as_i32(cells.length) + weight;
	}
	if (!($z)) {
		$y = 0;
	}
	return $y;
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
	return as_i32(total);
}
function guarded_width(rows2) {
	let total = 0;
	for (const row of rows2) {
		const $g = row;
		let $h = null;
		if ($g[1] > 1) {
			total = total + $g[0].length;
			$h = undefined;
		} else {
			$h = undefined;
		}
		$h;
	}
	return as_i32(total);
}
function first_or(held2, fallback) {
	const $i = held2;
	let $j = null;
	if ($i[0] === 0) {
		const inner2 = __clone($i[1]);
		$j = inner2;
	} else {
		$j = __clone(fallback);
	}
	return $j;
}
function first_or_guarded(held2, limit, fallback) {
	const $k = held2;
	let $l = null;
	if ($k[0] === 0 && limit > 0) {
		const inner2 = __clone($k[1]);
		$l = inner2;
	} else {
		$l = __clone(fallback);
	}
	return $l;
}
function grow_first(pair2) {
	const $o = pair2;
	let cells = __clone($o[0]);
	if (true) {
		cells.push($o[1]);
		return as_i32(cells.length);
	}
	return 0;
}
function slot(self) {
	return self[0];
}
function peek(self) {
	return self[0];
}
function called_component() {
	let cell = [ [ [ 1, 2 ], 3 ] ];
	const $A = slot(cell);
	const cells = __clone($A[0]);
	const weight = $A[1];
	if (true) {
		cell[0][1] = 9;
		return as_i32(cells.length) + weight;
	}
	return 0;
}
function called_readonly() {
	let cell = [ [ [ 1, 2 ], 3 ] ];
	const $B = peek(cell);
	const cells = __clone($B[0]);
	const weight = $B[1];
	if (true) {
		cell[0][1] = 9;
		return as_i32(cells.length) + weight;
	}
	return 0;
}
function fresh_pair() {
	return [ [ 1, 2 ], 3 ];
}
function owned_call() {
	const $C = fresh_pair();
	if (true) {
		return as_i32($C[0].length) + $C[1];
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
const $m = held;
let $n = null;
if ($m[0] === 0) {
	const inner = $m[1];
	$n = console.log(inner.length);
} else {
	$n = console.log(0);
}
$n;
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
console.log(place_component());
console.log(place_rebound());
console.log(place_guarded());
console.log(called_component());
console.log(called_readonly());
console.log(owned_call());
