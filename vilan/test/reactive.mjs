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
	return $o(self[0].v) && $o(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $n = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$n = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$n;
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
				while (!($o(turn[1].v)) && budget > 0) {
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
function dispose(self, $D) {
	const $E = $D;
	let $F = null;
	if ($E[0] === 0) {
		const established = $E[1];
		$F = [ 0, established ];
	} else {
		$F = $p(draining_turns.v);
	}
	const ambient = $F;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $G = [ 0, handle[0] ];
	let $H = null;
	if ($G[0] === 0) {
		const subscribers = $G[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$H = undefined;
	} else {
		$H = undefined;
	}
	$H;
	const $I = ambient;
	let $J = null;
	if ($I[0] === 0) {
		const turn = $I[1];
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
		$J = undefined;
	} else {
		$J = undefined;
	}
	$J;
	const $K = handle[3].v;
	let $L = null;
	if ($K[0] === 0) {
		const release = $K[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$L = undefined;
	} else {
		$L = undefined;
	}
	return $L;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function register_with_owner(subscription, $x, $y) {
	const $z = $y;
	let $A = null;
	if ($z[0] === 0) {
		const owner2 = $z[1];
		$A = $B(owner2, subscription, $x);
	} else {
		$A = __clone(subscription);
	}
	return $A;
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
function $o(self) {
	return self.length === 0;
}
function $p(self) {
	return __list_get(self, self.length - 1);
}
function $j(self, $k) {
	const $l = $k;
	let $m = null;
	if ($l[0] === 0) {
		const turn = $l[1];
		$m = enqueue(turn, self[1].v);
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
		$m = $r;
	}
	return $m;
}
function $h(self, value, $i) {
	self[0].v = __clone(value);
	$j(self, $i);
}
function $w(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $t(signal, observer) {
	const cell = signal[0];
	return $w(signal, mint_subscriber(() => {
		const $u = [ 0, cell ];
		let $v = null;
		if ($u[0] === 0) {
			const live = $u[1];
			$v = observer(live.v);
		} else {
			$v = undefined;
		}
		return $v;
	}));
}
function $s(self, observer) {
	return $t(self, observer);
}
function $B(self, item, $C) {
	if (self[1].v) {
		dispose(item, $C);
	} else {
		self[0].v.push(() => {
			dispose(item, $C);
			return;
		});
	}
	return __clone(item);
}
function $c(self, transform, $d, $e) {
	const derived = $b(transform($f(self)));
	as_derivation();
	register_with_owner($s(self, (value) => {
		$h(derived, transform(value), $d);
		return;
	}), $d, $e);
	return derived;
}
function $M(self, observer) {
	const subscription = $t(self, observer);
	observer($f(self));
	return subscription;
}
function $O(self, $k) {
	const $P = $k;
	let $Q = null;
	if ($P[0] === 0) {
		const turn = $P[1];
		$Q = enqueue(turn, self[1].v);
	} else {
		const $R = $p(draining_turns.v);
		let $S = null;
		if ($R[0] === 0) {
			const draining = $R[1];
			$S = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$S = undefined;
		}
		$Q = $S;
	}
	return $Q;
}
function $N(self, value, $i) {
	self[0].v = __clone(value);
	$O(self, $i);
}
function $T(self, transform, $U) {
	$N(self, transform($f(self)), $U);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const owner = new2();
const count = $a(0);
const doubled = $c(count, (n) => {
	return n * 2;
}, [ 1 ], [ 1 ]);
$B(owner, $M(doubled, (n) => {
	return console.log(n);
}), [ 1 ]);
$N(count, 1, [ 1 ]);
$T(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($f(doubled));
$B(owner, $M(count, (n) => {
	return console.log(n);
}), [ 1 ]);
$N(count, 20, [ 1 ]);
