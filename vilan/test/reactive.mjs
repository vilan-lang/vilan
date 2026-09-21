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
function dispose(self, $C) {
	self[2].v = false;
	const $D = [ 0, self[0] ];
	let $E = null;
	if ($D[0] === 0) {
		const subscribers = $D[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$E = undefined;
	} else {
		$E = undefined;
	}
	$E;
	const $F = $C;
	let $G = null;
	if ($F[0] === 0) {
		const established = $F[1];
		$G = [ 0, established ];
	} else {
		$G = $p(draining_turns.v);
	}
	const ambient = $G;
	const $H = ambient;
	let $I = null;
	if ($H[0] === 0) {
		const turn = $H[1];
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
		$I = undefined;
	} else {
		$I = undefined;
	}
	$I;
	const $J = self[3].v;
	let $K = null;
	if ($J[0] === 0) {
		const release = $J[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$K = undefined;
	} else {
		$K = undefined;
	}
	return $K;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function register_with_owner(subscription, $w, $x) {
	const $y = $x;
	let $z = null;
	if ($y[0] === 0) {
		const owner2 = $y[1];
		$z = $A(owner2, subscription, $w);
	} else {
		$z = __clone(subscription);
	}
	return $z;
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
function $t(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	signal[1].v.push([ id, () => {
		const $u = [ 0, cell ];
		let $v = null;
		if ($u[0] === 0) {
			const live2 = $u[1];
			$v = observer(live2.v);
		} else {
			$v = undefined;
		}
		return $v;
	}, live, derived ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $s(self, observer) {
	return $t(self, observer);
}
function $A(self, item, $B) {
	if (self[1].v) {
		dispose(item, $B);
	} else {
		self[0].v.push(() => {
			dispose(item, $B);
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
function $L(self, observer) {
	const subscription = $t(self, observer);
	observer($f(self));
	return subscription;
}
function $N(self, $k) {
	const $O = $k;
	let $P = null;
	if ($O[0] === 0) {
		const turn = $O[1];
		$P = enqueue(turn, self[1].v);
	} else {
		const $Q = $p(draining_turns.v);
		let $R = null;
		if ($Q[0] === 0) {
			const draining = $Q[1];
			$R = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$R = undefined;
		}
		$P = $R;
	}
	return $P;
}
function $M(self, value, $i) {
	self[0].v = __clone(value);
	$N(self, $i);
}
function $S(self, transform, $T) {
	$M(self, transform($f(self)), $T);
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
$A(owner, $L(doubled, (n) => {
	return console.log(n);
}), [ 1 ]);
$M(count, 1, [ 1 ]);
$S(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($f(doubled));
$A(owner, $L(count, (n) => {
	return console.log(n);
}), [ 1 ]);
$M(count, 20, [ 1 ]);
