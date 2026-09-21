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
			while (!($y(turn[0].v)) && budget > 0) {
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
	const $m = [ 0, self[0] ];
	let $n = null;
	if ($m[0] === 0) {
		const subscribers = $m[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$n = undefined;
	} else {
		$n = undefined;
	}
	$n;
	const ambient = $l;
	const $o = ambient;
	let $p = null;
	if ($o[0] === 0) {
		const turn = $o[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$p = undefined;
	} else {
		$p = undefined;
	}
	$p;
	const $q = self[2].v;
	let $r = null;
	if ($q[0] === 0) {
		const release = $q[1];
		self[2].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$r = undefined;
	} else {
		$r = undefined;
	}
	return $r;
}
function defer(self, cleanup) {
	if (self[1].v) {
		cleanup();
	} else {
		self[0].v.push(cleanup);
	}
}
function register_with_owner(subscription, $K, $L) {
	const $M = $L;
	let $N = null;
	if ($M[0] === 0) {
		const owner = $M[1];
		$N = $O(owner, subscription, $K);
	} else {
		$N = __clone(subscription);
	}
	return $N;
}
function defer_to_owner(cleanup, $S) {
	const $T = $S;
	let $U = null;
	if ($T[0] === 0) {
		const owner = $T[1];
		$U = defer(owner, cleanup);
	} else {
		$U = undefined;
	}
	return $U;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
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
function $y(self) {
	return self.length === 0;
}
function $z(self) {
	return __list_get(self, self.length - 1);
}
function $u(self, $v) {
	const $w = $v;
	let $x = null;
	if ($w[0] === 0) {
		const turn = $w[1];
		$x = enqueue(turn, self[1].v);
	} else {
		const $A = $z(draining_turns.v);
		let $B = null;
		if ($A[0] === 0) {
			const draining = $A[1];
			$B = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$B = undefined;
		}
		$x = $B;
	}
	return $x;
}
function $s(self, value, $t) {
	self[0].v = __clone(value);
	$u(self, $t);
}
function $D(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		const $E = [ 0, cell ];
		let $F = null;
		if ($E[0] === 0) {
			const live = $E[1];
			$F = observer(live.v);
		} else {
			$F = undefined;
		}
		return $F;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $C(self, observer) {
	const subscription = $D(self, observer);
	observer($h(self));
	return subscription;
}
function $G(self, observer) {
	const subscription = $D(self, observer);
	observer($h(self));
	return subscription;
}
function $O(self, item, $P) {
	if (self[1].v) {
		dispose(item, $P);
	} else {
		self[0].v.push(() => {
			dispose(item, $P);
			return;
		});
	}
	return __clone(item);
}
function $e(self, $f, $g) {
	const derived = $b($h($h(self)));
	const inner_subscription = __shared_new([ 1 ]);
	register_with_owner($G(self, (inner) => {
		const $j = inner_subscription.v;
		let $k = null;
		if ($j[0] === 1) {
			$k = $j;
		} else {
			$k = [ 0, dispose($j[1], $f) ];
		}
		$k;
		inner_subscription.v = [ 0, $C(inner, (value) => {
			$s(derived, value, $f);
			return;
		}) ];
		return;
	}), $f, $g);
	defer_to_owner(() => {
		const $Q = inner_subscription.v;
		let $R = null;
		if ($Q[0] === 1) {
			$R = $Q;
		} else {
			$R = [ 0, dispose($Q[1], $f) ];
		}
		$R;
		inner_subscription.v = [ 1 ];
		return;
	}, $g);
	return derived;
}
function $W(self, $v) {
	const $X = $v;
	let $Y = null;
	if ($X[0] === 0) {
		const turn = $X[1];
		$Y = enqueue(turn, self[1].v);
	} else {
		const $Z = $z(draining_turns.v);
		let $aa = null;
		if ($Z[0] === 0) {
			const draining = $Z[1];
			$aa = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$aa = undefined;
		}
		$Y = $aa;
	}
	return $Y;
}
function $V(self, value, $t) {
	self[0].v = __clone(value);
	$W(self, $t);
}
function $af(self, value, $t) {
	self[0].v = __clone(value);
	$W(self, $t);
}
function $al(self, observer) {
	return $D(self, observer);
}
function $ab(self, transform, $ac, $ad) {
	const derived = $b(transform($h(self)));
	register_with_owner($al(self, (value) => {
		$af(derived, transform(value), $ac);
		return;
	}), $ac, $ad);
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
$s(first, 2, [ 1 ]);
console.log($h(joined));
$V(outer, second, [ 1 ]);
console.log($h(joined));
$s(first, 99, [ 1 ]);
console.log($h(joined));
$s(second, 11, [ 1 ]);
console.log($h(joined));
const doubled = $ab(joined, (value) => {
	return value * 2;
}, [ 1 ], [ 1 ]);
$s(second, 21, [ 1 ]);
console.log($h(doubled));
