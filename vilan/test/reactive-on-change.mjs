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
function __insert_at(list, index, value) {
	if (index >= 0 && index < list.length) return void list.splice(index, 0, value);
	if (index === list.length) return void list.push(value);
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
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
function as_derivation() {
	minting_derivation.v = true;
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function is_quiescent(self) {
	return $p(self[0].v) && $p(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $o = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$o = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$o;
	}
	if (turn[5].v && !(turn[6].v) && !(turn[4].v)) {
		turn[6].v = true;
		queueMicrotask(() => {
			turn[6].v = false;
			drain(turn);
			return;
		});
	}
}
function drain(turn) {
	if (!(turn[4].v)) {
		turn[4].v = true;
		draining_turns.v.push(__clone(turn));
		__with_finally(() => {
			let budget = 100000;
			while (!(is_quiescent(turn)) && budget > 0) {
				while (!($p(turn[1].v)) && budget > 0) {
					const derivations = turn[1].v;
					turn[1].v = [  ];
					turn[3].v = new Map();
					for (const subscriber of derivations) {
						if (subscriber[2].v) {
							subscriber[1]();
						}
						budget = budget - 1;
					}
				}
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[2].v = new Map();
				for (const subscriber2 of wave) {
					if (subscriber2[2].v) {
						subscriber2[1]();
					}
					budget = budget - 1;
				}
			}
			return;
		}, () => {
			__list_pop(draining_turns.v);
			turn[4].v = false;
			return;
		});
	}
}
function dispose(self, $t) {
	self[2].v = false;
	const $u = [ 0, self[0] ];
	let $v = null;
	if ($u[0] === 0) {
		const subscribers = $u[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$v = undefined;
	} else {
		$v = undefined;
	}
	$v;
	const $w = $t;
	let $x = null;
	if ($w[0] === 0) {
		const established = $w[1];
		$x = [ 0, established ];
	} else {
		$x = $q(draining_turns.v);
	}
	const ambient = $x;
	const $y = ambient;
	let $z = null;
	if ($y[0] === 0) {
		const turn = $y[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[2].v.delete(hash(self[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn[1].v) {
			if (subscriber3[0] !== self[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn[1].v = kept_derived;
		turn[3].v.delete(hash(self[1]));
		$z = undefined;
	} else {
		$z = undefined;
	}
	$z;
	const $A = self[3].v;
	let $B = null;
	if ($A[0] === 0) {
		const release = $A[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$B = undefined;
	} else {
		$B = undefined;
	}
	return $B;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function dispose2(self) {
	let $R = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $L = __guarded(cleanup);
			let $M = null;
			if ($L[0] === 0) {
				const message = $L[1];
				if ($N(failure)) {
					failure = [ 0, message ];
				}
				$M = undefined;
			} else {
				$M = undefined;
			}
			$M;
		}
		self[0].v = [  ];
		const $P = failure;
		let $Q = null;
		if ($P[0] === 0) {
			const message2 = $P[1];
			$Q = (() => {
				throw message2;
			})();
		} else {
			$Q = undefined;
		}
		$R = $Q;
	}
	return $R;
}
function get_owner($G) {
	return $G;
}
function register_with_owner(subscription, $af, $ag) {
	const $ah = $ag;
	let $ai = null;
	if ($ah[0] === 0) {
		const owner = $ah[1];
		$ai = $H(owner, subscription, $af);
	} else {
		$ai = __clone(subscription);
	}
	return $ai;
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
	const derived = minting_derivation.v;
	minting_derivation.v = false;
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
	}, live, derived ]);
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
function $p(self) {
	return self.length === 0;
}
function $q(self) {
	return __list_get(self, self.length - 1);
}
function $k(self, $l) {
	const $m = $l;
	let $n = null;
	if ($m[0] === 0) {
		const turn = $m[1];
		$n = enqueue(turn, self[1].v);
	} else {
		const $r = $q(draining_turns.v);
		let $s = null;
		if ($r[0] === 0) {
			const draining = $r[1];
			$s = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$s = undefined;
		}
		$n = $s;
	}
	return $n;
}
function $i(self, value, $j) {
	self[0].v = __clone(value);
	$k(self, $j);
}
function $H(self, item, $I) {
	if (self[1].v) {
		dispose(item, $I);
	} else {
		self[0].v.push(() => {
			dispose(item, $I);
			return;
		});
	}
	return __clone(item);
}
function $D(self, observer, $E, $F) {
	$H(get_owner($F), $h(self, observer), $E);
}
function $J(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $N(self) {
	const $O = self;
	return $O[0] === 1;
}
function $T(self, observer) {
	return $h(self[0], observer);
}
function $U(self) {
	return $g(self[0]);
}
function $S(self, observer) {
	const subscription = $T(self, observer);
	observer($U(self));
	return subscription;
}
function $aa(self, $l) {
	const $ab = $l;
	let $ac = null;
	if ($ab[0] === 0) {
		const turn = $ab[1];
		$ac = enqueue(turn, self[1].v);
	} else {
		const $ad = $q(draining_turns.v);
		let $ae = null;
		if ($ad[0] === 0) {
			const draining = $ad[1];
			$ae = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$ae = undefined;
		}
		$ac = $ae;
	}
	return $ac;
}
function $Z(self, value, $j) {
	self[0].v = __clone(value);
	$aa(self, $j);
}
function $V(self, transform, $W, $X) {
	const derived = $b(transform($U(self)));
	as_derivation();
	register_with_owner($T(self, (value) => {
		$Z(derived, transform(value), $W);
		return;
	}), $W, $X);
	return derived;
}
function $ak(self, observer) {
	const subscription = $d(self, observer);
	observer($g(self));
	return subscription;
}
const minting_derivation = __shared_new(false);
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
const $K = $J(($C) => {
	$D(count, (value) => {
		return console.log("effect_on_change " + value);
	}, [ 1 ], $C);
	return;
});
const _built = $K[0];
const scope = $K[1];
$i(count, 4, [ 1 ]);
dispose2(scope);
$i(count, 5, [ 1 ]);
const stored = [ $a(10) ];
const eagerly = $S(stored, (value) => {
	return console.log("eager " + value);
});
$i(stored[0], 11, [ 1 ]);
dispose(eagerly, [ 1 ]);
const watched = $T(stored, (value) => {
	return console.log("stored " + value);
});
$i(stored[0], 12, [ 1 ]);
dispose(watched, [ 1 ]);
const labelled = $V(stored, (value) => {
	return "n=" + value;
}, [ 1 ], [ 1 ]);
console.log($g(labelled));
const shown = $ak(labelled, (value) => {
	return console.log("label " + value);
});
$i(stored[0], 13, [ 1 ]);
dispose(shown, [ 1 ]);
