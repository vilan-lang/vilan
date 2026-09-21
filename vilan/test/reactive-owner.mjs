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
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function is_quiescent(self) {
	return $I(self[0].v) && $I(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $H = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$H = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$H;
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
				while (!($I(turn[1].v)) && budget > 0) {
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
function dispose(self, $q) {
	self[2].v = false;
	const $r = [ 0, self[0] ];
	let $s = null;
	if ($r[0] === 0) {
		const subscribers = $r[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$s = undefined;
	} else {
		$s = undefined;
	}
	$s;
	const $t = $q;
	let $u = null;
	if ($t[0] === 0) {
		const established = $t[1];
		$u = [ 0, established ];
	} else {
		$u = $v(draining_turns.v);
	}
	const ambient = $u;
	const $w = ambient;
	let $x = null;
	if ($w[0] === 0) {
		const turn = $w[1];
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
		$x = undefined;
	} else {
		$x = undefined;
	}
	$x;
	const $y = self[3].v;
	let $z = null;
	if ($y[0] === 0) {
		const release = $y[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$z = undefined;
	} else {
		$z = undefined;
	}
	return $z;
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
function get_owner($j) {
	return $j;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $l(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	signal[1].v.push([ id, () => {
		const $m = [ 0, cell ];
		let $n = null;
		if ($m[0] === 0) {
			const live2 = $m[1];
			$n = observer(live2.v);
		} else {
			$n = undefined;
		}
		return $n;
	}, live, derived ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $k(self, observer) {
	return $l(self, observer);
}
function $v(self) {
	return __list_get(self, self.length - 1);
}
function $o(self, item, $p) {
	if (self[1].v) {
		dispose(item, $p);
	} else {
		self[0].v.push(() => {
			dispose(item, $p);
			return;
		});
	}
	return __clone(item);
}
function $g(self, observer, $h, $i) {
	$o(get_owner($i), $k(self, observer), $h);
}
function $A(self) {
	return __clone(self[0].v);
}
function $d(self, observer, $e, $f) {
	$g(self, observer, $e, $f);
	observer($A(self));
}
function $I(self) {
	return self.length === 0;
}
function $D(self, $E) {
	const $F = $E;
	let $G = null;
	if ($F[0] === 0) {
		const turn = $F[1];
		$G = enqueue(turn, self[1].v);
	} else {
		const $J = $v(draining_turns.v);
		let $K = null;
		if ($J[0] === 0) {
			const draining = $J[1];
			$K = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$K = undefined;
		}
		$G = $K;
	}
	return $G;
}
function $B(self, value, $C) {
	self[0].v = __clone(value);
	$D(self, $C);
}
function $N(self) {
	const $O = self;
	return $O[0] === 1;
}
function $V(owner2, body) {
	return body(owner2);
}
function $X(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const count = $a(1);
const owner = new2();
(($c) => {
	$d(count, (value) => {
		return console.log("seen " + value);
	}, [ 1 ], $c);
	return;
})(owner);
$B(count, 2, [ 1 ]);
dispose2(owner);
$B(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($S) => {
	(($T) => {
		$d(count, (value) => {
			return console.log("inner " + value);
		}, [ 1 ], $T);
		return;
	})(inner);
	$d(count, (value) => {
		return console.log("outer " + value);
	}, [ 1 ], $S);
	return;
})(outer);
$B(count, 4, [ 1 ]);
dispose2(inner);
$B(count, 5, [ 1 ]);
dispose2(outer);
$B(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$V(wrapped, ($U) => {
	$d(count, (value) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $U);
	return;
});
$B(count, 7, [ 1 ]);
dispose2(wrapped);
$B(count, 8, [ 1 ]);
console.log("fin");
const $Y = $X(($W) => {
	$d(count, (value) => {
		return console.log("comp " + value);
	}, [ 1 ], $W);
	return "built";
});
const label = $Y[0];
const scope = $Y[1];
console.log(label);
$B(count, 9, [ 1 ]);
dispose2(scope);
$B(count, 10, [ 1 ]);
console.log("post");
