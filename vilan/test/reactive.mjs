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
			while (!($n(turn[0].v)) && budget > 0) {
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
function dispose(self, $B) {
	self[2].v = false;
	const $C = [ 0, self[0] ];
	let $D = null;
	if ($C[0] === 0) {
		const subscribers = $C[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$D = undefined;
	} else {
		$D = undefined;
	}
	$D;
	const $E = $B;
	let $F = null;
	if ($E[0] === 0) {
		const established = $E[1];
		$F = [ 0, established ];
	} else {
		$F = $o(draining_turns.v);
	}
	const ambient = $F;
	const $G = ambient;
	let $H = null;
	if ($G[0] === 0) {
		const turn = $G[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$H = undefined;
	} else {
		$H = undefined;
	}
	$H;
	const $I = self[3].v;
	let $J = null;
	if ($I[0] === 0) {
		const release = $I[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$J = undefined;
	} else {
		$J = undefined;
	}
	return $J;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function register_with_owner(subscription, $v, $w) {
	const $x = $w;
	let $y = null;
	if ($x[0] === 0) {
		const owner2 = $x[1];
		$y = $z(owner2, subscription, $v);
	} else {
		$y = __clone(subscription);
	}
	return $y;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $f(self) {
	return __clone(self[0].v);
}
function $n(self) {
	return self.length === 0;
}
function $o(self) {
	return __list_get(self, self.length - 1);
}
function $j(self, $k) {
	const $l = $k;
	let $m = null;
	if ($l[0] === 0) {
		const turn = $l[1];
		$m = enqueue(turn, self[1].v);
	} else {
		const $p = $o(draining_turns.v);
		let $q = null;
		if ($p[0] === 0) {
			const draining = $p[1];
			$q = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$q = undefined;
		}
		$m = $q;
	}
	return $m;
}
function $h(self, value, $i) {
	self[0].v = __clone(value);
	$j(self, $i);
}
function $s(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	signal[1].v.push([ id, () => {
		const $t = [ 0, cell ];
		let $u = null;
		if ($t[0] === 0) {
			const live2 = $t[1];
			$u = observer(live2.v);
		} else {
			$u = undefined;
		}
		return $u;
	}, live ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $r(self, observer) {
	return $s(self, observer);
}
function $z(self, item, $A) {
	if (self[1].v) {
		dispose(item, $A);
	} else {
		self[0].v.push(() => {
			dispose(item, $A);
			return;
		});
	}
	return __clone(item);
}
function $c(self, transform, $d, $e) {
	const derived = $b(transform($f(self)));
	register_with_owner($r(self, (value) => {
		$h(derived, transform(value), $d);
		return;
	}), $d, $e);
	return derived;
}
function $K(self, observer) {
	const subscription = $s(self, observer);
	observer($f(self));
	return subscription;
}
function $M(self, $k) {
	const $N = $k;
	let $O = null;
	if ($N[0] === 0) {
		const turn = $N[1];
		$O = enqueue(turn, self[1].v);
	} else {
		const $P = $o(draining_turns.v);
		let $Q = null;
		if ($P[0] === 0) {
			const draining = $P[1];
			$Q = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$Q = undefined;
		}
		$O = $Q;
	}
	return $O;
}
function $L(self, value, $i) {
	self[0].v = __clone(value);
	$M(self, $i);
}
function $R(self, transform, $S) {
	$L(self, transform($f(self)), $S);
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const owner = new2();
const count = $a(0);
const doubled = $c(count, (n) => {
	return n * 2;
}, [ 1 ], [ 1 ]);
$z(owner, $K(doubled, (n) => {
	return console.log(n);
}), [ 1 ]);
$L(count, 1, [ 1 ]);
$R(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($f(doubled));
$z(owner, $K(count, (n) => {
	return console.log(n);
}), [ 1 ]);
$L(count, 20, [ 1 ]);
