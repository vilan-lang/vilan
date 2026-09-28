function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function default2() {
	return 0;
}
function $a(self, fn) {
	const $b = self;
	let $c = null;
	if ($b[0] === 0) {
		const x = $b[1];
		$c = [ 0, fn(x) ];
	} else {
		const e = $b[1];
		$c = [ 1, __clone(e) ];
	}
	return $c;
}
function $d(self, fallback) {
	const $e = self;
	let $f = null;
	if ($e[0] === 0) {
		const x = __clone($e[1]);
		$f = x;
	} else {
		$f = __clone(fallback);
	}
	return $f;
}
function $g(self, fn) {
	const $h = self;
	let $i = null;
	if ($h[0] === 0) {
		const x = $h[1];
		$i = [ 0, __clone(x) ];
	} else {
		const e = $h[1];
		$i = [ 1, fn(e) ];
	}
	return $i;
}
function $j(self, fn) {
	const $k = self;
	let $l = null;
	if ($k[0] === 0) {
		const x = $k[1];
		$l = fn(x);
	} else {
		$l = false;
	}
	return $l;
}
function $m(self, fn) {
	const $n = self;
	let $o = null;
	if ($n[0] === 1) {
		const e = $n[1];
		$o = fn(e);
	} else {
		$o = false;
	}
	return $o;
}
function $p(self, fn) {
	const $q = self;
	let $r = null;
	if ($q[0] === 0) {
		const x = $q[1];
		$r = fn(x);
	} else {
		const e = $q[1];
		$r = [ 1, __clone(e) ];
	}
	return $r;
}
function $s(self, fn) {
	const $t = self;
	let $u = null;
	if ($t[0] === 0) {
		const x = $t[1];
		$u = [ 0, __clone(x) ];
	} else {
		const e = $t[1];
		$u = fn(e);
	}
	return $u;
}
function $v(self, fn) {
	const $w = self;
	let $x = null;
	if ($w[0] === 0) {
		const x = __clone($w[1]);
		$x = x;
	} else {
		const e = $w[1];
		$x = fn(e);
	}
	return $x;
}
function $y(self) {
	const $z = self;
	let $A = null;
	if ($z[0] === 0) {
		const x = $z[1];
		$A = [ 0, __clone(x) ];
	} else {
		$A = [ 1 ];
	}
	return $A;
}
function $B(self) {
	const $C = self;
	return $C[0] === 0;
}
function $D(self) {
	const $E = self;
	let $F = null;
	if ($E[0] === 1) {
		const e = $E[1];
		$F = [ 0, __clone(e) ];
	} else {
		$F = [ 1 ];
	}
	return $F;
}
function $G(self, fallback) {
	const $H = self;
	let $I = null;
	if ($H[0] === 0) {
		const x = __clone($H[1]);
		$I = x;
	} else {
		$I = __clone(fallback);
	}
	return $I;
}
function $J(self) {
	const $K = self;
	let $L = null;
	if ($K[0] === 0) {
		const x = __clone($K[1]);
		$L = x;
	} else {
		$L = default2();
	}
	return $L;
}
function $M(self, b) {
	const $N = self;
	let $O = null;
	if ($N[0] === 0) {
		$O = b;
	} else {
		const e = $N[1];
		$O = [ 1, __clone(e) ];
	}
	return $O;
}
function $S(self, b) {
	const $T = self;
	let $U = null;
	if ($T[0] === 0) {
		const x = $T[1];
		$U = [ 0, __clone(x) ];
	} else {
		$U = b;
	}
	return $U;
}
function $V(self) {
	const $W = self;
	let $X = null;
	if ($W[0] === 0 && $W[1][0] === 0) {
		const x = $W[1][1];
		$X = [ 0, [ 0, __clone(x) ] ];
	} else if ($W[0] === 0 && $W[1][0] === 1) {
		$X = [ 1 ];
	} else {
		const e = $W[1];
		$X = [ 0, [ 1, __clone(e) ] ];
	}
	return $X;
}
const ok = [ 0, 10 ];
const err = [ 1, "boom" ];
console.log($d($a(ok, (n) => {
	return n + 1;
}), 0));
console.log($d($g(err, (e) => {
	return e;
}), 0));
console.log($j(ok, (n) => {
	return n > 5;
}));
console.log($m(err, (e) => {
	return true;
}));
console.log($d($p(ok, (n) => {
	return [ 0, n * 2 ];
}), 0));
console.log($d($s(err, (e) => {
	return [ 0, 7 ];
}), 0));
console.log($v(err, (e) => {
	return 99;
}));
console.log($B($y(ok)));
console.log($G($D(err), "none"));
console.log($J(err));
console.log($d($M(ok, [ 0, 5 ]), 0));
console.log($d($S(err, [ 0, 3 ]), 0));
const ro = [ 0, [ 0, 42 ] ];
console.log($B($V(ro)));
