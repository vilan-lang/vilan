function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __force(cell) {
	if (cell.state === 2) return cell.value;
	if (cell.state === 1) throw "lazy initialization cycle: `" + cell.name + "`";
	if (cell.state === 3) throw "lazy `" + cell.name + "` is poisoned: its initializer panicked: " + cell.value;
	cell.state = 1;
	try {
		cell.value = cell.thunk();
	} catch (failure) {
		cell.state = 3;
		cell.value = failure;
		throw failure;
	}
	cell.state = 2;
	cell.thunk = null;
	return cell.value;
}
function __lazy(name, thunk) {
	return { name: name, state: 0, value: undefined, thunk: thunk };
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
		$f = __clone(__force(fallback));
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
function $y(self, fn) {
	const $z = self;
	let $A = null;
	if ($z[0] === 0) {
		const x = __clone($z[1]);
		$A = x;
	} else {
		const e = $z[1];
		$A = fn(e);
	}
	return $A;
}
function $B(self) {
	const $C = self;
	let $D = null;
	if ($C[0] === 0) {
		const x = $C[1];
		$D = [ 0, __clone(x) ];
	} else {
		$D = [ 1 ];
	}
	return $D;
}
function $E(self) {
	const $F = self;
	return $F[0] === 0;
}
function $G(self) {
	const $H = self;
	let $I = null;
	if ($H[0] === 1) {
		const e = $H[1];
		$I = [ 0, __clone(e) ];
	} else {
		$I = [ 1 ];
	}
	return $I;
}
function $J(self, fallback) {
	const $K = self;
	let $L = null;
	if ($K[0] === 0) {
		const x = __clone($K[1]);
		$L = x;
	} else {
		$L = __clone(__force(fallback));
	}
	return $L;
}
function $M(self) {
	const $N = self;
	let $O = null;
	if ($N[0] === 0) {
		const x = __clone($N[1]);
		$O = x;
	} else {
		$O = default2();
	}
	return $O;
}
function $P(self, b) {
	const $Q = self;
	let $R = null;
	if ($Q[0] === 0) {
		$R = b;
	} else {
		const e = $Q[1];
		$R = [ 1, __clone(e) ];
	}
	return $R;
}
function $V(self, b) {
	const $W = self;
	let $X = null;
	if ($W[0] === 0) {
		const x = $W[1];
		$X = [ 0, __clone(x) ];
	} else {
		$X = b;
	}
	return $X;
}
function $ab(self) {
	const $ac = self;
	let $ad = null;
	if ($ac[0] === 0 && $ac[1][0] === 0) {
		const x = $ac[1][1];
		$ad = [ 0, [ 0, __clone(x) ] ];
	} else if ($ac[0] === 0 && $ac[1][0] === 1) {
		$ad = [ 1 ];
	} else {
		const e = $ac[1];
		$ad = [ 0, [ 1, __clone(e) ] ];
	}
	return $ad;
}
const ok = [ 0, 10 ];
const err = [ 1, "boom" ];
console.log($d($a(ok, (n) => {
	return n + 1;
}), __lazy("fallback", () => {
	return 0;
})));
console.log($d($g(err, (e) => {
	return e;
}), __lazy("fallback", () => {
	return 0;
})));
console.log($j(ok, (n) => {
	return n > 5;
}));
console.log($m(err, (e) => {
	return true;
}));
console.log($d($p(ok, (n) => {
	return [ 0, n * 2 ];
}), __lazy("fallback", () => {
	return 0;
})));
console.log($d($s(err, (e) => {
	return [ 0, 7 ];
}), __lazy("fallback", () => {
	return 0;
})));
console.log($y(err, (e) => {
	return 99;
}));
console.log($E($B(ok)));
console.log($J($G(err), __lazy("fallback", () => {
	return "none";
})));
console.log($M(err));
console.log($d($P(ok, [ 0, 5 ]), __lazy("fallback", () => {
	return 0;
})));
console.log($d($V(err, [ 0, 3 ]), __lazy("fallback", () => {
	return 0;
})));
const ro = [ 0, [ 0, 42 ] ];
console.log($E($ab(ro)));
