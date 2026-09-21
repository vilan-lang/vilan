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
	return [ __shared_new([  ]), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash2(subscriber[0]);
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
			while (!($i(turn[0].v)) && budget > 0) {
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
function dispose(self, $N) {
	self[2].v = false;
	const $O = [ 0, self[0] ];
	let $P = null;
	if ($O[0] === 0) {
		const subscribers = $O[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$P = undefined;
	} else {
		$P = undefined;
	}
	$P;
	const $Q = $N;
	let $R = null;
	if ($Q[0] === 0) {
		const established = $Q[1];
		$R = [ 0, established ];
	} else {
		$R = $j(draining_turns.v);
	}
	const ambient = $R;
	const $S = ambient;
	let $T = null;
	if ($S[0] === 0) {
		const turn = $S[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash2(self[1]));
		$T = undefined;
	} else {
		$T = undefined;
	}
	$T;
	const $U = self[3].v;
	let $V = null;
	if ($U[0] === 0) {
		const release = $U[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$V = undefined;
	} else {
		$V = undefined;
	}
	return $V;
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
function $i(self) {
	return self.length === 0;
}
function $j(self) {
	return __list_get(self, self.length - 1);
}
function $e(self, $f) {
	const $g = $f;
	let $h = null;
	if ($g[0] === 0) {
		const turn = $g[1];
		$h = enqueue(turn, self[1].v);
	} else {
		const $k = $j(draining_turns.v);
		let $l = null;
		if ($k[0] === 0) {
			const draining = $k[1];
			$l = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$l = undefined;
		}
		$h = $l;
	}
	return $h;
}
function $c(self, mutate, $d) {
	mutate(self[0].v);
	$e(self, $d);
}
function $m(self) {
	return __clone(self[0].v);
}
function $n(value) {
	return $b(value);
}
function $p(self, key, value) {
	self[0].set(hash(key), [ __clone(key), __clone(value) ]);
}
function $r(self, $f) {
	const $s = $f;
	let $t = null;
	if ($s[0] === 0) {
		const turn = $s[1];
		$t = enqueue(turn, self[1].v);
	} else {
		const $u = $j(draining_turns.v);
		let $v = null;
		if ($u[0] === 0) {
			const draining = $u[1];
			$v = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$v = undefined;
		}
		$t = $v;
	}
	return $t;
}
function $q(self, mutate, $d) {
	mutate(self[0].v);
	$r(self, $d);
}
function $x(self) {
	return self[0].size;
}
function $A(self, mutate, $d) {
	mutate([ self[0], "v" ]);
	$r(self, $d);
}
function $I(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	signal[1].v.push([ id, () => {
		const $J = [ 0, cell ];
		let $K = null;
		if ($J[0] === 0) {
			const live2 = $J[1];
			$K = observer(live2.v);
		} else {
			$K = undefined;
		}
		return $K;
	}, live ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $H(self, observer) {
	const subscription = $I(self, observer);
	observer($m(self));
	return subscription;
}
function $L(self, item, $M) {
	if (self[1].v) {
		dispose(item, $M);
	} else {
		self[0].v.push(() => {
			dispose(item, $M);
			return;
		});
	}
	return __clone(item);
}
function $X(body, $Y) {
	const $Z = $Y;
	let $aa = null;
	if ($Z[0] === 0) {
		const current = $Z[1];
		$aa = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[3].v = true;
		$aa = result;
	}
	return $aa;
}
function $ad(self, value, $ae) {
	self[0].v = __clone(value);
	$r(self, $ae);
}
function $ab(self, transform, $ac) {
	$ad(self, transform($m(self)), $ac);
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const owner = new4();
const todos = $a([ 1, 2 ]);
$c(todos, (list) => {
	list.push(5);
	return;
}, [ 1 ]);
console.log($m(todos).length);
const scores = $n(new2());
$q(scores, (entries) => {
	$p(entries, "a", 1);
	$p(entries, "b", 2);
	return;
}, [ 1 ]);
console.log($x($m(scores)));
const count = $n(1);
$A(count, (value) => {
	value[0][value[1]] = value[0][value[1]] + 10;
	return;
}, [ 1 ]);
console.log($m(count));
const watched = $a([ 0 ]);
$L(owner, $H(watched, (list) => {
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
$X(($W) => {
	$c(watched, (list) => {
		list.push(3);
		return;
	}, [ 0, $W ]);
	$c(watched, (list) => {
		list.push(4);
		return;
	}, [ 0, $W ]);
	console.log("inside");
	return;
}, [ 1 ]);
$c(todos, (list) => {
	list.push(6);
	console.log("reentrant " + $m(todos).length);
	return;
}, [ 1 ]);
$ab(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($m(count));
