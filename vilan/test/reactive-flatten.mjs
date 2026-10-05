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
function subscriber_of(notify2, derived) {
	return [ fresh_id(), notify2, __shared_new(true), derived ];
}
function is_quiescent(self) {
	return is_empty(self[0].v) && is_empty(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $X = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$X = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:414:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:417:25");
		}
		$X;
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
	const $V = turn;
	let $W = null;
	if ($V[0] === 0) {
		const ambient = $V[1];
		$W = enqueue(ambient, [ reissued(subscriber) ]);
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
		$W = $aa;
	}
	return $W;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function retiring(subscriber, release) {
	return detached(subscriber[0], subscriber[2], release);
}
function detached(id, live, release) {
	let subscribers = [  ];
	const empty = __shared_new(subscribers);
	return [ empty, id, live, __shared_new([ 0, release ]) ];
}
function dispose(self, $aL) {
	const $aM = $aL;
	let $aN = null;
	if ($aM[0] === 0) {
		const established = $aM[1];
		$aN = [ 0, established ];
	} else {
		$aN = last(draining_turns.v);
	}
	const ambient = $aN;
	release_under(self, ambient);
}
function detach(handle) {
	const $ac = last(releasing_turns.v);
	let $ad = null;
	if ($ac[0] === 0) {
		const at_release = $ac[1];
		$ad = at_release;
	} else {
		$ad = last(draining_turns.v);
	}
	const turn = $ad;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ae = [ 0, handle[0] ];
	let $af = null;
	if ($ae[0] === 0) {
		const subscribers = $ae[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$af = undefined;
	} else {
		$af = undefined;
	}
	$af;
	const $ag = ambient;
	let $ah = null;
	if ($ag[0] === 0) {
		const turn = $ag[1];
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
		$ah = undefined;
	} else {
		$ah = undefined;
	}
	$ah;
	const $ai = handle[3].v;
	let $aj = null;
	if ($ai[0] === 0) {
		const release = $ai[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aj = undefined;
	} else {
		$aj = undefined;
	}
	return $aj;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aO = null;
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
		$aO = undefined;
	}
	return $aO;
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
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
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
					throw __panic(message2, "std/src/reactive.vl:1207:27");
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
function register_with_owner(subscription, $aG, $aH) {
	const $aI = $aH;
	let $aJ = null;
	if ($aI[0] === 0) {
		const owner = $aI[1];
		$aJ = take(owner, subscription, $aG);
	} else {
		$aJ = __clone(subscription);
	}
	return $aJ;
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
			__at_put(kept, index, true, "std/src/reactive.vl:1624:5");
			next.push(__clone(__at(held, index, "std/src/reactive.vl:1625:15")));
			$U = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$U = undefined;
		}
		$U;
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
	const $P = identity;
	let $Q = null;
	if ($P[0] === 0) {
		const wanted = $P[1];
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
	const $as = tracker[0].v[6];
	let $at = null;
	if ($as[0] === 0) {
		const lists = $as[1];
		$at = lists;
	} else {
		return;
		$at = undefined;
	}
	const lists2 = $at;
	let $au = null;
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
		$au = undefined;
	}
	return $au;
}
function forget_reads(tracker) {
	const $av = tracker[0].v[6];
	let $aw = null;
	if ($av[0] === 0) {
		const lists = $av[1];
		$aw = lists;
	} else {
		return;
		$aw = undefined;
	}
	const lists2 = $aw;
	let $ax = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$ax = undefined;
	}
	$ax;
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
		const $aE = previous;
		let $aF = null;
		if ($aE[0] === 0) {
			const earlier = $aE[1];
			$aF = earlier();
		} else {
			$aF = undefined;
		}
		return $aF;
	} ];
}
function relay_to(subscriber) {
	return subscriber_of(() => {
		return wake(subscriber);
	}, true);
}
function detach_held(handle) {
	const $an = handle.v;
	let $ao = null;
	if ($an[0] === 0) {
		const held = $an[1];
		$ao = detach(held);
	} else {
		$ao = undefined;
	}
	$ao;
	handle.v = [ 1 ];
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
function new6(value) {
	return new3(value);
}
function switch2(self, select) {
	return [ self, select ];
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
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1318:11");
		})();
	}
	return $I;
}
function last(self) {
	let $Y = null;
	if (is_empty(self)) {
		$Y = [ 1 ];
	} else {
		$Y = __list_get(self, self.length - 1);
	}
	return $Y;
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
		value = run_once(runs, tracker, ($ak, $al, $am) => {
			return body($ak, $al, $am);
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
	const runs = new2();
	const tracker = new_tracker();
	const outer2 = start(__clone(self[0]));
	const outer_pull = outer2[0];
	const followed = __shared_new(start2(run_tracked(runs, tracker, ($i, $j, $k) => {
		return select(outer_pull(), $i, $j, $k);
	})));
	const followed_handle = __shared_new([ 1 ]);
	return [ () => {
		return followed.v[0]();
	}, (subscriber) => {
		followed_handle.v = [ 0, followed.v[1](relay_to(subscriber)) ];
		const reselect = subscriber_of(() => {
			detach_held(followed_handle);
			followed.v[2]();
			const next = start2(run_tracked(runs, tracker, ($ap, $aq, $ar) => {
				return select(outer_pull(), $ap, $aq, $ar);
			}));
			followed_handle.v = [ 0, next[1](relay_to(subscriber)) ];
			followed.v = next;
			wake(subscriber);
			return;
		}, true);
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
function notify(self, $az) {
	const $aA = $az;
	let $aB = null;
	if ($aA[0] === 0) {
		const turn = $aA[1];
		$aB = enqueue(turn, __clone(self[1].v));
	} else {
		const $aC = last(draining_turns.v);
		let $aD = null;
		if ($aC[0] === 0) {
			const draining = $aC[1];
			$aD = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$aD = undefined;
		}
		$aB = $aD;
	}
	return $aB;
}
function set(self, value, $ay) {
	self[0].v = __clone(value);
	notify(self, $ay);
}
function take(self, item, $aK) {
	defer(self, () => {
		dispose(item, $aK);
		return;
	});
	return __clone(item);
}
function cell(self, $g, $h) {
	const instance = start3(self);
	const pull = instance[0];
	const cached = new3(pull());
	const refreshed = instance[1](subscriber_of(() => {
		set(cached, pull(), $g);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $g, $h);
	return cached;
}
function memo(self, $e, $f) {
	return [ cell(self, $e, $f) ];
}
function comp(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function get3(self) {
	return get(self[0]);
}
function set2(self, value, $ay) {
	self[0].v = __clone(value);
	notify(self, $ay);
}
function derive(self, transform) {
	return [ self, transform ];
}
function on_settle3(self, subscriber) {
	return attach(self[0], subscriber);
}
function start4(self) {
	return [ () => {
		return get3(self);
	}, (subscriber) => {
		return on_settle3(self, subscriber);
	}, () => {
		return;
	} ];
}
function run_body2(runs, body) {
	const run = renew(runs);
	const $bb = nursery(run);
	let $bc = null;
	if ($bb[0] === 0) {
		const nursery2 = $bb[1];
		$bc = (($J) => {
			return (($K) => {
				return body($J, $K);
			})(nursery2);
		})(run);
	} else {
		$bc = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1318:11");
		})();
	}
	return $bc;
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
		value = run_once2(runs, tracker, ($ak, $al, $am) => {
			return body($ak, $al, $am);
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
	const upstream = start4(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new2();
	const tracker = new_tracker();
	return [ () => {
		const value = pull();
		return run_tracked2(runs, tracker, ($aY, $aZ, $ba) => {
			return transform(value, $aY, $aZ, $ba);
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
function cell2(self, $g, $h) {
	const instance = start5(self);
	const pull = instance[0];
	const cached = new3(pull());
	const refreshed = instance[1](subscriber_of(() => {
		set(cached, pull(), $g);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $g, $h);
	return cached;
}
function memo2(self, $e, $f) {
	return [ cell2(self, $e, $f) ];
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const run_nurseries_allocated_count = __shared_new(0);
const first = new4(1);
const second = new4(10);
const outer = new6(__clone(first));
const $aP = comp(($a) => {
	return memo(switch2(__clone(outer), (inner, $b, $c, $d) => {
		return __clone(inner);
	}), [ 1 ], [ 0, $a ]);
});
const joined = $aP[0];
const scope = $aP[1];
console.log(String(get3(joined)));
set(first, 2, [ 1 ]);
console.log(String(get3(joined)));
set2(outer, second, [ 1 ]);
console.log(String(get3(joined)));
set(first, 99, [ 1 ]);
console.log(String(get3(joined)));
set(second, 11, [ 1 ]);
console.log(String(get3(joined)));
const $bd = comp(($aU) => {
	return memo2(derive(__clone(joined), (value, $aV, $aW, $aX) => {
		return value * 2;
	}), [ 1 ], [ 0, $aU ]);
});
const doubled = $bd[0];
const stacked = $bd[1];
set(second, 21, [ 1 ]);
console.log(String(get3(doubled)));
dispose2(stacked);
dispose2(scope);
