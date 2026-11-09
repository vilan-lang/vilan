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
function __force(cell) {
	if (cell.state === 2) return cell.value;
	if (cell.state === 1) throw "lazy initialization cycle: `" + cell.name + "`";
	if (cell.state === 3) throw "lazy `" + cell.name + "` is poisoned: its initializer panicked: " + (cell.value && cell.value.location !== undefined ? cell.value.message : cell.value);
	cell.state = 1;
	try {
		cell.value = cell.thunk();
	} catch (failure) {
		cell.state = 3;
		cell.value = failure;
		throw failure;
	}
	cell.state = 2;
	cell.thunk = null;
	return cell.value;
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
function __lazy(name, thunk) {
	return { name: name, state: 0, value: undefined, thunk: thunk };
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
function __remove_at(list, index, location) {
	if (index >= 0 && index < list.length) return list.splice(index, 1)[0];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
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
function assert(condition, message, caller) {
	let $bp = null;
	if (!(condition)) {
		$bp = (() => {
			throw __panic(message, caller);
		})();
	}
	return $bp;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $h = null;
	if (wrapped < 0) {
		$h = wrapped + modulus;
	} else {
		$h = wrapped;
	}
	return $h;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $g = null;
	if (truncated > 0) {
		$g = truncated;
	} else {
		$g = 0;
	}
	return $g;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $i = null;
	if (wrapped >= half) {
		$i = wrapped - modulus;
	} else {
		$i = wrapped;
	}
	return $i;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
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
		let $aA = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$aA = undefined;
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
		$aA;
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
	const $ay = turn;
	let $az = null;
	if ($ay[0] === 0) {
		const ambient = $ay[1];
		$az = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $aB = last(draining_turns.v);
		let $aC = null;
		if ($aB[0] === 0) {
			const draining = $aB[1];
			$aC = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$aC = undefined;
		}
		$az = $aC;
	}
	return $az;
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
function retiring(subscriber, release4) {
	return detached(subscriber[0], subscriber[2], release4);
}
function detached(id, live, release4) {
	let subscribers = [  ];
	const empty = __shared_new(subscribers);
	return [ empty, id, live, __shared_new([ 0, release4 ]) ];
}
function dispose(self, $bm) {
	const $bn = $bm;
	let $bo = null;
	if ($bn[0] === 0) {
		const established = $bn[1];
		$bo = [ 0, established ];
	} else {
		$bo = last(draining_turns.v);
	}
	const ambient = $bo;
	release_under(self, ambient);
}
function detach(handle) {
	const $aG = last(releasing_turns.v);
	let $aH = null;
	if ($aG[0] === 0) {
		const at_release = $aG[1];
		$aH = at_release;
	} else {
		$aH = last(draining_turns.v);
	}
	const turn = $aH;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $aI = [ 0, handle[0] ];
	let $aJ = null;
	if ($aI[0] === 0) {
		const subscribers = $aI[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aJ = undefined;
	} else {
		$aJ = undefined;
	}
	$aJ;
	const $aK = ambient;
	let $aL = null;
	if ($aK[0] === 0) {
		const turn = $aK[1];
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
		$aL = undefined;
	} else {
		$aL = undefined;
	}
	$aL;
	const $aM = handle[3].v;
	let $aN = null;
	if ($aM[0] === 0) {
		const release4 = $aM[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release4, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aN = undefined;
	} else {
		$aN = undefined;
	}
	return $aN;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $p = null;
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
		$p = undefined;
	}
	return $p;
}
function renew(self) {
	const $X = self[0].v[3];
	let $Y = null;
	if ($X[0] === 0) {
		const nursery2 = __clone($X[1]);
		let $Z = null;
		if (has_spawned(nursery2)) {
			$Z = [ 1 ];
		} else {
			$Z = [ 0, nursery2 ];
		}
		$Y = $Z;
	} else {
		$Y = [ 1 ];
	}
	const carried = $Y;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $ak = null;
	if (is_disposed(self)) {
		$ak = [ 1 ];
	} else {
		$ak = self[0].v[3];
	}
	return $ak;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $aj = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $aa = held[3];
		let $ab = null;
		if ($aa[0] === 0) {
			const nursery2 = $aa[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$ab = undefined;
		} else {
			$ab = undefined;
		}
		$ab;
		let $ai = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $ad = __guarded(cleanup);
				let $ae = null;
				if ($ad[0] === 0) {
					const message = $ad[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$ae = undefined;
				} else {
					$ae = undefined;
				}
				$ae;
			}
			const $ag = failure;
			let $ah = null;
			if ($ag[0] === 0) {
				const message2 = $ag[1];
				$ah = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$ah = undefined;
			}
			$ai = $ah;
		}
		$aj = $ai;
	}
	return $aj;
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
function defer_to_owner(cleanup, $m) {
	const $n = $m;
	let $o = null;
	if ($n[0] === 0) {
		const owner = $n[1];
		$o = defer(owner, cleanup);
	} else {
		$o = undefined;
	}
	return $o;
}
function release_runs(runs2) {
	dispose2([ runs2[0], runs2[0].v[0] ]);
}
function new_tracker(site) {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ], site ]) ];
}
function lists_of(tracker) {
	const $C = tracker[0].v[6];
	let $D = null;
	if ($C[0] === 0) {
		const lists = $C[1];
		$D = lists;
	} else {
		trackers_allocated_count.v = trackers_allocated_count.v + 1;
		const made = __shared_new([ [  ], [  ], [  ] ]);
		tracker[0].v[6] = [ 0, made ];
		$D = made;
	}
	return $D;
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $S = tracker[0].v[6];
	let $T = null;
	if ($S[0] === 0) {
		const lists = $S[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$T = undefined;
	} else {
		$T = undefined;
	}
	$T;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
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
	const $ar = tracker[0].v[5];
	let $as = null;
	if ($ar[0] === 0) {
		const target = __clone($ar[1]);
		$as = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$as = undefined;
	}
	$as;
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
		const $av = reusable(held, kept, dependency[0], position);
		let $aw = null;
		if ($av[0] === 0) {
			const index = $av[1];
			__at_put(kept, index, true, "std/src/reactive.vl:1715:5");
			next.push(__clone(__at(held, index, "std/src/reactive.vl:1716:15")));
			$aw = undefined;
		} else {
			next.push([ __clone(dependency[0]), dependency[1](relay_for(tracker, target)) ]);
			$aw = undefined;
		}
		$aw;
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
function reusable(held, kept, identity3, position) {
	const $at = identity3;
	let $au = null;
	if ($at[0] === 0) {
		const wanted = $at[1];
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
		$au = [ 1 ];
	} else {
		$au = [ 1 ];
	}
	return $au;
}
function same_identity(identity3, wanted) {
	const $A = identity3;
	let $B = null;
	if ($A[0] === 0) {
		const held = $A[1];
		$B = held === wanted;
	} else {
		$B = false;
	}
	return $B;
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
	const $aD = tracker[0].v[7];
	let $aE = null;
	if ($aD[0] === 0) {
		const site = __clone($aD[1]);
		$aE = place_subscriber(relay[0], site);
	} else {
		$aE = undefined;
	}
	$aE;
	return relay;
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $aR = tracker[0].v[6];
	let $aS = null;
	if ($aR[0] === 0) {
		const lists = $aR[1];
		$aS = lists;
	} else {
		return;
		$aS = undefined;
	}
	const lists2 = $aS;
	let $aT = null;
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
		$aT = undefined;
	}
	return $aT;
}
function forget_reads(tracker) {
	const $aU = tracker[0].v[6];
	let $aV = null;
	if ($aU[0] === 0) {
		const lists = $aU[1];
		$aV = lists;
	} else {
		return;
		$aV = undefined;
	}
	const lists2 = $aV;
	let $aW = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aW = undefined;
	}
	$aW;
	if (!(is_empty(lists2.v[1]))) {
		lists2.v[1] = [  ];
	}
}
function detach_tracker(tracker) {
	tracker[0].v[5] = [ 1 ];
	forget_reads(tracker);
}
function admits(scope, identity3) {
	const tracker = __clone(scope[0]);
	if (!(tracker[0].v[1]) || tracker[0].v[0] !== scope[1]) {
		return false;
	}
	const $w = tracker[0].v[6];
	let $x = null;
	if ($w[0] === 0) {
		const lists = $w[1];
		$x = lists;
	} else {
		return true;
		$x = undefined;
	}
	const lists2 = $x;
	const $y = identity3;
	let $z = null;
	if ($y[0] === 0) {
		const wanted = $y[1];
		const count2 = lists2.v[0].length;
		let index = 0;
		while (index < count2) {
			if (same_identity(__at(lists2.v[0], index, "std/src/reactive.vl:1877:22")[0], wanted)) {
				return false;
			}
			index = index + 1;
		}
		$z = true;
	} else {
		$z = true;
	}
	return $z;
}
function place_subscriber(id, site) {
	if (tracing.v) {
		trace_hooks.v[8](id, site);
	}
}
function also_releasing(handle, release4) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release4();
		const $bf = previous;
		let $bg = null;
		if ($bf[0] === 0) {
			const earlier = $bf[1];
			$bg = earlier();
		} else {
			$bg = undefined;
		}
		return $bg;
	} ];
}
function settle_together(body, $cg) {
	const $ch = $cg;
	let $ci = null;
	if ($ch[0] === 0) {
		const current = $ch[1];
		$ci = body(current);
	} else {
		const $cj = last(draining_turns.v);
		let $ck = null;
		if ($cj[0] === 0) {
			const draining = $cj[1];
			$ck = body(draining);
		} else {
			$ck = batch(body, $cg);
		}
		$ci = $ck;
	}
	return $ci;
}
function new4() {
	return [ [  ] ];
}
function follow(self, at2) {
	if (self[0].length === 0) {
		self[0].push(new5());
	}
	return follow2(__at(self[0], 0, "std/src/reactive/lens.vl:142:3"), at2);
}
function apply(self, change2, wakes) {
	if (self[0].length > 0) {
		apply2(__at(self[0], 0, "std/src/reactive/lens.vl:147:4"), change2, wakes);
	}
}
function locate(self, slot2) {
	if (self[0].length === 0) {
		return [ 1 ];
	}
	return locate2(__at(self[0], 0, "std/src/reactive/lens.vl:155:3"), slot2);
}
function follows_items(self) {
	return true;
}
function release(self, slot2) {
	if (self[0].length > 0) {
		release2(__at(self[0], 0, "std/src/reactive/lens.vl:164:4"), slot2);
	}
}
function new5() {
	return [ [  ], [  ], [  ], [  ], [  ], [  ], 0 ];
}
function seat(self, at2) {
	let low = 0;
	let high = self[3].length;
	while (high - low > 1) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[3], middle, "std/src/reactive/lens.vl:210:7") <= at2) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at2 - __at(self[3], low, "std/src/reactive/lens.vl:216:14") ];
}
function restart(self) {
	let starts = [  ];
	let running = 0;
	let place = 0;
	for (const block of self[1]) {
		starts.push(running);
		__at_put(self[2], as_usize(block), place, "std/src/reactive/lens.vl:226:4");
		running = running + __at(self[0], as_usize(block), "std/src/reactive/lens.vl:227:15").length;
		place = place + 1;
	}
	self[3] = starts;
}
function renumber(self, block, from) {
	let index = from;
	const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:236:14").length;
	while (index < held) {
		const slot2 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:238:15"), index, "std/src/reactive/lens.vl:238:15");
		if (slot2 !== -(1)) {
			__at_put(self[4], as_usize(slot2), block, "std/src/reactive/lens.vl:240:5");
			__at_put(self[5], as_usize(slot2), index, "std/src/reactive/lens.vl:241:5");
		}
		index = index + 1;
	}
}
function open(self, place) {
	const block = as_i32(self[0].length);
	self[0].push([  ]);
	self[2].push(0);
	__insert_at(self[1], place, block, "std/src/reactive/lens.vl:252:14");
	return block;
}
function widen(self, at2, count2) {
	if (count2 === 0 || at2 >= self[6]) {
		return;
	}
	const $bB = seat(self, at2);
	const place = $bB[0];
	const index = $bB[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:262:15");
	let $bC = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:266:5"), index, -(1), "std/src/reactive/lens.vl:266:35");
			made = made + 1;
		}
		$bC = undefined;
	} else {
		const before = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:270:17"));
		let grown = [  ];
		let copied = 0;
		while (copied < index) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:274:16"));
			copied = copied + 1;
		}
		let made2 = 0;
		while (made2 < count2) {
			grown.push(-(1));
			made2 = made2 + 1;
		}
		const held = before.length;
		while (copied < held) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:284:16"));
			copied = copied + 1;
		}
		__at_put(self[0], as_usize(block), grown, "std/src/reactive/lens.vl:287:4");
	}
	$bC;
	self[6] = self[6] + count2;
	renumber(self, block, index + count2);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:291:6").length > 2 * block_size) {
		rebalance(self, place);
	}
	restart(self);
}
function close(self, at2, count2, wakes) {
	let left = count2;
	while (left > 0 && at2 < self[6]) {
		const $bz = seat(self, at2);
		const place = $bz[0];
		const index = $bz[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:303:16");
		const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:304:15").length;
		let $bA = null;
		if (left < held - index) {
			$bA = left;
		} else {
			$bA = held - index;
		}
		const take2 = $bA;
		let taken = 0;
		while (taken < take2) {
			const slot2 = __remove_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:308:16"), index, "std/src/reactive/lens.vl:308:46");
			if (slot2 !== -(1)) {
				__at_put(self[4], as_usize(slot2), -(1), "std/src/reactive/lens.vl:310:6");
				wakes.push([ 2, slot2 ]);
			}
			taken = taken + 1;
		}
		self[6] = self[6] - take2;
		left = left - take2;
		if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:317:7").length === 0) {
			__remove_at(self[1], place, "std/src/reactive/lens.vl:318:16");
		} else {
			renumber(self, block, index);
		}
		restart(self);
	}
}
function rebalance(self, place) {
	const block = __at(self[1], place, "std/src/reactive/lens.vl:328:15");
	const all = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:329:13"));
	let first = [  ];
	let index = 0;
	while (index < block_size) {
		first.push(__at(all, index, "std/src/reactive/lens.vl:333:15"));
		index = index + 1;
	}
	__at_put(self[0], as_usize(block), first, "std/src/reactive/lens.vl:336:3");
	let next = place + 1;
	const held = all.length;
	while (index < held) {
		const fresh = open(self, next);
		let filled = 0;
		while (filled < block_size && index < held) {
			__at(self[0], as_usize(fresh), "std/src/reactive/lens.vl:343:5").push(__at(all, index, "std/src/reactive/lens.vl:343:40"));
			index = index + 1;
			filled = filled + 1;
		}
		renumber(self, fresh, 0);
		next = next + 1;
	}
}
function follow2(self, at2) {
	while (self[6] <= at2) {
		const places = self[1].length;
		if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:358:34")), "std/src/reactive/lens.vl:358:22").length >= block_size) {
			const fresh = open(self, places);
			__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:360:5");
			self[3].push(self[6]);
		}
		const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:363:15");
		__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:364:4").push(-(1));
		self[6] = self[6] + 1;
	}
	const $j = seat(self, at2);
	const place = $j[0];
	const index = $j[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:368:15");
	const held = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:369:14"), index, "std/src/reactive/lens.vl:369:14");
	if (held !== -(1)) {
		return held;
	}
	const slot2 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:376:3"), index, slot2, "std/src/reactive/lens.vl:376:3");
	return slot2;
}
function apply2(self, change2, wakes) {
	const $bx = change2;
	let $by = null;
	if ($bx[0] === 0) {
		const at2 = $bx[1];
		const removed = $bx[2];
		const inserted = $bx[3];
		close(self, at2, removed, wakes);
		widen(self, at2, inserted);
		$by = undefined;
	} else if ($bx[0] === 1) {
		const at3 = $bx[1];
		const count2 = $bx[2];
		$by = widen(self, at3, count2);
	} else if ($bx[0] === 3) {
		const at4 = $bx[1];
		const count3 = $bx[2];
		$by = close(self, at4, count3, wakes);
	} else if ($bx[0] === 2) {
		const at5 = $bx[1];
		let $bE = null;
		if (at5 < self[6]) {
			const $bD = seat(self, at5);
			const place = $bD[0];
			const index = $bD[1];
			const slot2 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:393:29")), "std/src/reactive/lens.vl:393:17"), index, "std/src/reactive/lens.vl:393:17");
			if (slot2 !== -(1)) {
				wakes.push([ 0, slot2 ]);
			}
			$bE = undefined;
		}
		$by = $bE;
	} else if ($bx[0] === 4) {
		const at6 = $bx[1];
		const part2 = $bx[2];
		let $bG = null;
		if (at6 < self[6]) {
			const $bF = seat(self, at6);
			const place2 = $bF[0];
			const index2 = $bF[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:402:29")), "std/src/reactive/lens.vl:402:17"), index2, "std/src/reactive/lens.vl:402:17");
			if (slot3 !== -(1)) {
				wakes.push([ 1, slot3, part2 ]);
			}
			$bG = undefined;
		}
		$by = $bG;
	} else if ($bx[0] === 5) {
		const from = $bx[1];
		const to = $bx[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $bH = null;
		if (from > to) {
			$bH = from;
		} else {
			$bH = to;
		}
		const far = $bH;
		while (self[6] <= far) {
			const places = self[1].length;
			if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:416:36")), "std/src/reactive/lens.vl:416:24").length >= block_size) {
				const fresh = open(self, places);
				__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:418:7");
				self[3].push(self[6]);
			}
			const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:421:17");
			__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:422:6").push(-(1));
			self[6] = self[6] + 1;
		}
		const $bI = seat(self, from);
		const left = $bI[0];
		const index3 = $bI[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:426:18");
		const slot4 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:427:16"), index3, "std/src/reactive/lens.vl:427:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:429:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:430:17");
		} else {
			renumber(self, source, index3);
		}
		restart(self);
		let $bJ = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$bJ = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:438:25")), "std/src/reactive/lens.vl:438:13").length ];
		} else {
			$bJ = seat(self, to);
		}
		const $bK = $bJ;
		const place3 = $bK[0];
		const landing = $bK[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:442:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:443:5"), landing, slot4, "std/src/reactive/lens.vl:443:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing);
		restart(self);
		$by = undefined;
	} else {
		let slot5 = 0;
		const slots = as_i32(self[4].length);
		while (slot5 < slots) {
			if (__at(self[4], as_usize(slot5), "std/src/reactive/lens.vl:452:9") !== -(1)) {
				__at_put(self[4], as_usize(slot5), -(1), "std/src/reactive/lens.vl:453:7");
				wakes.push([ 2, slot5 ]);
			}
			slot5 = slot5 + 1;
		}
		self[0] = [  ];
		self[1] = [  ];
		self[2] = [  ];
		self[3] = [  ];
		self[6] = 0;
		$by = undefined;
	}
	return $by;
}
function release2(self, slot2) {
	const block = __at(self[4], as_usize(slot2), "std/src/reactive/lens.vl:468:15");
	if (block === -(1)) {
		return;
	}
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:472:3"), __at(self[5], as_usize(slot2), "std/src/reactive/lens.vl:472:33"), -(1), "std/src/reactive/lens.vl:472:3");
	__at_put(self[4], as_usize(slot2), -(1), "std/src/reactive/lens.vl:473:3");
}
function locate2(self, slot2) {
	const block = __at(self[4], as_usize(slot2), "std/src/reactive/lens.vl:477:15");
	if (block === -(1)) {
		return [ 1 ];
	}
	return [ 0, __at(self[3], __at(self[2], as_usize(block), "std/src/reactive/lens.vl:481:20"), "std/src/reactive/lens.vl:481:8") + __at(self[5], as_usize(slot2), "std/src/reactive/lens.vl:481:51") ];
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function count(self) {
	return self.length;
}
function item(self, at2) {
	return __list_get(self, at2);
}
function lend(self, at2, read) {
	if (at2 < self.length) {
		read(__at(self, at2, "std/src/reactive/lens.vl:2193:10"));
	}
}
function put(self, at2, value) {
	__at_put(self, at2, __clone(value), "std/src/reactive/lens.vl:2188:3");
}
function change(self, at2, with2) {
	if (at2 < self.length) {
		with2(__at(self, at2, "std/src/reactive/lens.vl:2199:14"));
	}
}
function new6(value, caller) {
	let subscribers = [  ];
	const cell2 = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell2[0]), caller);
	}
	return cell2;
}
function lens_over(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new6(0, "std/src/reactive/lens.vl:726:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over(value, handler, access) {
	return lens_over([ 0, __shared_new(value) ], handler, access);
}
function sequence(value, $a) {
	return over(value, new4(), [ (held, at2) => {
		return at2 < count(held);
	}, (held, at2) => {
		return item(held, at2);
	}, (held, at2, read) => {
		return lend(held, at2, read);
	}, (held, at2, value2) => {
		return put(held, at2, value2, $a);
	}, (held, at2, with2) => {
		return change(held, at2, with2, $a);
	} ]);
}
function lend2(value, read) {
	return read(value);
}
function viewing(cell2, read) {
	return lend2(cell2.v, read);
}
function viewing2(cell2, read) {
	return lend2(cell2.v, read);
}
function peek_storage(storage, read) {
	const $c = storage;
	let $d = null;
	if ($c[0] === 0) {
		const cell2 = $c[1];
		$d = viewing2(cell2, read);
	} else {
		const port = $c[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $e = out;
		let $f = null;
		if ($e[0] === 0) {
			const value = __clone($e[1]);
			$f = value;
		} else {
			$f = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:710:13");
			})();
		}
		$d = $f;
	}
	return $d;
}
function follow_id(storage, watch, holds, at2) {
	if (viewing(watch, (held) => {
		return follows_items(held[0]);
	}) && !(peek_storage(storage, (held) => {
		return holds(held, at2);
	}))) {
		return -(1);
	}
	const id = follow(watch.v[0], at2);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1058:2")[2] = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1058:2")[2] + 1;
	return id;
}
function slot_of(self, id) {
	const $l = __clone(self);
	let $k = null;
	if (id < 0) {
		$k = -(1);
	} else {
		$k = id;
	}
	return [ $l, $k ];
}
function follow3(self, at2) {
	const core = self[0];
	return slot_of(self, follow_id(core.v[0], core.v[3], core.v[1][0], at2));
}
function release_id(watch, id) {
	if (id < 0) {
		return;
	}
	const entry = as_usize(id);
	const left = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1068:13")[2] - 1;
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1069:2")[2] = left;
	if (left <= 0) {
		release(watch.v[0], id);
	}
}
function release3(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot(self, at2, $b) {
	const handle = follow3(self, at2);
	defer_to_owner(() => {
		return release3(handle);
	}, $b);
	return handle;
}
function part_cell(watch, slot2, name) {
	if (slot2 < 0) {
		return new6(0, "std/src/reactive/lens.vl:1094:19");
	}
	const found = viewing2(watch, (held) => {
		let out = [ 1 ];
		for (const part2 of __at(held[1], as_usize(slot2), "std/src/reactive/lens.vl:1098:15")[1]) {
			if (part2[0] === name) {
				out = [ 0, __clone(part2[1]) ];
			}
		}
		return out;
	});
	const $q = found;
	if ($q[0] === 0) {
		return $q[1];
	}
	const woke = new6(0, "std/src/reactive/lens.vl:1108:25");
	const entry = as_usize(slot2);
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1110:2")[1].push([ name, __clone(woke) ]);
	return woke;
}
function part(self, name, read) {
	return [ __clone(self), part_cell(self[0][0].v[3], self[1], name), read ];
}
function identity(self) {
	return [ 0, __shared_identity(self[0]) ];
}
function identity2(self) {
	return identity(self[1]);
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function on_settle(self, subscriber) {
	return attach(self, subscriber);
}
function on_settle2(self, subscriber) {
	return on_settle(self[1], subscriber);
}
function at(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][0].v[3], (held) => {
		return locate(held[0], self[1]);
	});
}
function peek_storage2(storage, read) {
	const $F = storage;
	let $G = null;
	if ($F[0] === 0) {
		const cell2 = $F[1];
		$G = viewing2(cell2, read);
	} else {
		const port = $F[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $H = out;
		let $I = null;
		if ($H[0] === 0) {
			const value = __clone($H[1]);
			$I = value;
		} else {
			$I = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:710:13");
			})();
		}
		$G = $I;
	}
	return $G;
}
function peek(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function get(self) {
	const $E = at(self[0]);
	if ($E[0] === 0) {

	} else {
		return [ 1 ];
	}
	const view = self[0][0][0].v[1][2];
	const read = self[2];
	let out = [ 1 ];
	peek(self[0][0], (held) => {
		return view(held, $E[1], (item2) => {
			return out = [ 0, read(item2) ];
		});
	});
	return out;
}
function track(self, $v) {
	const scope = $v;
	const identity3 = identity2(self);
	if (admits(scope, identity3)) {
		lists_of(scope[0]).v[0].push([ identity3, (subscriber) => {
			return on_settle2(self, subscriber);
		} ]);
	}
	return get(self);
}
function unwrap_or(self, fallback) {
	const $J = self;
	let $K = null;
	if ($J[0] === 0) {
		const x = $J[1];
		$K = x;
	} else {
		$K = __clone(__force(fallback));
	}
	return $K;
}
function derive(body, caller) {
	return [ body, caller ];
}
function is_empty(self) {
	return self.length === 0;
}
function is_none(self) {
	const $ac = self;
	return $ac[0] === 1;
}
function run_body(runs2, body) {
	const run = renew(runs2);
	const $al = nursery(run);
	let $am = null;
	if ($al[0] === 0) {
		const nursery2 = $al[1];
		$am = (($an) => {
			return (($ao) => {
				return body($an, $ao);
			})(nursery2);
		})(run);
	} else {
		$am = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $am;
}
function last(self) {
	let $ax = null;
	if (is_empty(self)) {
		$ax = [ 1 ];
	} else {
		$ax = __list_get(self, self.length - 1);
	}
	return $ax;
}
function run_once(runs2, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body(runs2, ($U, $V) => {
		return (($W) => {
			return body($U, $W, $V);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs2, tracker, body) {
	let value = run_once(runs2, tracker, ($P, $Q, $R) => {
		return body($P, $Q, $R);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs2, tracker, ($aO, $aP, $aQ) => {
			return body($aO, $aP, $aQ);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function start(self) {
	const body = self[0];
	const runs2 = new3();
	const tracker = new_tracker([ 0, __clone(self[1]) ]);
	return [ () => {
		return run_tracked(runs2, tracker, body);
	}, (subscriber) => {
		attach_tracker(tracker, subscriber);
		return retiring(subscriber, () => {
			return detach_tracker(tracker);
		});
	}, () => {
		detach_tracker(tracker);
		release_runs(runs2);
		return;
	} ];
}
function notify(self, $aY) {
	const traced = tracing.v;
	let $aZ = null;
	if (traced) {
		$aZ = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$aZ = "";
	}
	const cause = $aZ;
	const $ba = $aY;
	let $bb = null;
	if ($ba[0] === 0) {
		const turn = $ba[1];
		$bb = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $bc = last(draining_turns.v);
		let $bd = null;
		if ($bc[0] === 0) {
			const draining = $bc[1];
			$bd = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $be = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$be = undefined;
				}
				$be;
			}
			$bd = undefined;
		}
		$bb = $bd;
	}
	return $bb;
}
function set(self, value, $aX, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $aX);
}
function take(self, item2, $bl) {
	defer(self, () => {
		dispose(item2, $bl);
		return;
	});
	return __clone(item2);
}
function cell(self, $N, $O, caller) {
	const instance = start(self);
	const pull = instance[0];
	const cached = new6(pull(), caller);
	const refresh = subscriber_of(() => {
		set(cached, pull(), $N, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed = instance[1](refresh);
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $N, $O);
	return cached;
}
function memo(self, $L, $M, caller) {
	return [ cell(self, $L, $M, caller) ];
}
function get2(self) {
	return __clone(self[0].v);
}
function get3(self) {
	return get2(self[0]);
}
function peek2(self, read) {
	return peek_storage(self[0].v[0], read);
}
function has(self, at2) {
	const holds = self[0].v[1][0];
	return peek2(self, (held) => {
		return holds(held, at2);
	});
}
function coming_one(self, change2) {
	if (self[0].v[4].length === 0) {
		return;
	}
	const taps = __clone(self[0].v[4]);
	for (const tap of taps) {
		tap[1](change2);
	}
}
function mutate(self, change2) {
	const $br = self[0].v[0];
	let $bs = null;
	if ($br[0] === 0) {
		const cell2 = $br[1];
		$bs = change2(cell2.v);
	} else {
		const port = __clone($br[1]);
		$bs = port[1](change2);
	}
	return $bs;
}
function wake_all(watch, slot2, $ca) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot2), "std/src/reactive/lens.vl:1117:36"));
	});
	const $cb = held[0];
	if ($cb[0] === 0) {
		notify($cb[1], $ca);
	}
	for (const part2 of held[1]) {
		notify(part2[1], $ca);
	}
}
function wake_part(watch, slot2, name, $cc) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot2), "std/src/reactive/lens.vl:1127:36"));
	});
	const $cd = held[0];
	if ($cd[0] === 0) {
		notify($cd[1], $cc);
	}
	for (const part2 of held[1]) {
		if (part2[0] === name) {
			notify(part2[1], $cc);
		}
	}
}
function batch(body, $cl) {
	const $cm = $cl;
	let $cn = null;
	if ($cm[0] === 0) {
		const current = $cm[1];
		$cn = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$cn = result;
	}
	return $cn;
}
function landed(self, changes, late, $bu) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index = 0;
	for (const change2 of changes) {
		const $bv = change2;
		let $bw = null;
		if ($bv[0] === 0) {
			const _at = $bv[1];
			const removed = $bv[2];
			const inserted = $bv[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$bw = undefined;
		} else if ($bv[0] === 1) {
			const _at2 = $bv[1];
			const _count = $bv[2];
			$bw = resized = true;
		} else if ($bv[0] === 3) {
			const _at3 = $bv[1];
			const _count2 = $bv[2];
			$bw = resized = true;
		} else if ($bv[0] === 6) {
			$bw = resized = true;
		} else if ($bv[0] === 5) {
			const _from = $bv[1];
			const _to = $bv[2];
			$bw = shaped = true;
		} else {
			$bw = undefined;
		}
		$bw;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change2, wakes);
		}
		let $bM = null;
		if (self[0].v[4].length > 0) {
			let $bL = null;
			if (!(late)) {
				$bL = [ 0 ];
			} else if (!(several)) {
				$bL = [ 1 ];
			} else if (index === 0) {
				$bL = [ 2 ];
			} else {
				$bL = [ 3 ];
			}
			const stage = $bL;
			const taps = __clone(self[0].v[4]);
			for (const tap of taps) {
				tap[2](change2, stage);
			}
			$bM = undefined;
		}
		$bM;
		index = index + 1;
	}
	const published = __clone(self[0].v[2]);
	let $bN = null;
	if (shaped || resized) {
		$bN = watch.v[2];
	} else {
		$bN = [ 1 ];
	}
	const shape = __clone($bN);
	let $bO = null;
	if (resized) {
		$bO = watch.v[3];
	} else {
		$bO = [ 1 ];
	}
	const sized = __clone($bO);
	let $bP = undefined;
	let $bQ = undefined;
	if (wakes.length === 0 && (($bP = shape), $bP[0] === 1) && (($bQ = sized), $bQ[0] === 1)) {
		notify(published, $bu);
		return;
	}
	settle_together(($bX) => {
		for (const wake2 of wakes) {
			const $bY = wake2;
			let $bZ = null;
			if ($bY[0] === 0) {
				const slot2 = $bY[1];
				$bZ = wake_all(watch, slot2, [ 0, $bX ]);
			} else if ($bY[0] === 2) {
				const slot3 = $bY[1];
				$bZ = wake_all(watch, slot3, [ 0, $bX ]);
			} else {
				const slot4 = $bY[1];
				const part2 = $bY[2];
				$bZ = wake_part(watch, slot4, part2, [ 0, $bX ]);
			}
			$bZ;
		}
		const $ce = sized;
		if ($ce[0] === 0) {
			notify($ce[1], [ 0, $bX ]);
		}
		const $cf = shape;
		if ($cf[0] === 0) {
			notify($cf[1], [ 0, $bX ]);
		}
		notify(published, [ 0, $bX ]);
		return;
	}, $bu);
}
function landed_one(self, change2, $bt) {
	landed(self, [ __clone(change2) ], false, $bt);
}
function edit(self, at2, part2, change2, $bq) {
	if (!(has(self, at2))) {
		return false;
	}
	coming_one(self, [ 4, __clone(at2), part2 ]);
	const modify = self[0].v[1][4];
	mutate(self, (held) => {
		return modify(held, at2, change2);
	});
	landed_one(self, [ 4, __clone(at2), part2 ], $bq);
	return true;
}
function peek3(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function size(self) {
	return peek3(self, (held) => {
		return count(held);
	});
}
function splice(self, at2, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self, at2, "std/src/reactive/lens.vl:2177:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at2 + offset, __clone(value), "std/src/reactive/lens.vl:2182:9");
		offset = offset + 1;
	}
}
function splice2(self, at2, removed, inserted, $cp) {
	const held = size(self);
	let $cu = null;
	if (at2 > held) {
		$cu = held;
	} else {
		$cu = at2;
	}
	const start2 = $cu;
	let $cv = null;
	if (start2 + removed > held) {
		$cv = held - start2;
	} else {
		$cv = removed;
	}
	const taking = $cv;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start2, taking, arriving ];
	coming_one(self, change2);
	mutate(self, (held2) => {
		return splice(held2, start2, taking, inserted);
	});
	landed_one(self, change2, $cp);
}
function insert_at(self, at2, value, $co) {
	return splice2(self, at2, 0, [ __clone(value) ], $co);
}
function edit2(self, part2, change2, $cw) {
	const $cx = at(self);
	if ($cx[0] === 0) {

	} else {
		return false;
	}
	return edit(self[0], $cx[1], part2, change2, $cw);
}
function remove_at(self, at2, $cy) {
	return splice2(self, at2, 1, [  ], $cy);
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
const trackers_allocated_count = __shared_new(0);
const block_size = 128;
const list = sequence([ [ "a", "open" ], [ "b", "open" ], [ "c", "open" ] ], [ 1 ]);
const second = slot(list, 1, [ 1 ]);
const title = part(second, "title", (task) => {
	return task[0];
});
const status = part(second, "status", (task) => {
	return task[1];
});
const runs = __shared_new(0);
const label = memo(derive(($r, $s, $t) => {
	const $u = [ runs, "v" ];
	runs.v = $u[0][$u[1]] + 1;
	return "" + unwrap_or(track(title, $s), __lazy("fallback", () => {
		return "?";
	})) + " [" + unwrap_or(track(status, $s), __lazy("fallback", () => {
		return "?";
	})) + "]";
}, "lens-tracked.vl:24:14"), [ 1 ], [ 1 ], "lens-tracked.vl:27:5");
assert(get3(label) === "b [open]", "the tracked body read both parts", "lens-tracked.vl:28:2");
const seeded = runs.v;
edit(list, 0, "title", (task) => {
	return task[0] = "A";
}, [ 1 ]);
insert_at(list, 0, [ "new", "open" ], [ 1 ]);
assert(get3(label) === "b [open]" && runs.v === seeded, "another row\'s edit and an insert above do not re-run it", "lens-tracked.vl:33:2");
edit2(second, "status", (task) => {
	return task[1] = "done";
}, [ 1 ]);
assert(get3(label) === "b [done]" && runs.v === seeded + 1, "one part\'s edit re-runs it once", "lens-tracked.vl:39:2");
remove_at(list, 2, [ 1 ]);
assert(get3(label) === "? [?]" && runs.v === seeded + 2, "the row\'s deletion re-runs it once", "lens-tracked.vl:45:2");
console.log("tracked: all passed");
//# sourceMappingURL=lens-tracked.mjs.map
