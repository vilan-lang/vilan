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
let __shared_identity_next = 1;
function __shared_identity(cell) {
	return cell.__id ??= __shared_identity_next++;
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
		let $k = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$k = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			if (tracing.v) {
				trace_hooks.v[3](turn, subscriber[0]);
			}
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:421:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:424:25");
		}
		$k;
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
						if (tracing.v) {
							trace_hooks.v[4](turn, subscriber);
						} else if (subscriber[2].v) {
							subscriber[1]();
						}
						budget = budget - 1;
					}
				}
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[2].v = new Map();
				for (const subscriber2 of wave) {
					if (tracing.v) {
						trace_hooks.v[4](turn, subscriber2);
					} else if (subscriber2[2].v) {
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
function queue_wakes(turn, subscribers, traced, cause) {
	if (traced) {
		waking.v = [ 0, cause ];
		enqueue(turn, subscribers);
		waking.v = [ 1 ];
	} else {
		enqueue(turn, subscribers);
	}
}
function dispose(self, $y) {
	const $z = $y;
	let $A = null;
	if ($z[0] === 0) {
		const established = $z[1];
		$A = [ 0, established ];
	} else {
		$A = last(draining_turns.v);
	}
	const ambient = $A;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $B = [ 0, handle[0] ];
	let $C = null;
	if ($B[0] === 0) {
		const subscribers = $B[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$C = undefined;
	} else {
		$C = undefined;
	}
	$C;
	const $D = ambient;
	let $E = null;
	if ($D[0] === 0) {
		const turn = $D[1];
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
		if (tracing.v) {
			trace_hooks.v[7](turn, handle[1]);
		}
		$E = undefined;
	} else {
		$E = undefined;
	}
	$E;
	const $F = handle[3].v;
	let $G = null;
	if ($F[0] === 0) {
		const release = $F[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$G = undefined;
	} else {
		$G = undefined;
	}
	return $G;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $H = null;
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
		$H = undefined;
	}
	return $H;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $ah = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $Y = held[3];
		let $Z = null;
		if ($Y[0] === 0) {
			const nursery = $Y[1];
			if (is_none(carried)) {
				nursery.cancel();
			}
			$Z = undefined;
		} else {
			$Z = undefined;
		}
		$Z;
		let $ag = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $ab = __guarded(cleanup);
				let $ac = null;
				if ($ab[0] === 0) {
					const message = $ab[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$ac = undefined;
				} else {
					$ac = undefined;
				}
				$ac;
			}
			const $ae = failure;
			let $af = null;
			if ($ae[0] === 0) {
				const message2 = $ae[1];
				$af = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$af = undefined;
			}
			$ag = $af;
		}
		$ah = $ag;
	}
	return $ah;
}
function register_with_owner(subscription, $t, $u) {
	const $v = $u;
	let $w = null;
	if ($v[0] === 0) {
		const owner = $v[1];
		$w = take(owner, subscription, $t);
	} else {
		$w = __clone(subscription);
	}
	return $w;
}
function defer_to_owner(cleanup, $M) {
	const $N = $M;
	let $O = null;
	if ($N[0] === 0) {
		const owner = $N[1];
		$O = defer(owner, cleanup);
	} else {
		$O = undefined;
	}
	return $O;
}
function place_subscription(subscription, site) {
	if (tracing.v) {
		trace_hooks.v[8](subscription[1], site);
	}
	return subscription;
}
function new4(value, caller) {
	let subscribers = [  ];
	const cell = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell[0]), caller);
	}
	return cell;
}
function new5(value, caller) {
	return new4(value, caller);
}
function get(self) {
	return __clone(self[0].v);
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $l = null;
	if (is_empty(self)) {
		$l = [ 1 ];
	} else {
		$l = __list_get(self, self.length - 1);
	}
	return $l;
}
function notify(self, $g) {
	const traced = tracing.v;
	let $h = null;
	if (traced) {
		$h = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$h = "";
	}
	const cause = $h;
	const $i = $g;
	let $j = null;
	if ($i[0] === 0) {
		const turn = $i[1];
		$j = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $m = last(draining_turns.v);
		let $n = null;
		if ($m[0] === 0) {
			const draining = $m[1];
			$n = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $o = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$o = undefined;
				}
				$o;
			}
			$n = undefined;
		}
		$j = $n;
	}
	return $j;
}
function set(self, value, $f, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
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
		const $r = [ 0, cell ];
		let $s = null;
		if ($r[0] === 0) {
			const live = $r[1];
			$s = observer(live.v);
		} else {
			$s = undefined;
		}
		return $s;
	}));
}
function attach_observer(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function on_change(self, observer, caller) {
	return place_subscription(attach_observer(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function take(self, item, $x) {
	defer(self, () => {
		dispose(item, $x);
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
			$e = set(leaving, false, $a, "std/src/reactive.vl:3016:33");
		} else {
			$e = undefined;
		}
		$e;
		const $p = __map_get(cells.v, hash(value));
		let $q = null;
		if ($p[0] === 0) {
			const arriving = $p[1];
			$q = set(arriving, true, $a, "std/src/reactive.vl:3020:35");
		} else {
			$q = undefined;
		}
		return $q;
	}, "std/src/reactive.vl:3009:42"), $a, $b);
	return [ cells, current2 ];
}
function of(self, key, $J) {
	const hash2 = hash(key);
	const $K = __map_get(self[0].v, hash2);
	let $L = null;
	if ($K[0] === 0) {
		const existing = $K[1];
		$L = existing;
	} else {
		const cell = new4(key === self[1].v, "std/src/reactive.vl:3040:46");
		self[0].v.set(hash2, cell);
		defer_to_owner(() => {
			self[0].v.delete(hash2);
			return;
		}, $J);
		$L = cell;
	}
	return $L;
}
function run_with_owner(owner, body) {
	return body(owner);
}
function notify2(self, $g) {
	const traced = tracing.v;
	let $R = null;
	if (traced) {
		$R = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$R = "";
	}
	const cause = $R;
	const $S = $g;
	let $T = null;
	if ($S[0] === 0) {
		const turn = $S[1];
		$T = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $U = last(draining_turns.v);
		let $V = null;
		if ($U[0] === 0) {
			const draining = $U[1];
			$V = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $W = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$W = undefined;
				}
				$W;
			}
			$V = undefined;
		}
		$T = $V;
	}
	return $T;
}
function set2(self, value, $f, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $f);
}
function is_none(self) {
	const $aa = self;
	return $aa[0] === 1;
}
function batch(body, $ak) {
	const $al = $ak;
	let $am = null;
	if ($al[0] === 0) {
		const current2 = $al[1];
		$am = body(current2);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$am = result;
	}
	return $am;
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const tracing = __shared_new(false);
const trace_hooks = __shared_new([ (_identity, _site) => {
	return;
}, (_site) => {
	return;
}, (_identity) => {
	return "";
}, (_turn, _id) => {
	return;
}, (_turn, _subscriber) => {
	return;
}, (_subscriber, _cause) => {
	return;
}, (_subscriber, _draining) => {
	return false;
}, (_turn, _id) => {
	return;
}, (_id, _site) => {
	return;
} ]);
const waking = __shared_new([ 1 ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const current = new5(1, "reactive-selector.vl:20:41");
const selected = selector(current, [ 1 ], [ 1 ]);
const rows = new3();
const one = run_with_owner(rows, ($I) => {
	return of(selected, 1, [ 0, $I ]);
});
const two = run_with_owner(rows, ($P) => {
	return of(selected, 2, [ 0, $P ]);
});
const three = run_with_owner(rows, ($Q) => {
	return of(selected, 3, [ 0, $Q ]);
});
console.log("seeded " + get(one) + " " + get(two) + " " + get(three));
set2(current, 2, [ 1 ], "reactive-selector.vl:34:10");
console.log("after 2: " + get(one) + " " + get(two) + " " + get(three));
set2(current, 9, [ 1 ], "reactive-selector.vl:38:10");
console.log("after 9: " + get(one) + " " + get(two) + " " + get(three));
const again = run_with_owner(rows, ($X) => {
	return of(selected, 2, [ 0, $X ]);
});
set2(current, 2, [ 1 ], "reactive-selector.vl:43:10");
console.log("same cell=" + get(again) + " entries=" + selected[0].v.size);
dispose2(rows);
console.log("after dispose=" + selected[0].v.size);
const counted = new5(0, "reactive-selector.vl:56:41");
let hits = 0;
const watch = on_change(__clone(counted), (_, $ai) => {
	hits = hits + 1;
	return;
}, "reactive-selector.vl:58:22");
batch(($aj) => {
	set2(counted, 1, [ 0, $aj ], "reactive-selector.vl:62:11");
	set2(counted, 2, [ 0, $aj ], "reactive-selector.vl:63:11");
	return;
}, [ 1 ]);
console.log("hits=" + hits);
dispose(watch, [ 1 ]);
//# sourceMappingURL=reactive-selector.mjs.map
