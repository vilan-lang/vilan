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
	return $C(self[0].v) && $C(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $B = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$B = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$B;
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
				while (!($C(turn[1].v)) && budget > 0) {
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
		turn[2].v.delete(hash(self[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn[1].v) {
			if (subscriber3[0] !== self[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn[1].v = kept_derived;
		turn[3].v.delete(hash(self[1]));
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
function register_with_owner(subscription, $P, $Q) {
	const $R = $Q;
	let $S = null;
	if ($R[0] === 0) {
		const owner = $R[1];
		$S = $T(owner, subscription, $P);
	} else {
		$S = __clone(subscription);
	}
	return $S;
}
function defer_to_owner(cleanup, $X) {
	const $Y = $X;
	let $Z = null;
	if ($Y[0] === 0) {
		const owner = $Y[1];
		$Z = defer(owner, cleanup);
	} else {
		$Z = undefined;
	}
	return $Z;
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
function $C(self) {
	return self.length === 0;
}
function $x(self, $y) {
	const $z = $y;
	let $A = null;
	if ($z[0] === 0) {
		const turn = $z[1];
		$A = enqueue(turn, self[1].v);
	} else {
		const $D = $q(draining_turns.v);
		let $E = null;
		if ($D[0] === 0) {
			const draining = $D[1];
			$E = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$E = undefined;
		}
		$A = $E;
	}
	return $A;
}
function $v(self, value, $w) {
	self[0].v = __clone(value);
	$x(self, $w);
}
function $J(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $G(signal, observer) {
	const cell = signal[0];
	return $J(signal, mint_subscriber(() => {
		const $H = [ 0, cell ];
		let $I = null;
		if ($H[0] === 0) {
			const live = $H[1];
			$I = observer(live.v);
		} else {
			$I = undefined;
		}
		return $I;
	}));
}
function $F(self, observer) {
	const subscription = $G(self, observer);
	observer($h(self));
	return subscription;
}
function $L(signal, observer) {
	const cell = signal[0];
	return $J(signal, mint_subscriber(() => {
		const $M = [ 0, cell ];
		let $N = null;
		if ($M[0] === 0) {
			const live = $M[1];
			$N = observer(live.v);
		} else {
			$N = undefined;
		}
		return $N;
	}));
}
function $K(self, observer) {
	const subscription = $L(self, observer);
	observer($h(self));
	return subscription;
}
function $T(self, item, $U) {
	if (self[1].v) {
		dispose(item, $U);
	} else {
		self[0].v.push(() => {
			dispose(item, $U);
			return;
		});
	}
	return __clone(item);
}
function $e(self, $f, $g) {
	const derived = $b($h($h(self)));
	const inner_subscription = __shared_new([ 1 ]);
	as_derivation();
	register_with_owner($K(self, (inner) => {
		const $j = inner_subscription.v;
		let $k = null;
		if ($j[0] === 1) {
			$k = $j;
		} else {
			$k = [ 0, dispose($j[1], $f) ];
		}
		$k;
		as_derivation();
		inner_subscription.v = [ 0, $F(inner, (value) => {
			$v(derived, value, $f);
			return;
		}) ];
		return;
	}), $f, $g);
	defer_to_owner(() => {
		const $V = inner_subscription.v;
		let $W = null;
		if ($V[0] === 1) {
			$W = $V;
		} else {
			$W = [ 0, dispose($V[1], $f) ];
		}
		$W;
		inner_subscription.v = [ 1 ];
		return;
	}, $g);
	return derived;
}
function $ab(self, $y) {
	const $ac = $y;
	let $ad = null;
	if ($ac[0] === 0) {
		const turn = $ac[1];
		$ad = enqueue(turn, self[1].v);
	} else {
		const $ae = $q(draining_turns.v);
		let $af = null;
		if ($ae[0] === 0) {
			const draining = $ae[1];
			$af = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$af = undefined;
		}
		$ad = $af;
	}
	return $ad;
}
function $aa(self, value, $w) {
	self[0].v = __clone(value);
	$ab(self, $w);
}
function $ak(self, value, $w) {
	self[0].v = __clone(value);
	$ab(self, $w);
}
function $aq(self, observer) {
	return $G(self, observer);
}
function $ag(self, transform, $ah, $ai) {
	const derived = $b(transform($h(self)));
	as_derivation();
	register_with_owner($aq(self, (value) => {
		$ak(derived, transform(value), $ah);
		return;
	}), $ah, $ai);
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
$v(first, 2, [ 1 ]);
console.log($h(joined));
$aa(outer, second, [ 1 ]);
console.log($h(joined));
$v(first, 99, [ 1 ]);
console.log($h(joined));
$v(second, 11, [ 1 ]);
console.log($h(joined));
const doubled = $ag(joined, (value) => {
	return value * 2;
}, [ 1 ], [ 1 ]);
$v(second, 21, [ 1 ]);
console.log($h(doubled));
