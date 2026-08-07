function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
}
function __list_get(list, index) {
	return index >= 0 && index < list.length ? [ 0, __clone(list[index]) ] : [ 1 ];
}
function __list_pop(list) {
	return list.length === 0 ? [ 1 ] : [ 0, list.pop() ];
}
function __shared_new(value) {
	return { v: value };
}
function hash(self) {
	return __hash(self);
}
function new2() {
	const table = new Map();
	return [ table ];
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function new3() {
	return [ __shared_new([  ]), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		let seen = false;
		for (const queued of turn[0].v) {
			if (queued[0] === subscriber[0]) {
				seen = true;
			}
		}
		if (!(seen)) {
			turn[0].v.push(__clone(subscriber));
		}
	}
	if (turn[2].v && !(turn[3].v) && !(turn[1].v)) {
		turn[3].v = true;
		queueMicrotask(() => {
			turn[3].v = false;
			drain(turn);
			return;
		});
	}
}
function drain(turn) {
	if (!(turn[1].v)) {
		turn[1].v = true;
		draining_turns.v.push(__clone(turn));
		let budget = 100000;
		while (!($h(turn[0].v)) && budget > 0) {
			const wave = turn[0].v;
			turn[0].v = [  ];
			for (const subscriber of wave) {
				subscriber[1]();
				budget = budget - 1;
			}
		}
		__list_pop(draining_turns.v);
		turn[1].v = false;
	}
}
function dispose(self, $J) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $K = $J;
	let $L = null;
	if ($K[0] === 0) {
		const turn = $K[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		$L = undefined;
	} else {
		$L = undefined;
	}
	return $L;
}
function new4() {
	return [ __shared_new([  ]) ];
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $h(self) {
	return self.length === 0;
}
function $i(self) {
	return __list_get(self, self.length - 1);
}
function $d(self, $e) {
	const $f = $e;
	let $g = null;
	if ($f[0] === 0) {
		const turn = $f[1];
		$g = enqueue(turn, self[1].v);
	} else {
		const $j = $i(draining_turns.v);
		let $k = null;
		if ($j[0] === 0) {
			const draining = $j[1];
			$k = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$k = undefined;
		}
		$g = $k;
	}
	return $g;
}
function $b(self, mutate, $c) {
	mutate(self[0].v);
	$d(self, $c);
}
function $l(self) {
	return self[0].v;
}
function $m(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $n(self, key, value) {
	self[0].set(hash(key), [ __clone(key), __clone(value) ]);
}
function $p(self, $e) {
	const $q = $e;
	let $r = null;
	if ($q[0] === 0) {
		const turn = $q[1];
		$r = enqueue(turn, self[1].v);
	} else {
		const $s = $i(draining_turns.v);
		let $t = null;
		if ($s[0] === 0) {
			const draining = $s[1];
			$t = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$t = undefined;
		}
		$r = $t;
	}
	return $r;
}
function $o(self, mutate, $c) {
	mutate(self[0].v);
	$p(self, $c);
}
function $u(self) {
	return self[0].v;
}
function $v(self) {
	return self[0].size;
}
function $w(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $y(self, $e) {
	const $z = $e;
	let $A = null;
	if ($z[0] === 0) {
		const turn = $z[1];
		$A = enqueue(turn, self[1].v);
	} else {
		const $B = $i(draining_turns.v);
		let $C = null;
		if ($B[0] === 0) {
			const draining = $B[1];
			$C = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$C = undefined;
		}
		$A = $C;
	}
	return $A;
}
function $x(self, mutate, $c) {
	mutate([ self[0], "v" ]);
	$y(self, $c);
}
function $D(self) {
	return self[0].v;
}
function $E(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $G(self) {
	return self[0].v;
}
function $F(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($G(self));
		return;
	} ]);
	observer($G(self));
	return [ self[1], id ];
}
function $H(self, item, $I) {
	self[0].v.push(() => {
		dispose(item, $I);
		return;
	});
	return __clone(item);
}
function $N(self, $e) {
	const $O = $e;
	let $P = null;
	if ($O[0] === 0) {
		const turn = $O[1];
		$P = enqueue(turn, self[1].v);
	} else {
		const $Q = $i(draining_turns.v);
		let $R = null;
		if ($Q[0] === 0) {
			const draining = $Q[1];
			$R = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$R = undefined;
		}
		$P = $R;
	}
	return $P;
}
function $M(self, mutate, $c) {
	mutate(self[0].v);
	$N(self, $c);
}
function $T(self, $e) {
	const $U = $e;
	let $V = null;
	if ($U[0] === 0) {
		const turn = $U[1];
		$V = enqueue(turn, self[1].v);
	} else {
		const $W = $i(draining_turns.v);
		let $X = null;
		if ($W[0] === 0) {
			const draining = $W[1];
			$X = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$X = undefined;
		}
		$V = $X;
	}
	return $V;
}
function $S(self, mutate, $c) {
	mutate(self[0].v);
	$T(self, $c);
}
function $aa(self, $e) {
	const $ab = $e;
	let $ac = null;
	if ($ab[0] === 0) {
		const turn = $ab[1];
		$ac = enqueue(turn, self[1].v);
	} else {
		const $ad = $i(draining_turns.v);
		let $ae = null;
		if ($ad[0] === 0) {
			const draining = $ad[1];
			$ae = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$ae = undefined;
		}
		$ac = $ae;
	}
	return $ac;
}
function $Z(self, mutate, $c) {
	mutate(self[0].v);
	$aa(self, $c);
}
function $ag(self, $e) {
	const $ah = $e;
	let $ai = null;
	if ($ah[0] === 0) {
		const turn = $ah[1];
		$ai = enqueue(turn, self[1].v);
	} else {
		const $aj = $i(draining_turns.v);
		let $ak = null;
		if ($aj[0] === 0) {
			const draining = $aj[1];
			$ak = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$ak = undefined;
		}
		$ai = $ak;
	}
	return $ai;
}
function $af(self, mutate, $c) {
	mutate(self[0].v);
	$ag(self, $c);
}
function $al(body, $am) {
	const $an = $am;
	let $ao = null;
	if ($an[0] === 0) {
		const current = $an[1];
		$ao = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[2].v = true;
		$ao = result;
	}
	return $ao;
}
function $aq(self, $e) {
	const $ar = $e;
	let $as = null;
	if ($ar[0] === 0) {
		const turn = $ar[1];
		$as = enqueue(turn, self[1].v);
	} else {
		const $at = $i(draining_turns.v);
		let $au = null;
		if ($at[0] === 0) {
			const draining = $at[1];
			$au = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$au = undefined;
		}
		$as = $au;
	}
	return $as;
}
function $ap(self, mutate, $c) {
	mutate(self[0].v);
	$aq(self, $c);
}
function $ax(self, value, $ay) {
	self[0].v = value;
	$y(self, $ay);
}
function $av(self, transform, $aw) {
	$ax(self, transform($D(self)), $aw);
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const owner = new4();
const todos = $a([ 1, 2 ]);
$b(todos, (list) => {
	list.push(5);
	return;
}, [ 1 ]);
console.log($l(todos).length);
const scores = $m(new2());
$o(scores, (entries) => {
	$n(entries, "a", 1);
	$n(entries, "b", 2);
	return;
}, [ 1 ]);
console.log($v($u(scores)));
const count = $w(1);
$x(count, (value) => {
	value[0][value[1]] = value[0][value[1]] + 10;
	return;
}, [ 1 ]);
console.log($D(count));
const watched = $E([ 0 ]);
$H(owner, $F(watched, (list) => {
	return console.log("len " + list.length);
}), [ 1 ]);
$M(watched, (list) => {
	list.push(1);
	list.push(2);
	return;
}, [ 1 ]);
$S(watched, (list) => {
	return;
}, [ 1 ]);
console.log("---");
$al(($Y) => {
	$Z(watched, (list) => {
		list.push(3);
		return;
	}, [ 0, $Y ]);
	$af(watched, (list) => {
		list.push(4);
		return;
	}, [ 0, $Y ]);
	console.log("inside");
	return;
}, [ 1 ]);
$ap(todos, (list) => {
	list.push(6);
	console.log("reentrant " + $l(todos).length);
	return;
}, [ 1 ]);
$av(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($D(count));
