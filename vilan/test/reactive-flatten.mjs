function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
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
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
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
		while (!($u(turn[0].v)) && budget > 0) {
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
function dispose(self, $j) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $k = $j;
	let $l = null;
	if ($k[0] === 0) {
		const turn = $k[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		$l = undefined;
	} else {
		$l = undefined;
	}
	$l;
	const $m = self[2].v;
	let $n = null;
	if ($m[0] === 0) {
		const release = $m[1];
		self[2].v = [ 1 ];
		release();
		$n = undefined;
	} else {
		$n = undefined;
	}
	return $n;
}
function defer(self, cleanup) {
	self[0].v.push(cleanup);
}
function register_with_owner(subscription, $C, $D) {
	const $E = $D;
	let $F = null;
	if ($E[0] === 0) {
		const owner = $E[1];
		$F = $G(owner, subscription, $C);
	} else {
		$F = __clone(subscription);
	}
	return $F;
}
function defer_to_owner(cleanup, $K) {
	const $L = $K;
	let $M = null;
	if ($L[0] === 0) {
		const owner = $L[1];
		$M = defer(owner, cleanup);
	} else {
		$M = undefined;
	}
	return $M;
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $f(self) {
	return self[0].v;
}
function $u(self) {
	return self.length === 0;
}
function $v(self) {
	return __list_get(self, self.length - 1);
}
function $q(self, $r) {
	const $s = $r;
	let $t = null;
	if ($s[0] === 0) {
		const turn = $s[1];
		$t = enqueue(turn, self[1].v);
	} else {
		const $w = $v(draining_turns.v);
		let $x = null;
		if ($w[0] === 0) {
			const draining = $w[1];
			$x = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$x = undefined;
		}
		$t = $x;
	}
	return $t;
}
function $o(self, value, $p) {
	self[0].v = value;
	$q(self, $p);
}
function $z(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $y(self, observer) {
	const subscription = $z(self, observer);
	observer($f(self));
	return subscription;
}
function $A(self, observer) {
	const subscription = $z(self, observer);
	observer($f(self));
	return subscription;
}
function $G(self, item, $H) {
	self[0].v.push(() => {
		dispose(item, $H);
		return;
	});
	return __clone(item);
}
function $c(self, $d, $e) {
	const derived = $a($f($f(self)));
	const inner_subscription = __shared_new([ 1 ]);
	register_with_owner($A(self, (inner) => {
		const $h = inner_subscription.v;
		let $i = null;
		if ($h[0] === 1) {
			$i = $h;
		} else {
			$i = [ 0, dispose($h[1], $d) ];
		}
		$i;
		inner_subscription.v = [ 0, $y(inner, (value) => {
			$o(derived, value, $d);
			return;
		}) ];
		return;
	}), $d, $e);
	defer_to_owner(() => {
		const $I = inner_subscription.v;
		let $J = null;
		if ($I[0] === 1) {
			$J = $I;
		} else {
			$J = [ 0, dispose($I[1], $d) ];
		}
		$J;
		inner_subscription.v = [ 1 ];
		return;
	}, $e);
	return derived;
}
function $O(self, $r) {
	const $P = $r;
	let $Q = null;
	if ($P[0] === 0) {
		const turn = $P[1];
		$Q = enqueue(turn, self[1].v);
	} else {
		const $R = $v(draining_turns.v);
		let $S = null;
		if ($R[0] === 0) {
			const draining = $R[1];
			$S = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$S = undefined;
		}
		$Q = $S;
	}
	return $Q;
}
function $N(self, value, $p) {
	self[0].v = value;
	$O(self, $p);
}
function $T(self, transform, $U, $V) {
	const derived = $a(transform($f(self)));
	register_with_owner($z(self, (value) => {
		$o(derived, transform(value), $U);
		return;
	}), $U, $V);
	return derived;
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const first = $a(1);
const second = $a(10);
const outer = $a(first);
const joined = $c(outer, [ 1 ], [ 1 ]);
console.log($f(joined));
$o(first, 2, [ 1 ]);
console.log($f(joined));
$N(outer, second, [ 1 ]);
console.log($f(joined));
$o(first, 99, [ 1 ]);
console.log($f(joined));
$o(second, 11, [ 1 ]);
console.log($f(joined));
const doubled = $T(joined, (value) => {
	return value * 2;
}, [ 1 ], [ 1 ]);
$o(second, 21, [ 1 ]);
console.log($f(doubled));
