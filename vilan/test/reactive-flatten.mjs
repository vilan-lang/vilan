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
		while (!($w(turn[0].v)) && budget > 0) {
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
function dispose(self, $l) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $m = $l;
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
		release();
		$p = undefined;
	} else {
		$p = undefined;
	}
	return $p;
}
function defer(self, cleanup) {
	self[0].v.push(cleanup);
}
function register_with_owner(subscription, $E, $F) {
	const $G = $F;
	let $H = null;
	if ($G[0] === 0) {
		const owner = $G[1];
		$H = $I(owner, subscription, $E);
	} else {
		$H = __clone(subscription);
	}
	return $H;
}
function defer_to_owner(cleanup, $M) {
	const $N = $M;
	let $O = null;
	if ($N[0] === 0) {
		const owner = $N[1];
		$O = defer(owner, cleanup);
	} else {
		$O = undefined;
	}
	return $O;
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
	return self[0].v;
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
	self[0].v = value;
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
function $I(self, item, $J) {
	self[0].v.push(() => {
		dispose(item, $J);
		return;
	});
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
		const $K = inner_subscription.v;
		let $L = null;
		if ($K[0] === 1) {
			$L = $K;
		} else {
			$L = [ 0, dispose($K[1], $f) ];
		}
		$L;
		inner_subscription.v = [ 1 ];
		return;
	}, $g);
	return derived;
}
function $Q(self, $t) {
	const $R = $t;
	let $S = null;
	if ($R[0] === 0) {
		const turn = $R[1];
		$S = enqueue(turn, self[1].v);
	} else {
		const $T = $x(draining_turns.v);
		let $U = null;
		if ($T[0] === 0) {
			const draining = $T[1];
			$U = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$U = undefined;
		}
		$S = $U;
	}
	return $S;
}
function $P(self, value, $r) {
	self[0].v = value;
	$Q(self, $r);
}
function $aa(self, $t) {
	const $ab = $t;
	let $ac = null;
	if ($ab[0] === 0) {
		const turn = $ab[1];
		$ac = enqueue(turn, self[1].v);
	} else {
		const $ad = $x(draining_turns.v);
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
function $Z(self, value, $r) {
	self[0].v = value;
	$aa(self, $r);
}
function $af(self, observer) {
	return $B(self, observer);
}
function $V(self, transform, $W, $X) {
	const derived = $b(transform($h(self)));
	register_with_owner($af(self, (value) => {
		$Z(derived, transform(value), $W);
		return;
	}), $W, $X);
	return derived;
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const first = $a(1);
const second = $a(10);
const outer = $c(first);
const joined = $e(outer, [ 1 ], [ 1 ]);
console.log($h(joined));
$q(first, 2, [ 1 ]);
console.log($h(joined));
$P(outer, second, [ 1 ]);
console.log($h(joined));
$q(first, 99, [ 1 ]);
console.log($h(joined));
$q(second, 11, [ 1 ]);
console.log($h(joined));
const doubled = $V(joined, (value) => {
	return value * 2;
}, [ 1 ], [ 1 ]);
$q(second, 21, [ 1 ]);
console.log($h(doubled));
