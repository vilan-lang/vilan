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
	return $v(self[0].v) && $v(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $K = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$K = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$K;
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
function dispose(self, $r) {
	const $s = $r;
	let $t = null;
	if ($s[0] === 0) {
		const established = $s[1];
		$t = [ 0, established ];
	} else {
		$t = $u(draining_turns.v);
	}
	const ambient = $t;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $x = [ 0, handle[0] ];
	let $y = null;
	if ($x[0] === 0) {
		const subscribers = $x[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$y = undefined;
	} else {
		$y = undefined;
	}
	$y;
	const $z = ambient;
	let $A = null;
	if ($z[0] === 0) {
		const turn = $z[1];
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
		$A = undefined;
	} else {
		$A = undefined;
	}
	$A;
	const $B = handle[3].v;
	let $C = null;
	if ($B[0] === 0) {
		const release = $B[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$C = undefined;
	} else {
		$C = undefined;
	}
	return $C;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function dispose2(self) {
	let $U = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $O = __guarded(cleanup);
			let $P = null;
			if ($O[0] === 0) {
				const message = $O[1];
				if ($Q(failure)) {
					failure = [ 0, message ];
				}
				$P = undefined;
			} else {
				$P = undefined;
			}
			$P;
		}
		self[0].v = [  ];
		const $S = failure;
		let $T = null;
		if ($S[0] === 0) {
			const message2 = $S[1];
			$T = (() => {
				throw message2;
			})();
		} else {
			$T = undefined;
		}
		$U = $T;
	}
	return $U;
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
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
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
function $v(self) {
	return self.length === 0;
}
function $u(self) {
	let $w = null;
	if ($v(self)) {
		$w = [ 1 ];
	} else {
		$w = __list_get(self, self.length - 1);
	}
	return $w;
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
function $D(self) {
	return __clone(self[0].v);
}
function $d(self, observer, $e, $f) {
	$g(self, observer, $e, $f);
	observer($D(self));
}
function $G(self, $H) {
	const $I = $H;
	let $J = null;
	if ($I[0] === 0) {
		const turn = $I[1];
		$J = enqueue(turn, self[1].v);
	} else {
		const $M = $u(draining_turns.v);
		let $N = null;
		if ($M[0] === 0) {
			const draining = $M[1];
			$N = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$N = undefined;
		}
		$J = $N;
	}
	return $J;
}
function $E(self, value, $F) {
	self[0].v = __clone(value);
	$G(self, $F);
}
function $Q(self) {
	const $R = self;
	return $R[0] === 1;
}
function $Y(owner2, body) {
	return body(owner2);
}
function $aa(body) {
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
$E(count, 2, [ 1 ]);
dispose2(owner);
$E(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($V) => {
	(($W) => {
		$d(count, (value) => {
			return console.log("inner " + value);
		}, [ 1 ], $W);
		return;
	})(inner);
	$d(count, (value) => {
		return console.log("outer " + value);
	}, [ 1 ], $V);
	return;
})(outer);
$E(count, 4, [ 1 ]);
dispose2(inner);
$E(count, 5, [ 1 ]);
dispose2(outer);
$E(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$Y(wrapped, ($X) => {
	$d(count, (value) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $X);
	return;
});
$E(count, 7, [ 1 ]);
dispose2(wrapped);
$E(count, 8, [ 1 ]);
console.log("fin");
const $ab = $aa(($Z) => {
	$d(count, (value) => {
		return console.log("comp " + value);
	}, [ 1 ], $Z);
	return "built";
});
const label = $ab[0];
const scope = $ab[1];
console.log(label);
$E(count, 9, [ 1 ]);
dispose2(scope);
$E(count, 10, [ 1 ]);
console.log("post");
