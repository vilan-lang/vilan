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
function subscriber_of(notify2, derived) {
	return [ fresh_id(), notify2, __shared_new(true), derived ];
}
function is_quiescent(self) {
	return is_empty(self[0].v) && is_empty(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $Y = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$Y = undefined;
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
		$Y;
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
	const $W = turn;
	let $X = null;
	if ($W[0] === 0) {
		const ambient = $W[1];
		$X = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $Z = last(draining_turns.v);
		let $aa = null;
		if ($Z[0] === 0) {
			const draining = $Z[1];
			$aa = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$aa = undefined;
		}
		$X = $aa;
	}
	return $X;
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
function retiring(subscriber, release) {
	return detached(subscriber[0], subscriber[2], release);
}
function detached(id, live, release) {
	let subscribers = [  ];
	const empty = __shared_new(subscribers);
	return [ empty, id, live, __shared_new([ 0, release ]) ];
}
function dispose(self, $aP) {
	const $aQ = $aP;
	let $aR = null;
	if ($aQ[0] === 0) {
		const established = $aQ[1];
		$aR = [ 0, established ];
	} else {
		$aR = last(draining_turns.v);
	}
	const ambient = $aR;
	release_under(self, ambient);
}
function detach(handle) {
	const $ae = last(releasing_turns.v);
	let $af = null;
	if ($ae[0] === 0) {
		const at_release = $ae[1];
		$af = at_release;
	} else {
		$af = last(draining_turns.v);
	}
	const turn = $af;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ag = [ 0, handle[0] ];
	let $ah = null;
	if ($ag[0] === 0) {
		const subscribers = $ag[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$ah = undefined;
	} else {
		$ah = undefined;
	}
	$ah;
	const $ai = ambient;
	let $aj = null;
	if ($ai[0] === 0) {
		const turn = $ai[1];
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
		$aj = undefined;
	} else {
		$aj = undefined;
	}
	$aj;
	const $ak = handle[3].v;
	let $al = null;
	if ($ak[0] === 0) {
		const release = $ak[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$al = undefined;
	} else {
		$al = undefined;
	}
	return $al;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aS = null;
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
		$aS = undefined;
	}
	return $aS;
}
function renew(self) {
	const $t = self[0].v[3];
	let $u = null;
	if ($t[0] === 0) {
		const nursery2 = __clone($t[1]);
		let $v = null;
		if (has_spawned(nursery2)) {
			$v = [ 1 ];
		} else {
			$v = [ 0, nursery2 ];
		}
		$u = $v;
	} else {
		$u = [ 1 ];
	}
	const carried = $u;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $G = null;
	if (is_disposed(self)) {
		$G = [ 1 ];
	} else {
		$G = self[0].v[3];
	}
	return $G;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $F = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $w = held[3];
		let $x = null;
		if ($w[0] === 0) {
			const nursery2 = $w[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$x = undefined;
		} else {
			$x = undefined;
		}
		$x;
		let $E = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $z = __guarded(cleanup);
				let $A = null;
				if ($z[0] === 0) {
					const message = $z[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$A = undefined;
				} else {
					$A = undefined;
				}
				$A;
			}
			const $C = failure;
			let $D = null;
			if ($C[0] === 0) {
				const message2 = $C[1];
				$D = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$D = undefined;
			}
			$E = $D;
		}
		$F = $E;
	}
	return $F;
}
function register_with_owner(subscription, $aK, $aL) {
	const $aM = $aL;
	let $aN = null;
	if ($aM[0] === 0) {
		const owner = $aM[1];
		$aN = take(owner, subscription, $aK);
	} else {
		$aN = __clone(subscription);
	}
	return $aN;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker(site) {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ], site ]) ];
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $o = tracker[0].v[6];
	let $p = null;
	if ($o[0] === 0) {
		const lists = $o[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$p = undefined;
	} else {
		$p = undefined;
	}
	$p;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $L = tracker[0].v[6];
	let $M = null;
	if ($L[0] === 0) {
		const lists = $L[1];
		$M = lists;
	} else {
		return;
		$M = undefined;
	}
	const lists2 = $M;
	const $N = tracker[0].v[5];
	let $O = null;
	if ($N[0] === 0) {
		const target = __clone($N[1]);
		$O = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$O = undefined;
	}
	$O;
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
		const $T = reusable(held, kept, dependency[0], position);
		let $U = null;
		if ($T[0] === 0) {
			const index = $T[1];
			__at_put(kept, index, true, "std/src/reactive.vl:1715:5");
			next.push(__clone(__at(held, index, "std/src/reactive.vl:1716:15")));
			$U = undefined;
		} else {
			next.push([ __clone(dependency[0]), dependency[1](relay_for(tracker, target)) ]);
			$U = undefined;
		}
		$U;
		position = position + 1;
	}
	lists.v[2] = next;
	let index2 = 0;
	for (const edge of held) {
		if (!(__at(kept, index2, "std/src/reactive.vl:1730:7"))) {
			detach(edge[1]);
		}
		index2 = index2 + 1;
	}
	tracker[0].v[3] = false;
}
function reusable(held, kept, identity, position) {
	const $P = identity;
	let $Q = null;
	if ($P[0] === 0) {
		const wanted = $P[1];
		if (position < held.length && !(__at(kept, position, "std/src/reactive.vl:1753:9")) && same_identity(__at(held, position, "std/src/reactive.vl:1754:22")[0], wanted)) {
			return [ 0, position ];
		}
		let index = 0;
		while (index < held.length) {
			if (!(__at(kept, index, "std/src/reactive.vl:1759:9")) && same_identity(__at(held, index, "std/src/reactive.vl:1759:38")[0], wanted)) {
				return [ 0, index ];
			}
			index = index + 1;
		}
		$Q = [ 1 ];
	} else {
		$Q = [ 1 ];
	}
	return $Q;
}
function same_identity(identity, wanted) {
	const $R = identity;
	let $S = null;
	if ($R[0] === 0) {
		const held = $R[1];
		$S = held === wanted;
	} else {
		$S = false;
	}
	return $S;
}
function relay_for(tracker, target) {
	const relay = subscriber_of(() => {
		const connecting = tracker[0].v[3];
		tracker[0].v[2] = true;
		if (connecting) {
			tracker[0].v[4] = true;
		} else {
			wake(target);
		}
		return;
	}, true);
	const $ab = tracker[0].v[7];
	let $ac = null;
	if ($ab[0] === 0) {
		const site = __clone($ab[1]);
		$ac = place_subscriber(relay[0], site);
	} else {
		$ac = undefined;
	}
	$ac;
	return relay;
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $au = tracker[0].v[6];
	let $av = null;
	if ($au[0] === 0) {
		const lists = $au[1];
		$av = lists;
	} else {
		return;
		$av = undefined;
	}
	const lists2 = $av;
	let $aw = null;
	if (!(is_empty(lists2.v[1]))) {
		const read = __clone(lists2.v[1]);
		lists2.v[1] = [  ];
		tracker[0].v[3] = true;
		let edges = [  ];
		for (const dependency of read) {
			edges.push([ __clone(dependency[0]), dependency[1](relay_for(tracker, target)) ]);
		}
		lists2.v[2] = edges;
		tracker[0].v[3] = false;
		if (tracker[0].v[4]) {
			tracker[0].v[4] = false;
			wake(target);
		}
		$aw = undefined;
	}
	return $aw;
}
function forget_reads(tracker) {
	const $ax = tracker[0].v[6];
	let $ay = null;
	if ($ax[0] === 0) {
		const lists = $ax[1];
		$ay = lists;
	} else {
		return;
		$ay = undefined;
	}
	const lists2 = $ay;
	let $az = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$az = undefined;
	}
	$az;
	if (!(is_empty(lists2.v[1]))) {
		lists2.v[1] = [  ];
	}
}
function detach_tracker(tracker) {
	tracker[0].v[5] = [ 1 ];
	forget_reads(tracker);
}
function place_subscriber(id, site) {
	if (tracing.v) {
		trace_hooks.v[8](id, site);
	}
}
function also_releasing(handle, release) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release();
		const $aI = previous;
		let $aJ = null;
		if ($aI[0] === 0) {
			const earlier = $aI[1];
			$aJ = earlier();
		} else {
			$aJ = undefined;
		}
		return $aJ;
	} ];
}
function relay_to(subscriber) {
	return subscriber_of(() => {
		return wake(subscriber);
	}, true);
}
function placed_at(subscriber, site) {
	place_subscriber(subscriber[0], site);
	return subscriber;
}
function detach_held(handle) {
	const $ap = handle.v;
	let $aq = null;
	if ($ap[0] === 0) {
		const held = $ap[1];
		$aq = detach(held);
	} else {
		$aq = undefined;
	}
	$aq;
	handle.v = [ 1 ];
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function new3(value, caller) {
	let subscribers = [  ];
	const cell3 = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell3[0]), caller);
	}
	return cell3;
}
function new4(value, caller) {
	return new3(value, caller);
}
function new5(value, caller) {
	return new3(value, caller);
}
function switch2(self, select, caller) {
	return [ self, select, caller ];
}
function get(self) {
	return __clone(self[0].v);
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function on_settle(self, subscriber) {
	return attach(self, subscriber);
}
function start(self) {
	return [ () => {
		return get(self);
	}, (subscriber) => {
		return on_settle(self, subscriber);
	}, () => {
		return;
	} ];
}
function is_empty(self) {
	return self.length === 0;
}
function is_none(self) {
	const $y = self;
	return $y[0] === 1;
}
function run_body(runs, body) {
	const run = renew(runs);
	const $H = nursery(run);
	let $I = null;
	if ($H[0] === 0) {
		const nursery2 = $H[1];
		$I = (($J) => {
			return (($K) => {
				return body($J, $K);
			})(nursery2);
		})(run);
	} else {
		$I = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $I;
}
function last(self) {
	let $V = null;
	if (is_empty(self)) {
		$V = [ 1 ];
	} else {
		$V = __list_get(self, self.length - 1);
	}
	return $V;
}
function run_once(runs, tracker, body) {
	const scope2 = open_run(tracker);
	const value = run_body(runs, ($q, $r) => {
		return (($s) => {
			return body($q, $s, $r);
		})(scope2);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs, tracker, body) {
	let value = run_once(runs, tracker, ($l, $m, $n) => {
		return body($l, $m, $n);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs, tracker, ($am, $an, $ao) => {
			return body($am, $an, $ao);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function on_settle2(self, subscriber) {
	return attach(self, subscriber);
}
function start2(self) {
	return [ () => {
		return get(self);
	}, (subscriber) => {
		return on_settle2(self, subscriber);
	}, () => {
		return;
	} ];
}
function start3(self) {
	const select = self[1];
	const site = __clone(self[2]);
	const runs = new2();
	const tracker = new_tracker([ 0, __clone(site) ]);
	const outer2 = start(__clone(self[0]));
	const outer_pull = outer2[0];
	const followed = __shared_new(start2(run_tracked(runs, tracker, ($i, $j, $k) => {
		return select(outer_pull(), $i, $j, $k);
	})));
	const followed_handle = __shared_new([ 1 ]);
	return [ () => {
		return followed.v[0]();
	}, (subscriber) => {
		followed_handle.v = [ 0, followed.v[1](placed_at(relay_to(subscriber), site)) ];
		const reselect = placed_at(subscriber_of(() => {
			detach_held(followed_handle);
			followed.v[2]();
			const next = start2(run_tracked(runs, tracker, ($ar, $as, $at) => {
				return select(outer_pull(), $ar, $as, $at);
			}));
			followed_handle.v = [ 0, next[1](placed_at(relay_to(subscriber), site)) ];
			followed.v = next;
			wake(subscriber);
			return;
		}, true), site);
		const outer_handle = outer2[1](reselect);
		attach_tracker(tracker, reselect);
		return retiring(subscriber, () => {
			detach_held(followed_handle);
			detach(outer_handle);
			detach_tracker(tracker);
			return;
		});
	}, () => {
		detach_tracker(tracker);
		release_runs(runs);
		followed.v[2]();
		outer2[2]();
		return;
	} ];
}
function notify(self, $aB) {
	const traced = tracing.v;
	let $aC = null;
	if (traced) {
		$aC = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$aC = "";
	}
	const cause = $aC;
	const $aD = $aB;
	let $aE = null;
	if ($aD[0] === 0) {
		const turn = $aD[1];
		$aE = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $aF = last(draining_turns.v);
		let $aG = null;
		if ($aF[0] === 0) {
			const draining = $aF[1];
			$aG = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $aH = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$aH = undefined;
				}
				$aH;
			}
			$aG = undefined;
		}
		$aE = $aG;
	}
	return $aE;
}
function set(self, value, $aA, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $aA);
}
function take(self, item, $aO) {
	defer(self, () => {
		dispose(item, $aO);
		return;
	});
	return __clone(item);
}
function cell(self, $g, $h, caller) {
	const instance = start3(self);
	const pull = instance[0];
	const cached = new3(pull(), caller);
	const refresh = subscriber_of(() => {
		set(cached, pull(), $g, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed = instance[1](refresh);
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $g, $h);
	return cached;
}
function memo(self, $e, $f, caller) {
	return [ cell(self, $e, $f, caller) ];
}
function comp(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function get2(self) {
	return get(self[0]);
}
function set2(self, value, $aA, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $aA);
}
function derive(self, transform, caller) {
	return [ self, transform, caller ];
}
function on_settle3(self, subscriber) {
	return attach(self[0], subscriber);
}
function start4(self) {
	return [ () => {
		return get2(self);
	}, (subscriber) => {
		return on_settle3(self, subscriber);
	}, () => {
		return;
	} ];
}
function run_body2(runs, body) {
	const run = renew(runs);
	const $bh = nursery(run);
	let $bi = null;
	if ($bh[0] === 0) {
		const nursery2 = $bh[1];
		$bi = (($J) => {
			return (($K) => {
				return body($J, $K);
			})(nursery2);
		})(run);
	} else {
		$bi = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $bi;
}
function run_once2(runs, tracker, body) {
	const scope2 = open_run(tracker);
	const value = run_body2(runs, ($q, $r) => {
		return (($s) => {
			return body($q, $s, $r);
		})(scope2);
	});
	close_run(tracker);
	return value;
}
function run_tracked2(runs, tracker, body) {
	let value = run_once2(runs, tracker, ($l, $m, $n) => {
		return body($l, $m, $n);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once2(runs, tracker, ($am, $an, $ao) => {
			return body($am, $an, $ao);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function start5(self) {
	const transform = self[1];
	const site = __clone(self[2]);
	const upstream = start4(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new2();
	const tracker = new_tracker([ 0, site ]);
	return [ () => {
		const value = pull();
		return run_tracked2(runs, tracker, ($be, $bf, $bg) => {
			return transform(value, $be, $bf, $bg);
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
function cell2(self, $g, $h, caller) {
	const instance = start5(self);
	const pull = instance[0];
	const cached = new3(pull(), caller);
	const refresh = subscriber_of(() => {
		set(cached, pull(), $g, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed = instance[1](refresh);
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $g, $h);
	return cached;
}
function memo2(self, $e, $f, caller) {
	return [ cell2(self, $e, $f, caller) ];
}
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
const first = new4(1, "reactive-flatten.vl:9:22");
const second = new4(10, "reactive-flatten.vl:10:23");
const outer = new5(__clone(first), "reactive-flatten.vl:11:22");
const $aT = comp(($a) => {
	return memo(switch2(__clone(outer), (inner, $b, $c, $d) => {
		return __clone(inner);
	}, "reactive-flatten.vl:13:38"), [ 1 ], [ 0, $a ], "reactive-flatten.vl:13:60");
});
const joined = $aT[0];
const scope = $aT[1];
console.log(String(get2(joined)));
set(first, 2, [ 1 ], "reactive-flatten.vl:15:8");
console.log(String(get2(joined)));
set2(outer, second, [ 1 ], "reactive-flatten.vl:17:8");
console.log(String(get2(joined)));
set(first, 99, [ 1 ], "reactive-flatten.vl:19:8");
console.log(String(get2(joined)));
set(second, 11, [ 1 ], "reactive-flatten.vl:21:9");
console.log(String(get2(joined)));
const $bj = comp(($ba) => {
	return memo2(derive(__clone(joined), (value, $bb, $bc, $bd) => {
		return value * 2;
	}, "reactive-flatten.vl:25:42"), [ 1 ], [ 0, $ba ], "reactive-flatten.vl:25:68");
});
const doubled = $bj[0];
const stacked = $bj[1];
set(second, 21, [ 1 ], "reactive-flatten.vl:26:9");
console.log(String(get2(doubled)));
dispose2(stacked);
dispose2(scope);
//# sourceMappingURL=reactive-flatten.mjs.map
