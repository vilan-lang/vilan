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
	return $v(self[0].v) && $v(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $u = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$u = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$u;
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
				while (!($v(turn[1].v)) && budget > 0) {
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
function dispose(self, $J) {
	const $K = $J;
	let $L = null;
	if ($K[0] === 0) {
		const established = $K[1];
		$L = [ 0, established ];
	} else {
		$L = $w(draining_turns.v);
	}
	const ambient = $L;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $M = [ 0, handle[0] ];
	let $N = null;
	if ($M[0] === 0) {
		const subscribers = $M[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$N = undefined;
	} else {
		$N = undefined;
	}
	$N;
	const $O = ambient;
	let $P = null;
	if ($O[0] === 0) {
		const turn = $O[1];
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
		$P = undefined;
	} else {
		$P = undefined;
	}
	$P;
	const $Q = handle[3].v;
	let $R = null;
	if ($Q[0] === 0) {
		const release = $Q[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$R = undefined;
	} else {
		$R = undefined;
	}
	return $R;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function register_with_owner(subscription, $D, $E) {
	const $F = $E;
	let $G = null;
	if ($F[0] === 0) {
		const owner2 = $F[1];
		$G = $H(owner2, subscription, $D);
	} else {
		$G = __clone(subscription);
	}
	return $G;
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $B = previous;
		let $C = null;
		if ($B[0] === 0) {
			const earlier = $B[1];
			$C = earlier();
		} else {
			$C = undefined;
		}
		return $C;
	} ];
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $c(self, transform) {
	return [ __clone(self), transform ];
}
function $l(self) {
	return __clone(self[0].v);
}
function $n(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $m(self, subscriber) {
	return $n(self, subscriber);
}
function $k(self) {
	return [ () => {
		return $l(self);
	}, (subscriber) => {
		return $m(self, subscriber);
	}, () => {
		return;
	} ];
}
function $j(self) {
	const transform = self[1];
	const upstream = $k(__clone(self[0]));
	const pull = upstream[0];
	return [ () => {
		return transform(pull());
	}, upstream[1], upstream[2] ];
}
function $v(self) {
	return self.length === 0;
}
function $w(self) {
	let $y = null;
	if ($v(self)) {
		$y = [ 1 ];
	} else {
		$y = __list_get(self, self.length - 1);
	}
	return $y;
}
function $q(self, $r) {
	const $s = $r;
	let $t = null;
	if ($s[0] === 0) {
		const turn = $s[1];
		$t = enqueue(turn, self[1].v);
	} else {
		const $z = $w(draining_turns.v);
		let $A = null;
		if ($z[0] === 0) {
			const draining = $z[1];
			$A = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$A = undefined;
		}
		$t = $A;
	}
	return $t;
}
function $o(self, value, $p) {
	self[0].v = __clone(value);
	$q(self, $p);
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
function $g(self, $h, $i) {
	const instance = $j(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$o(cached, pull(), $h);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $h, $i);
	return cached;
}
function $d(self, $e, $f) {
	return [ $g(self, $e, $f) ];
}
function $V(signal, observer) {
	const cell = signal[0];
	return $n(signal, mint_subscriber(() => {
		const $W = [ 0, cell ];
		let $X = null;
		if ($W[0] === 0) {
			const live = $W[1];
			$X = observer(live.v);
		} else {
			$X = undefined;
		}
		return $X;
	}));
}
function $U(self, observer, immediately) {
	const subscription = $V(self, observer);
	if (immediately) {
		observer($l(self));
	}
	return subscription;
}
function $T(self, observer, immediately) {
	return $U(self[0], observer, immediately);
}
function $S(self, observer) {
	return $T(self, observer, true);
}
function $Y(self, transform, $Z) {
	$o(self, transform($l(self)), $Z);
}
function $aa(self) {
	return $l(self[0]);
}
function $ab(self, observer) {
	return $U(self, observer, true);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const owner = new2();
const count = $a(0);
const doubled = $d($c(__clone(count), (n) => {
	return n * 2;
}), [ 1 ], [ 1 ]);
$H(owner, $S(__clone(doubled), (n) => {
	return console.log(n);
}), [ 1 ]);
$o(count, 1, [ 1 ]);
$Y(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($aa(doubled));
$H(owner, $ab(__clone(count), (n) => {
	return console.log(n);
}), [ 1 ]);
$o(count, 20, [ 1 ]);
