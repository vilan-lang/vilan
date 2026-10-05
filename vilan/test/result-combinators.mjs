function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function default2() {
	return 0;
}
function map(self, fn) {
	const $a = self;
	let $b = null;
	if ($a[0] === 0) {
		const x = $a[1];
		$b = [ 0, fn(x) ];
	} else {
		const e = $a[1];
		$b = [ 1, __clone(e) ];
	}
	return $b;
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
function map_err(self, fn) {
	const $e = self;
	let $f = null;
	if ($e[0] === 0) {
		const x = $e[1];
		$f = [ 0, __clone(x) ];
	} else {
		const e = $e[1];
		$f = [ 1, fn(e) ];
	}
	return $f;
}
function is_ok_and(self, fn) {
	const $g = self;
	let $h = null;
	if ($g[0] === 0) {
		const x = $g[1];
		$h = fn(x);
	} else {
		$h = false;
	}
	return $h;
}
function is_err_and(self, fn) {
	const $i = self;
	let $j = null;
	if ($i[0] === 1) {
		const e = $i[1];
		$j = fn(e);
	} else {
		$j = false;
	}
	return $j;
}
function and_then(self, fn) {
	const $k = self;
	let $l = null;
	if ($k[0] === 0) {
		const x = $k[1];
		$l = fn(x);
	} else {
		const e = $k[1];
		$l = [ 1, __clone(e) ];
	}
	return $l;
}
function or_else(self, fn) {
	const $m = self;
	let $n = null;
	if ($m[0] === 0) {
		const x = $m[1];
		$n = [ 0, __clone(x) ];
	} else {
		const e = $m[1];
		$n = fn(e);
	}
	return $n;
}
function unwrap_or_else(self, fn) {
	const $o = self;
	let $p = null;
	if ($o[0] === 0) {
		const x = __clone($o[1]);
		$p = x;
	} else {
		const e = $o[1];
		$p = fn(e);
	}
	return $p;
}
function ok(self) {
	const $q = self;
	let $r = null;
	if ($q[0] === 0) {
		const x = $q[1];
		$r = [ 0, __clone(x) ];
	} else {
		$r = [ 1 ];
	}
	return $r;
}
function is_some(self) {
	const $s = self;
	return $s[0] === 0;
}
function err(self) {
	const $t = self;
	let $u = null;
	if ($t[0] === 1) {
		const e = $t[1];
		$u = [ 0, __clone(e) ];
	} else {
		$u = [ 1 ];
	}
	return $u;
}
function unwrap_or2(self, fallback) {
	const $v = self;
	let $w = null;
	if ($v[0] === 0) {
		const x = __clone($v[1]);
		$w = x;
	} else {
		$w = __clone(fallback);
	}
	return $w;
}
function unwrap_or_default(self) {
	const $x = self;
	let $y = null;
	if ($x[0] === 0) {
		const x = __clone($x[1]);
		$y = x;
	} else {
		$y = default2();
	}
	return $y;
}
function and(self, b) {
	const $z = self;
	let $A = null;
	if ($z[0] === 0) {
		$A = b;
	} else {
		const e = $z[1];
		$A = [ 1, __clone(e) ];
	}
	return $A;
}
function or(self, b) {
	const $B = self;
	let $C = null;
	if ($B[0] === 0) {
		const x = $B[1];
		$C = [ 0, __clone(x) ];
	} else {
		$C = b;
	}
	return $C;
}
function transpose(self) {
	const $D = self;
	let $E = null;
	if ($D[0] === 0 && $D[1][0] === 0) {
		const x = $D[1][1];
		$E = [ 0, [ 0, __clone(x) ] ];
	} else if ($D[0] === 0 && $D[1][0] === 1) {
		$E = [ 1 ];
	} else {
		const e = $D[1];
		$E = [ 0, [ 1, __clone(e) ] ];
	}
	return $E;
}
const ok2 = [ 0, 10 ];
const err2 = [ 1, "boom" ];
console.log(String(unwrap_or(map(ok2, (n) => {
	return n + 1;
}), 0)));
console.log(String(unwrap_or(map_err(err2, (e) => {
	return e;
}), 0)));
console.log(is_ok_and(ok2, (n) => {
	return n > 5;
}));
console.log(is_err_and(err2, (e) => {
	return true;
}));
console.log(String(unwrap_or(and_then(ok2, (n) => {
	return [ 0, n * 2 ];
}), 0)));
console.log(String(unwrap_or(or_else(err2, (e) => {
	return [ 0, 7 ];
}), 0)));
console.log(String(unwrap_or_else(err2, (e) => {
	return 99;
})));
console.log(is_some(ok(ok2)));
console.log(unwrap_or2(err(err2), "none"));
console.log(String(unwrap_or_default(err2)));
console.log(String(unwrap_or(and(ok2, [ 0, 5 ]), 0)));
console.log(String(unwrap_or(or(err2, [ 0, 3 ]), 0)));
const ro = [ 0, [ 0, 42 ] ];
console.log(is_some(transpose(ro)));
