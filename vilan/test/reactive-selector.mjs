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
function mint_subscriber(notify3) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify3, derived);
}
function subscriber_of(notify3, derived) {
	return [ fresh_id(), notify3, __shared_new(true), derived ];
}
function new2() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return is_empty(self[0].v) && is_empty(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $j = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$j = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:414:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:417:25");
		}
		$j;
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
				while (!(is_empty(turn[1].v)) && budget > 0) {
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
		$y = last(draining_turns.v);
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
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $F = null;
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
		$F = undefined;
	}
	return $F;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $ad = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $U = held[3];
		let $V = null;
		if ($U[0] === 0) {
			const nursery = $U[1];
			if (is_none(carried)) {
				nursery.cancel();
			}
			$V = undefined;
		} else {
			$V = undefined;
		}
		$V;
		let $ac = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $X = __guarded(cleanup);
				let $Y = null;
				if ($X[0] === 0) {
					const message = $X[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$Y = undefined;
				} else {
					$Y = undefined;
				}
				$Y;
			}
			const $aa = failure;
			let $ab = null;
			if ($aa[0] === 0) {
				const message2 = $aa[1];
				$ab = (() => {
					throw __panic(message2, "std/src/reactive.vl:1207:27");
				})();
			} else {
				$ab = undefined;
			}
			$ac = $ab;
		}
		$ad = $ac;
	}
	return $ad;
}
function register_with_owner(subscription, $r, $s) {
	const $t = $s;
	let $u = null;
	if ($t[0] === 0) {
		const owner = $t[1];
		$u = take(owner, subscription, $r);
	} else {
		$u = __clone(subscription);
	}
	return $u;
}
function defer_to_owner(cleanup, $K) {
	const $L = $K;
	let $M = null;
	if ($L[0] === 0) {
		const owner = $L[1];
		$M = defer(owner, cleanup);
	} else {
		$M = undefined;
	}
	return $M;
}
function new4(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function new5(value) {
	return new4(value);
}
function get(self) {
	return __clone(self[0].v);
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $k = null;
	if (is_empty(self)) {
		$k = [ 1 ];
	} else {
		$k = __list_get(self, self.length - 1);
	}
	return $k;
}
function notify(self, $g) {
	const $h = $g;
	let $i = null;
	if ($h[0] === 0) {
		const turn = $h[1];
		$i = enqueue(turn, __clone(self[1].v));
	} else {
		const $l = last(draining_turns.v);
		let $m = null;
		if ($l[0] === 0) {
			const draining = $l[1];
			$m = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$m = undefined;
		}
		$i = $m;
	}
	return $i;
}
function set(self, value, $f) {
	self[0].v = __clone(value);
	notify(self, $f);
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function observe(signal, observer) {
	const cell = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $p = [ 0, cell ];
		let $q = null;
		if ($p[0] === 0) {
			const live = $p[1];
			$q = observer(live.v);
		} else {
			$q = undefined;
		}
		return $q;
	}));
}
function attach_observer(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function on_change(self, observer) {
	return attach_observer(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false);
}
function take(self, item, $v) {
	defer(self, () => {
		dispose(item, $v);
		return;
	});
	return __clone(item);
}
function selector(source, $a, $b) {
	const cells = __shared_new(new Map());
	const current2 = __shared_new(get(source));
	as_derivation();
	register_with_owner(on_change(__clone(source), (value, $c) => {
		const previous = current2.v;
		current2.v = __clone(value);
		const $d = __map_get(cells.v, hash(previous));
		let $e = null;
		if ($d[0] === 0) {
			const leaving = $d[1];
			$e = set(leaving, false, $a);
		} else {
			$e = undefined;
		}
		$e;
		const $n = __map_get(cells.v, hash(value));
		let $o = null;
		if ($n[0] === 0) {
			const arriving = $n[1];
			$o = set(arriving, true, $a);
		} else {
			$o = undefined;
		}
		return $o;
	}), $a, $b);
	return [ cells, current2 ];
}
function of(self, key, $H) {
	const hash2 = hash(key);
	const $I = __map_get(self[0].v, hash2);
	let $J = null;
	if ($I[0] === 0) {
		const existing = $I[1];
		$J = existing;
	} else {
		const cell = new4(key === self[1].v);
		self[0].v.set(hash2, cell);
		defer_to_owner(() => {
			self[0].v.delete(hash2);
			return;
		}, $H);
		$J = cell;
	}
	return $J;
}
function run_with_owner(owner, body) {
	return body(owner);
}
function notify2(self, $g) {
	const $P = $g;
	let $Q = null;
	if ($P[0] === 0) {
		const turn = $P[1];
		$Q = enqueue(turn, __clone(self[1].v));
	} else {
		const $R = last(draining_turns.v);
		let $S = null;
		if ($R[0] === 0) {
			const draining = $R[1];
			$S = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$S = undefined;
		}
		$Q = $S;
	}
	return $Q;
}
function set2(self, value, $f) {
	self[0].v = __clone(value);
	notify2(self, $f);
}
function is_none(self) {
	const $W = self;
	return $W[0] === 1;
}
function batch(body, $ag) {
	const $ah = $ag;
	let $ai = null;
	if ($ah[0] === 0) {
		const current2 = $ah[1];
		$ai = body(current2);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$ai = result;
	}
	return $ai;
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const current = new5(1);
const selected = selector(current, [ 1 ], [ 1 ]);
const rows = new3();
const one = run_with_owner(rows, ($G) => {
	return of(selected, 1, [ 0, $G ]);
});
const two = run_with_owner(rows, ($N) => {
	return of(selected, 2, [ 0, $N ]);
});
const three = run_with_owner(rows, ($O) => {
	return of(selected, 3, [ 0, $O ]);
});
console.log("seeded " + get(one) + " " + get(two) + " " + get(three));
set2(current, 2, [ 1 ]);
console.log("after 2: " + get(one) + " " + get(two) + " " + get(three));
set2(current, 9, [ 1 ]);
console.log("after 9: " + get(one) + " " + get(two) + " " + get(three));
const again = run_with_owner(rows, ($T) => {
	return of(selected, 2, [ 0, $T ]);
});
set2(current, 2, [ 1 ]);
console.log("same cell=" + get(again) + " entries=" + selected[0].v.size);
dispose2(rows);
console.log("after dispose=" + selected[0].v.size);
const counted = new5(0);
let hits = 0;
const watch = on_change(__clone(counted), (_, $ae) => {
	hits = hits + 1;
	return;
});
batch(($af) => {
	set2(counted, 1, [ 0, $af ]);
	set2(counted, 2, [ 0, $af ]);
	return;
}, [ 1 ]);
console.log("hits=" + hits);
dispose(watch, [ 1 ]);
