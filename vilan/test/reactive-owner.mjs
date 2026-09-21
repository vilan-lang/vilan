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
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
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
			while (!($H(turn[0].v)) && budget > 0) {
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
		turn[1].v.delete(hash(self[1]));
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
	let $Q = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $K = __guarded(cleanup);
			let $L = null;
			if ($K[0] === 0) {
				const message = $K[1];
				if ($M(failure)) {
					failure = [ 0, message ];
				}
				$L = undefined;
			} else {
				$L = undefined;
			}
			$L;
		}
		self[0].v = [  ];
		const $O = failure;
		let $P = null;
		if ($O[0] === 0) {
			const message2 = $O[1];
			$P = (() => {
				throw message2;
			})();
		} else {
			$P = undefined;
		}
		$Q = $P;
	}
	return $Q;
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
	}, live ]);
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
function $H(self) {
	return self.length === 0;
}
function $D(self, $E) {
	const $F = $E;
	let $G = null;
	if ($F[0] === 0) {
		const turn = $F[1];
		$G = enqueue(turn, self[1].v);
	} else {
		const $I = $v(draining_turns.v);
		let $J = null;
		if ($I[0] === 0) {
			const draining = $I[1];
			$J = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$J = undefined;
		}
		$G = $J;
	}
	return $G;
}
function $B(self, value, $C) {
	self[0].v = __clone(value);
	$D(self, $C);
}
function $M(self) {
	const $N = self;
	return $N[0] === 1;
}
function $U(owner2, body) {
	return body(owner2);
}
function $W(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
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
(($R) => {
	(($S) => {
		$d(count, (value) => {
			return console.log("inner " + value);
		}, [ 1 ], $S);
		return;
	})(inner);
	$d(count, (value) => {
		return console.log("outer " + value);
	}, [ 1 ], $R);
	return;
})(outer);
$B(count, 4, [ 1 ]);
dispose2(inner);
$B(count, 5, [ 1 ]);
dispose2(outer);
$B(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$U(wrapped, ($T) => {
	$d(count, (value) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $T);
	return;
});
$B(count, 7, [ 1 ]);
dispose2(wrapped);
$B(count, 8, [ 1 ]);
console.log("fin");
const $X = $W(($V) => {
	$d(count, (value) => {
		return console.log("comp " + value);
	}, [ 1 ], $V);
	return "built";
});
const label = $X[0];
const scope = $X[1];
console.log(label);
$B(count, 9, [ 1 ]);
dispose2(scope);
$B(count, 10, [ 1 ]);
console.log("post");
