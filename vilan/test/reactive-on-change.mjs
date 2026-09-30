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
	return $r(self[0].v) && $r(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $q = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$q = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$q;
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
				while (!($r(turn[1].v)) && budget > 0) {
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
function dispose(self, $x) {
	const $y = $x;
	let $z = null;
	if ($y[0] === 0) {
		const established = $y[1];
		$z = [ 0, established ];
	} else {
		$z = $s(draining_turns.v);
	}
	const ambient = $z;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $A = [ 0, handle[0] ];
	let $B = null;
	if ($A[0] === 0) {
		const subscribers = $A[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$B = undefined;
	} else {
		$B = undefined;
	}
	$B;
	const $C = ambient;
	let $D = null;
	if ($C[0] === 0) {
		const turn = $C[1];
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
		$D = undefined;
	} else {
		$D = undefined;
	}
	$D;
	const $E = handle[3].v;
	let $F = null;
	if ($E[0] === 0) {
		const release = $E[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$F = undefined;
	} else {
		$F = undefined;
	}
	return $F;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function dispose2(self) {
	let $W = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $Q = __guarded(cleanup);
			let $R = null;
			if ($Q[0] === 0) {
				const message = $Q[1];
				if ($S(failure)) {
					failure = [ 0, message ];
				}
				$R = undefined;
			} else {
				$R = undefined;
			}
			$R;
		}
		self[0].v = [  ];
		const $U = failure;
		let $V = null;
		if ($U[0] === 0) {
			const message2 = $U[1];
			$V = (() => {
				throw message2;
			})();
		} else {
			$V = undefined;
		}
		$W = $V;
	}
	return $W;
}
function get_owner($K) {
	return $K;
}
function register_with_owner(subscription, $av, $aw) {
	const $ax = $aw;
	let $ay = null;
	if ($ax[0] === 0) {
		const owner = $ax[1];
		$ay = $M(owner, subscription, $av);
	} else {
		$ay = __clone(subscription);
	}
	return $ay;
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $at = previous;
		let $au = null;
		if ($at[0] === 0) {
			const earlier = $at[1];
			$au = earlier();
		} else {
			$au = undefined;
		}
		return $au;
	} ];
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $h(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $e(signal, observer) {
	const cell = signal[0];
	return $h(signal, mint_subscriber(() => {
		const $f = [ 0, cell ];
		let $g = null;
		if ($f[0] === 0) {
			const live = $f[1];
			$g = observer(live.v);
		} else {
			$g = undefined;
		}
		return $g;
	}));
}
function $i(self) {
	return __clone(self[0].v);
}
function $d(self, observer, immediately) {
	const subscription = $e(self, observer);
	if (immediately) {
		observer($i(self));
	}
	return subscription;
}
function $c(self, observer) {
	return $d(self, observer, true);
}
function $j(self, observer) {
	return $d(self, observer, false);
}
function $r(self) {
	return self.length === 0;
}
function $s(self) {
	let $u = null;
	if ($r(self)) {
		$u = [ 1 ];
	} else {
		$u = __list_get(self, self.length - 1);
	}
	return $u;
}
function $m(self, $n) {
	const $o = $n;
	let $p = null;
	if ($o[0] === 0) {
		const turn = $o[1];
		$p = enqueue(turn, self[1].v);
	} else {
		const $v = $s(draining_turns.v);
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
		$p = $w;
	}
	return $p;
}
function $k(self, value, $l) {
	self[0].v = __clone(value);
	$m(self, $l);
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
function $H(self, observer, $I, $J) {
	$M(get_owner($J), $j(self, observer), $I);
}
function $O(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $S(self) {
	const $T = self;
	return $T[0] === 1;
}
function $Z(self) {
	return $i(self[0]);
}
function $ab(self, subscriber) {
	return $h(self, subscriber);
}
function $aa(self, subscriber) {
	return $ab(self[0], subscriber);
}
function $Y(self, observer, immediately) {
	const subscription = $aa(self, mint_subscriber(() => {
		return observer($Z(self));
	}));
	if (immediately) {
		observer($Z(self));
	}
	return subscription;
}
function $X(self, observer) {
	return $Y(self, observer, true);
}
function $ac(self, observer) {
	return $Y(self, observer, false);
}
function $ad(self, transform) {
	return [ __clone(self), transform ];
}
function $al(self) {
	return [ () => {
		return $Z(self);
	}, (subscriber) => {
		return $aa(self, subscriber);
	}, () => {
		return;
	} ];
}
function $ak(self) {
	const transform = self[1];
	const upstream = $al(__clone(self[0]));
	const pull = upstream[0];
	return [ () => {
		return transform(pull());
	}, upstream[1], upstream[2] ];
}
function $ao(self, $n) {
	const $ap = $n;
	let $aq = null;
	if ($ap[0] === 0) {
		const turn = $ap[1];
		$aq = enqueue(turn, self[1].v);
	} else {
		const $ar = $s(draining_turns.v);
		let $as = null;
		if ($ar[0] === 0) {
			const draining = $ar[1];
			$as = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$as = undefined;
		}
		$aq = $as;
	}
	return $aq;
}
function $an(self, value, $l) {
	self[0].v = __clone(value);
	$ao(self, $l);
}
function $ah(self, $ai, $aj) {
	const instance = $ak(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$an(cached, pull(), $ai);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $ai, $aj);
	return cached;
}
function $ae(self, $af, $ag) {
	return [ $ah(self, $af, $ag) ];
}
function $az(self) {
	return $i(self[0]);
}
function $aE(signal, observer) {
	const cell = signal[0];
	return $h(signal, mint_subscriber(() => {
		const $aF = [ 0, cell ];
		let $aG = null;
		if ($aF[0] === 0) {
			const live = $aF[1];
			$aG = observer(live.v);
		} else {
			$aG = undefined;
		}
		return $aG;
	}));
}
function $aD(self, observer, immediately) {
	const subscription = $aE(self, observer);
	if (immediately) {
		observer($i(self));
	}
	return subscription;
}
function $aC(self, observer, immediately) {
	return $aD(self[0], observer, immediately);
}
function $aB(self, observer) {
	return $aC(self, observer, true);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const count = $a(1);
const eager = $c(__clone(count), (value) => {
	return console.log("sub " + value);
});
const quiet = $j(__clone(count), (value) => {
	return console.log("on_change " + value);
});
console.log("attached");
$k(count, 2, [ 1 ]);
dispose(eager, [ 1 ]);
dispose(quiet, [ 1 ]);
$k(count, 3, [ 1 ]);
const $P = $O(($G) => {
	$H(__clone(count), (value) => {
		return console.log("effect_on_change " + value);
	}, [ 1 ], $G);
	return;
});
const _built = $P[0];
const scope = $P[1];
$k(count, 4, [ 1 ]);
dispose2(scope);
$k(count, 5, [ 1 ]);
const stored = [ $a(10) ];
const eagerly = $X(__clone(stored), (value) => {
	return console.log("eager " + value);
});
$k(stored[0], 11, [ 1 ]);
dispose(eagerly, [ 1 ]);
const watched = $ac(__clone(stored), (value) => {
	return console.log("stored " + value);
});
$k(stored[0], 12, [ 1 ]);
dispose(watched, [ 1 ]);
const labelled = $ae($ad(__clone(stored), (value) => {
	return "n=" + value;
}), [ 1 ], [ 1 ]);
console.log($az(labelled));
const shown = $aB(labelled, (value) => {
	return console.log("label " + value);
});
$k(stored[0], 13, [ 1 ]);
dispose(shown, [ 1 ]);
