function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_put(list, index, value, location) {
	if (index >= 0 && index < list.length) return list[index] = value;
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
function mint_subscriber(notify3) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify3, derived);
}
function subscriber_of(notify3, derived) {
	return [ fresh_id(), notify3, __shared_new(true), derived ];
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
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$j = undefined;
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
function defer_subscriber(turn, subscriber) {
	const $aL = turn;
	let $aM = null;
	if ($aL[0] === 0) {
		const ambient = $aL[1];
		$aM = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $aN = last(draining_turns.v);
		let $aO = null;
		if ($aN[0] === 0) {
			const draining = $aN[1];
			$aO = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$aO = undefined;
		}
		$aM = $aO;
	}
	return $aM;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	if (tracing.v && trace_hooks.v[6](subscriber, last(draining_turns.v))) {
		return;
	}
	defer_subscriber([ 1 ], subscriber);
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
function dispose(self, $o) {
	const $p = $o;
	let $q = null;
	if ($p[0] === 0) {
		const established = $p[1];
		$q = [ 0, established ];
	} else {
		$q = last(draining_turns.v);
	}
	const ambient = $q;
	release_under(self, ambient);
}
function detach(handle) {
	const $aQ = last(releasing_turns.v);
	let $aR = null;
	if ($aQ[0] === 0) {
		const at_release = $aQ[1];
		$aR = at_release;
	} else {
		$aR = last(draining_turns.v);
	}
	const turn = $aR;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $r = [ 0, handle[0] ];
	let $s = null;
	if ($r[0] === 0) {
		const subscribers = $r[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$s = undefined;
	} else {
		$s = undefined;
	}
	$s;
	const $t = ambient;
	let $u = null;
	if ($t[0] === 0) {
		const turn = $t[1];
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
		$u = undefined;
	} else {
		$u = undefined;
	}
	$u;
	const $v = handle[3].v;
	let $w = null;
	if ($v[0] === 0) {
		const release = $v[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$w = undefined;
	} else {
		$w = undefined;
	}
	return $w;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $ac = null;
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
		$ac = undefined;
	}
	return $ac;
}
function renew(self) {
	const $G = self[0].v[3];
	let $H = null;
	if ($G[0] === 0) {
		const nursery2 = __clone($G[1]);
		let $I = null;
		if (has_spawned(nursery2)) {
			$I = [ 1 ];
		} else {
			$I = [ 0, nursery2 ];
		}
		$H = $I;
	} else {
		$H = [ 1 ];
	}
	const carried = $H;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $T = null;
	if (is_disposed(self)) {
		$T = [ 1 ];
	} else {
		$T = self[0].v[3];
	}
	return $T;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $S = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $J = held[3];
		let $K = null;
		if ($J[0] === 0) {
			const nursery2 = $J[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$K = undefined;
		} else {
			$K = undefined;
		}
		$K;
		let $R = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $M = __guarded(cleanup);
				let $N = null;
				if ($M[0] === 0) {
					const message = $M[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$N = undefined;
				} else {
					$N = undefined;
				}
				$N;
			}
			const $P = failure;
			let $Q = null;
			if ($P[0] === 0) {
				const message2 = $P[1];
				$Q = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$Q = undefined;
			}
			$R = $Q;
		}
		$S = $R;
	}
	return $S;
}
function get_owner($aa) {
	return $aa;
}
function register_with_owner(subscription, $bh, $bi) {
	const $bj = $bi;
	let $bk = null;
	if ($bj[0] === 0) {
		const owner = $bj[1];
		$bk = take(owner, subscription, $bh);
	} else {
		$bk = __clone(subscription);
	}
	return $bk;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker() {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ] ]) ];
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $au = tracker[0].v[6];
	let $av = null;
	if ($au[0] === 0) {
		const lists = $au[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$av = undefined;
	} else {
		$av = undefined;
	}
	$av;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $aB = tracker[0].v[6];
	let $aC = null;
	if ($aB[0] === 0) {
		const lists = $aB[1];
		$aC = lists;
	} else {
		return;
		$aC = undefined;
	}
	const lists2 = $aC;
	const $aD = tracker[0].v[5];
	let $aE = null;
	if ($aD[0] === 0) {
		const target = __clone($aD[1]);
		$aE = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$aE = undefined;
	}
	$aE;
	if (!(is_empty(lists2.v[0]))) {
		lists2.v[0] = [  ];
	}
}
function reconnect(tracker, lists, target) {
	if (is_empty(lists.v[0]) && is_empty(lists.v[2])) {
		return;
	}
	const held = __clone(lists.v[2]);
	const reading = __clone(lists.v[0]);
	let kept = [  ];
	for (const _edge of held) {
		kept.push(false);
	}
	let next = [  ];
	tracker[0].v[3] = true;
	let position = 0;
	for (const dependency of reading) {
		const $aJ = reusable(held, kept, dependency[0], position);
		let $aK = null;
		if ($aJ[0] === 0) {
			const index = $aJ[1];
			__at_put(kept, index, true, "std/src/reactive.vl:1711:5");
			next.push(__clone(__at(held, index, "std/src/reactive.vl:1712:15")));
			$aK = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$aK = undefined;
		}
		$aK;
		position = position + 1;
	}
	lists.v[2] = next;
	let index2 = 0;
	for (const edge of held) {
		if (!(__at(kept, index2, "std/src/reactive.vl:1726:7"))) {
			detach(edge[1]);
		}
		index2 = index2 + 1;
	}
	tracker[0].v[3] = false;
}
function reusable(held, kept, identity, position) {
	const $aF = identity;
	let $aG = null;
	if ($aF[0] === 0) {
		const wanted = $aF[1];
		if (position < held.length && !(__at(kept, position, "std/src/reactive.vl:1749:9")) && same_identity(__at(held, position, "std/src/reactive.vl:1750:22")[0], wanted)) {
			return [ 0, position ];
		}
		let index = 0;
		while (index < held.length) {
			if (!(__at(kept, index, "std/src/reactive.vl:1755:9")) && same_identity(__at(held, index, "std/src/reactive.vl:1755:38")[0], wanted)) {
				return [ 0, index ];
			}
			index = index + 1;
		}
		$aG = [ 1 ];
	} else {
		$aG = [ 1 ];
	}
	return $aG;
}
function same_identity(identity, wanted) {
	const $aH = identity;
	let $aI = null;
	if ($aH[0] === 0) {
		const held = $aH[1];
		$aI = held === wanted;
	} else {
		$aI = false;
	}
	return $aI;
}
function relay_for(tracker, target) {
	return subscriber_of(() => {
		const connecting = tracker[0].v[3];
		tracker[0].v[2] = true;
		if (connecting) {
			tracker[0].v[4] = true;
		} else {
			wake(target);
		}
		return;
	}, true);
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $aV = tracker[0].v[6];
	let $aW = null;
	if ($aV[0] === 0) {
		const lists = $aV[1];
		$aW = lists;
	} else {
		return;
		$aW = undefined;
	}
	const lists2 = $aW;
	let $aX = null;
	if (!(is_empty(lists2.v[1]))) {
		const read = __clone(lists2.v[1]);
		lists2.v[1] = [  ];
		tracker[0].v[3] = true;
		let edges = [  ];
		for (const dependency of read) {
			edges.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
		}
		lists2.v[2] = edges;
		tracker[0].v[3] = false;
		if (tracker[0].v[4]) {
			tracker[0].v[4] = false;
			wake(target);
		}
		$aX = undefined;
	}
	return $aX;
}
function forget_reads(tracker) {
	const $aY = tracker[0].v[6];
	let $aZ = null;
	if ($aY[0] === 0) {
		const lists = $aY[1];
		$aZ = lists;
	} else {
		return;
		$aZ = undefined;
	}
	const lists2 = $aZ;
	let $ba = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$ba = undefined;
	}
	$ba;
	if (!(is_empty(lists2.v[1]))) {
		lists2.v[1] = [  ];
	}
}
function detach_tracker(tracker) {
	tracker[0].v[5] = [ 1 ];
	forget_reads(tracker);
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
		const $Y = previous;
		let $Z = null;
		if ($Y[0] === 0) {
			const earlier = $Y[1];
			$Z = earlier();
		} else {
			$Z = undefined;
		}
		return $Z;
	} ];
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function new3(value, caller) {
	let subscribers = [  ];
	const cell2 = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell2[0]), caller);
	}
	return cell2;
}
function new4(value, caller) {
	return new3(value, caller);
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function observe(signal, observer) {
	const cell2 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $b = [ 0, cell2 ];
		let $c = null;
		if ($b[0] === 0) {
			const live = $b[1];
			$c = observer(live.v);
		} else {
			$c = undefined;
		}
		return $c;
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
function sub(self, observer, caller) {
	return place_subscription(attach_observer(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true), caller);
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
	let $k = null;
	if (is_empty(self)) {
		$k = [ 1 ];
	} else {
		$k = __list_get(self, self.length - 1);
	}
	return $k;
}
function notify(self, $f) {
	const traced = tracing.v;
	let $g = null;
	if (traced) {
		$g = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$g = "";
	}
	const cause = $g;
	const $h = $f;
	let $i = null;
	if ($h[0] === 0) {
		const turn = $h[1];
		$i = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $l = last(draining_turns.v);
		let $m = null;
		if ($l[0] === 0) {
			const draining = $l[1];
			$m = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $n = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$n = undefined;
				}
				$n;
			}
			$m = undefined;
		}
		$i = $m;
	}
	return $i;
}
function set(self, value, $e, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $e);
}
function is_none(self) {
	const $L = self;
	return $L[0] === 1;
}
function run_body(runs, body) {
	const run = renew(runs);
	const $U = nursery(run);
	let $V = null;
	if ($U[0] === 0) {
		const nursery2 = $U[1];
		$V = (($W) => {
			return (($X) => {
				return body($W, $X);
			})(nursery2);
		})(run);
	} else {
		$V = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $V;
}
function take(self, item, $ab) {
	defer(self, () => {
		dispose(item, $ab);
		return;
	});
	return __clone(item);
}
function effect_on_change(self, body, $B, $C, caller) {
	const runs = new2();
	const subscription = on_change(self, (value, $D) => {
		return run_body(runs, ($E, $F) => {
			return (() => {
				return body(value, $E, [ 1 ], $F);
			})();
		});
	}, caller);
	also_releasing(subscription, () => {
		return release_runs(runs);
	});
	take(get_owner($C), subscription, $B);
}
function comp(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function get2(self) {
	return get(self[0]);
}
function on_settle(self, subscriber) {
	return attach(self, subscriber);
}
function on_settle2(self, subscriber) {
	return on_settle(self[0], subscriber);
}
function attach_observer2(self, observer, immediately) {
	const subscription = on_settle2(self, mint_subscriber(() => {
		return observer(get2(self));
	}));
	if (immediately) {
		observer(get2(self));
	}
	return subscription;
}
function sub2(self, observer, caller) {
	return place_subscription(attach_observer2(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true), caller);
}
function on_change2(self, observer, caller) {
	return place_subscription(attach_observer2(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function derive(self, transform) {
	return [ self, transform ];
}
function start(self) {
	return [ () => {
		return get2(self);
	}, (subscriber) => {
		return on_settle2(self, subscriber);
	}, () => {
		return;
	} ];
}
function run_body2(runs, body) {
	const run = renew(runs);
	const $az = nursery(run);
	let $aA = null;
	if ($az[0] === 0) {
		const nursery2 = $az[1];
		$aA = (($W) => {
			return (($X) => {
				return body($W, $X);
			})(nursery2);
		})(run);
	} else {
		$aA = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $aA;
}
function run_once(runs, tracker, body) {
	const scope2 = open_run(tracker);
	const value = run_body2(runs, ($aw, $ax) => {
		return (($ay) => {
			return body($aw, $ay, $ax);
		})(scope2);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs, tracker, body) {
	let value = run_once(runs, tracker, ($ar, $as, $at) => {
		return body($ar, $as, $at);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs, tracker, ($aS, $aT, $aU) => {
			return body($aS, $aT, $aU);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function start2(self) {
	const transform = self[1];
	const upstream = start(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new2();
	const tracker = new_tracker();
	return [ () => {
		const value = pull();
		return run_tracked(runs, tracker, ($ao, $ap, $aq) => {
			return transform(value, $ao, $ap, $aq);
		});
	}, (subscriber) => {
		const handle = upstream_attach(subscriber);
		attach_tracker(tracker, subscriber);
		return handle;
	}, () => {
		detach_tracker(tracker);
		release_runs(runs);
		upstream[2]();
		return;
	} ];
}
function notify2(self, $f) {
	const traced = tracing.v;
	let $bb = null;
	if (traced) {
		$bb = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$bb = "";
	}
	const cause = $bb;
	const $bc = $f;
	let $bd = null;
	if ($bc[0] === 0) {
		const turn = $bc[1];
		$bd = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $be = last(draining_turns.v);
		let $bf = null;
		if ($be[0] === 0) {
			const draining = $be[1];
			$bf = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $bg = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$bg = undefined;
				}
				$bg;
			}
			$bf = undefined;
		}
		$bd = $bf;
	}
	return $bd;
}
function set2(self, value, $e, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $e);
}
function cell(self, $am, $an) {
	const instance = start2(self);
	const pull = instance[0];
	const cached = new3(pull(), "std/src/reactive.vl:2388:28");
	const refreshed = instance[1](subscriber_of(() => {
		set2(cached, pull(), $am, "std/src/reactive.vl:2392:11");
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $am, $an);
	return cached;
}
function memo(self, $ak, $al) {
	return [ cell(self, $ak, $al) ];
}
function get3(self) {
	return get(self[0]);
}
function observe2(signal, observer) {
	const cell2 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $bn = [ 0, cell2 ];
		let $bo = null;
		if ($bn[0] === 0) {
			const live = $bn[1];
			$bo = observer(live.v);
		} else {
			$bo = undefined;
		}
		return $bo;
	}));
}
function attach_observer3(self, observer, immediately) {
	const subscription = observe2(self, observer);
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function attach_observer4(self, observer, immediately) {
	return attach_observer3(self[0], observer, immediately);
}
function sub3(self, observer, caller) {
	return place_subscription(attach_observer4(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true), caller);
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
const count = new4(1, "reactive-on-change.vl:44:39");
const eager = sub(__clone(count), (value, $a) => {
	return console.log("sub " + value);
}, "reactive-on-change.vl:46:20");
const quiet = on_change(__clone(count), (value, $d) => {
	return console.log("on_change " + value);
}, "reactive-on-change.vl:48:20");
console.log("attached");
set(count, 2, [ 1 ], "reactive-on-change.vl:50:8");
dispose(eager, [ 1 ]);
dispose(quiet, [ 1 ]);
set(count, 3, [ 1 ], "reactive-on-change.vl:54:8");
const $ad = comp(($x) => {
	effect_on_change(__clone(count), (value, $y, $z, $A) => {
		return console.log("effect_on_change " + value);
	}, [ 1 ], $x, "reactive-on-change.vl:58:9");
	return;
});
const _built = $ad[0];
const scope = $ad[1];
set(count, 4, [ 1 ], "reactive-on-change.vl:60:8");
dispose2(scope);
set(count, 5, [ 1 ], "reactive-on-change.vl:62:8");
const stored = [ new4(10, "reactive-on-change.vl:71:40") ];
const eagerly = sub2(__clone(stored), (value, $ae) => {
	return console.log("eager " + value);
}, "reactive-on-change.vl:72:23");
set(stored[0], 11, [ 1 ], "reactive-on-change.vl:73:15");
dispose(eagerly, [ 1 ]);
const watched = on_change2(__clone(stored), (value, $af) => {
	return console.log("stored " + value);
}, "reactive-on-change.vl:77:23");
set(stored[0], 12, [ 1 ], "reactive-on-change.vl:78:15");
dispose(watched, [ 1 ]);
const $bl = comp(($ag) => {
	return memo(derive(__clone(stored), (value, $ah, $ai, $aj) => {
		return "n=" + value;
	}), [ 1 ], [ 0, $ag ]);
});
const labelled = $bl[0];
const sealed = $bl[1];
console.log(get3(labelled));
const shown = sub3(labelled, (value, $bm) => {
	return console.log("label " + value);
}, "reactive-on-change.vl:86:23");
set(stored[0], 13, [ 1 ], "reactive-on-change.vl:87:15");
dispose(shown, [ 1 ]);
dispose2(sealed);
//# sourceMappingURL=reactive-on-change.mjs.map
