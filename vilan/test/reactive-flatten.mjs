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
function __with_finally(body, after) {
	try {
		body();
	} finally {
		after();
	}
}
function hash(self) {
	return __hash(self);
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		if (!(turn[1].v.has(key))) {
			turn[1].v.set(key, true);
			turn[0].v.push(__clone(subscriber));
		}
	}
	if (turn[3].v && !(turn[4].v) && !(turn[2].v)) {
		turn[4].v = true;
		queueMicrotask(() => {
			turn[4].v = false;
			drain(turn);
			return;
		});
	}
}
function drain(turn) {
	if (!(turn[2].v)) {
		turn[2].v = true;
		draining_turns.v.push(__clone(turn));
		__with_finally(() => {
			let budget = 100000;
			while (!($w(turn[0].v)) && budget > 0) {
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[1].v = new Map();
				for (const subscriber of wave) {
					subscriber[1]();
					budget = budget - 1;
				}
			}
			return;
		}, () => {
			__list_pop(draining_turns.v);
			turn[2].v = false;
			return;
		});
	}
}
function dispose(self, $l) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const ambient = $l;
	const $m = ambient;
	let $n = null;
	if ($m[0] === 0) {
		const turn = $m[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$n = undefined;
	} else {
		$n = undefined;
	}
	$n;
	const $o = self[2].v;
	let $p = null;
	if ($o[0] === 0) {
		const release = $o[1];
		self[2].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$p = undefined;
	} else {
		$p = undefined;
	}
	return $p;
}
function defer(self, cleanup) {
	if (self[1].v) {
		cleanup();
	} else {
		self[0].v.push(cleanup);
	}
}
function register_with_owner(subscription, $F, $G) {
	const $H = $G;
	let $I = null;
	if ($H[0] === 0) {
		const owner = $H[1];
		$I = $J(owner, subscription, $F);
	} else {
		$I = __clone(subscription);
	}
	return $I;
}
function defer_to_owner(cleanup, $N) {
	const $O = $N;
	let $P = null;
	if ($O[0] === 0) {
		const owner = $O[1];
		$P = defer(owner, cleanup);
	} else {
		$P = undefined;
	}
	return $P;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers), fresh_id() ];
}
function $a(value) {
	return $b(value);
}
function $c(value) {
	return $b(value);
}
function $h(self) {
	return __clone(self[0].v);
}
function $w(self) {
	return self.length === 0;
}
function $x(self) {
	return __list_get(self, self.length - 1);
}
function $s(self, $t) {
	const $u = $t;
	let $v = null;
	if ($u[0] === 0) {
		const turn = $u[1];
		$v = enqueue(turn, self[1].v);
	} else {
		const $y = $x(draining_turns.v);
		let $z = null;
		if ($y[0] === 0) {
			const draining = $y[1];
			$z = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$z = undefined;
		}
		$v = $z;
	}
	return $v;
}
function $q(self, value, $r) {
	self[0].v = __clone(value);
	$s(self, $r);
}
function $B(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $A(self, observer) {
	const subscription = $B(self, observer);
	observer($h(self));
	return subscription;
}
function $C(self, observer) {
	const subscription = $B(self, observer);
	observer($h(self));
	return subscription;
}
function $J(self, item, $K) {
	if (self[1].v) {
		dispose(item, $K);
	} else {
		self[0].v.push(() => {
			dispose(item, $K);
			return;
		});
	}
	return __clone(item);
}
function $e(self, $f, $g) {
	const derived = $b($h($h(self)));
	const inner_subscription = __shared_new([ 1 ]);
	register_with_owner($C(self, (inner) => {
		const $j = inner_subscription.v;
		let $k = null;
		if ($j[0] === 1) {
			$k = $j;
		} else {
			$k = [ 0, dispose($j[1], $f) ];
		}
		$k;
		inner_subscription.v = [ 0, $A(inner, (value) => {
			$q(derived, value, $f);
			return;
		}) ];
		return;
	}), $f, $g);
	defer_to_owner(() => {
		const $L = inner_subscription.v;
		let $M = null;
		if ($L[0] === 1) {
			$M = $L;
		} else {
			$M = [ 0, dispose($L[1], $f) ];
		}
		$M;
		inner_subscription.v = [ 1 ];
		return;
	}, $g);
	return derived;
}
function $R(self, $t) {
	const $S = $t;
	let $T = null;
	if ($S[0] === 0) {
		const turn = $S[1];
		$T = enqueue(turn, self[1].v);
	} else {
		const $U = $x(draining_turns.v);
		let $V = null;
		if ($U[0] === 0) {
			const draining = $U[1];
			$V = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$V = undefined;
		}
		$T = $V;
	}
	return $T;
}
function $Q(self, value, $r) {
	self[0].v = __clone(value);
	$R(self, $r);
}
function $ab(self, $t) {
	const $ac = $t;
	let $ad = null;
	if ($ac[0] === 0) {
		const turn = $ac[1];
		$ad = enqueue(turn, self[1].v);
	} else {
		const $ae = $x(draining_turns.v);
		let $af = null;
		if ($ae[0] === 0) {
			const draining = $ae[1];
			$af = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$af = undefined;
		}
		$ad = $af;
	}
	return $ad;
}
function $aa(self, value, $r) {
	self[0].v = __clone(value);
	$ab(self, $r);
}
function $ag(self, observer) {
	return $B(self, observer);
}
function $W(self, transform, $X, $Y) {
	const derived = $b(transform($h(self)));
	register_with_owner($ag(self, (value) => {
		$aa(derived, transform(value), $X);
		return;
	}), $X, $Y);
	return derived;
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const first = $a(1);
const second = $a(10);
const outer = $c(first);
const joined = $e(outer, [ 1 ], [ 1 ]);
console.log($h(joined));
$q(first, 2, [ 1 ]);
console.log($h(joined));
$Q(outer, second, [ 1 ]);
console.log($h(joined));
$q(first, 99, [ 1 ]);
console.log($h(joined));
$q(second, 11, [ 1 ]);
console.log($h(joined));
const doubled = $W(joined, (value) => {
	return value * 2;
}, [ 1 ], [ 1 ]);
$q(second, 21, [ 1 ]);
console.log($h(doubled));
