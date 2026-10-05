function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __parse_i32(text) {
	const trimmed = text.trim();
	const value = Number(trimmed);
	return /^[+-]?[0-9]+$/.test(trimmed) && value >= -2147483648 && value <= 2147483647 ? [ 0, value ] : [ 1 ];
}
function shelf(self) {
	let $n = null;
	if (self[0] === "dune") {
		$n = [ 0, "sci-fi" ];
	} else {
		$n = [ 1 ];
	}
	return $n;
}
function find(key) {
	let $a = null;
	if (key === "hit") {
		$a = [ 0, [ "dune", "messiah" ] ];
	} else {
		$a = [ 1 ];
	}
	return $a;
}
function to_number(text) {
	const $q = __parse_i32(text);
	let $r = null;
	if ($q[0] === 0) {
		const value = $q[1];
		$r = [ 0, value ];
	} else {
		$r = [ 1, text ];
	}
	return $r;
}
function headline(key) {
	const $A = find(key);
	let $B = null;
	if ($A[0] === 1) {
		$B = $A;
	} else {
		$B = [ 0, $A[1][0] ];
	}
	const $C = $B;
	if ($C[0] === 1) {
		return $C;
	}
	const title = $C[1];
	return [ 0, title.toUpperCase() ];
}
function unwrap_or(self, fallback) {
	const $d = self;
	let $e = null;
	if ($d[0] === 0) {
		const x = __clone($d[1]);
		$e = x;
	} else {
		$e = __clone(fallback);
	}
	return $e;
}
const $b = find("hit");
let $c = null;
if ($b[0] === 1) {
	$c = $b;
} else {
	$c = [ 0, $b[1][0] ];
}
console.log(unwrap_or($c, "?"));
const $f = find("hit");
let $g = null;
if ($f[0] === 1) {
	$g = $f;
} else {
	$g = [ 0, $f[1][1].length ];
}
console.log(unwrap_or($g, 0));
const $j = find("miss");
let $k = null;
if ($j[0] === 1) {
	$k = $j;
} else {
	$k = [ 0, $j[1][0] ];
}
console.log(unwrap_or($k, "?"));
const $l = find("hit");
let $m = null;
if ($l[0] === 1) {
	$m = $l;
} else {
	$m = shelf($l[1]);
}
console.log(unwrap_or($m, "?"));
const $o = find("miss");
let $p = null;
if ($o[0] === 1) {
	$p = $o;
} else {
	$p = shelf($o[1]);
}
console.log(unwrap_or($p, "?"));
const $s = to_number("40");
let $t = null;
if ($s[0] === 1) {
	$t = $s;
} else {
	$t = [ 0, Math.max($s[1], 2) ];
}
const $u = $t;
let $v = null;
if ($u[0] === 0) {
	const v = $u[1];
	$v = console.log(v);
} else {
	const e = $u[1];
	$v = console.log(e);
}
$v;
const $w = to_number("nope");
let $x = null;
if ($w[0] === 1) {
	$x = $w;
} else {
	$x = [ 0, Math.max($w[1], 2) ];
}
const $y = $x;
let $z = null;
if ($y[0] === 0) {
	const v2 = $y[1];
	$z = console.log(v2);
} else {
	const e2 = $y[1];
	$z = console.log(e2);
}
$z;
console.log(unwrap_or(headline("hit"), "?"));
console.log(unwrap_or(headline("miss"), "?"));
