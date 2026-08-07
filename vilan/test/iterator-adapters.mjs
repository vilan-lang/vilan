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
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
}
function __map_get(map, key) {
	return map.has(key) ? [ 0, __clone(map.get(key)) ] : [ 1 ];
}
function hash(self) {
	return __hash(self);
}
function hash2(self) {
	return __hash(self);
}
function new2(start, end) {
	return [ start, end ];
}
function next(self) {
	let $G = null;
	if (self[0] < self[1]) {
		const value = self[0];
		self[0] = self[0] + 1;
		$G = [ 0, value ];
	} else {
		$G = [ 1 ];
	}
	return $G;
}
function next2(self) {
	self[0] = self[0] + 1;
	return [ 0, self[0] ];
}
function $a(self) {
	return [ __clone(self), 0 ];
}
function $b(self, predicate) {
	return [ self, predicate ];
}
function $c(self, fn) {
	return [ self, fn ];
}
function $d(self, count) {
	return [ self, count ];
}
function $e(self, count) {
	return [ self, count ];
}
function $k(self) {
	let $l = null;
	if (self[1] < self[0].length) {
		const value = __clone(__at(self[0], self[1]));
		self[1] = self[1] + 1;
		$l = [ 0, value ];
	} else {
		$l = [ 1 ];
	}
	return $l;
}
function $j(self) {
	let found = [ 1 ];
	let searching = true;
	while (searching) {
		const $m = $k(self[0]);
		let $n = null;
		if ($m[0] === 0) {
			if (self[1]($m[1])) {
				found = [ 0, __clone($m[1]) ];
				searching = false;
			}
			$n = undefined;
		} else {
			searching = false;
		}
		$n;
	}
	return found;
}
function $i(self) {
	const $o = $j(self[0]);
	if ($o[0] === 0) {
		return [ 0, self[1]($o[1]) ];
	}
	return [ 1 ];
}
function $h(self) {
	while (self[1] > 0) {
		self[1] = self[1] - 1;
		const $p = $i(self[0]);
		if ($p[0] === 1) {
			self[1] = 0;
			return [ 1 ];
		}
	}
	return $i(self[0]);
}
function $g(self) {
	if (self[1] <= 0) {
		return [ 1 ];
	}
	self[1] = self[1] - 1;
	return $h(self[0]);
}
function $f(self) {
	let result = [  ];
	const $q = self;
	while (true) {
		const $r = $g($q);
		if ($r[0] !== 0) {
			break;
		}
		const value = $r[1];
		result.push(__clone(value));
	}
	return result;
}
function $s(self, fn) {
	return [ self, fn ];
}
function $t(self, count) {
	return [ self, count ];
}
function $w(self) {
	const $x = next2(self[0]);
	if ($x[0] === 0) {
		return [ 0, self[1]($x[1]) ];
	}
	return [ 1 ];
}
function $v(self) {
	if (self[1] <= 0) {
		return [ 1 ];
	}
	self[1] = self[1] - 1;
	return $w(self[0]);
}
function $u(self) {
	let result = [  ];
	const $y = self;
	while (true) {
		const $z = $v($y);
		if ($z[0] !== 0) {
			break;
		}
		const value = $z[1];
		result.push(__clone(value));
	}
	return result;
}
function $A(self, predicate) {
	const $B = self;
	while (true) {
		const $C = next2($B);
		if ($C[0] !== 0) {
			break;
		}
		const value = $C[1];
		if (predicate(value)) {
			return true;
		}
	}
	return false;
}
function $D(self) {
	return [ __clone(self), 0 ];
}
function $E(self, other) {
	return [ self, __clone(other) ];
}
function $I(self) {
	let $J = null;
	if (self[1] < self[0].length) {
		const value = __clone(__at(self[0], self[1]));
		self[1] = self[1] + 1;
		$J = [ 0, value ];
	} else {
		$J = [ 1 ];
	}
	return $J;
}
function $F(self) {
	const $H = next(self[0]);
	let $L = null;
	if ($H[0] === 0) {
		const $K = $I(self[1]);
		if ($K[0] === 0) {
			return [ 0, [ $H[1], $K[1] ] ];
		}
		$L = undefined;
	}
	$L;
	return [ 1 ];
}
function $O(self, other) {
	return [ self, __clone(other), true ];
}
function $Q(self) {
	if (self[2]) {
		const $R = $k(self[0]);
		if ($R[0] === 0) {
			return [ 0, $R[1] ];
		}
		self[2] = false;
	}
	return $k(self[1]);
}
function $P(self) {
	let seen = 0;
	const $S = self;
	while (true) {
		const $T = $Q($S);
		if ($T[0] !== 0) {
			break;
		}
		const _value = $T[1];
		seen = seen + 1;
	}
	return seen;
}
function $U(self) {
	return [ self, 0 ];
}
function $V(self) {
	const $W = $I(self[0]);
	if ($W[0] === 0) {
		const index = self[1];
		self[1] = index + 1;
		return [ 0, [ index, $W[1] ] ];
	}
	return [ 1 ];
}
function $Z(self, init, fn) {
	let accumulator = __clone(init);
	const $aa = self;
	while (true) {
		const $ab = $k($aa);
		if ($ab[0] !== 0) {
			break;
		}
		const value = $ab[1];
		accumulator = fn(accumulator, value);
	}
	return accumulator;
}
function $ac(self, predicate) {
	const $ad = self;
	while (true) {
		const $ae = $k($ad);
		if ($ae[0] !== 0) {
			break;
		}
		const value = $ae[1];
		if (!(predicate(value))) {
			return false;
		}
	}
	return true;
}
function $ag(self) {
	let result = [  ];
	const $ah = self;
	while (true) {
		const $ai = $k($ah);
		if ($ai[0] !== 0) {
			break;
		}
		const value = $ai[1];
		result.push(__clone(value));
	}
	return result;
}
function $aj(self) {
	let result = [  ];
	let index = self.length - 1;
	while (index >= 0) {
		result.push(__clone(__at(self, index)));
		index = index - 1;
	}
	return result;
}
function $af(self) {
	return [ $aj($ag(self)), 0 ];
}
function $ak(self) {
	let result = [  ];
	const $al = self;
	while (true) {
		const $am = $k($al);
		if ($am[0] !== 0) {
			break;
		}
		const value = $am[1];
		result.push(__clone(value));
	}
	return result;
}
function $an(self, fn) {
	const $ao = self;
	while (true) {
		const $ap = $k($ao);
		if ($ap[0] !== 0) {
			break;
		}
		const value = $ap[1];
		fn(value);
	}
}
function $aq(self, predicate) {
	return [ self, predicate ];
}
function $as(self) {
	let found = [ 1 ];
	let searching = true;
	while (searching) {
		const $at = $k(self[0]);
		let $au = null;
		if ($at[0] === 0) {
			if (self[1]($at[1])) {
				found = [ 0, __clone($at[1]) ];
				searching = false;
			}
			$au = undefined;
		} else {
			searching = false;
		}
		$au;
	}
	return found;
}
function $ar(self) {
	let result = [  ];
	const $av = self;
	while (true) {
		const $aw = $as($av);
		if ($aw[0] !== 0) {
			break;
		}
		const value = $aw[1];
		result.push(__clone(value));
	}
	return result;
}
function $ay() {
	const table = new Map();
	return [ table ];
}
function $az(self, value) {
	self[0].set(hash2(value), value);
}
function $ax(self) {
	let result = $ay();
	for (const value of self) {
		$az(result, value);
	}
	return result;
}
function $aA(self) {
	return self[0].size;
}
function $aB(self, fn) {
	return [ self, fn ];
}
function $aD(self) {
	const $aE = $I(self[0]);
	if ($aE[0] === 0) {
		return [ 0, self[1]($aE[1]) ];
	}
	return [ 1 ];
}
function $aC(self) {
	let result = [  ];
	const $aF = self;
	while (true) {
		const $aG = $aD($aF);
		if ($aG[0] !== 0) {
			break;
		}
		const value = $aG[1];
		result.push(__clone(value));
	}
	return result;
}
function $aI() {
	const table = new Map();
	return [ table ];
}
function $aJ(self, key, value) {
	self[0].set(hash(key), [ __clone(key), __clone(value) ]);
}
function $aH(self) {
	let result = $aI();
	for (const entry2 of self) {
		$aJ(result, entry2[0], entry2[1]);
	}
	return result;
}
function $aK(self, key) {
	const $aL = __map_get(self[0], hash(key));
	let $aM = null;
	if ($aL[0] === 0) {
		const entry2 = $aL[1];
		$aM = [ 0, __clone(entry2[1]) ];
	} else {
		$aM = [ 1 ];
	}
	return $aM;
}
function $aN(self, fallback) {
	const $aO = self;
	let $aP = null;
	if ($aO[0] === 0) {
		const x = __clone($aO[1]);
		$aP = x;
	} else {
		$aP = __clone(fallback);
	}
	return $aP;
}
function $aQ(self) {
	let seen = 0;
	const $aR = self;
	while (true) {
		const $aS = $k($aR);
		if ($aS[0] !== 0) {
			break;
		}
		const _value = $aS[1];
		seen = seen + 1;
	}
	return seen;
}
console.log($f($e($d($c($b($a([ 1, 2, 3, 4, 5, 6 ]), (n) => {
	return n % 2 === 0;
}), (n) => {
	return n * 10;
}), 1), 2)));
console.log($u($t($s([ 0 ], (n) => {
	return n * n;
}), 4)));
console.log($A([ 0 ], (n) => {
	return n === 3;
}));
let zipped = $E(new2(0, 9), $D([ "a", "b" ]));
const $M = zipped;
while (true) {
	const $N = $F($M);
	if ($N[0] !== 0) {
		break;
	}
	const pair = $N[1];
	console.log("" + pair[0] + pair[1]);
}
console.log($P($O($a([ 1, 2 ]), $a([ 3 ]))));
let numbered = $U($D([ "x", "y" ]));
const $X = numbered;
while (true) {
	const $Y = $V($X);
	if ($Y[0] !== 0) {
		break;
	}
	const entry = $Y[1];
	console.log("" + entry[0] + "=" + entry[1]);
}
console.log($Z($a([ 1, 2, 3 ]), 0, (total, n) => {
	return total + n;
}));
console.log($ac($a([ 1, 2, 3 ]), (n) => {
	return n > 0;
}));
console.log($ak($af($a([ 1, 2, 3 ]))));
$an($a([ 1, 2 ]), (n) => {
	return console.log(n);
});
console.log($aA($ax($ar($aq($a([ 1, 2, 2, 3 ]), (n) => {
	return n > 1;
})))));
const lengths = $aH($aC($aB($D([ "alpha", "hi" ]), (word) => {
	return [ word, word.length ];
})));
console.log($aN($aK(lengths, "hi"), -(1)));
let live = [ 1, 2 ];
let cursor = $a(live);
live.push(3);
console.log($aQ(cursor));
console.log(live.length);
