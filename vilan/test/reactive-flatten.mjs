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
			while (!($B(turn[0].v)) && budget > 0) {
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[1].v = new Map();
				for (const subscriber of wave) {
					if (subscriber[2].v) {
						subscriber[1]();
					}
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
	self[2].v = false;
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
	const $o = $l;
	let $p = null;
	if ($o[0] === 0) {
		const established = $o[1];
		$p = [ 0, established ];
	} else {
		$p = $q(draining_turns.v);
	}
	const ambient = $p;
	const $r = ambient;
	let $s = null;
	if ($r[0] === 0) {
		const turn = $r[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$s = undefined;
	} else {
		$s = undefined;
	}
	$s;
	const $t = self[3].v;
	let $u = null;
	if ($t[0] === 0) {
		const release = $t[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$u = undefined;
	} else {
		$u = undefined;
	}
	return $u;
}
function defer(self, cleanup) {
	if (self[1].v) {
		cleanup();
	} else {
		self[0].v.push(cleanup);
	}
}
function register_with_owner(subscription, $M, $N) {
	const $O = $N;
	let $P = null;
	if ($O[0] === 0) {
		const owner = $O[1];
		$P = $Q(owner, subscription, $M);
	} else {
		$P = __clone(subscription);
	}
	return $P;
}
function defer_to_owner(cleanup, $U) {
	const $V = $U;
	let $W = null;
	if ($V[0] === 0) {
		const owner = $V[1];
		$W = defer(owner, cleanup);
	} else {
		$W = undefined;
	}
	return $W;
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
function $q(self) {
	return __list_get(self, self.length - 1);
}
function $B(self) {
	return self.length === 0;
}
function $x(self, $y) {
	const $z = $y;
	let $A = null;
	if ($z[0] === 0) {
		const turn = $z[1];
		$A = enqueue(turn, self[1].v);
	} else {
		const $C = $q(draining_turns.v);
		let $D = null;
		if ($C[0] === 0) {
			const draining = $C[1];
			$D = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$D = undefined;
		}
		$A = $D;
	}
	return $A;
}
function $v(self, value, $w) {
	self[0].v = __clone(value);
	$x(self, $w);
}
function $F(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	signal[1].v.push([ id, () => {
		const $G = [ 0, cell ];
		let $H = null;
		if ($G[0] === 0) {
			const live2 = $G[1];
			$H = observer(live2.v);
		} else {
			$H = undefined;
		}
		return $H;
	}, live ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $E(self, observer) {
	const subscription = $F(self, observer);
	observer($h(self));
	return subscription;
}
function $I(self, observer) {
	const subscription = $F(self, observer);
	observer($h(self));
	return subscription;
}
function $Q(self, item, $R) {
	if (self[1].v) {
		dispose(item, $R);
	} else {
		self[0].v.push(() => {
			dispose(item, $R);
			return;
		});
	}
	return __clone(item);
}
function $e(self, $f, $g) {
	const derived = $b($h($h(self)));
	const inner_subscription = __shared_new([ 1 ]);
	register_with_owner($I(self, (inner) => {
		const $j = inner_subscription.v;
		let $k = null;
		if ($j[0] === 1) {
			$k = $j;
		} else {
			$k = [ 0, dispose($j[1], $f) ];
		}
		$k;
		inner_subscription.v = [ 0, $E(inner, (value) => {
			$v(derived, value, $f);
			return;
		}) ];
		return;
	}), $f, $g);
	defer_to_owner(() => {
		const $S = inner_subscription.v;
		let $T = null;
		if ($S[0] === 1) {
			$T = $S;
		} else {
			$T = [ 0, dispose($S[1], $f) ];
		}
		$T;
		inner_subscription.v = [ 1 ];
		return;
	}, $g);
	return derived;
}
function $Y(self, $y) {
	const $Z = $y;
	let $aa = null;
	if ($Z[0] === 0) {
		const turn = $Z[1];
		$aa = enqueue(turn, self[1].v);
	} else {
		const $ab = $q(draining_turns.v);
		let $ac = null;
		if ($ab[0] === 0) {
			const draining = $ab[1];
			$ac = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$ac = undefined;
		}
		$aa = $ac;
	}
	return $aa;
}
function $X(self, value, $w) {
	self[0].v = __clone(value);
	$Y(self, $w);
}
function $ah(self, value, $w) {
	self[0].v = __clone(value);
	$Y(self, $w);
}
function $an(self, observer) {
	return $F(self, observer);
}
function $ad(self, transform, $ae, $af) {
	const derived = $b(transform($h(self)));
	register_with_owner($an(self, (value) => {
		$ah(derived, transform(value), $ae);
		return;
	}), $ae, $af);
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
$v(first, 2, [ 1 ]);
console.log($h(joined));
$X(outer, second, [ 1 ]);
console.log($h(joined));
$v(first, 99, [ 1 ]);
console.log($h(joined));
$v(second, 11, [ 1 ]);
console.log($h(joined));
const doubled = $ad(joined, (value) => {
	return value * 2;
}, [ 1 ], [ 1 ]);
$v(second, 21, [ 1 ]);
console.log($h(doubled));
