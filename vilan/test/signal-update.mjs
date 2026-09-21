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
function hash2(self) {
	return __hash(self);
}
function new2() {
	const table = new Map();
	return [ table ];
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function new3() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return $j(self[0].v) && $j(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash2(subscriber[0]);
		let $i = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$i = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$i;
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
				while (!($j(turn[1].v)) && budget > 0) {
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
function dispose(self, $O) {
	self[2].v = false;
	const $P = [ 0, self[0] ];
	let $Q = null;
	if ($P[0] === 0) {
		const subscribers = $P[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$Q = undefined;
	} else {
		$Q = undefined;
	}
	$Q;
	const $R = $O;
	let $S = null;
	if ($R[0] === 0) {
		const established = $R[1];
		$S = [ 0, established ];
	} else {
		$S = $k(draining_turns.v);
	}
	const ambient = $S;
	const $T = ambient;
	let $U = null;
	if ($T[0] === 0) {
		const turn = $T[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[2].v.delete(hash2(self[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn[1].v) {
			if (subscriber3[0] !== self[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn[1].v = kept_derived;
		turn[3].v.delete(hash2(self[1]));
		$U = undefined;
	} else {
		$U = undefined;
	}
	$U;
	const $V = self[3].v;
	let $W = null;
	if ($V[0] === 0) {
		const release = $V[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$W = undefined;
	} else {
		$W = undefined;
	}
	return $W;
}
function new4() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $j(self) {
	return self.length === 0;
}
function $k(self) {
	return __list_get(self, self.length - 1);
}
function $e(self, $f) {
	const $g = $f;
	let $h = null;
	if ($g[0] === 0) {
		const turn = $g[1];
		$h = enqueue(turn, self[1].v);
	} else {
		const $l = $k(draining_turns.v);
		let $m = null;
		if ($l[0] === 0) {
			const draining = $l[1];
			$m = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$m = undefined;
		}
		$h = $m;
	}
	return $h;
}
function $c(self, mutate, $d) {
	mutate(self[0].v);
	$e(self, $d);
}
function $n(self) {
	return __clone(self[0].v);
}
function $o(value) {
	return $b(value);
}
function $q(self, key, value) {
	self[0].set(hash(key), [ __clone(key), __clone(value) ]);
}
function $s(self, $f) {
	const $t = $f;
	let $u = null;
	if ($t[0] === 0) {
		const turn = $t[1];
		$u = enqueue(turn, self[1].v);
	} else {
		const $v = $k(draining_turns.v);
		let $w = null;
		if ($v[0] === 0) {
			const draining = $v[1];
			$w = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$w = undefined;
		}
		$u = $w;
	}
	return $u;
}
function $r(self, mutate, $d) {
	mutate(self[0].v);
	$s(self, $d);
}
function $y(self) {
	return self[0].size;
}
function $B(self, mutate, $d) {
	mutate([ self[0], "v" ]);
	$s(self, $d);
}
function $J(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	signal[1].v.push([ id, () => {
		const $K = [ 0, cell ];
		let $L = null;
		if ($K[0] === 0) {
			const live2 = $K[1];
			$L = observer(live2.v);
		} else {
			$L = undefined;
		}
		return $L;
	}, live, derived ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $I(self, observer) {
	const subscription = $J(self, observer);
	observer($n(self));
	return subscription;
}
function $M(self, item, $N) {
	if (self[1].v) {
		dispose(item, $N);
	} else {
		self[0].v.push(() => {
			dispose(item, $N);
			return;
		});
	}
	return __clone(item);
}
function $Y(body, $Z) {
	const $aa = $Z;
	let $ab = null;
	if ($aa[0] === 0) {
		const current = $aa[1];
		$ab = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$ab = result;
	}
	return $ab;
}
function $ae(self, value, $af) {
	self[0].v = __clone(value);
	$s(self, $af);
}
function $ac(self, transform, $ad) {
	$ae(self, transform($n(self)), $ad);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const owner = new4();
const todos = $a([ 1, 2 ]);
$c(todos, (list) => {
	list.push(5);
	return;
}, [ 1 ]);
console.log($n(todos).length);
const scores = $o(new2());
$r(scores, (entries) => {
	$q(entries, "a", 1);
	$q(entries, "b", 2);
	return;
}, [ 1 ]);
console.log($y($n(scores)));
const count = $o(1);
$B(count, (value) => {
	value[0][value[1]] = value[0][value[1]] + 10;
	return;
}, [ 1 ]);
console.log($n(count));
const watched = $a([ 0 ]);
$M(owner, $I(watched, (list) => {
	return console.log("len " + list.length);
}), [ 1 ]);
$c(watched, (list) => {
	list.push(1);
	list.push(2);
	return;
}, [ 1 ]);
$c(watched, (list) => {
	return;
}, [ 1 ]);
console.log("---");
$Y(($X) => {
	$c(watched, (list) => {
		list.push(3);
		return;
	}, [ 0, $X ]);
	$c(watched, (list) => {
		list.push(4);
		return;
	}, [ 0, $X ]);
	console.log("inside");
	return;
}, [ 1 ]);
$c(todos, (list) => {
	list.push(6);
	console.log("reentrant " + $n(todos).length);
	return;
}, [ 1 ]);
$ac(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($n(count));
