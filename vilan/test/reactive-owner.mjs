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
function mint_subscriber(notify) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify, derived);
}
function subscriber_of(notify, derived) {
	return [ fresh_id(), notify, __shared_new(true), derived ];
}
function is_quiescent(self) {
	return $J(self[0].v) && $J(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $I = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$I = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$I;
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
				while (!($J(turn[1].v)) && budget > 0) {
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
function dispose(self, $r) {
	self[2].v = false;
	const $s = [ 0, self[0] ];
	let $t = null;
	if ($s[0] === 0) {
		const subscribers = $s[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$t = undefined;
	} else {
		$t = undefined;
	}
	$t;
	const $u = $r;
	let $v = null;
	if ($u[0] === 0) {
		const established = $u[1];
		$v = [ 0, established ];
	} else {
		$v = $w(draining_turns.v);
	}
	const ambient = $v;
	const $x = ambient;
	let $y = null;
	if ($x[0] === 0) {
		const turn = $x[1];
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
		$y = undefined;
	} else {
		$y = undefined;
	}
	$y;
	const $z = self[3].v;
	let $A = null;
	if ($z[0] === 0) {
		const release = $z[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$A = undefined;
	} else {
		$A = undefined;
	}
	return $A;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function dispose2(self) {
	let $S = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $M = __guarded(cleanup);
			let $N = null;
			if ($M[0] === 0) {
				const message = $M[1];
				if ($O(failure)) {
					failure = [ 0, message ];
				}
				$N = undefined;
			} else {
				$N = undefined;
			}
			$N;
		}
		self[0].v = [  ];
		const $Q = failure;
		let $R = null;
		if ($Q[0] === 0) {
			const message2 = $Q[1];
			$R = (() => {
				throw message2;
			})();
		} else {
			$R = undefined;
		}
		$S = $R;
	}
	return $S;
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
function $o(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $l(signal, observer) {
	const cell = signal[0];
	return $o(signal, mint_subscriber(() => {
		const $m = [ 0, cell ];
		let $n = null;
		if ($m[0] === 0) {
			const live = $m[1];
			$n = observer(live.v);
		} else {
			$n = undefined;
		}
		return $n;
	}));
}
function $k(self, observer) {
	return $l(self, observer);
}
function $w(self) {
	return __list_get(self, self.length - 1);
}
function $p(self, item, $q) {
	if (self[1].v) {
		dispose(item, $q);
	} else {
		self[0].v.push(() => {
			dispose(item, $q);
			return;
		});
	}
	return __clone(item);
}
function $g(self, observer, $h, $i) {
	$p(get_owner($i), $k(self, observer), $h);
}
function $B(self) {
	return __clone(self[0].v);
}
function $d(self, observer, $e, $f) {
	$g(self, observer, $e, $f);
	observer($B(self));
}
function $J(self) {
	return self.length === 0;
}
function $E(self, $F) {
	const $G = $F;
	let $H = null;
	if ($G[0] === 0) {
		const turn = $G[1];
		$H = enqueue(turn, self[1].v);
	} else {
		const $K = $w(draining_turns.v);
		let $L = null;
		if ($K[0] === 0) {
			const draining = $K[1];
			$L = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$L = undefined;
		}
		$H = $L;
	}
	return $H;
}
function $C(self, value, $D) {
	self[0].v = __clone(value);
	$E(self, $D);
}
function $O(self) {
	const $P = self;
	return $P[0] === 1;
}
function $W(owner2, body) {
	return body(owner2);
}
function $Y(body) {
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
$C(count, 2, [ 1 ]);
dispose2(owner);
$C(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($T) => {
	(($U) => {
		$d(count, (value) => {
			return console.log("inner " + value);
		}, [ 1 ], $U);
		return;
	})(inner);
	$d(count, (value) => {
		return console.log("outer " + value);
	}, [ 1 ], $T);
	return;
})(outer);
$C(count, 4, [ 1 ]);
dispose2(inner);
$C(count, 5, [ 1 ]);
dispose2(outer);
$C(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$W(wrapped, ($V) => {
	$d(count, (value) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $V);
	return;
});
$C(count, 7, [ 1 ]);
dispose2(wrapped);
$C(count, 8, [ 1 ]);
console.log("fin");
const $Z = $Y(($X) => {
	$d(count, (value) => {
		return console.log("comp " + value);
	}, [ 1 ], $X);
	return "built";
});
const label = $Z[0];
const scope = $Z[1];
console.log(label);
$C(count, 9, [ 1 ]);
dispose2(scope);
$C(count, 10, [ 1 ]);
console.log("post");
