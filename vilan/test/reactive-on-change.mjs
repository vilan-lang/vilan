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
			while (!($m(turn[0].v)) && budget > 0) {
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[1].v = new Map();
				for (const subscriber of wave) {
					subscriber[1]();
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
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const ambient = $q;
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
		turn[1].v.delete(hash(self[1]));
		$s = undefined;
	} else {
		$s = undefined;
	}
	$s;
	const $t = self[2].v;
	let $u = null;
	if ($t[0] === 0) {
		const release = $t[1];
		self[2].v = [ 1 ];
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
function new2() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function dispose2(self) {
	let $K = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $E = __guarded(cleanup);
			let $F = null;
			if ($E[0] === 0) {
				const message = $E[1];
				if ($G(failure)) {
					failure = [ 0, message ];
				}
				$F = undefined;
			} else {
				$F = undefined;
			}
			$F;
		}
		self[0].v = [  ];
		const $I = failure;
		let $J = null;
		if ($I[0] === 0) {
			const message2 = $I[1];
			$J = (() => {
				throw message2;
			})();
		} else {
			$J = undefined;
		}
		$K = $J;
	}
	return $K;
}
function get_owner($z) {
	return $z;
}
function register_with_owner(subscription, $Y, $Z) {
	const $aa = $Z;
	let $ab = null;
	if ($aa[0] === 0) {
		const owner = $aa[1];
		$ab = $A(owner, subscription, $Y);
	} else {
		$ab = __clone(subscription);
	}
	return $ab;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers), fresh_id() ];
}
function $a(value) {
	return $b(value);
}
function $d(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $e(self) {
	return __clone(self[0].v);
}
function $c(self, observer) {
	const subscription = $d(self, observer);
	observer($e(self));
	return subscription;
}
function $f(self, observer) {
	return $d(self, observer);
}
function $m(self) {
	return self.length === 0;
}
function $n(self) {
	return __list_get(self, self.length - 1);
}
function $i(self, $j) {
	const $k = $j;
	let $l = null;
	if ($k[0] === 0) {
		const turn = $k[1];
		$l = enqueue(turn, self[1].v);
	} else {
		const $o = $n(draining_turns.v);
		let $p = null;
		if ($o[0] === 0) {
			const draining = $o[1];
			$p = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$p = undefined;
		}
		$l = $p;
	}
	return $l;
}
function $g(self, value, $h) {
	self[0].v = __clone(value);
	$i(self, $h);
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
function $w(self, observer, $x, $y) {
	$A(get_owner($y), $f(self, observer), $x);
}
function $C(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $G(self) {
	const $H = self;
	return $H[0] === 1;
}
function $M(self, observer) {
	return $f(self[0], observer);
}
function $N(self) {
	return $e(self[0]);
}
function $L(self, observer) {
	const subscription = $M(self, observer);
	observer($N(self));
	return subscription;
}
function $T(self, $j) {
	const $U = $j;
	let $V = null;
	if ($U[0] === 0) {
		const turn = $U[1];
		$V = enqueue(turn, self[1].v);
	} else {
		const $W = $n(draining_turns.v);
		let $X = null;
		if ($W[0] === 0) {
			const draining = $W[1];
			$X = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$X = undefined;
		}
		$V = $X;
	}
	return $V;
}
function $S(self, value, $h) {
	self[0].v = __clone(value);
	$T(self, $h);
}
function $O(self, transform, $P, $Q) {
	const derived = $b(transform($N(self)));
	register_with_owner($M(self, (value) => {
		$S(derived, transform(value), $P);
		return;
	}), $P, $Q);
	return derived;
}
function $ad(self, observer) {
	const subscription = $d(self, observer);
	observer($e(self));
	return subscription;
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const count = $a(1);
const eager = $c(count, (value) => {
	return console.log("sub " + value);
});
const lazy = $f(count, (value) => {
	return console.log("on_change " + value);
});
console.log("attached");
$g(count, 2, [ 1 ]);
dispose(eager, [ 1 ]);
dispose(lazy, [ 1 ]);
$g(count, 3, [ 1 ]);
const $D = $C(($v) => {
	$w(count, (value) => {
		return console.log("effect_on_change " + value);
	}, [ 1 ], $v);
	return;
});
const _built = $D[0];
const scope = $D[1];
$g(count, 4, [ 1 ]);
dispose2(scope);
$g(count, 5, [ 1 ]);
const stored = [ $a(10) ];
const eagerly = $L(stored, (value) => {
	return console.log("eager " + value);
});
$g(stored[0], 11, [ 1 ]);
dispose(eagerly, [ 1 ]);
const watched = $M(stored, (value) => {
	return console.log("stored " + value);
});
$g(stored[0], 12, [ 1 ]);
dispose(watched, [ 1 ]);
const labelled = $O(stored, (value) => {
	return "n=" + value;
}, [ 1 ], [ 1 ]);
console.log($e(labelled));
const shown = $ad(labelled, (value) => {
	return console.log("label " + value);
});
$g(stored[0], 13, [ 1 ]);
dispose(shown, [ 1 ]);
