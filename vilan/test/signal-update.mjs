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
function hash2(self) {
	return __hash(self);
}
function new2() {
	const table = new Map();
	return [ table ];
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
function new3() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return is_empty(self[0].v) && is_empty(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash2(subscriber[0]);
		let $f = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$f = undefined;
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
		$f;
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
function dispose(self, $A) {
	const $B = $A;
	let $C = null;
	if ($B[0] === 0) {
		const established = $B[1];
		$C = [ 0, established ];
	} else {
		$C = last(draining_turns.v);
	}
	const ambient = $C;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $D = [ 0, handle[0] ];
	let $E = null;
	if ($D[0] === 0) {
		const subscribers = $D[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$E = undefined;
	} else {
		$E = undefined;
	}
	$E;
	const $F = ambient;
	let $G = null;
	if ($F[0] === 0) {
		const turn = $F[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== handle[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[2].v.delete(hash2(handle[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn[1].v) {
			if (subscriber3[0] !== handle[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn[1].v = kept_derived;
		turn[3].v.delete(hash2(handle[1]));
		if (tracing.v) {
			trace_hooks.v[7](turn, handle[1]);
		}
		$G = undefined;
	} else {
		$G = undefined;
	}
	$G;
	const $H = handle[3].v;
	let $I = null;
	if ($H[0] === 0) {
		const release = $H[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$I = undefined;
	} else {
		$I = undefined;
	}
	return $I;
}
function new4() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $J = null;
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
		$J = undefined;
	}
	return $J;
}
function place_subscription(subscription, site) {
	if (tracing.v) {
		trace_hooks.v[8](subscription[1], site);
	}
	return subscription;
}
function new5(value, caller) {
	let subscribers = [  ];
	const cell = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell[0]), caller);
	}
	return cell;
}
function new6(value, caller) {
	return new5(value, caller);
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $g = null;
	if (is_empty(self)) {
		$g = [ 1 ];
	} else {
		$g = __list_get(self, self.length - 1);
	}
	return $g;
}
function notify(self, $b) {
	const traced = tracing.v;
	let $c = null;
	if (traced) {
		$c = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$c = "";
	}
	const cause = $c;
	const $d = $b;
	let $e = null;
	if ($d[0] === 0) {
		const turn = $d[1];
		$e = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $h = last(draining_turns.v);
		let $i = null;
		if ($h[0] === 0) {
			const draining = $h[1];
			$i = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $j = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$j = undefined;
				}
				$j;
			}
			$i = undefined;
		}
		$e = $i;
	}
	return $e;
}
function update(self, mutate, $a, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	mutate(self[0].v);
	notify(self, $a);
}
function get(self) {
	return __clone(self[0].v);
}
function new7(value, caller) {
	return new5(value, caller);
}
function insert(self, key, value) {
	self[0].set(hash(key), [ __clone(key), __clone(value) ]);
}
function notify2(self, $b) {
	const traced = tracing.v;
	let $k = null;
	if (traced) {
		$k = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$k = "";
	}
	const cause = $k;
	const $l = $b;
	let $m = null;
	if ($l[0] === 0) {
		const turn = $l[1];
		$m = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $n = last(draining_turns.v);
		let $o = null;
		if ($n[0] === 0) {
			const draining = $n[1];
			$o = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $p = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$p = undefined;
				}
				$p;
			}
			$o = undefined;
		}
		$m = $o;
	}
	return $m;
}
function update2(self, mutate, $a, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	mutate(self[0].v);
	notify2(self, $a);
}
function len(self) {
	return self[0].size;
}
function update3(self, mutate, $a, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	mutate([ self[0], "v" ]);
	notify2(self, $a);
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function observe(signal, observer) {
	const cell = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $x = [ 0, cell ];
		let $y = null;
		if ($x[0] === 0) {
			const live = $x[1];
			$y = observer(live.v);
		} else {
			$y = undefined;
		}
		return $y;
	}));
}
function attach_observer(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function sub(self, observer, caller) {
	return place_subscription(attach_observer(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true), caller);
}
function take(self, item, $z) {
	defer(self, () => {
		dispose(item, $z);
		return;
	});
	return __clone(item);
}
function batch(body, $L) {
	const $M = $L;
	let $N = null;
	if ($M[0] === 0) {
		const current = $M[1];
		$N = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$N = result;
	}
	return $N;
}
function set(self, value, $Q, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $Q);
}
function set2(self, value, $P, caller) {
	set(self, value, $P, caller);
}
function set_with(self, transform, $O) {
	set2(self, transform(get(self)), $O, "std/src/reactive.vl:2510:8");
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
const owner = new4();
const todos = new6([ 1, 2 ], "signal-update.vl:14:22");
update(todos, (list) => {
	list.push(5);
	return;
}, [ 1 ], "signal-update.vl:15:8");
console.log(String(get(todos).length));
const scores = new7(new2(), "signal-update.vl:21:54");
update2(scores, (entries) => {
	insert(entries, "a", 1);
	insert(entries, "b", 2);
	return;
}, [ 1 ], "signal-update.vl:22:9");
console.log(String(len(get(scores))));
const count = new7(1, "signal-update.vl:29:22");
update3(count, (value) => {
	value[0][value[1]] = value[0][value[1]] + 10;
	return;
}, [ 1 ], "signal-update.vl:30:8");
console.log(String(get(count)));
const watched = new6([ 0 ], "signal-update.vl:36:24");
take(owner, sub(__clone(watched), (list, $w) => {
	return console.log("len " + list.length);
}, "signal-update.vl:37:21"), [ 1 ]);
update(watched, (list) => {
	list.push(1);
	list.push(2);
	return;
}, [ 1 ], "signal-update.vl:38:10");
update(watched, (list) => {
	return;
}, [ 1 ], "signal-update.vl:42:10");
console.log("---");
batch(($K) => {
	update(watched, (list) => {
		list.push(3);
		return;
	}, [ 0, $K ], "signal-update.vl:47:11");
	update(watched, (list) => {
		list.push(4);
		return;
	}, [ 0, $K ], "signal-update.vl:50:11");
	console.log("inside");
	return;
}, [ 1 ]);
update(todos, (list) => {
	list.push(6);
	console.log("reentrant " + get(todos).length);
	return;
}, [ 1 ], "signal-update.vl:58:8");
set_with(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log(String(get(count)));
//# sourceMappingURL=signal-update.mjs.map
