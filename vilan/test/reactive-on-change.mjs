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
function as_derivation() {
	minting_derivation.v = true;
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
	return $q(self[0].v) && $q(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $p = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$p = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$p;
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
				while (!($q(turn[1].v)) && budget > 0) {
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
function dispose(self, $w) {
	const $x = $w;
	let $y = null;
	if ($x[0] === 0) {
		const established = $x[1];
		$y = [ 0, established ];
	} else {
		$y = $r(draining_turns.v);
	}
	const ambient = $y;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $z = [ 0, handle[0] ];
	let $A = null;
	if ($z[0] === 0) {
		const subscribers = $z[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$A = undefined;
	} else {
		$A = undefined;
	}
	$A;
	const $B = ambient;
	let $C = null;
	if ($B[0] === 0) {
		const turn = $B[1];
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
		$C = undefined;
	} else {
		$C = undefined;
	}
	$C;
	const $D = handle[3].v;
	let $E = null;
	if ($D[0] === 0) {
		const release = $D[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$E = undefined;
	} else {
		$E = undefined;
	}
	return $E;
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
function get_owner($J) {
	return $J;
}
function register_with_owner(subscription, $ai, $aj) {
	const $ak = $aj;
	let $al = null;
	if ($ak[0] === 0) {
		const owner = $ak[1];
		$al = $K(owner, subscription, $ai);
	} else {
		$al = __clone(subscription);
	}
	return $al;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $g(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $d(signal, observer) {
	const cell = signal[0];
	return $g(signal, mint_subscriber(() => {
		const $e = [ 0, cell ];
		let $f = null;
		if ($e[0] === 0) {
			const live = $e[1];
			$f = observer(live.v);
		} else {
			$f = undefined;
		}
		return $f;
	}));
}
function $h(self) {
	return __clone(self[0].v);
}
function $c(self, observer) {
	const subscription = $d(self, observer);
	observer($h(self));
	return subscription;
}
function $i(self, observer) {
	return $d(self, observer);
}
function $q(self) {
	return self.length === 0;
}
function $r(self) {
	let $t = null;
	if ($q(self)) {
		$t = [ 1 ];
	} else {
		$t = __list_get(self, self.length - 1);
	}
	return $t;
}
function $l(self, $m) {
	const $n = $m;
	let $o = null;
	if ($n[0] === 0) {
		const turn = $n[1];
		$o = enqueue(turn, self[1].v);
	} else {
		const $u = $r(draining_turns.v);
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
		$o = $v;
	}
	return $o;
}
function $j(self, value, $k) {
	self[0].v = __clone(value);
	$l(self, $k);
}
function $K(self, item, $L) {
	if (self[1].v) {
		dispose(item, $L);
	} else {
		self[0].v.push(() => {
			dispose(item, $L);
			return;
		});
	}
	return __clone(item);
}
function $G(self, observer, $H, $I) {
	$K(get_owner($I), $i(self, observer), $H);
}
function $M(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $Q(self) {
	const $R = self;
	return $R[0] === 1;
}
function $W(self, observer) {
	return $i(self[0], observer);
}
function $X(self) {
	return $h(self[0]);
}
function $V(self, observer) {
	const subscription = $W(self, observer);
	observer($X(self));
	return subscription;
}
function $ad(self, $m) {
	const $ae = $m;
	let $af = null;
	if ($ae[0] === 0) {
		const turn = $ae[1];
		$af = enqueue(turn, self[1].v);
	} else {
		const $ag = $r(draining_turns.v);
		let $ah = null;
		if ($ag[0] === 0) {
			const draining = $ag[1];
			$ah = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$ah = undefined;
		}
		$af = $ah;
	}
	return $af;
}
function $ac(self, value, $k) {
	self[0].v = __clone(value);
	$ad(self, $k);
}
function $Y(self, transform, $Z, $aa) {
	const derived = $b(transform($X(self)));
	as_derivation();
	register_with_owner($W(self, (value) => {
		$ac(derived, transform(value), $Z);
		return;
	}), $Z, $aa);
	return derived;
}
function $ao(signal, observer) {
	const cell = signal[0];
	return $g(signal, mint_subscriber(() => {
		const $ap = [ 0, cell ];
		let $aq = null;
		if ($ap[0] === 0) {
			const live = $ap[1];
			$aq = observer(live.v);
		} else {
			$aq = undefined;
		}
		return $aq;
	}));
}
function $an(self, observer) {
	const subscription = $ao(self, observer);
	observer($h(self));
	return subscription;
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const count = $a(1);
const eager = $c(count, (value) => {
	return console.log("sub " + value);
});
const quiet = $i(count, (value) => {
	return console.log("on_change " + value);
});
console.log("attached");
$j(count, 2, [ 1 ]);
dispose(eager, [ 1 ]);
dispose(quiet, [ 1 ]);
$j(count, 3, [ 1 ]);
const $N = $M(($F) => {
	$G(count, (value) => {
		return console.log("effect_on_change " + value);
	}, [ 1 ], $F);
	return;
});
const _built = $N[0];
const scope = $N[1];
$j(count, 4, [ 1 ]);
dispose2(scope);
$j(count, 5, [ 1 ]);
const stored = [ $a(10) ];
const eagerly = $V(stored, (value) => {
	return console.log("eager " + value);
});
$j(stored[0], 11, [ 1 ]);
dispose(eagerly, [ 1 ]);
const watched = $W(stored, (value) => {
	return console.log("stored " + value);
});
$j(stored[0], 12, [ 1 ]);
dispose(watched, [ 1 ]);
const labelled = $Y(stored, (value) => {
	return "n=" + value;
}, [ 1 ], [ 1 ]);
console.log($h(labelled));
const shown = $an(labelled, (value) => {
	return console.log("label " + value);
});
$j(stored[0], 13, [ 1 ]);
dispose(shown, [ 1 ]);
