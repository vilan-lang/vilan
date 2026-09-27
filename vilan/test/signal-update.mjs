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
function mint_subscriber(notify) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify, derived);
}
function subscriber_of(notify, derived) {
	return [ fresh_id(), notify, __shared_new(true), derived ];
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
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function dispose(self, $R) {
	const $S = $R;
	let $T = null;
	if ($S[0] === 0) {
		const established = $S[1];
		$T = [ 0, established ];
	} else {
		$T = $k(draining_turns.v);
	}
	const ambient = $T;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $U = [ 0, handle[0] ];
	let $V = null;
	if ($U[0] === 0) {
		const subscribers = $U[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$V = undefined;
	} else {
		$V = undefined;
	}
	$V;
	const $W = ambient;
	let $X = null;
	if ($W[0] === 0) {
		const turn = $W[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== handle[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[2].v.delete(hash2(handle[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn[1].v) {
			if (subscriber3[0] !== handle[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn[1].v = kept_derived;
		turn[3].v.delete(hash2(handle[1]));
		$X = undefined;
	} else {
		$X = undefined;
	}
	$X;
	const $Y = handle[3].v;
	let $Z = null;
	if ($Y[0] === 0) {
		const release = $Y[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$Z = undefined;
	} else {
		$Z = undefined;
	}
	return $Z;
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
	let $m = null;
	if ($j(self)) {
		$m = [ 1 ];
	} else {
		$m = __list_get(self, self.length - 1);
	}
	return $m;
}
function $e(self, $f) {
	const $g = $f;
	let $h = null;
	if ($g[0] === 0) {
		const turn = $g[1];
		$h = enqueue(turn, self[1].v);
	} else {
		const $n = $k(draining_turns.v);
		let $o = null;
		if ($n[0] === 0) {
			const draining = $n[1];
			$o = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$o = undefined;
		}
		$h = $o;
	}
	return $h;
}
function $c(self, mutate, $d) {
	mutate(self[0].v);
	$e(self, $d);
}
function $p(self) {
	return __clone(self[0].v);
}
function $q(value) {
	return $b(value);
}
function $s(self, key, value) {
	self[0].set(hash(key), [ __clone(key), __clone(value) ]);
}
function $u(self, $f) {
	const $v = $f;
	let $w = null;
	if ($v[0] === 0) {
		const turn = $v[1];
		$w = enqueue(turn, self[1].v);
	} else {
		const $x = $k(draining_turns.v);
		let $y = null;
		if ($x[0] === 0) {
			const draining = $x[1];
			$y = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$y = undefined;
		}
		$w = $y;
	}
	return $w;
}
function $t(self, mutate, $d) {
	mutate(self[0].v);
	$u(self, $d);
}
function $A(self) {
	return self[0].size;
}
function $D(self, mutate, $d) {
	mutate([ self[0], "v" ]);
	$u(self, $d);
}
function $O(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $L(signal, observer) {
	const cell = signal[0];
	return $O(signal, mint_subscriber(() => {
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
	observer($p(self));
	return subscription;
}
function $P(self, item, $Q) {
	if (self[1].v) {
		dispose(item, $Q);
	} else {
		self[0].v.push(() => {
			dispose(item, $Q);
			return;
		});
	}
	return __clone(item);
}
function $ab(body, $ac) {
	const $ad = $ac;
	let $ae = null;
	if ($ad[0] === 0) {
		const current = $ad[1];
		$ae = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$ae = result;
	}
	return $ae;
}
function $ah(self, value, $ai) {
	self[0].v = __clone(value);
	$u(self, $ai);
}
function $af(self, transform, $ag) {
	$ah(self, transform($p(self)), $ag);
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
console.log($p(todos).length);
const scores = $q(new2());
$t(scores, (entries) => {
	$s(entries, "a", 1);
	$s(entries, "b", 2);
	return;
}, [ 1 ]);
console.log($A($p(scores)));
const count = $q(1);
$D(count, (value) => {
	value[0][value[1]] = value[0][value[1]] + 10;
	return;
}, [ 1 ]);
console.log($p(count));
const watched = $a([ 0 ]);
$P(owner, $K(watched, (list) => {
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
$ab(($aa) => {
	$c(watched, (list) => {
		list.push(3);
		return;
	}, [ 0, $aa ]);
	$c(watched, (list) => {
		list.push(4);
		return;
	}, [ 0, $aa ]);
	console.log("inside");
	return;
}, [ 1 ]);
$c(todos, (list) => {
	list.push(6);
	console.log("reentrant " + $p(todos).length);
	return;
}, [ 1 ]);
$af(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($p(count));
