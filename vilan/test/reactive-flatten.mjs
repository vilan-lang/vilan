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
function mint_subscriber(notify) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify, derived);
}
function subscriber_of(notify, derived) {
	return [ fresh_id(), notify, __shared_new(true), derived ];
}
function is_quiescent(self) {
	return $p(self[0].v) && $p(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $D = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$D = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$D;
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
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function dispose(self, $l) {
	const $m = $l;
	let $n = null;
	if ($m[0] === 0) {
		const established = $m[1];
		$n = [ 0, established ];
	} else {
		$n = $o(draining_turns.v);
	}
	const ambient = $n;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $r = [ 0, handle[0] ];
	let $s = null;
	if ($r[0] === 0) {
		const subscribers = $r[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$s = undefined;
	} else {
		$s = undefined;
	}
	$s;
	const $t = ambient;
	let $u = null;
	if ($t[0] === 0) {
		const turn = $t[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== handle[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[2].v.delete(hash(handle[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn[1].v) {
			if (subscriber3[0] !== handle[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn[1].v = kept_derived;
		turn[3].v.delete(hash(handle[1]));
		$u = undefined;
	} else {
		$u = undefined;
	}
	$u;
	const $v = handle[3].v;
	let $w = null;
	if ($v[0] === 0) {
		const release = $v[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$w = undefined;
	} else {
		$w = undefined;
	}
	return $w;
}
function defer(self, cleanup) {
	if (self[1].v) {
		cleanup();
	} else {
		self[0].v.push(cleanup);
	}
}
function register_with_owner(subscription, $R, $S) {
	const $T = $S;
	let $U = null;
	if ($T[0] === 0) {
		const owner = $T[1];
		$U = $V(owner, subscription, $R);
	} else {
		$U = __clone(subscription);
	}
	return $U;
}
function defer_to_owner(cleanup, $Z) {
	const $aa = $Z;
	let $ab = null;
	if ($aa[0] === 0) {
		const owner = $aa[1];
		$ab = defer(owner, cleanup);
	} else {
		$ab = undefined;
	}
	return $ab;
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
function $p(self) {
	return self.length === 0;
}
function $o(self) {
	let $q = null;
	if ($p(self)) {
		$q = [ 1 ];
	} else {
		$q = __list_get(self, self.length - 1);
	}
	return $q;
}
function $z(self, $A) {
	const $B = $A;
	let $C = null;
	if ($B[0] === 0) {
		const turn = $B[1];
		$C = enqueue(turn, self[1].v);
	} else {
		const $F = $o(draining_turns.v);
		let $G = null;
		if ($F[0] === 0) {
			const draining = $F[1];
			$G = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$G = undefined;
		}
		$C = $G;
	}
	return $C;
}
function $x(self, value, $y) {
	self[0].v = __clone(value);
	$z(self, $y);
}
function $L(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $I(signal, observer) {
	const cell = signal[0];
	return $L(signal, mint_subscriber(() => {
		const $J = [ 0, cell ];
		let $K = null;
		if ($J[0] === 0) {
			const live = $J[1];
			$K = observer(live.v);
		} else {
			$K = undefined;
		}
		return $K;
	}));
}
function $H(self, observer) {
	const subscription = $I(self, observer);
	observer($h(self));
	return subscription;
}
function $N(signal, observer) {
	const cell = signal[0];
	return $L(signal, mint_subscriber(() => {
		const $O = [ 0, cell ];
		let $P = null;
		if ($O[0] === 0) {
			const live = $O[1];
			$P = observer(live.v);
		} else {
			$P = undefined;
		}
		return $P;
	}));
}
function $M(self, observer) {
	const subscription = $N(self, observer);
	observer($h(self));
	return subscription;
}
function $V(self, item, $W) {
	if (self[1].v) {
		dispose(item, $W);
	} else {
		self[0].v.push(() => {
			dispose(item, $W);
			return;
		});
	}
	return __clone(item);
}
function $e(self, $f, $g) {
	const derived = $b($h($h(self)));
	const inner_subscription = __shared_new([ 1 ]);
	as_derivation();
	register_with_owner($M(self, (inner) => {
		const $j = inner_subscription.v;
		let $k = null;
		if ($j[0] === 1) {
			$k = $j;
		} else {
			$k = [ 0, dispose($j[1], $f) ];
		}
		$k;
		as_derivation();
		inner_subscription.v = [ 0, $H(inner, (value) => {
			$x(derived, value, $f);
			return;
		}) ];
		return;
	}), $f, $g);
	defer_to_owner(() => {
		const $X = inner_subscription.v;
		let $Y = null;
		if ($X[0] === 1) {
			$Y = $X;
		} else {
			$Y = [ 0, dispose($X[1], $f) ];
		}
		$Y;
		inner_subscription.v = [ 1 ];
		return;
	}, $g);
	return derived;
}
function $ac(self, value, $y) {
	self[0].v = __clone(value);
	$z(self, $y);
}
function $as(self, observer) {
	return $I(self, observer);
}
function $ai(self, transform, $aj, $ak) {
	const derived = $b(transform($h(self)));
	as_derivation();
	register_with_owner($as(self, (value) => {
		$ac(derived, transform(value), $aj);
		return;
	}), $aj, $ak);
	return derived;
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const first = $a(1);
const second = $a(10);
const outer = $c(first);
const joined = $e(outer, [ 1 ], [ 1 ]);
console.log($h(joined));
$x(first, 2, [ 1 ]);
console.log($h(joined));
$ac(outer, second, [ 1 ]);
console.log($h(joined));
$x(first, 99, [ 1 ]);
console.log($h(joined));
$x(second, 11, [ 1 ]);
console.log($h(joined));
const doubled = $ai(joined, (value) => {
	return value * 2;
}, [ 1 ], [ 1 ]);
$x(second, 21, [ 1 ]);
console.log($h(doubled));
