function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __guarded(body) {
	try {
		body();
		return [ 1 ];
	} catch (error) {
		return [ 0, error && error.message ? error.message : String(error) ];
	}
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
			while (!($o(turn[0].v)) && budget > 0) {
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
function dispose(self, $s) {
	self[2].v = false;
	const $t = [ 0, self[0] ];
	let $u = null;
	if ($t[0] === 0) {
		const subscribers = $t[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$u = undefined;
	} else {
		$u = undefined;
	}
	$u;
	const $v = $s;
	let $w = null;
	if ($v[0] === 0) {
		const established = $v[1];
		$w = [ 0, established ];
	} else {
		$w = $p(draining_turns.v);
	}
	const ambient = $w;
	const $x = ambient;
	let $y = null;
	if ($x[0] === 0) {
		const turn = $x[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$y = undefined;
	} else {
		$y = undefined;
	}
	$y;
	const $z = self[3].v;
	let $A = null;
	if ($z[0] === 0) {
		const release = $z[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$A = undefined;
	} else {
		$A = undefined;
	}
	return $A;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function dispose2(self) {
	let $Q = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $K = __guarded(cleanup);
			let $L = null;
			if ($K[0] === 0) {
				const message = $K[1];
				if ($M(failure)) {
					failure = [ 0, message ];
				}
				$L = undefined;
			} else {
				$L = undefined;
			}
			$L;
		}
		self[0].v = [  ];
		const $O = failure;
		let $P = null;
		if ($O[0] === 0) {
			const message2 = $O[1];
			$P = (() => {
				throw message2;
			})();
		} else {
			$P = undefined;
		}
		$Q = $P;
	}
	return $Q;
}
function get_owner($F) {
	return $F;
}
function register_with_owner(subscription, $ae, $af) {
	const $ag = $af;
	let $ah = null;
	if ($ag[0] === 0) {
		const owner = $ag[1];
		$ah = $G(owner, subscription, $ae);
	} else {
		$ah = __clone(subscription);
	}
	return $ah;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $d(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	signal[1].v.push([ id, () => {
		const $e = [ 0, cell ];
		let $f = null;
		if ($e[0] === 0) {
			const live2 = $e[1];
			$f = observer(live2.v);
		} else {
			$f = undefined;
		}
		return $f;
	}, live ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $g(self) {
	return __clone(self[0].v);
}
function $c(self, observer) {
	const subscription = $d(self, observer);
	observer($g(self));
	return subscription;
}
function $h(self, observer) {
	return $d(self, observer);
}
function $o(self) {
	return self.length === 0;
}
function $p(self) {
	return __list_get(self, self.length - 1);
}
function $k(self, $l) {
	const $m = $l;
	let $n = null;
	if ($m[0] === 0) {
		const turn = $m[1];
		$n = enqueue(turn, self[1].v);
	} else {
		const $q = $p(draining_turns.v);
		let $r = null;
		if ($q[0] === 0) {
			const draining = $q[1];
			$r = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$r = undefined;
		}
		$n = $r;
	}
	return $n;
}
function $i(self, value, $j) {
	self[0].v = __clone(value);
	$k(self, $j);
}
function $G(self, item, $H) {
	if (self[1].v) {
		dispose(item, $H);
	} else {
		self[0].v.push(() => {
			dispose(item, $H);
			return;
		});
	}
	return __clone(item);
}
function $C(self, observer, $D, $E) {
	$G(get_owner($E), $h(self, observer), $D);
}
function $I(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $M(self) {
	const $N = self;
	return $N[0] === 1;
}
function $S(self, observer) {
	return $h(self[0], observer);
}
function $T(self) {
	return $g(self[0]);
}
function $R(self, observer) {
	const subscription = $S(self, observer);
	observer($T(self));
	return subscription;
}
function $Z(self, $l) {
	const $aa = $l;
	let $ab = null;
	if ($aa[0] === 0) {
		const turn = $aa[1];
		$ab = enqueue(turn, self[1].v);
	} else {
		const $ac = $p(draining_turns.v);
		let $ad = null;
		if ($ac[0] === 0) {
			const draining = $ac[1];
			$ad = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$ad = undefined;
		}
		$ab = $ad;
	}
	return $ab;
}
function $Y(self, value, $j) {
	self[0].v = __clone(value);
	$Z(self, $j);
}
function $U(self, transform, $V, $W) {
	const derived = $b(transform($T(self)));
	register_with_owner($S(self, (value) => {
		$Y(derived, transform(value), $V);
		return;
	}), $V, $W);
	return derived;
}
function $aj(self, observer) {
	const subscription = $d(self, observer);
	observer($g(self));
	return subscription;
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const count = $a(1);
const eager = $c(count, (value) => {
	return console.log("sub " + value);
});
const quiet = $h(count, (value) => {
	return console.log("on_change " + value);
});
console.log("attached");
$i(count, 2, [ 1 ]);
dispose(eager, [ 1 ]);
dispose(quiet, [ 1 ]);
$i(count, 3, [ 1 ]);
const $J = $I(($B) => {
	$C(count, (value) => {
		return console.log("effect_on_change " + value);
	}, [ 1 ], $B);
	return;
});
const _built = $J[0];
const scope = $J[1];
$i(count, 4, [ 1 ]);
dispose2(scope);
$i(count, 5, [ 1 ]);
const stored = [ $a(10) ];
const eagerly = $R(stored, (value) => {
	return console.log("eager " + value);
});
$i(stored[0], 11, [ 1 ]);
dispose(eagerly, [ 1 ]);
const watched = $S(stored, (value) => {
	return console.log("stored " + value);
});
$i(stored[0], 12, [ 1 ]);
dispose(watched, [ 1 ]);
const labelled = $U(stored, (value) => {
	return "n=" + value;
}, [ 1 ], [ 1 ]);
console.log($g(labelled));
const shown = $aj(labelled, (value) => {
	return console.log("label " + value);
});
$i(stored[0], 13, [ 1 ]);
dispose(shown, [ 1 ]);
