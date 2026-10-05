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
		let $i = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$i = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:414:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:417:25");
		}
		$i;
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
function defer_subscriber(turn, subscriber) {
	const $aJ = turn;
	let $aK = null;
	if ($aJ[0] === 0) {
		const ambient = $aJ[1];
		$aK = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $aL = last(draining_turns.v);
		let $aM = null;
		if ($aL[0] === 0) {
			const draining = $aL[1];
			$aM = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$aM = undefined;
		}
		$aK = $aM;
	}
	return $aK;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $m) {
	const $n = $m;
	let $o = null;
	if ($n[0] === 0) {
		const established = $n[1];
		$o = [ 0, established ];
	} else {
		$o = last(draining_turns.v);
	}
	const ambient = $o;
	release_under(self, ambient);
}
function detach(handle) {
	const $aO = last(releasing_turns.v);
	let $aP = null;
	if ($aO[0] === 0) {
		const at_release = $aO[1];
		$aP = at_release;
	} else {
		$aP = last(draining_turns.v);
	}
	const turn = $aP;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $p = [ 0, handle[0] ];
	let $q = null;
	if ($p[0] === 0) {
		const subscribers = $p[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$q = undefined;
	} else {
		$q = undefined;
	}
	$q;
	const $r = ambient;
	let $s = null;
	if ($r[0] === 0) {
		const turn = $r[1];
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
		$s = undefined;
	} else {
		$s = undefined;
	}
	$s;
	const $t = handle[3].v;
	let $u = null;
	if ($t[0] === 0) {
		const release = $t[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$u = undefined;
	} else {
		$u = undefined;
	}
	return $u;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aa = null;
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
		$aa = undefined;
	}
	return $aa;
}
function renew(self) {
	const $E = self[0].v[3];
	let $F = null;
	if ($E[0] === 0) {
		const nursery2 = __clone($E[1]);
		let $G = null;
		if (has_spawned(nursery2)) {
			$G = [ 1 ];
		} else {
			$G = [ 0, nursery2 ];
		}
		$F = $G;
	} else {
		$F = [ 1 ];
	}
	const carried = $F;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $R = null;
	if (is_disposed(self)) {
		$R = [ 1 ];
	} else {
		$R = self[0].v[3];
	}
	return $R;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $Q = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $H = held[3];
		let $I = null;
		if ($H[0] === 0) {
			const nursery2 = $H[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$I = undefined;
		} else {
			$I = undefined;
		}
		$I;
		let $P = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $K = __guarded(cleanup);
				let $L = null;
				if ($K[0] === 0) {
					const message = $K[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$L = undefined;
				} else {
					$L = undefined;
				}
				$L;
			}
			const $N = failure;
			let $O = null;
			if ($N[0] === 0) {
				const message2 = $N[1];
				$O = (() => {
					throw __panic(message2, "std/src/reactive.vl:1207:27");
				})();
			} else {
				$O = undefined;
			}
			$P = $O;
		}
		$Q = $P;
	}
	return $Q;
}
function get_owner($Y) {
	return $Y;
}
function register_with_owner(subscription, $bd, $be) {
	const $bf = $be;
	let $bg = null;
	if ($bf[0] === 0) {
		const owner = $bf[1];
		$bg = take(owner, subscription, $bd);
	} else {
		$bg = __clone(subscription);
	}
	return $bg;
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
	const $as = tracker[0].v[6];
	let $at = null;
	if ($as[0] === 0) {
		const lists = $as[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$at = undefined;
	} else {
		$at = undefined;
	}
	$at;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $az = tracker[0].v[6];
	let $aA = null;
	if ($az[0] === 0) {
		const lists = $az[1];
		$aA = lists;
	} else {
		return;
		$aA = undefined;
	}
	const lists2 = $aA;
	const $aB = tracker[0].v[5];
	let $aC = null;
	if ($aB[0] === 0) {
		const target = __clone($aB[1]);
		$aC = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$aC = undefined;
	}
	$aC;
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
		const $aH = reusable(held, kept, dependency[0], position);
		let $aI = null;
		if ($aH[0] === 0) {
			const index = $aH[1];
			__at_put(kept, index, true, "std/src/reactive.vl:1624:5");
			next.push(__clone(__at(held, index, "std/src/reactive.vl:1625:15")));
			$aI = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$aI = undefined;
		}
		$aI;
		position = position + 1;
	}
	lists.v[2] = next;
	let index2 = 0;
	for (const edge of held) {
		if (!(__at(kept, index2, "std/src/reactive.vl:1639:7"))) {
			detach(edge[1]);
		}
		index2 = index2 + 1;
	}
	tracker[0].v[3] = false;
}
function reusable(held, kept, identity, position) {
	const $aD = identity;
	let $aE = null;
	if ($aD[0] === 0) {
		const wanted = $aD[1];
		if (position < held.length && !(__at(kept, position, "std/src/reactive.vl:1662:9")) && same_identity(__at(held, position, "std/src/reactive.vl:1663:22")[0], wanted)) {
			return [ 0, position ];
		}
		let index = 0;
		while (index < held.length) {
			if (!(__at(kept, index, "std/src/reactive.vl:1668:9")) && same_identity(__at(held, index, "std/src/reactive.vl:1668:38")[0], wanted)) {
				return [ 0, index ];
			}
			index = index + 1;
		}
		$aE = [ 1 ];
	} else {
		$aE = [ 1 ];
	}
	return $aE;
}
function same_identity(identity, wanted) {
	const $aF = identity;
	let $aG = null;
	if ($aF[0] === 0) {
		const held = $aF[1];
		$aG = held === wanted;
	} else {
		$aG = false;
	}
	return $aG;
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
	const $aT = tracker[0].v[6];
	let $aU = null;
	if ($aT[0] === 0) {
		const lists = $aT[1];
		$aU = lists;
	} else {
		return;
		$aU = undefined;
	}
	const lists2 = $aU;
	let $aV = null;
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
		$aV = undefined;
	}
	return $aV;
}
function forget_reads(tracker) {
	const $aW = tracker[0].v[6];
	let $aX = null;
	if ($aW[0] === 0) {
		const lists = $aW[1];
		$aX = lists;
	} else {
		return;
		$aX = undefined;
	}
	const lists2 = $aX;
	let $aY = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aY = undefined;
	}
	$aY;
	if (!(is_empty(lists2.v[1]))) {
		lists2.v[1] = [  ];
	}
}
function detach_tracker(tracker) {
	tracker[0].v[5] = [ 1 ];
	forget_reads(tracker);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $W = previous;
		let $X = null;
		if ($W[0] === 0) {
			const earlier = $W[1];
			$X = earlier();
		} else {
			$X = undefined;
		}
		return $X;
	} ];
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function new3(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function new4(value) {
	return new3(value);
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
function sub(self, observer) {
	return attach_observer(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function on_change(self, observer) {
	return attach_observer(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false);
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $j = null;
	if (is_empty(self)) {
		$j = [ 1 ];
	} else {
		$j = __list_get(self, self.length - 1);
	}
	return $j;
}
function notify(self, $f) {
	const $g = $f;
	let $h = null;
	if ($g[0] === 0) {
		const turn = $g[1];
		$h = enqueue(turn, __clone(self[1].v));
	} else {
		const $k = last(draining_turns.v);
		let $l = null;
		if ($k[0] === 0) {
			const draining = $k[1];
			$l = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$l = undefined;
		}
		$h = $l;
	}
	return $h;
}
function set(self, value, $e) {
	self[0].v = __clone(value);
	notify(self, $e);
}
function is_none(self) {
	const $J = self;
	return $J[0] === 1;
}
function run_body(runs, body) {
	const run = renew(runs);
	const $S = nursery(run);
	let $T = null;
	if ($S[0] === 0) {
		const nursery2 = $S[1];
		$T = (($U) => {
			return (($V) => {
				return body($U, $V);
			})(nursery2);
		})(run);
	} else {
		$T = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1318:11");
		})();
	}
	return $T;
}
function take(self, item, $Z) {
	defer(self, () => {
		dispose(item, $Z);
		return;
	});
	return __clone(item);
}
function effect_on_change(self, body, $z, $A) {
	const runs = new2();
	const subscription = on_change(self, (value, $B) => {
		return run_body(runs, ($C, $D) => {
			return (() => {
				return body(value, $C, [ 1 ], $D);
			})();
		});
	});
	also_releasing(subscription, () => {
		return release_runs(runs);
	});
	take(get_owner($A), subscription, $z);
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
function sub2(self, observer) {
	return attach_observer2(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function on_change2(self, observer) {
	return attach_observer2(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false);
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
	const $ax = nursery(run);
	let $ay = null;
	if ($ax[0] === 0) {
		const nursery2 = $ax[1];
		$ay = (($U) => {
			return (($V) => {
				return body($U, $V);
			})(nursery2);
		})(run);
	} else {
		$ay = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1318:11");
		})();
	}
	return $ay;
}
function run_once(runs, tracker, body) {
	const scope2 = open_run(tracker);
	const value = run_body2(runs, ($au, $av) => {
		return (($aw) => {
			return body($au, $aw, $av);
		})(scope2);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs, tracker, body) {
	let value = run_once(runs, tracker, ($ap, $aq, $ar) => {
		return body($ap, $aq, $ar);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs, tracker, ($aQ, $aR, $aS) => {
			return body($aQ, $aR, $aS);
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
		return run_tracked(runs, tracker, ($am, $an, $ao) => {
			return transform(value, $am, $an, $ao);
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
	const $aZ = $f;
	let $ba = null;
	if ($aZ[0] === 0) {
		const turn = $aZ[1];
		$ba = enqueue(turn, __clone(self[1].v));
	} else {
		const $bb = last(draining_turns.v);
		let $bc = null;
		if ($bb[0] === 0) {
			const draining = $bb[1];
			$bc = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bc = undefined;
		}
		$ba = $bc;
	}
	return $ba;
}
function set2(self, value, $e) {
	self[0].v = __clone(value);
	notify2(self, $e);
}
function cell(self, $ak, $al) {
	const instance = start2(self);
	const pull = instance[0];
	const cached = new3(pull());
	const refreshed = instance[1](subscriber_of(() => {
		set2(cached, pull(), $ak);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $ak, $al);
	return cached;
}
function memo(self, $ai, $aj) {
	return [ cell(self, $ai, $aj) ];
}
function get3(self) {
	return get(self[0]);
}
function observe2(signal, observer) {
	const cell2 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $bj = [ 0, cell2 ];
		let $bk = null;
		if ($bj[0] === 0) {
			const live = $bj[1];
			$bk = observer(live.v);
		} else {
			$bk = undefined;
		}
		return $bk;
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
function sub3(self, observer) {
	return attach_observer4(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const run_nurseries_allocated_count = __shared_new(0);
const count = new4(1);
const eager = sub(__clone(count), (value, $a) => {
	return console.log("sub " + value);
});
const quiet = on_change(__clone(count), (value, $d) => {
	return console.log("on_change " + value);
});
console.log("attached");
set(count, 2, [ 1 ]);
dispose(eager, [ 1 ]);
dispose(quiet, [ 1 ]);
set(count, 3, [ 1 ]);
const $ab = comp(($v) => {
	effect_on_change(__clone(count), (value, $w, $x, $y) => {
		return console.log("effect_on_change " + value);
	}, [ 1 ], $v);
	return;
});
const _built = $ab[0];
const scope = $ab[1];
set(count, 4, [ 1 ]);
dispose2(scope);
set(count, 5, [ 1 ]);
const stored = [ new4(10) ];
const eagerly = sub2(__clone(stored), (value, $ac) => {
	return console.log("eager " + value);
});
set(stored[0], 11, [ 1 ]);
dispose(eagerly, [ 1 ]);
const watched = on_change2(__clone(stored), (value, $ad) => {
	return console.log("stored " + value);
});
set(stored[0], 12, [ 1 ]);
dispose(watched, [ 1 ]);
const $bh = comp(($ae) => {
	return memo(derive(__clone(stored), (value, $af, $ag, $ah) => {
		return "n=" + value;
	}), [ 1 ], [ 0, $ae ]);
});
const labelled = $bh[0];
const sealed = $bh[1];
console.log(get3(labelled));
const shown = sub3(labelled, (value, $bi) => {
	return console.log("label " + value);
});
set(stored[0], 13, [ 1 ]);
dispose(shown, [ 1 ]);
dispose2(sealed);
