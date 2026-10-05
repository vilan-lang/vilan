function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
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
function __insert_at(list, index, value, location) {
	if (index >= 0 && index < list.length) return void list.splice(index, 0, value);
	if (index === list.length) return void list.push(value);
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __list_get(list, index) {
	return index >= 0 && index < list.length ? [ 0, __clone(list[index]) ] : [ 1 ];
}
function __list_pop(list) {
	return list.length === 0 ? [ 1 ] : [ 0, list.pop() ];
}
function __map_get(map, key) {
	return map.has(key) ? [ 0, __clone(map.get(key)) ] : [ 1 ];
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
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
function new2() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
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
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:414:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:417:25");
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
function dispose(self, $K) {
	const $L = $K;
	let $M = null;
	if ($L[0] === 0) {
		const established = $L[1];
		$M = [ 0, established ];
	} else {
		$M = $r(draining_turns.v);
	}
	const ambient = $M;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $N = [ 0, handle[0] ];
	let $O = null;
	if ($N[0] === 0) {
		const subscribers = $N[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$O = undefined;
	} else {
		$O = undefined;
	}
	$O;
	const $P = ambient;
	let $Q = null;
	if ($P[0] === 0) {
		const turn = $P[1];
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
		$Q = undefined;
	} else {
		$Q = undefined;
	}
	$Q;
	const $R = handle[3].v;
	let $S = null;
	if ($R[0] === 0) {
		const release = $R[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$S = undefined;
	} else {
		$S = undefined;
	}
	return $S;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $T = null;
	if (is_disposed(self)) {
		cleanup();
	} else {
		if (self[0].v[2]) {
			self[0].v[1].v.push(cleanup);
		} else {
			owner_lists_allocated_count.v = owner_lists_allocated_count.v + 1;
			self[0].v[1] = __shared_new([ cleanup ]);
			self[0].v[2] = true;
		}
		$T = undefined;
	}
	return $T;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $az = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $ao = held[3];
		let $ap = null;
		if ($ao[0] === 0) {
			const nursery = $ao[1];
			if ($aq(carried)) {
				nursery.cancel();
			}
			$ap = undefined;
		} else {
			$ap = undefined;
		}
		$ap;
		let $ay = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $as = __guarded(cleanup);
				let $at = null;
				if ($as[0] === 0) {
					const message = $as[1];
					if ($aq(failure)) {
						failure = [ 0, message ];
					}
					$at = undefined;
				} else {
					$at = undefined;
				}
				$at;
			}
			const $aw = failure;
			let $ax = null;
			if ($aw[0] === 0) {
				const message2 = $aw[1];
				$ax = (() => {
					throw __panic(message2, "std/src/reactive.vl:1207:27");
				})();
			} else {
				$ax = undefined;
			}
			$ay = $ax;
		}
		$az = $ay;
	}
	return $az;
}
function register_with_owner(subscription, $E, $F) {
	const $G = $F;
	let $H = null;
	if ($G[0] === 0) {
		const owner = $G[1];
		$H = $I(owner, subscription, $E);
	} else {
		$H = __clone(subscription);
	}
	return $H;
}
function defer_to_owner(cleanup, $aa) {
	const $ab = $aa;
	let $ac = null;
	if ($ab[0] === 0) {
		const owner = $ab[1];
		$ac = defer(owner, cleanup);
	} else {
		$ac = undefined;
	}
	return $ac;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $f(self) {
	return __clone(self[0].v);
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
		$o = enqueue(turn, __clone(self[1].v));
	} else {
		const $u = $r(draining_turns.v);
		let $v = null;
		if ($u[0] === 0) {
			const draining = $u[1];
			$v = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
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
function $D(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $A(signal, observer) {
	const cell = signal[0];
	return $D(signal, mint_subscriber(() => {
		const $B = [ 0, cell ];
		let $C = null;
		if ($B[0] === 0) {
			const live = $B[1];
			$C = observer(live.v);
		} else {
			$C = undefined;
		}
		return $C;
	}));
}
function $z(self, observer, immediately) {
	const subscription = $A(self, observer);
	if (immediately) {
		observer($f(self));
	}
	return subscription;
}
function $y(self, observer) {
	return $z(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false);
}
function $I(self, item, $J) {
	defer(self, () => {
		dispose(item, $J);
		return;
	});
	return __clone(item);
}
function $c(source, $d, $e) {
	const cells = __shared_new(new Map());
	const current2 = __shared_new($f(source));
	as_derivation();
	register_with_owner($y(__clone(source), (value, $g) => {
		const previous = current2.v;
		current2.v = __clone(value);
		const $h = __map_get(cells.v, hash(previous));
		let $i = null;
		if ($h[0] === 0) {
			const leaving = $h[1];
			$i = $j(leaving, false, $d);
		} else {
			$i = undefined;
		}
		$i;
		const $w = __map_get(cells.v, hash(value));
		let $x = null;
		if ($w[0] === 0) {
			const arriving = $w[1];
			$x = $j(arriving, true, $d);
		} else {
			$x = undefined;
		}
		return $x;
	}), $d, $e);
	return [ cells, current2 ];
}
function $V(self, key, $W) {
	const hash2 = hash(key);
	const $X = __map_get(self[0].v, hash2);
	let $Y = null;
	if ($X[0] === 0) {
		const existing = $X[1];
		$Y = existing;
	} else {
		const cell = $b(key === self[1].v);
		self[0].v.set(hash2, cell);
		defer_to_owner(() => {
			self[0].v.delete(hash2);
			return;
		}, $W);
		$Y = cell;
	}
	return $Y;
}
function $ad(owner, body) {
	return body(owner);
}
function $ai(self, $m) {
	const $aj = $m;
	let $ak = null;
	if ($aj[0] === 0) {
		const turn = $aj[1];
		$ak = enqueue(turn, __clone(self[1].v));
	} else {
		const $al = $r(draining_turns.v);
		let $am = null;
		if ($al[0] === 0) {
			const draining = $al[1];
			$am = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$am = undefined;
		}
		$ak = $am;
	}
	return $ak;
}
function $ah(self, value, $k) {
	self[0].v = __clone(value);
	$ai(self, $k);
}
function $aq(self) {
	const $ar = self;
	return $ar[0] === 1;
}
function $aC(body, $aD) {
	const $aE = $aD;
	let $aF = null;
	if ($aE[0] === 0) {
		const current2 = $aE[1];
		$aF = body(current2);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$aF = result;
	}
	return $aF;
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const current = $a(1);
const selected = $c(current, [ 1 ], [ 1 ]);
const rows = new3();
const one = $ad(rows, ($U) => {
	return $V(selected, 1, [ 0, $U ]);
});
const two = $ad(rows, ($ae) => {
	return $V(selected, 2, [ 0, $ae ]);
});
const three = $ad(rows, ($af) => {
	return $V(selected, 3, [ 0, $af ]);
});
console.log("seeded " + $f(one) + " " + $f(two) + " " + $f(three));
$ah(current, 2, [ 1 ]);
console.log("after 2: " + $f(one) + " " + $f(two) + " " + $f(three));
$ah(current, 9, [ 1 ]);
console.log("after 9: " + $f(one) + " " + $f(two) + " " + $f(three));
const again = $ad(rows, ($an) => {
	return $V(selected, 2, [ 0, $an ]);
});
$ah(current, 2, [ 1 ]);
console.log("same cell=" + $f(again) + " entries=" + selected[0].v.size);
dispose2(rows);
console.log("after dispose=" + selected[0].v.size);
const counted = $a(0);
let hits = 0;
const watch = $y(__clone(counted), (_, $aA) => {
	hits = hits + 1;
	return;
});
$aC(($aB) => {
	$ah(counted, 1, [ 0, $aB ]);
	$ah(counted, 2, [ 0, $aB ]);
	return;
}, [ 1 ]);
console.log("hits=" + hits);
dispose(watch, [ 1 ]);
