function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function to_string(self) {
	return "" + self;
}
function fetch2(log3, value) {
	log3.push(1);
	return value;
}
function parse(tag) {
	let $n = null;
	if (tag === "good") {
		$n = [ 0, 21 ];
	} else {
		$n = [ 1, "bad: " + tag ];
	}
	return $n;
}
function total(a, b) {
	let $z = null;
	const $A = a;
	if ($A[0] === 1) {
		$z = $A;
	} else {
		const $B = b;
		if ($B[0] === 1) {
			$z = $B;
		} else {
			$z = [ 0, $A[1] + $B[1] ];
		}
	}
	const $C = $z;
	if ($C[0] === 1) {
		return $C;
	}
	const sum2 = $C[1];
	return [ 0, sum2 * 10 ];
}
function unwrap_or(self, fallback) {
	const $c = self;
	let $d = null;
	if ($c[0] === 0) {
		const x = __clone($c[1]);
		$d = x;
	} else {
		$d = __clone(fallback);
	}
	return $d;
}
function map(self, fn) {
	return [ fn(self[0]), self[1] + ".map" ];
}
function format(value) {
	return to_string(value);
}
function and_then(self, fn) {
	const inner = fn(self[0]);
	return [ __clone(inner[0]), self[1] + "+" + inner[1] ];
}
const count = [ 0, 2 ];
let $a = null;
const $b = count;
if ($b[0] === 1) {
	$a = $b;
} else {
	$a = [ 0, $b[1] * 2 ];
}
console.log(String(unwrap_or($a, -(1))));
let $e = null;
const $f = count;
if ($f[0] === 1) {
	$e = $f;
} else {
	$e = [ 0, 2 * $f[1] ];
}
console.log(String(unwrap_or($e, -(1))));
let log = [  ];
let $g = null;
const $h = fetch2(log, [ 0, 40 ]);
if ($h[0] === 1) {
	$g = $h;
} else {
	const $i = fetch2(log, [ 0, 2 ]);
	if ($i[0] === 1) {
		$g = $i;
	} else {
		$g = [ 0, $h[1] + $i[1] ];
	}
}
const both = $g;
console.log(String(unwrap_or(both, -(1))));
console.log(String(log.length));
let log2 = [  ];
let $j = null;
const $k = fetch2(log2, [ 1 ]);
if ($k[0] === 1) {
	$j = $k;
} else {
	const $l = fetch2(log2, [ 0, 2 ]);
	if ($l[0] === 1) {
		$j = $l;
	} else {
		$j = [ 0, $k[1] + $l[1] ];
	}
}
const bad = $j;
console.log(String(unwrap_or(bad, -(1))));
console.log(String(log2.length));
let $m = null;
const $o = parse("good");
if ($o[0] === 1) {
	$m = $o;
} else {
	const $p = parse("good");
	if ($p[0] === 1) {
		$m = $p;
	} else {
		$m = [ 0, $o[1] + $p[1] ];
	}
}
const sum = $m;
const $q = sum;
let $r = null;
if ($q[0] === 0) {
	const n = $q[1];
	$r = console.log(String(n));
} else {
	const e = $q[1];
	$r = console.log(e);
}
$r;
let $s = null;
const $t = parse("x");
if ($t[0] === 1) {
	$s = $t;
} else {
	const $u = parse("y");
	if ($u[0] === 1) {
		$s = $u;
	} else {
		$s = [ 0, $t[1] + $u[1] ];
	}
}
const $v = $s;
let $w = null;
if ($v[0] === 0) {
	const n2 = $v[1];
	$w = console.log(String(n2));
} else {
	const e2 = $v[1];
	$w = console.log(e2);
}
$w;
const rows = [ 0, [ [ 0, 7 ], [ 1 ] ] ];
let $x = null;
const $y = rows;
if ($y[0] === 1) {
	$x = $y;
} else {
	$x = __at($y[1], 0, "expression-lift.vl:77:27");
}
const first = $x;
console.log(String(unwrap_or(first, -(1))));
console.log(String(unwrap_or(total([ 0, 4 ], [ 0, 2 ]), -(1))));
console.log(String(unwrap_or(total([ 0, 4 ], [ 1 ]), -(1))));
const size = [ 0, 4 ];
let $D = null;
const $E = size;
if ($E[0] === 1) {
	$D = $E;
} else {
	const $F = size;
	if ($F[0] === 1) {
		$D = $F;
	} else {
		$D = [ 0, $E[1] * $F[1] ];
	}
}
console.log(String(unwrap_or($D, -(1))));
const boxed = [ 20, "a" ];
const doubled = map(boxed, ($G) => {
	return $G * 2;
});
console.log("" + format(doubled[0]) + " [" + doubled[1] + "]");
const left = [ 40, "L" ];
const right = [ 2, "R" ];
const paired = and_then(left, ($H) => {
	return map(right, ($I) => {
		return $H + $I;
	});
});
console.log("" + format(paired[0]) + " [" + paired[1] + "]");
const boxes = [ [ [ 7, "inner" ] ], "outer" ];
const picked = and_then(boxes, ($J) => {
	return __at($J, 0, "expression-lift.vl:102:26");
});
console.log("" + format(picked[0]) + " [" + picked[1] + "]");
