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
			while (!($E(turn[0].v)) && budget > 0) {
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
	const ambient = $q;
	const $t = ambient;
	let $u = null;
	if ($t[0] === 0) {
		const turn = $t[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$u = undefined;
	} else {
		$u = undefined;
	}
	$u;
	const $v = self[3].v;
	let $w = null;
	if ($v[0] === 0) {
		const release = $v[1];
		self[3].v = [ 1 ];
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
function new2() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function dispose2(self) {
	let $O = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $I = __guarded(cleanup);
			let $J = null;
			if ($I[0] === 0) {
				const message = $I[1];
				if ($K(failure)) {
					failure = [ 0, message ];
				}
				$J = undefined;
			} else {
				$J = undefined;
			}
			$J;
		}
		self[0].v = [  ];
		const $M = failure;
		let $N = null;
		if ($M[0] === 0) {
			const message2 = $M[1];
			$N = (() => {
				throw message2;
			})();
		} else {
			$N = undefined;
		}
		$O = $N;
	}
	return $O;
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
function $x(self) {
	return __clone(self[0].v);
}
function $d(self, observer, $e, $f) {
	$g(self, observer, $e, $f);
	observer($x(self));
}
function $E(self) {
	return self.length === 0;
}
function $F(self) {
	return __list_get(self, self.length - 1);
}
function $A(self, $B) {
	const $C = $B;
	let $D = null;
	if ($C[0] === 0) {
		const turn = $C[1];
		$D = enqueue(turn, self[1].v);
	} else {
		const $G = $F(draining_turns.v);
		let $H = null;
		if ($G[0] === 0) {
			const draining = $G[1];
			$H = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$H = undefined;
		}
		$D = $H;
	}
	return $D;
}
function $y(self, value, $z) {
	self[0].v = __clone(value);
	$A(self, $z);
}
function $K(self) {
	const $L = self;
	return $L[0] === 1;
}
function $S(owner2, body) {
	return body(owner2);
}
function $U(body) {
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
$y(count, 2, [ 1 ]);
dispose2(owner);
$y(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($P) => {
	(($Q) => {
		$d(count, (value) => {
			return console.log("inner " + value);
		}, [ 1 ], $Q);
		return;
	})(inner);
	$d(count, (value) => {
		return console.log("outer " + value);
	}, [ 1 ], $P);
	return;
})(outer);
$y(count, 4, [ 1 ]);
dispose2(inner);
$y(count, 5, [ 1 ]);
dispose2(outer);
$y(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$S(wrapped, ($R) => {
	$d(count, (value) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $R);
	return;
});
$y(count, 7, [ 1 ]);
dispose2(wrapped);
$y(count, 8, [ 1 ]);
console.log("fin");
const $V = $U(($T) => {
	$d(count, (value) => {
		return console.log("comp " + value);
	}, [ 1 ], $T);
	return "built";
});
const label = $V[0];
const scope = $V[1];
console.log(label);
$y(count, 9, [ 1 ]);
dispose2(scope);
$y(count, 10, [ 1 ]);
console.log("post");
