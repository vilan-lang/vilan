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
function mint_subscriber(notify2) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify2, derived);
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
		let $W = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$W = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:414:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:417:25");
		}
		$W;
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
	const $U = turn;
	let $V = null;
	if ($U[0] === 0) {
		const ambient = $U[1];
		$V = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $Y = last(draining_turns.v);
		let $Z = null;
		if ($Y[0] === 0) {
			const draining = $Y[1];
			$Z = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$Z = undefined;
		}
		$V = $Z;
	}
	return $V;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $aF) {
	const $aG = $aF;
	let $aH = null;
	if ($aG[0] === 0) {
		const established = $aG[1];
		$aH = [ 0, established ];
	} else {
		$aH = last(draining_turns.v);
	}
	const ambient = $aH;
	release_under(self, ambient);
}
function detach(handle) {
	const $ab = last(releasing_turns.v);
	let $ac = null;
	if ($ab[0] === 0) {
		const at_release = $ab[1];
		$ac = at_release;
	} else {
		$ac = last(draining_turns.v);
	}
	const turn = $ac;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ad = [ 0, handle[0] ];
	let $ae = null;
	if ($ad[0] === 0) {
		const subscribers = $ad[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$ae = undefined;
	} else {
		$ae = undefined;
	}
	$ae;
	const $af = ambient;
	let $ag = null;
	if ($af[0] === 0) {
		const turn = $af[1];
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
		$ag = undefined;
	} else {
		$ag = undefined;
	}
	$ag;
	const $ah = handle[3].v;
	let $ai = null;
	if ($ah[0] === 0) {
		const release = $ah[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ai = undefined;
	} else {
		$ai = undefined;
	}
	return $ai;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aI = null;
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
		$aI = undefined;
	}
	return $aI;
}
function renew(self) {
	const $s = self[0].v[3];
	let $t = null;
	if ($s[0] === 0) {
		const nursery2 = __clone($s[1]);
		let $u = null;
		if (has_spawned(nursery2)) {
			$u = [ 1 ];
		} else {
			$u = [ 0, nursery2 ];
		}
		$t = $u;
	} else {
		$t = [ 1 ];
	}
	const carried = $t;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $F = null;
	if (is_disposed(self)) {
		$F = [ 1 ];
	} else {
		$F = self[0].v[3];
	}
	return $F;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $E = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $v = held[3];
		let $w = null;
		if ($v[0] === 0) {
			const nursery2 = $v[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$w = undefined;
		} else {
			$w = undefined;
		}
		$w;
		let $D = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $y = __guarded(cleanup);
				let $z = null;
				if ($y[0] === 0) {
					const message = $y[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$z = undefined;
				} else {
					$z = undefined;
				}
				$z;
			}
			const $B = failure;
			let $C = null;
			if ($B[0] === 0) {
				const message2 = $B[1];
				$C = (() => {
					throw __panic(message2, "std/src/reactive.vl:1207:27");
				})();
			} else {
				$C = undefined;
			}
			$D = $C;
		}
		$E = $D;
	}
	return $E;
}
function register_with_owner(subscription, $aA, $aB) {
	const $aC = $aB;
	let $aD = null;
	if ($aC[0] === 0) {
		const owner2 = $aC[1];
		$aD = take(owner2, subscription, $aA);
	} else {
		$aD = __clone(subscription);
	}
	return $aD;
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
	const $n = tracker[0].v[6];
	let $o = null;
	if ($n[0] === 0) {
		const lists = $n[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$o = undefined;
	} else {
		$o = undefined;
	}
	$o;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $K = tracker[0].v[6];
	let $L = null;
	if ($K[0] === 0) {
		const lists = $K[1];
		$L = lists;
	} else {
		return;
		$L = undefined;
	}
	const lists2 = $L;
	const $M = tracker[0].v[5];
	let $N = null;
	if ($M[0] === 0) {
		const target = __clone($M[1]);
		$N = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$N = undefined;
	}
	$N;
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
		const $S = reusable(held, kept, dependency[0], position);
		let $T = null;
		if ($S[0] === 0) {
			const index = $S[1];
			__at_put(kept, index, true, "std/src/reactive.vl:1624:5");
			next.push(__clone(__at(held, index, "std/src/reactive.vl:1625:15")));
			$T = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$T = undefined;
		}
		$T;
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
	const $O = identity;
	let $P = null;
	if ($O[0] === 0) {
		const wanted = $O[1];
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
		$P = [ 1 ];
	} else {
		$P = [ 1 ];
	}
	return $P;
}
function same_identity(identity, wanted) {
	const $Q = identity;
	let $R = null;
	if ($Q[0] === 0) {
		const held = $Q[1];
		$R = held === wanted;
	} else {
		$R = false;
	}
	return $R;
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
	const $am = tracker[0].v[6];
	let $an = null;
	if ($am[0] === 0) {
		const lists = $am[1];
		$an = lists;
	} else {
		return;
		$an = undefined;
	}
	const lists2 = $an;
	let $ao = null;
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
		$ao = undefined;
	}
	return $ao;
}
function forget_reads(tracker) {
	const $ap = tracker[0].v[6];
	let $aq = null;
	if ($ap[0] === 0) {
		const lists = $ap[1];
		$aq = lists;
	} else {
		return;
		$aq = undefined;
	}
	const lists2 = $aq;
	let $ar = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$ar = undefined;
	}
	$ar;
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
		const $ay = previous;
		let $az = null;
		if ($ay[0] === 0) {
			const earlier = $ay[1];
			$az = earlier();
		} else {
			$az = undefined;
		}
		return $az;
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
function derive(self, transform) {
	return [ self, transform ];
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
	const $x = self;
	return $x[0] === 1;
}
function run_body(runs, body) {
	const run = renew(runs);
	const $G = nursery(run);
	let $H = null;
	if ($G[0] === 0) {
		const nursery2 = $G[1];
		$H = (($I) => {
			return (($J) => {
				return body($I, $J);
			})(nursery2);
		})(run);
	} else {
		$H = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1318:11");
		})();
	}
	return $H;
}
function last(self) {
	let $X = null;
	if (is_empty(self)) {
		$X = [ 1 ];
	} else {
		$X = __list_get(self, self.length - 1);
	}
	return $X;
}
function run_once(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body(runs, ($p, $q) => {
		return (($r) => {
			return body($p, $r, $q);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs, tracker, body) {
	let value = run_once(runs, tracker, ($k, $l, $m) => {
		return body($k, $l, $m);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs, tracker, ($aj, $ak, $al) => {
			return body($aj, $ak, $al);
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
		return run_tracked(runs, tracker, ($h, $i, $j) => {
			return transform(value, $h, $i, $j);
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
function notify(self, $at) {
	const $au = $at;
	let $av = null;
	if ($au[0] === 0) {
		const turn = $au[1];
		$av = enqueue(turn, __clone(self[1].v));
	} else {
		const $aw = last(draining_turns.v);
		let $ax = null;
		if ($aw[0] === 0) {
			const draining = $aw[1];
			$ax = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$ax = undefined;
		}
		$av = $ax;
	}
	return $av;
}
function set(self, value, $as) {
	self[0].v = __clone(value);
	notify(self, $as);
}
function take(self, item, $aE) {
	defer(self, () => {
		dispose(item, $aE);
		return;
	});
	return __clone(item);
}
function cell(self, $f, $g) {
	const instance = start2(self);
	const pull = instance[0];
	const cached = new3(pull());
	const refreshed = instance[1](subscriber_of(() => {
		set(cached, pull(), $f);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $f, $g);
	return cached;
}
function memo(self, $d, $e) {
	return [ cell(self, $d, $e) ];
}
function observe(signal, observer) {
	const cell2 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $aK = [ 0, cell2 ];
		let $aL = null;
		if ($aK[0] === 0) {
			const live = $aK[1];
			$aL = observer(live.v);
		} else {
			$aL = undefined;
		}
		return $aL;
	}));
}
function attach_observer(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function attach_observer2(self, observer, immediately) {
	return attach_observer(self[0], observer, immediately);
}
function sub(self, observer) {
	return attach_observer2(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function set_with(self, transform, $aM) {
	set(self, transform(get(self)), $aM);
}
function get2(self) {
	return get(self[0]);
}
function sub2(self, observer) {
	return attach_observer(self, (value) => {
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
const owner = new2();
const count = new4(0);
const doubled = memo(derive(__clone(count), (n, $a, $b, $c) => {
	return n * 2;
}), [ 1 ], [ 1 ]);
take(owner, sub(__clone(doubled), (n, $aJ) => {
	return console.log(n);
}), [ 1 ]);
set(count, 1, [ 1 ]);
set_with(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log(String(get2(doubled)));
take(owner, sub2(__clone(count), (n, $aN) => {
	return console.log(n);
}), [ 1 ]);
set(count, 20, [ 1 ]);
