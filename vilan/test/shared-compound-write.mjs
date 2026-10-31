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
function __nursery_has_spawned(n) {
	return n.children.length > 0;
}
class __Nursery {
	constructor(parent) {
		this.children = [];
		this.failedTask = undefined;
		this.failWake = undefined;
		this.controller = new AbortController();
		if (parent) {
			const signal = parent.controller.signal;
			if (signal.aborted) this.controller.abort(signal.reason);
			else signal.addEventListener("abort", () => this.controller.abort(signal.reason), { once: true });
		}
	}
	cancel() {
		this.controller.abort();
	}
	__fail(task) {
		if (this.failedTask === undefined) {
			this.failedTask = task;
			this.controller.abort();
			if (this.failWake) this.failWake();
		}
	}
	is_cancelled() {
		return this.controller.signal.aborted;
	}
	signal_of() {
		return this.controller.signal;
	}
}
function __nursery_new(parent) {
	return new __Nursery(parent && parent[0] === 0 ? parent[1] : undefined);
}
function __nursery_new_detached() {
	const n = __nursery_new(undefined);
	n.detached = true;
	n.__fail = function (task) {
		if (!task.observed) {
			globalThis.setTimeout(() => {
				if (!task.observed) console.error("unhandled task error (spawned in " + task.origin + "): " + String(task.error));
			}, 0);
		}
	};
	return n;
}
function __nursery_is_cancel(error) {
	return !!error && error.name === "AbortError";
}
async function __nursery_run(n, body) {
	let result;
	let bodyError;
	let bodyFailed = false;
	try {
		result = await body();
	} catch (error) {
		bodyFailed = true;
		bodyError = error;
	}
	if (bodyFailed) n.controller.abort();
	const failed = new Promise((resolve) => {
		n.failWake = resolve;
		if (n.failedTask !== undefined) resolve();
	});
	let index = 0;
	while (!bodyFailed && n.failedTask === undefined && index < n.children.length) {
		try {
			await Promise.race([n.children[index], failed]);
		} catch (error) {}
		if (n.failedTask === undefined) index += 1;
	}
	if (!bodyFailed && n.failedTask === undefined) return result;
	for (const task of n.children) task.then(null, () => {});
	if (bodyFailed) throw bodyError;
	const winner = n.failedTask;
	const failure = winner.error;
	if (typeof failure === "string") throw failure + " (in task spawned in " + winner.origin + ")";
	if (failure && failure.location !== undefined) throw __panic(failure.message + " (in task spawned in " + winner.origin + ")", failure.location);
	throw failure;
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
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function mint_subscriber(notify2) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify2, derived);
}
function subscriber_of(notify2, derived) {
	return [ fresh_id(), notify2, __shared_new(true), derived ];
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
		let $ag = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$ag = undefined;
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
		$ag;
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
function dispose(self, $M) {
	const $N = $M;
	let $O = null;
	if ($N[0] === 0) {
		const established = $N[1];
		$O = [ 0, established ];
	} else {
		$O = last(draining_turns.v);
	}
	const ambient = $O;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $Q = [ 0, handle[0] ];
	let $R = null;
	if ($Q[0] === 0) {
		const subscribers = $Q[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$R = undefined;
	} else {
		$R = undefined;
	}
	$R;
	const $S = ambient;
	let $T = null;
	if ($S[0] === 0) {
		const turn = $S[1];
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
		$T = undefined;
	} else {
		$T = undefined;
	}
	$T;
	const $U = handle[3].v;
	let $V = null;
	if ($U[0] === 0) {
		const release = $U[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$V = undefined;
	} else {
		$V = undefined;
	}
	return $V;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $W = null;
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
		$W = undefined;
	}
	return $W;
}
function renew(self) {
	const $o = self[0].v[3];
	let $p = null;
	if ($o[0] === 0) {
		const nursery2 = __clone($o[1]);
		let $q = null;
		if (has_spawned(nursery2)) {
			$q = [ 1 ];
		} else {
			$q = [ 0, nursery2 ];
		}
		$p = $q;
	} else {
		$p = [ 1 ];
	}
	const carried = $p;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $B = null;
	if (is_disposed(self)) {
		$B = [ 1 ];
	} else {
		$B = self[0].v[3];
	}
	return $B;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $A = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $r = held[3];
		let $s = null;
		if ($r[0] === 0) {
			const nursery2 = $r[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$s = undefined;
		} else {
			$s = undefined;
		}
		$s;
		let $z = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $u = __guarded(cleanup);
				let $v = null;
				if ($u[0] === 0) {
					const message = $u[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$v = undefined;
				} else {
					$v = undefined;
				}
				$v;
			}
			const $x = failure;
			let $y = null;
			if ($x[0] === 0) {
				const message2 = $x[1];
				$y = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$y = undefined;
			}
			$z = $y;
		}
		$A = $z;
	}
	return $A;
}
function get_owner($K) {
	return $K;
}
function release_runs(runs2) {
	dispose2([ runs2[0], runs2[0].v[0] ]);
}
function place_subscription(subscription, site) {
	if (tracing.v) {
		trace_hooks.v[8](subscription[1], site);
	}
	return subscription;
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $I = previous;
		let $J = null;
		if ($I[0] === 0) {
			const earlier = $I[1];
			$J = earlier();
		} else {
			$J = undefined;
		}
		return $J;
	} ];
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function tick() {
	ticks = ticks + 2;
	ticks = ticks * 3;
}
function new4(value, caller) {
	let subscribers = [  ];
	const cell = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell[0]), caller);
	}
	return cell;
}
function is_none(self) {
	const $t = self;
	return $t[0] === 1;
}
function run_body(runs2, body) {
	const run = renew(runs2);
	const $C = nursery(run);
	let $D = null;
	if ($C[0] === 0) {
		const nursery2 = $C[1];
		$D = (($E) => {
			return (($F) => {
				return body($E, $F);
			})(nursery2);
		})(run);
	} else {
		$D = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $D;
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function observe(signal, observer) {
	const cell = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $G = [ 0, cell ];
		let $H = null;
		if ($G[0] === 0) {
			const live = $G[1];
			$H = observer(live.v);
		} else {
			$H = undefined;
		}
		return $H;
	}));
}
function get(self) {
	return __clone(self[0].v);
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
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $P = null;
	if (is_empty(self)) {
		$P = [ 1 ];
	} else {
		$P = __list_get(self, self.length - 1);
	}
	return $P;
}
function take(self, item, $L) {
	defer(self, () => {
		dispose(item, $L);
		return;
	});
	return __clone(item);
}
function effect_on_change(self, body, $j, $k, caller) {
	const runs2 = new3();
	const subscription = on_change(self, (value, $l) => {
		return run_body(runs2, ($m, $n) => {
			return (() => {
				return body(value, $m, [ 1 ], $n);
			})();
		});
	}, caller);
	also_releasing(subscription, () => {
		return release_runs(runs2);
	});
	take(get_owner($k), subscription, $j);
}
function run_with_owner(owner, body) {
	return body(owner);
}
function notify(self, $ac) {
	const traced = tracing.v;
	let $ad = null;
	if (traced) {
		$ad = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$ad = "";
	}
	const cause = $ad;
	const $ae = $ac;
	let $af = null;
	if ($ae[0] === 0) {
		const turn = $ae[1];
		$af = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $ah = last(draining_turns.v);
		let $ai = null;
		if ($ah[0] === 0) {
			const draining = $ah[1];
			$ai = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $aj = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$aj = undefined;
				}
				$aj;
			}
			$ai = undefined;
		}
		$af = $ai;
	}
	return $af;
}
function set(self, value, $ab, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $ab);
}
function batch(body, $al) {
	const $am = $al;
	let $an = null;
	if ($am[0] === 0) {
		const current = $am[1];
		$an = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$an = result;
	}
	return $an;
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
const run_nurseries_allocated_count = __shared_new(0);
let ticks = 0;
const fired = __shared_new(0);
const $a = [ fired, "v" ];
fired.v = $a[0][$a[1]] + 1;
const $b = [ fired, "v" ];
fired.v = $b[0][$b[1]] - 3;
const $c = [ fired, "v" ];
fired.v = $c[0][$c[1]] * -(5);
console.log("statement: " + fired.v);
const bump = () => {
	const $d = [ fired, "v" ];
	return fired.v = $d[0][$d[1]] + 1;
};
bump();
bump();
console.log("closure: " + fired.v);
const tally = __shared_new([ 1 ]);
tally.v[0] = tally.v[0] + 41;
console.log("field: " + tally.v[0]);
const a = new4(0, "shared-compound-write.vl:39:22");
const b = new4(0, "shared-compound-write.vl:40:22");
const runs = __shared_new(0);
const watching = new3();
run_with_owner(watching, ($e) => {
	effect_on_change(__clone(a), (_x, $f, $g, $h) => {
		const $i = [ runs, "v" ];
		return runs.v = $i[0][$i[1]] + 1;
	}, [ 1 ], $e, "shared-compound-write.vl:44:5");
	effect_on_change(__clone(b), (_x, $X, $Y, $Z) => {
		const $aa = [ runs, "v" ];
		return runs.v = $aa[0][$aa[1]] + 10;
	}, [ 1 ], $e, "shared-compound-write.vl:45:5");
	return;
});
set(a, 1, [ 1 ], "shared-compound-write.vl:47:4");
console.log("inline: " + runs.v);
batch(($ak) => {
	set(a, 2, [ 0, $ak ], "shared-compound-write.vl:50:5");
	set(b, 2, [ 0, $ak ], "shared-compound-write.vl:51:5");
	return;
}, [ 1 ]);
console.log("drained: " + runs.v);
dispose2(watching);
let captured = 1;
const double = () => {
	return captured = captured * 2;
};
double();
double();
console.log("captured: " + captured);
tick();
tick();
console.log("module: " + ticks);
//# sourceMappingURL=shared-compound-write.mjs.map
