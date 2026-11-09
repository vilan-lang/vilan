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
function __nmap_get(table, hash) {
	const entry = table[0].get(hash);
	return entry !== undefined && entry[1][0] === 0 ? [ 0, __clone(entry[1][1]) ] : [ 1 ];
}
function __nmap_insert(table, hash, key, value) {
	__nmap_settle(table);
	const entry = table[0].get(hash);
	if (entry === undefined) table[0].set(hash, [ key, [ 0, value ] ]);
	else {
		entry[0] = key;
		if (entry[1][0] !== 0 || entry[1][1] !== value) entry[1] = [ 0, value ];
	}
}
function __nmap_keys(table) {
	const out = [];
	for (const entry of table[0].values()) if (entry[1][0] === 0) out.push(entry[0]);
	return out.map(__clone);
}
function __nmap_remove(table, hash) {
	__nmap_settle(table);
	table[0].delete(hash);
}
function __nmap_settle(table) {
	const hash = table[1];
	if (hash === undefined) return;
	table[1] = undefined;
	const entry = table[0].get(hash);
	if (entry !== undefined && entry[1][0] === 1) table[0].delete(hash);
}
function __nmap_values(table) {
	const out = [];
	for (const entry of table[0].values()) if (entry[1][0] === 0) out.push(entry[1][1]);
	return out.map(__clone);
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
function eq(self, b2) {
	if (self.length !== b2.length) {
		return false;
	}
	let index2 = 0;
	for (const item2 of self) {
		if (!(item2 === __at(b2, index2, "std/src/compare.vl:147:17"))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function eq2(self, b2) {
	return self === b2;
}
function hash(self) {
	return __hash(self);
}
function hash2(self) {
	return __hash(self);
}
function new2() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function assert(condition, message, caller) {
	let $dI = null;
	if (!(condition)) {
		$dI = (() => {
			throw __panic(message, caller);
		})();
	}
	return $dI;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $o = null;
	if (wrapped < 0) {
		$o = wrapped + modulus;
	} else {
		$o = wrapped;
	}
	return $o;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $n = null;
	if (truncated > 0) {
		$n = truncated;
	} else {
		$n = 0;
	}
	return $n;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $p = null;
	if (wrapped >= half) {
		$p = wrapped - modulus;
	} else {
		$p = wrapped;
	}
	return $p;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function eq3(self, b2) {
	const $fT = self;
	let $fW = null;
	if ($fT[0] === 0) {
		const $fU = b2;
		let $fV = null;
		if ($fU[0] === 0) {
			$fV = $fT[1] === $fU[1];
		} else {
			$fV = false;
		}
		$fW = $fV;
	} else {
		const $fX = b2;
		$fW = $fX[0] === 1;
	}
	return $fW;
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
function new3() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return is_empty(self[0].v) && is_empty(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash2(subscriber[0]);
		let $bG = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$bG = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			if (tracing.v) {
				trace_hooks.v[3](turn, subscriber[0]);
			}
			let index2 = turn[0].v.length;
			while (index2 > 0 && __at(turn[0].v, index2 - 1, "std/src/reactive.vl:421:21")[0] > subscriber[0]) {
				index2 = index2 - 1;
			}
			__insert_at(turn[0].v, index2, __clone(subscriber), "std/src/reactive.vl:424:25");
		}
		$bG;
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
	const $bE = turn;
	let $bF = null;
	if ($bE[0] === 0) {
		const ambient = $bE[1];
		$bF = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $bH = last(draining_turns.v);
		let $bI = null;
		if ($bH[0] === 0) {
			const draining = $bH[1];
			$bI = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$bI = undefined;
		}
		$bF = $bI;
	}
	return $bF;
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
function retiring(subscriber, release6) {
	return detached(subscriber[0], subscriber[2], release6);
}
function also_retiring(handle, subscriber) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		subscriber[2].v = false;
		const $ci = previous;
		let $cj = null;
		if ($ci[0] === 0) {
			const release6 = $ci[1];
			$cj = release6();
		} else {
			$cj = undefined;
		}
		return $cj;
	} ];
}
function detached(id, live, release6) {
	let subscribers = [  ];
	const empty = __shared_new(subscribers);
	return [ empty, id, live, __shared_new([ 0, release6 ]) ];
}
function dispose(self, $cz) {
	const $cA = $cz;
	let $cB = null;
	if ($cA[0] === 0) {
		const established = $cA[1];
		$cB = [ 0, established ];
	} else {
		$cB = last(draining_turns.v);
	}
	const ambient = $cB;
	release_under(self, ambient);
}
function detach(handle) {
	const $bM = last(releasing_turns.v);
	let $bN = null;
	if ($bM[0] === 0) {
		const at_release = $bM[1];
		$bN = at_release;
	} else {
		$bN = last(draining_turns.v);
	}
	const turn = $bN;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $bO = [ 0, handle[0] ];
	let $bP = null;
	if ($bO[0] === 0) {
		const subscribers = $bO[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$bP = undefined;
	} else {
		$bP = undefined;
	}
	$bP;
	const $bQ = ambient;
	let $bR = null;
	if ($bQ[0] === 0) {
		const turn = $bQ[1];
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
		$bR = undefined;
	} else {
		$bR = undefined;
	}
	$bR;
	const $bS = handle[3].v;
	let $bT = null;
	if ($bS[0] === 0) {
		const release6 = $bS[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release6, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$bT = undefined;
	} else {
		$bT = undefined;
	}
	return $bT;
}
function new4() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $R = null;
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
		$R = undefined;
	}
	return $R;
}
function renew(self) {
	const $bd = self[0].v[3];
	let $be = null;
	if ($bd[0] === 0) {
		const nursery2 = __clone($bd[1]);
		let $bf = null;
		if (has_spawned(nursery2)) {
			$bf = [ 1 ];
		} else {
			$bf = [ 0, nursery2 ];
		}
		$be = $bf;
	} else {
		$be = [ 1 ];
	}
	const carried = $be;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $bq = null;
	if (is_disposed(self)) {
		$bq = [ 1 ];
	} else {
		$bq = self[0].v[3];
	}
	return $bq;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $bp = null;
	if (!(is_disposed(self))) {
		const held2 = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $bg = held2[3];
		let $bh = null;
		if ($bg[0] === 0) {
			const nursery2 = $bg[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$bh = undefined;
		} else {
			$bh = undefined;
		}
		$bh;
		let $bo = null;
		if (held2[2]) {
			let failure = [ 1 ];
			for (const cleanup of held2[1].v) {
				const $bj = __guarded(cleanup);
				let $bk = null;
				if ($bj[0] === 0) {
					const message = $bj[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$bk = undefined;
				} else {
					$bk = undefined;
				}
				$bk;
			}
			const $bm = failure;
			let $bn = null;
			if ($bm[0] === 0) {
				const message2 = $bm[1];
				$bn = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$bn = undefined;
			}
			$bo = $bn;
		}
		$bp = $bo;
	}
	return $bp;
}
function register_with_owner(subscription, $cu, $cv) {
	const $cw = $cv;
	let $cx = null;
	if ($cw[0] === 0) {
		const owner = $cw[1];
		$cx = take(owner, subscription, $cu);
	} else {
		$cx = __clone(subscription);
	}
	return $cx;
}
function defer_to_owner(cleanup, $O) {
	const $P = $O;
	let $Q = null;
	if ($P[0] === 0) {
		const owner = $P[1];
		$Q = defer(owner, cleanup);
	} else {
		$Q = undefined;
	}
	return $Q;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker(site) {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ], site ]) ];
}
function lists_of(tracker) {
	const $am = tracker[0].v[6];
	let $an = null;
	if ($am[0] === 0) {
		const lists = $am[1];
		$an = lists;
	} else {
		trackers_allocated_count.v = trackers_allocated_count.v + 1;
		const made = __shared_new([ [  ], [  ], [  ] ]);
		tracker[0].v[6] = [ 0, made ];
		$an = made;
	}
	return $an;
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $aY = tracker[0].v[6];
	let $aZ = null;
	if ($aY[0] === 0) {
		const lists = $aY[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$aZ = undefined;
	} else {
		$aZ = undefined;
	}
	$aZ;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $bv = tracker[0].v[6];
	let $bw = null;
	if ($bv[0] === 0) {
		const lists = $bv[1];
		$bw = lists;
	} else {
		return;
		$bw = undefined;
	}
	const lists2 = $bw;
	const $bx = tracker[0].v[5];
	let $by = null;
	if ($bx[0] === 0) {
		const target = __clone($bx[1]);
		$by = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$by = undefined;
	}
	$by;
	if (!(is_empty(lists2.v[0]))) {
		lists2.v[0] = [  ];
	}
}
function reconnect(tracker, lists, target) {
	if (is_empty(lists.v[0]) && is_empty(lists.v[2])) {
		return;
	}
	const held2 = __clone(lists.v[2]);
	const reading = __clone(lists.v[0]);
	let kept = [  ];
	for (const _edge of held2) {
		kept.push(false);
	}
	let next = [  ];
	tracker[0].v[3] = true;
	let position = 0;
	for (const dependency of reading) {
		const $bB = reusable(held2, kept, dependency[0], position);
		let $bC = null;
		if ($bB[0] === 0) {
			const index2 = $bB[1];
			__at_put(kept, index2, true, "std/src/reactive.vl:1715:5");
			next.push(__clone(__at(held2, index2, "std/src/reactive.vl:1716:15")));
			$bC = undefined;
		} else {
			next.push([ __clone(dependency[0]), dependency[1](relay_for(tracker, target)) ]);
			$bC = undefined;
		}
		$bC;
		position = position + 1;
	}
	lists.v[2] = next;
	let index3 = 0;
	for (const edge of held2) {
		if (!(__at(kept, index3, "std/src/reactive.vl:1730:7"))) {
			detach(edge[1]);
		}
		index3 = index3 + 1;
	}
	tracker[0].v[3] = false;
}
function reusable(held2, kept, identity5, position) {
	const $bz = identity5;
	let $bA = null;
	if ($bz[0] === 0) {
		const wanted = $bz[1];
		if (position < held2.length && !(__at(kept, position, "std/src/reactive.vl:1753:9")) && same_identity(__at(held2, position, "std/src/reactive.vl:1754:22")[0], wanted)) {
			return [ 0, position ];
		}
		let index2 = 0;
		while (index2 < held2.length) {
			if (!(__at(kept, index2, "std/src/reactive.vl:1759:9")) && same_identity(__at(held2, index2, "std/src/reactive.vl:1759:38")[0], wanted)) {
				return [ 0, index2 ];
			}
			index2 = index2 + 1;
		}
		$bA = [ 1 ];
	} else {
		$bA = [ 1 ];
	}
	return $bA;
}
function same_identity(identity5, wanted) {
	const $ak = identity5;
	let $al = null;
	if ($ak[0] === 0) {
		const held2 = $ak[1];
		$al = held2 === wanted;
	} else {
		$al = false;
	}
	return $al;
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
	const $bJ = tracker[0].v[7];
	let $bK = null;
	if ($bJ[0] === 0) {
		const site = __clone($bJ[1]);
		$bK = place_subscriber(relay[0], site);
	} else {
		$bK = undefined;
	}
	$bK;
	return relay;
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $bX = tracker[0].v[6];
	let $bY = null;
	if ($bX[0] === 0) {
		const lists = $bX[1];
		$bY = lists;
	} else {
		return;
		$bY = undefined;
	}
	const lists2 = $bY;
	let $bZ = null;
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
		$bZ = undefined;
	}
	return $bZ;
}
function forget_reads(tracker) {
	const $ca = tracker[0].v[6];
	let $cb = null;
	if ($ca[0] === 0) {
		const lists = $ca[1];
		$cb = lists;
	} else {
		return;
		$cb = undefined;
	}
	const lists2 = $cb;
	let $cc = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$cc = undefined;
	}
	$cc;
	if (!(is_empty(lists2.v[1]))) {
		lists2.v[1] = [  ];
	}
}
function detach_tracker(tracker) {
	tracker[0].v[5] = [ 1 ];
	forget_reads(tracker);
}
function admits(scope, identity5) {
	const tracker = __clone(scope[0]);
	if (!(tracker[0].v[1]) || tracker[0].v[0] !== scope[1]) {
		return false;
	}
	const $ag = tracker[0].v[6];
	let $ah = null;
	if ($ag[0] === 0) {
		const lists = $ag[1];
		$ah = lists;
	} else {
		return true;
		$ah = undefined;
	}
	const lists2 = $ah;
	const $ai = identity5;
	let $aj = null;
	if ($ai[0] === 0) {
		const wanted = $ai[1];
		const count2 = lists2.v[0].length;
		let index2 = 0;
		while (index2 < count2) {
			if (same_identity(__at(lists2.v[0], index2, "std/src/reactive.vl:1877:22")[0], wanted)) {
				return false;
			}
			index2 = index2 + 1;
		}
		$aj = true;
	} else {
		$aj = true;
	}
	return $aj;
}
function place_subscriber(id, site) {
	if (tracing.v) {
		trace_hooks.v[8](id, site);
	}
}
function place_subscription(subscription, site) {
	if (tracing.v) {
		trace_hooks.v[8](subscription[1], site);
	}
	return subscription;
}
function also_releasing(handle, release6) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release6();
		const $cs = previous;
		let $ct = null;
		if ($cs[0] === 0) {
			const earlier = $cs[1];
			$ct = earlier();
		} else {
			$ct = undefined;
		}
		return $ct;
	} ];
}
function placed_at(subscriber, site) {
	place_subscriber(subscriber[0], site);
	return subscriber;
}
function settle_together(body, $eR) {
	const $eS = $eR;
	let $eT = null;
	if ($eS[0] === 0) {
		const current = $eS[1];
		$eT = body(current);
	} else {
		const $eU = last(draining_turns.v);
		let $eV = null;
		if ($eU[0] === 0) {
			const draining = $eU[1];
			$eV = body(draining);
		} else {
			$eV = batch(body, $eR);
		}
		$eT = $eV;
	}
	return $eT;
}
function new5() {
	return [ [  ] ];
}
function follow(self, at2) {
	if (self[0].length === 0) {
		self[0].push(new6());
	}
	return follow2(__at(self[0], 0, "std/src/reactive/lens.vl:145:3"), at2);
}
function apply(self, change2, wakes) {
	if (self[0].length > 0) {
		apply2(__at(self[0], 0, "std/src/reactive/lens.vl:150:4"), change2, wakes);
	}
}
function locate(self, slot3) {
	if (self[0].length === 0) {
		return [ 1 ];
	}
	return locate2(__at(self[0], 0, "std/src/reactive/lens.vl:158:3"), slot3);
}
function follows_items(self) {
	return true;
}
function release(self, slot3) {
	if (self[0].length > 0) {
		release2(__at(self[0], 0, "std/src/reactive/lens.vl:167:4"), slot3);
	}
}
function new6() {
	return [ [  ], [  ], [  ], [  ], [  ], [  ], 0 ];
}
function seat(self, at2) {
	let low = 0;
	let high = self[3].length;
	while (high - low > 1) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[3], middle, "std/src/reactive/lens.vl:213:7") <= at2) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at2 - __at(self[3], low, "std/src/reactive/lens.vl:219:14") ];
}
function restart(self) {
	let starts = [  ];
	let running = 0;
	let place = 0;
	for (const block of self[1]) {
		starts.push(running);
		__at_put(self[2], as_usize(block), place, "std/src/reactive/lens.vl:229:4");
		running = running + __at(self[0], as_usize(block), "std/src/reactive/lens.vl:230:15").length;
		place = place + 1;
	}
	self[3] = starts;
}
function renumber(self, block, from) {
	let index2 = from;
	const held2 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:239:14").length;
	while (index2 < held2) {
		const slot3 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:241:15"), index2, "std/src/reactive/lens.vl:241:15");
		if (slot3 !== -(1)) {
			__at_put(self[4], as_usize(slot3), block, "std/src/reactive/lens.vl:243:5");
			__at_put(self[5], as_usize(slot3), index2, "std/src/reactive/lens.vl:244:5");
		}
		index2 = index2 + 1;
	}
}
function open(self, place) {
	const block = as_i32(self[0].length);
	self[0].push([  ]);
	self[2].push(0);
	__insert_at(self[1], place, block, "std/src/reactive/lens.vl:255:14");
	return block;
}
function widen(self, at2, count2) {
	if (count2 === 0 || at2 >= self[6]) {
		return;
	}
	const $es = seat(self, at2);
	const place = $es[0];
	const index2 = $es[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:265:15");
	let $et = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:269:5"), index2, -(1), "std/src/reactive/lens.vl:269:35");
			made = made + 1;
		}
		$et = undefined;
	} else {
		const before = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:273:17"));
		let grown = [  ];
		let copied = 0;
		while (copied < index2) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:277:16"));
			copied = copied + 1;
		}
		let made2 = 0;
		while (made2 < count2) {
			grown.push(-(1));
			made2 = made2 + 1;
		}
		const held2 = before.length;
		while (copied < held2) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:287:16"));
			copied = copied + 1;
		}
		__at_put(self[0], as_usize(block), grown, "std/src/reactive/lens.vl:290:4");
	}
	$et;
	self[6] = self[6] + count2;
	renumber(self, block, index2 + count2);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:294:6").length > 2 * block_size) {
		rebalance(self, place);
	}
	restart(self);
}
function close(self, at2, count2, wakes) {
	let left = count2;
	while (left > 0 && at2 < self[6]) {
		const $eq = seat(self, at2);
		const place = $eq[0];
		const index2 = $eq[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:306:16");
		const held2 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:307:15").length;
		let $er = null;
		if (left < held2 - index2) {
			$er = left;
		} else {
			$er = held2 - index2;
		}
		const take2 = $er;
		let taken = 0;
		while (taken < take2) {
			const slot3 = __remove_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:311:16"), index2, "std/src/reactive/lens.vl:311:46");
			if (slot3 !== -(1)) {
				__at_put(self[4], as_usize(slot3), -(1), "std/src/reactive/lens.vl:313:6");
				wakes.push([ 2, slot3 ]);
			}
			taken = taken + 1;
		}
		self[6] = self[6] - take2;
		left = left - take2;
		if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:320:7").length === 0) {
			__remove_at(self[1], place, "std/src/reactive/lens.vl:321:16");
		} else {
			renumber(self, block, index2);
		}
		restart(self);
	}
}
function rebalance(self, place) {
	const block = __at(self[1], place, "std/src/reactive/lens.vl:331:15");
	const all3 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:332:13"));
	let first = [  ];
	let index2 = 0;
	while (index2 < block_size) {
		first.push(__at(all3, index2, "std/src/reactive/lens.vl:336:15"));
		index2 = index2 + 1;
	}
	__at_put(self[0], as_usize(block), first, "std/src/reactive/lens.vl:339:3");
	let next = place + 1;
	const held2 = all3.length;
	while (index2 < held2) {
		const fresh = open(self, next);
		let filled = 0;
		while (filled < block_size && index2 < held2) {
			__at(self[0], as_usize(fresh), "std/src/reactive/lens.vl:346:5").push(__at(all3, index2, "std/src/reactive/lens.vl:346:40"));
			index2 = index2 + 1;
			filled = filled + 1;
		}
		renumber(self, fresh, 0);
		next = next + 1;
	}
}
function follow2(self, at2) {
	while (self[6] <= at2) {
		const places = self[1].length;
		if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:361:34")), "std/src/reactive/lens.vl:361:22").length >= block_size) {
			const fresh = open(self, places);
			__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:363:5");
			self[3].push(self[6]);
		}
		const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:366:15");
		__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:367:4").push(-(1));
		self[6] = self[6] + 1;
	}
	const $q = seat(self, at2);
	const place = $q[0];
	const index2 = $q[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:371:15");
	const held2 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:372:14"), index2, "std/src/reactive/lens.vl:372:14");
	if (held2 !== -(1)) {
		return held2;
	}
	const slot3 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index2);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:379:3"), index2, slot3, "std/src/reactive/lens.vl:379:3");
	return slot3;
}
function apply2(self, change2, wakes) {
	const $eo = change2;
	let $ep = null;
	if ($eo[0] === 0) {
		const at2 = $eo[1];
		const removed3 = $eo[2];
		const inserted = $eo[3];
		close(self, at2, removed3, wakes);
		widen(self, at2, inserted);
		$ep = undefined;
	} else if ($eo[0] === 1) {
		const at3 = $eo[1];
		const count2 = $eo[2];
		$ep = widen(self, at3, count2);
	} else if ($eo[0] === 3) {
		const at4 = $eo[1];
		const count3 = $eo[2];
		$ep = close(self, at4, count3, wakes);
	} else if ($eo[0] === 2) {
		const at5 = $eo[1];
		let $ev = null;
		if (at5 < self[6]) {
			const $eu = seat(self, at5);
			const place = $eu[0];
			const index2 = $eu[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:396:29")), "std/src/reactive/lens.vl:396:17"), index2, "std/src/reactive/lens.vl:396:17");
			if (slot3 !== -(1)) {
				wakes.push([ 0, slot3 ]);
			}
			$ev = undefined;
		}
		$ep = $ev;
	} else if ($eo[0] === 4) {
		const at6 = $eo[1];
		const part = $eo[2];
		let $ex = null;
		if (at6 < self[6]) {
			const $ew = seat(self, at6);
			const place2 = $ew[0];
			const index3 = $ew[1];
			const slot4 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:405:29")), "std/src/reactive/lens.vl:405:17"), index3, "std/src/reactive/lens.vl:405:17");
			if (slot4 !== -(1)) {
				wakes.push([ 1, slot4, part ]);
			}
			$ex = undefined;
		}
		$ep = $ex;
	} else if ($eo[0] === 5) {
		const from = $eo[1];
		const to = $eo[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $ey = null;
		if (from > to) {
			$ey = from;
		} else {
			$ey = to;
		}
		const far = $ey;
		while (self[6] <= far) {
			const places = self[1].length;
			if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:419:36")), "std/src/reactive/lens.vl:419:24").length >= block_size) {
				const fresh = open(self, places);
				__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:421:7");
				self[3].push(self[6]);
			}
			const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:424:17");
			__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:425:6").push(-(1));
			self[6] = self[6] + 1;
		}
		const $ez = seat(self, from);
		const left = $ez[0];
		const index4 = $ez[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:429:18");
		const slot5 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:430:16"), index4, "std/src/reactive/lens.vl:430:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:432:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:433:17");
		} else {
			renumber(self, source, index4);
		}
		restart(self);
		let $eA = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$eA = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:441:25")), "std/src/reactive/lens.vl:441:13").length ];
		} else {
			$eA = seat(self, to);
		}
		const $eB = $eA;
		const place3 = $eB[0];
		const landing2 = $eB[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:445:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:446:5"), landing2, slot5, "std/src/reactive/lens.vl:446:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing2);
		restart(self);
		$ep = undefined;
	} else {
		let slot6 = 0;
		const slots = as_i32(self[4].length);
		while (slot6 < slots) {
			if (__at(self[4], as_usize(slot6), "std/src/reactive/lens.vl:455:9") !== -(1)) {
				__at_put(self[4], as_usize(slot6), -(1), "std/src/reactive/lens.vl:456:7");
				wakes.push([ 2, slot6 ]);
			}
			slot6 = slot6 + 1;
		}
		self[0] = [  ];
		self[1] = [  ];
		self[2] = [  ];
		self[3] = [  ];
		self[6] = 0;
		$ep = undefined;
	}
	return $ep;
}
function release2(self, slot3) {
	const block = __at(self[4], as_usize(slot3), "std/src/reactive/lens.vl:471:15");
	if (block === -(1)) {
		return;
	}
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:475:3"), __at(self[5], as_usize(slot3), "std/src/reactive/lens.vl:475:33"), -(1), "std/src/reactive/lens.vl:475:3");
	__at_put(self[4], as_usize(slot3), -(1), "std/src/reactive/lens.vl:476:3");
}
function locate2(self, slot3) {
	const block = __at(self[4], as_usize(slot3), "std/src/reactive/lens.vl:480:15");
	if (block === -(1)) {
		return [ 1 ];
	}
	return [ 0, __at(self[3], __at(self[2], as_usize(block), "std/src/reactive/lens.vl:484:20"), "std/src/reactive/lens.vl:484:8") + __at(self[5], as_usize(slot3), "std/src/reactive/lens.vl:484:51") ];
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function card(id, title) {
	return [ id, title ];
}
function ids(cards) {
	let out = [  ];
	for (const item2 of cards) {
		out.push(item2[0]);
	}
	return out;
}
function row_keys(rows5) {
	let out = [  ];
	for (const row2 of rows5) {
		const $fv = out;
		const $ft = row2[1];
		let $fu = null;
		if ($ft[0] === 0) {
			$fu = row2[0];
		} else {
			$fu = "(" + row2[0] + ")";
		}
		$fv.push($fu);
	}
	return out;
}
function eq4(self, other) {
	return self[0] === other[0] && self[1] === other[1];
}
function count(self) {
	return self.length;
}
function item(self, at2) {
	return __list_get(self, at2);
}
function lend(self, at2, read) {
	if (at2 < self.length) {
		read(__at(self, at2, "std/src/reactive/lens.vl:2222:10"));
	}
}
function put(self, at2, value) {
	__at_put(self, at2, __clone(value), "std/src/reactive/lens.vl:2242:3");
}
function change(self, at2, with2) {
	if (at2 < self.length) {
		with2(__at(self, at2, "std/src/reactive/lens.vl:2247:14"));
	}
}
function new7(value, caller) {
	let subscribers = [  ];
	const cell3 = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell3[0]), caller);
	}
	return cell3;
}
function lens_over(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new7(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over(value, handler, access) {
	return lens_over([ 0, __shared_new(value) ], handler, access);
}
function sequence(value, $b) {
	return over(value, new5(), [ (held2, at2) => {
		return at2 < count(held2);
	}, (held2, at2) => {
		return item(held2, at2);
	}, (held2, at2, read) => {
		return lend(held2, at2, read);
	}, (held2, at2, value2) => {
		return put(held2, at2, value2, $b);
	}, (held2, at2, with2) => {
		return change(held2, at2, with2, $b);
	} ]);
}
function over2(value, $a) {
	return [ sequence(value, $a) ];
}
function new8() {
	return [ __shared_new(new2()), __shared_new([  ]), __shared_new(new2()), __shared_new(new2()), new7(0, "std/src/reactive/lens/loaded.vl:130:24"), new7(0, "std/src/reactive/lens/loaded.vl:131:25"), __shared_new(0) ];
}
function lend2(value, read) {
	return read(value);
}
function viewing(cell3, read) {
	return lend2(cell3.v, read);
}
function peek_storage(storage, read) {
	const $f = storage;
	let $g = null;
	if ($f[0] === 0) {
		const cell3 = $f[1];
		$g = viewing(cell3, read);
	} else {
		const port = $f[1];
		let out = [ 1 ];
		port[0]((held2) => {
			return out = [ 0, read(held2) ];
		});
		const $h = out;
		let $i = null;
		if ($h[0] === 0) {
			const value = __clone($h[1]);
			$i = value;
		} else {
			$i = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$g = $i;
	}
	return $g;
}
function viewing2(cell3, read) {
	return lend2(cell3.v, read);
}
function peek_storage2(storage, read) {
	const $j = storage;
	let $k = null;
	if ($j[0] === 0) {
		const cell3 = $j[1];
		$k = viewing2(cell3, read);
	} else {
		const port = $j[1];
		let out = [ 1 ];
		port[0]((held2) => {
			return out = [ 0, read(held2) ];
		});
		const $l = out;
		let $m = null;
		if ($l[0] === 0) {
			const value = __clone($l[1]);
			$m = value;
		} else {
			$m = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$k = $m;
	}
	return $k;
}
function follow_id(storage, watch, holds, at2) {
	if (viewing2(watch, (held2) => {
		return follows_items(held2[0]);
	}) && !(peek_storage2(storage, (held2) => {
		return holds(held2, at2);
	}))) {
		return -(1);
	}
	const id = follow(watch.v[0], at2);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] + 1;
	return id;
}
function get(self, key) {
	return __nmap_get(self[0], hash(key));
}
function insert(self, key, value) {
	__nmap_insert(self[0], hash(key), __clone(key), __clone(value));
}
function release_id(watch, id) {
	if (id < 0) {
		return;
	}
	const entry = as_usize(id);
	const left = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1071:13")[2] - 1;
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1072:2")[2] = left;
	if (left <= 0) {
		release(watch.v[0], id);
	}
}
function remove(self, key) {
	__nmap_remove(self[0], hash(key));
}
function is_empty(self) {
	return self.length === 0;
}
function entries(self) {
	let keys2 = __nmap_keys(self[0]);
	let values = __nmap_values(self[0]);
	let backwards = [  ];
	while (!(is_empty(keys2))) {
		const $t = __list_pop(keys2);
		let $u = null;
		if ($t[0] === 0) {
			const key = $t[1];
			const $v = __list_pop(values);
			let $w = null;
			if ($v[0] === 0) {
				const value = $v[1];
				$w = backwards.push([ key, value ]);
			} else {
				$w = undefined;
			}
			$u = $w;
		} else {
			$u = undefined;
		}
		$u;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $x = __list_pop(backwards);
		let $y = null;
		if ($x[0] === 0) {
			const pair = $x[1];
			$y = result.push(pair);
		} else {
			$y = undefined;
		}
		$y;
	}
	return result;
}
function new9() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function tap(self, before, after) {
	const id = self[0].v[5];
	self[0].v[5] = id + 1;
	self[0].v[4].push([ id, before, after ]);
	return id;
}
function untap(self, id) {
	let kept = [  ];
	const taps = __clone(self[0].v[4]);
	for (const tap2 of taps) {
		if (tap2[0] !== id) {
			kept.push(__clone(tap2));
		}
	}
	self[0].v[4] = kept;
}
function release3(self) {
	untap(self[0], self[2]);
	for (const _ of entries(self[1].v)) {
		const $N = _;
		const _key = $N[0];
		const id = $N[1];
		release_id(self[0][0].v[3], id);
	}
	self[1].v = new9();
}
function index_by(self, key, $e) {
	const found = __shared_new(new2());
	const leaving = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const view2 = self[0].v[1][2];
	const key_at = (at2) => {
		let out = [ 1 ];
		peek_storage(storage, (held2) => {
			return view2(held2, at2, (item2) => {
				return out = [ 0, key(item2) ];
			});
		});
		return out;
	};
	const map2 = (held2, at2) => {
		const id = follow_id(storage, watch, holds, at2);
		const before = get(found.v, held2);
		insert(found.v, held2, id);
		const $r = before;
		if ($r[0] === 0) {
			release_id(watch, $r[1]);
		}
		return;
	};
	const unmap = (held2) => {
		const $s = get(found.v, held2);
		if ($s[0] === 0) {
			remove(found.v, held2);
			release_id(watch, $s[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $z = _;
			const _held = $z[0];
			const old = $z[1];
			release_id(watch, old);
		}
		found.v = new9();
		let at2 = 0;
		const count2 = peek_storage2(storage, (held2) => {
			return count(held2);
		});
		while (at2 < count2) {
			const $E = key_at(at2);
			if ($E[0] === 0) {
				map2($E[1], at2);
			}
			at2 = at2 + 1;
		}
		return;
	};
	fill();
	const note = (at2, count2) => {
		let gone2 = [  ];
		let index3 = at2;
		while (index3 < at2 + count2) {
			const $F = key_at(index3);
			if ($F[0] === 0) {
				gone2.push($F[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone2;
		return;
	};
	const arrive = (at2, count2) => {
		let index3 = at2;
		while (index3 < at2 + count2) {
			const $G = key_at(index3);
			if ($G[0] === 0) {
				map2($G[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change2) => {
		const $H = change2;
		let $I = null;
		if ($H[0] === 0) {
			const at2 = $H[1];
			const removed3 = $H[2];
			const _inserted = $H[3];
			$I = note(at2, removed3);
		} else if ($H[0] === 3) {
			const at3 = $H[1];
			const count2 = $H[2];
			$I = note(at3, count2);
		} else if ($H[0] === 2) {
			const at4 = $H[1];
			$I = note(at4, 1);
		} else if ($H[0] === 4) {
			const at5 = $H[1];
			const _part = $H[2];
			$I = note(at5, 1);
		} else {
			$I = undefined;
		}
		return $I;
	}, (change2, stage) => {
		const $J = stage;
		let $K = null;
		if ($J[0] === 0) {
			$K = undefined;
		} else if ($J[0] === 3) {
			return;
		} else {
			fill();
			return;
			$K = undefined;
		}
		$K;
		const $L = change2;
		let $M = null;
		if ($L[0] === 0) {
			const at2 = $L[1];
			const _removed = $L[2];
			const inserted = $L[3];
			for (const held2 of leaving.v) {
				unmap(held2);
			}
			arrive(at2, inserted);
			$M = undefined;
		} else if ($L[0] === 1) {
			const at3 = $L[1];
			const count2 = $L[2];
			$M = arrive(at3, count2);
		} else if ($L[0] === 3) {
			const _at = $L[1];
			const _count = $L[2];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			$M = undefined;
		} else if ($L[0] === 2) {
			const at4 = $L[1];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			arrive(at4, 1);
			$M = undefined;
		} else if ($L[0] === 4) {
			const at5 = $L[1];
			const _part = $L[2];
			for (const held5 of leaving.v) {
				unmap(held5);
			}
			arrive(at5, 1);
			$M = undefined;
		} else if ($L[0] === 5) {
			const _from = $L[1];
			const _to = $L[2];
			$M = undefined;
		} else {
			$M = fill();
		}
		return $M;
	});
	const index2 = [ __clone(self), found, tap2 ];
	defer_to_owner(() => {
		return release3(index2);
	}, $e);
	return index2;
}
function index_by2(self, key, $d) {
	return index_by(self[0], key, $d);
}
function new10() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function trim(self) {
	let lowest = self[1].v;
	for (const cursor of self[3].v) {
		const at2 = cursor[1].v;
		if (at2 < lowest) {
			lowest = at2;
		}
	}
	const base = self[2].v;
	if (lowest > base) {
		const dropped = lowest - base;
		let kept = [  ];
		let index2 = as_usize(dropped);
		const held2 = __clone(self[0].v);
		const length = held2.length;
		while (index2 < length) {
			const $W = __list_get(held2, index2);
			let $X = null;
			if ($W[0] === 0) {
				const op = $W[1];
				$X = kept.push(op);
			} else {
				$X = undefined;
			}
			$X;
			index2 = index2 + 1;
		}
		self[0].v = kept;
		self[2].v = lowest;
	}
}
function record(self, op) {
	trim(self);
	const next = self[1].v + 1;
	self[1].v = next;
	if (self[0].v.length >= self[4]) {
		self[0].v = [  ];
		self[2].v = next;
	} else {
		self[0].v.push(__clone(op));
	}
}
function release4(self) {
	untap(self[0], self[3]);
	for (const id of self[1].v) {
		release_id(self[0][0].v[3], id);
	}
	self[1].v = [  ];
}
function rows(self, $T) {
	const log = new10();
	const held2 = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const fill = () => {
		let ids2 = [  ];
		let at2 = 0;
		const count2 = peek_storage2(storage, (list) => {
			return count(list);
		});
		while (at2 < count2) {
			ids2.push(follow_id(storage, watch, holds, at2));
			at2 = at2 + 1;
		}
		return ids2;
	};
	const leave = (at2, count2) => {
		let gone2 = [  ];
		let taken = 0;
		while (taken < count2) {
			const id = __remove_at(held2.v, at2, "std/src/reactive/lens.vl:1839:27");
			release_id(watch, id);
			gone2.push(id);
			taken = taken + 1;
		}
		return gone2;
	};
	const arrive = (at2, count2) => {
		let arrived = [  ];
		let index2 = at2;
		while (index2 < at2 + count2) {
			const id = follow_id(storage, watch, holds, index2);
			__insert_at(held2.v, index2, id, "std/src/reactive/lens.vl:1851:18");
			arrived.push(id);
			index2 = index2 + 1;
		}
		return arrived;
	};
	held2.v = fill();
	const tap2 = tap(self, (_change) => {
		return;
	}, (change2, _stage) => {
		const $U = change2;
		let $V = null;
		if ($U[0] === 0) {
			const at2 = $U[1];
			const removed3 = $U[2];
			const inserted = $U[3];
			const gone2 = leave(at2, removed3);
			record(log, [ 0, at2, gone2, arrive(at2, inserted) ]);
			$V = undefined;
		} else if ($U[0] === 1) {
			const at3 = $U[1];
			const count2 = $U[2];
			$V = record(log, [ 0, at3, [  ], arrive(at3, count2) ]);
		} else if ($U[0] === 3) {
			const at4 = $U[1];
			const count3 = $U[2];
			$V = record(log, [ 0, at4, leave(at4, count3), [  ] ]);
		} else if ($U[0] === 5) {
			const from = $U[1];
			const to = $U[2];
			const id = __remove_at(held2.v, from, "std/src/reactive/lens.vl:1867:28");
			__insert_at(held2.v, to, id, "std/src/reactive/lens.vl:1868:19");
			record(log, [ 3, from, 1, to ]);
			$V = undefined;
		} else if ($U[0] === 6) {
			for (const id2 of held2.v) {
				release_id(watch, id2);
			}
			held2.v = fill();
			record(log, [ 2, __clone(held2.v) ]);
			$V = undefined;
		} else {
			$V = undefined;
		}
		return $V;
	});
	const feed = [ __clone(self), held2, __clone(log), tap2 ];
	defer_to_owner(() => {
		return release4(feed);
	}, $T);
	return feed;
}
function rows2(self, $S) {
	return rows(self[0], $S);
}
function lens(self) {
	return __clone(self[0]);
}
function slot_of(self, id) {
	const $Z = __clone(self);
	let $Y = null;
	if (id < 0) {
		$Y = -(1);
	} else {
		$Y = id;
	}
	return [ $Z, $Y ];
}
function index(self) {
	return [ __clone(self) ];
}
function beside(lens2, key, $c) {
	const ledger = new8();
	return [ __clone(lens2), key, ledger, index_by2(lens2, key, $c), rows2(lens2, $c), index(slot_of(lens(lens2), -(1))) ];
}
function identity(self) {
	return [ 1 ];
}
function shape_cell(self) {
	const watch = self[0].v[3];
	const $ao = watch.v[2];
	const cell3 = __clone($ao[1]);
	if ($ao[0] === 0) {
		return cell3;
	}
	const cell4 = new7(0, "std/src/reactive/lens.vl:1031:26");
	watch.v[2] = [ 0, __clone(cell4) ];
	return cell4;
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
	return on_settle(shape_cell(self[0]), subscriber);
}
function handles(self, ids2) {
	let out = [  ];
	for (const id of ids2) {
		out.push(slot_of(self[0], id));
	}
	return out;
}
function get2(self) {
	return handles(self, self[1].v);
}
function track(self, $af) {
	const scope = $af;
	const identity5 = identity(self);
	if (admits(scope, identity5)) {
		lists_of(scope[0]).v[0].push([ identity5, (subscriber) => {
			return on_settle2(self, subscriber);
		} ]);
	}
	return get2(self);
}
function identity2(self) {
	return [ 0, __shared_identity(self[0]) ];
}
function get3(self) {
	return __clone(self[0].v);
}
function track2(self, $af) {
	const scope = $af;
	const identity5 = identity2(self);
	if (admits(scope, identity5)) {
		lists_of(scope[0]).v[0].push([ identity5, (subscriber) => {
			return on_settle(self, subscriber);
		} ]);
	}
	return get3(self);
}
function at(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][0].v[3], (held2) => {
		return locate(held2[0], self[1]);
	});
}
function peek(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function get4(self) {
	const $ap = at(self);
	if ($ap[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held2) => {
		return read(held2, $ap[1]);
	});
}
function keys(self) {
	return __clone(self[1].v);
}
function provisional(self, key) {
	const $av = get(self[0].v, key);
	if ($av[0] === 0) {

	} else {
		return false;
	}
	let out = false;
	for (const proposal of $av[1][0]) {
		if (proposal[2]) {
			out = true;
		}
	}
	return out;
}
function map(self, fn) {
	const $aw = self;
	let $ax = null;
	if ($aw[0] === 0) {
		const x2 = $aw[1];
		$ax = [ 0, fn(x2) ];
	} else {
		$ax = [ 1 ];
	}
	return $ax;
}
function find(self, key) {
	const lens2 = __clone(self[0]);
	return map(viewing2(self[1], (held2) => {
		return get(held2, key);
	}), (id) => {
		return slot_of(lens2, id);
	});
}
function showing(self, key) {
	const $az = get(self[0].v, key);
	if ($az[0] === 0) {

	} else {
		return [ 1 ];
	}
	let out = [ 1 ];
	for (const proposal of $az[1][0]) {
		if (proposal[0] > $az[1][1]) {
			out = [ 0, __clone(proposal) ];
		}
	}
	return out;
}
function over3(self, key, held2) {
	const $aA = showing(self, key);
	let $aB = null;
	if ($aA[0] === 1) {
		$aB = __clone(held2);
	} else {
		const proposal = $aA[1];
		const $aC = proposal[1];
		let $aD = null;
		if ($aC[0] === 1) {
			$aD = [ 1 ];
		} else {
			const value = $aC[1];
			const $aE = held2;
			let $aF = null;
			if ($aE[0] === 0 || provisional(self, key)) {
				$aF = [ 0, __clone(value) ];
			} else {
				$aF = [ 1 ];
			}
			$aD = $aF;
		}
		$aB = $aD;
	}
	return $aB;
}
function anchors(self, key) {
	const $aH = get(self[0].v, key);
	let $aI = null;
	if ($aH[0] === 0) {
		const entry = $aH[1];
		$aI = [ __clone(entry[2]), __clone(entry[3]) ];
	} else {
		$aI = [ [ 1 ], [ 1 ] ];
	}
	return $aI;
}
function position_of(rows5, key) {
	let at2 = 0;
	for (const row2 of rows5) {
		if (row2[0] === key) {
			return [ 0, at2 ];
		}
		at2 = at2 + 1;
	}
	return [ 1 ];
}
function landing_among(rows5, above, below) {
	const $aK = above;
	let $aL = undefined;
	if ($aK[0] === 0 && (($aL = position_of(rows5, $aK[1])), $aL[0] === 0)) {
		return $aL[1] + 1;
	}
	const $aM = below;
	let $aN = undefined;
	if ($aM[0] === 0 && (($aN = position_of(rows5, $aM[1])), $aN[0] === 0)) {
		return $aN[1];
	}
	const $aO = above;
	if ($aO[0] === 0) {
		return rows5.length;
	}
	return 0;
}
function hides(self, key) {
	const $aP = showing(self, key);
	if ($aP[0] === 0) {
		const $aQ = $aP[1][1];
		return $aQ[0] === 1;
	}
	return false;
}
function compose(self, with_hidden) {
	const key = self[1];
	let all3 = [  ];
	for (const slot3 of get2(self[4])) {
		const $au = get4(slot3);
		if ($au[0] === 0) {
			all3.push([ key($au[1]), [ 0, __clone(slot3) ] ]);
		}
	}
	for (const row_key of keys(self[2])) {
		let $ay = undefined;
		let $aG = undefined;
		if (provisional(self[2], row_key) && (($ay = find(self[3], row_key)), $ay[0] === 1) && (($aG = over3(self[2], row_key, [ 1 ])), $aG[0] === 0)) {
			const $aJ = anchors(self[2], row_key);
			const above = $aJ[0];
			const below = $aJ[1];
			const at2 = landing_among(all3, above, below);
			__insert_at(all3, at2, [ __clone(row_key), [ 1 ] ], "std/src/reactive/lens/loaded.vl:588:9");
		}
	}
	if (with_hidden) {
		return all3;
	}
	let out = [  ];
	for (const row2 of all3) {
		if (!(hides(self[2], row2[0]))) {
			out.push(__clone(row2));
		}
	}
	return out;
}
function derive(body, caller) {
	return [ body, caller ];
}
function distinct(self, caller) {
	return [ self, caller ];
}
function is_none(self) {
	const $bi = self;
	return $bi[0] === 1;
}
function run_body(runs, body) {
	const run = renew(runs);
	const $br = nursery(run);
	let $bs = null;
	if ($br[0] === 0) {
		const nursery2 = $br[1];
		$bs = (($bt) => {
			return (($bu) => {
				return body($bt, $bu);
			})(nursery2);
		})(run);
	} else {
		$bs = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $bs;
}
function last(self) {
	let $bD = null;
	if (is_empty(self)) {
		$bD = [ 1 ];
	} else {
		$bD = __list_get(self, self.length - 1);
	}
	return $bD;
}
function run_once(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body(runs, ($ba, $bb) => {
		return (($bc) => {
			return body($ba, $bc, $bb);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs, tracker, body) {
	let value = run_once(runs, tracker, ($aV, $aW, $aX) => {
		return body($aV, $aW, $aX);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs, tracker, ($bU, $bV, $bW) => {
			return body($bU, $bV, $bW);
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
	const runs = new4();
	const tracker = new_tracker([ 0, __clone(self[1]) ]);
	return [ () => {
		return run_tracked(runs, tracker, body);
	}, (subscriber) => {
		attach_tracker(tracker, subscriber);
		return retiring(subscriber, () => {
			return detach_tracker(tracker);
		});
	}, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	} ];
}
function eq5(self, other) {
	return self[1] === other[1];
}
function eq6(self, b2) {
	const $cd = self;
	let $cg = null;
	if ($cd[0] === 0) {
		const $ce = b2;
		let $cf = null;
		if ($ce[0] === 0) {
			$cf = eq5($cd[1], $ce[1]);
		} else {
			$cf = false;
		}
		$cg = $cf;
	} else {
		const $ch = b2;
		$cg = $ch[0] === 1;
	}
	return $cg;
}
function eq7(self, other) {
	return self[0] === other[0] && eq6(self[1], other[1]);
}
function eq8(self, b2) {
	if (self.length !== b2.length) {
		return false;
	}
	let index2 = 0;
	for (const item2 of self) {
		if (!(eq7(item2, __at(b2, index2, "std/src/compare.vl:147:17")))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function start2(self) {
	const site = __clone(self[1]);
	const upstream = start(self[0]);
	const pull = upstream[0];
	const last2 = __shared_new(pull());
	return [ () => {
		return __clone(last2.v);
	}, (subscriber) => {
		const relayed = upstream[1](placed_at(subscriber_of(() => {
			const now = pull();
			if (!(eq8(now, last2.v))) {
				last2.v = now;
				wake(subscriber);
			}
			return;
		}, true), site));
		also_retiring(relayed, subscriber);
		return relayed;
	}, upstream[2] ];
}
function notify(self, $cl) {
	const traced = tracing.v;
	let $cm = null;
	if (traced) {
		$cm = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$cm = "";
	}
	const cause = $cm;
	const $cn = $cl;
	let $co = null;
	if ($cn[0] === 0) {
		const turn = $cn[1];
		$co = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $cp = last(draining_turns.v);
		let $cq = null;
		if ($cp[0] === 0) {
			const draining = $cp[1];
			$cq = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $cr = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$cr = undefined;
				}
				$cr;
			}
			$cq = undefined;
		}
		$co = $cq;
	}
	return $co;
}
function set(self, value, $ck, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $ck);
}
function take(self, item2, $cy) {
	defer(self, () => {
		dispose(item2, $cy);
		return;
	});
	return __clone(item2);
}
function cell(self, $aT, $aU, caller) {
	const instance = start2(self);
	const pull = instance[0];
	const cached = new7(pull(), caller);
	const refresh = subscriber_of(() => {
		set(cached, pull(), $aT, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed = instance[1](refresh);
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $aT, $aU);
	return cached;
}
function memo(self, $aR, $aS, caller) {
	return [ cell(self, $aR, $aS, caller) ];
}
function rows3(self, $aa, $ab) {
	const overlay2 = __clone(self);
	const feed = __clone(self[4]);
	const layout = __clone(self[2][5]);
	return memo(distinct(derive(($ac, $ad, $ae) => {
		track(feed, $ad);
		track2(layout, $ad);
		return compose(overlay2, false);
	}, "std/src/reactive/lens/loaded.vl:544:3"), "std/src/reactive/lens/loaded.vl:548:6"), $aa, $ab, "std/src/reactive/lens/loaded.vl:548:17");
}
function observe(signal, observer) {
	const cell3 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $cE = [ 0, cell3 ];
		let $cF = null;
		if ($cE[0] === 0) {
			const live = $cE[1];
			$cF = observer(live.v);
		} else {
			$cF = undefined;
		}
		return $cF;
	}));
}
function attach_observer(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get3(self));
	}
	return subscription;
}
function attach_observer2(self, observer, immediately) {
	return attach_observer(self[0], observer, immediately);
}
function on_change(self, observer, caller) {
	return place_subscription(attach_observer2(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function follow3(self, at2) {
	const core = self[0];
	return slot_of(self, follow_id(core.v[0], core.v[3], core.v[1][0], at2));
}
function release5(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot(self, at2, $cH) {
	const handle = follow3(self, at2);
	defer_to_owner(() => {
		return release5(handle);
	}, $cH);
	return handle;
}
function slot2(self, at2, $cG) {
	return slot(self[0], at2, $cG);
}
function slot_cell(watch, slot3) {
	if (slot3 < 0) {
		return new7(0, "std/src/reactive/lens.vl:1082:19");
	}
	const entry = as_usize(slot3);
	const $cN = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1085:5")[0];
	const cell3 = __clone($cN[1]);
	if ($cN[0] === 0) {
		return cell3;
	}
	const cell4 = new7(0, "std/src/reactive/lens.vl:1088:25");
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1089:2")[0] = [ 0, __clone(cell4) ];
	return cell4;
}
function identity3(self) {
	return identity2(slot_cell(self[0][0].v[3], self[1]));
}
function on_settle3(self, subscriber) {
	return on_settle(slot_cell(self[0][0].v[3], self[1]), subscriber);
}
function track3(self, $af) {
	const scope = $af;
	const identity5 = identity3(self);
	if (admits(scope, identity5)) {
		lists_of(scope[0]).v[0].push([ identity5, (subscriber) => {
			return on_settle3(self, subscriber);
		} ]);
	}
	return get4(self);
}
function row(self, key) {
	const $cQ = get(self[3].v, key);
	if ($cQ[0] === 0) {
		return $cQ[1];
	}
	const made = [ new7(0, "std/src/reactive/lens/loaded.vl:141:44"), new7(false, "std/src/reactive/lens/loaded.vl:141:74") ];
	insert(self[3].v, key, made);
	return made;
}
function run_body2(runs, body) {
	const run = renew(runs);
	const $cR = nursery(run);
	let $cS = null;
	if ($cR[0] === 0) {
		const nursery2 = $cR[1];
		$cS = (($bt) => {
			return (($bu) => {
				return body($bt, $bu);
			})(nursery2);
		})(run);
	} else {
		$cS = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $cS;
}
function run_once2(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body2(runs, ($ba, $bb) => {
		return (($bc) => {
			return body($ba, $bc, $bb);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked2(runs, tracker, body) {
	let value = run_once2(runs, tracker, ($aV, $aW, $aX) => {
		return body($aV, $aW, $aX);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once2(runs, tracker, ($bU, $bV, $bW) => {
			return body($bU, $bV, $bW);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function start3(self) {
	const body = self[0];
	const runs = new4();
	const tracker = new_tracker([ 0, __clone(self[1]) ]);
	return [ () => {
		return run_tracked2(runs, tracker, body);
	}, (subscriber) => {
		attach_tracker(tracker, subscriber);
		return retiring(subscriber, () => {
			return detach_tracker(tracker);
		});
	}, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	} ];
}
function eq9(self, b2) {
	const $cT = self;
	let $cW = null;
	if ($cT[0] === 0) {
		const $cU = b2;
		let $cV = null;
		if ($cU[0] === 0) {
			$cV = eq4($cT[1], $cU[1]);
		} else {
			$cV = false;
		}
		$cW = $cV;
	} else {
		const $cX = b2;
		$cW = $cX[0] === 1;
	}
	return $cW;
}
function start4(self) {
	const site = __clone(self[1]);
	const upstream = start3(self[0]);
	const pull = upstream[0];
	const last2 = __shared_new(pull());
	return [ () => {
		return __clone(last2.v);
	}, (subscriber) => {
		const relayed = upstream[1](placed_at(subscriber_of(() => {
			const now = pull();
			if (!(eq9(now, last2.v))) {
				last2.v = now;
				wake(subscriber);
			}
			return;
		}, true), site));
		also_retiring(relayed, subscriber);
		return relayed;
	}, upstream[2] ];
}
function set2(self, value, $ck, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $ck);
}
function cell2(self, $aT, $aU, caller) {
	const instance = start4(self);
	const pull = instance[0];
	const cached = new7(pull(), caller);
	const refresh = subscriber_of(() => {
		set2(cached, pull(), $aT, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed = instance[1](refresh);
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $aT, $aU);
	return cached;
}
function memo2(self, $aR, $aS, caller) {
	return [ cell2(self, $aR, $aS, caller) ];
}
function view(self, slot3, $cI, $cJ) {
	const ledger = __clone(self[2]);
	const key = self[1];
	return memo2(distinct(derive(($cK, $cL, $cM) => {
		const $cO = track3(slot3, $cL);
		let $cP = null;
		if ($cO[0] === 0) {
			const item2 = $cO[1];
			const id = key(item2);
			track2(row(ledger, id)[0], $cL);
			$cP = over3(ledger, id, [ 0, item2 ]);
		} else {
			$cP = [ 1 ];
		}
		return $cP;
	}, "std/src/reactive/lens/loaded.vl:507:3"), "std/src/reactive/lens/loaded.vl:516:6"), $cI, $cJ, "std/src/reactive/lens/loaded.vl:516:17");
}
function attach_observer3(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get3(self));
	}
	return subscription;
}
function attach_observer4(self, observer, immediately) {
	return attach_observer3(self[0], observer, immediately);
}
function on_change2(self, observer, caller) {
	return place_subscription(attach_observer4(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function key_of(self, slot3) {
	const key = self[1];
	return map(get4(slot3), (item2) => {
		return key(item2);
	});
}
function insert2(self, key, value) {
	__nmap_insert(self[0], hash2(key), __clone(key), __clone(value));
}
function batch(body, $dD) {
	const $dE = $dD;
	let $dF = null;
	if ($dE[0] === 0) {
		const current = $dE[1];
		$dF = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$dF = result;
	}
	return $dF;
}
function open2(self, key, value, insert4, above, below, $dm) {
	const id = self[6].v + 1;
	self[6].v = id;
	const ledger = __clone(self);
	batch(($dn) => {
		const cells = row(ledger, key);
		const found = get(ledger[0].v, key);
		const $do = found;
		let $dp = null;
		if ($do[0] === 0) {
			const held2 = $do[1];
			$dp = held2;
		} else {
			ledger[1].v.push(__clone(key));
			set2(cells[1], true, [ 0, $dn ], "std/src/reactive/lens/loaded.vl:176:20");
			$dp = [ [  ], 0, __clone(above), __clone(below) ];
		}
		let entry = $dp;
		if (insert4) {
			entry[2] = __clone(above);
			entry[3] = __clone(below);
		}
		entry[0].push([ id, __clone(value), insert4 ]);
		insert(ledger[0].v, key, entry);
		insert2(ledger[2].v, id, key);
		set2(ledger[4], get3(ledger[4]) + 1, [ 0, $dn ], "std/src/reactive/lens/loaded.vl:187:17");
		notify(cells[0], [ 0, $dn ]);
		let $dC = undefined;
		if (insert4 || (($dC = value), $dC[0] === 1)) {
			notify(ledger[5], [ 0, $dn ]);
		}
		return;
	}, $dm);
	return id;
}
function propose(self, slot3, value, $di) {
	const $dl = key_of(self, slot3);
	if ($dl[0] === 0) {

	} else {
		return [ 1 ];
	}
	return [ 0, open2(self[2], $dl[1], [ 0, __clone(value) ], false, [ 1 ], [ 1 ], $di) ];
}
function unwrap(self, caller) {
	const $dG = self;
	let $dH = null;
	if ($dG[0] === 0) {
		const x2 = $dG[1];
		$dH = x2;
	} else {
		$dH = (() => {
			throw __panic("expected Some but got None", caller);
		})();
	}
	return $dH;
}
function get5(self) {
	return get3(self[0]);
}
function get6(self, key) {
	return __nmap_get(self[0], hash2(key));
}
function landing(self, key) {
	let at2 = 0;
	for (const row2 of compose(self, true)) {
		if (row2[0] === key) {
			return at2;
		}
		const $dO = row2[1];
		if ($dO[0] === 0) {
			at2 = at2 + 1;
		}
	}
	return at2;
}
function remove2(self, key) {
	__nmap_remove(self[0], hash2(key));
}
function close2(self, id, accepted, $dP) {
	const $dQ = get6(self[2].v, id);
	let $dR = null;
	if ($dQ[0] === 0) {
		const held2 = $dQ[1];
		$dR = held2;
	} else {
		return [ 1 ];
	}
	const key = $dR;
	const found = get(self[0].v, key);
	const $dS = found;
	if ($dS[0] === 0) {

	} else {
		return [ 1 ];
	}
	let entry = __clone($dS[1]);
	let ended = [ 1 ];
	let kept = [  ];
	for (const proposal of entry[0]) {
		if (proposal[0] === id) {
			ended = [ 0, __clone(proposal) ];
		} else {
			kept.push(__clone(proposal));
		}
	}
	const $dT = ended;
	if ($dT[0] === 0) {

	} else {
		return [ 1 ];
	}
	const lands = accepted && id > entry[1];
	if (lands) {
		entry[1] = id;
	}
	entry[0] = __clone(kept);
	const ledger = __clone(self);
	const settled = kept.length === 0;
	batch(($dU) => {
		const cells = row(ledger, key);
		remove2(ledger[2].v, id);
		if (settled) {
			remove(ledger[0].v, key);
			let order = [  ];
			for (const held_key of ledger[1].v) {
				if (held_key !== key) {
					order.push(__clone(held_key));
				}
			}
			ledger[1].v = order;
			set2(cells[1], false, [ 0, $dU ], "std/src/reactive/lens/loaded.vl:235:19");
		} else {
			insert(ledger[0].v, key, entry);
		}
		set2(ledger[4], get3(ledger[4]) - 1, [ 0, $dU ], "std/src/reactive/lens/loaded.vl:239:17");
		notify(cells[0], [ 0, $dU ]);
		let $dV = undefined;
		if ($dT[1][2] || (($dV = $dT[1][1]), $dV[0] === 1)) {
			notify(ledger[5], [ 0, $dU ]);
		}
		return;
	}, $dP);
	return [ 0, [ __clone(key), __clone($dT[1][1]), $dT[1][2], lands ] ];
}
function held(self, key) {
	return find(self[3], key);
}
function size(self) {
	return peek(self, (held2) => {
		return count(held2);
	});
}
function coming_one(self, change2) {
	if (self[0].v[4].length === 0) {
		return;
	}
	const taps = __clone(self[0].v[4]);
	for (const tap2 of taps) {
		tap2[1](change2);
	}
}
function splice(self, at2, removed3, inserted) {
	let taken = 0;
	while (taken < removed3) {
		__remove_at(self, at2, "std/src/reactive/lens.vl:2231:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at2 + offset, __clone(value), "std/src/reactive/lens.vl:2236:9");
		offset = offset + 1;
	}
}
function mutate(self, change2) {
	const $ei = self[0].v[0];
	let $ej = null;
	if ($ei[0] === 0) {
		const cell3 = $ei[1];
		$ej = change2(cell3.v);
	} else {
		const port = __clone($ei[1]);
		$ej = port[1](change2);
	}
	return $ej;
}
function wake_all(watch, slot3, $eL) {
	const held2 = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot3), "std/src/reactive/lens.vl:1120:36"));
	});
	const $eM = held2[0];
	if ($eM[0] === 0) {
		notify($eM[1], $eL);
	}
	for (const part of held2[1]) {
		notify(part[1], $eL);
	}
}
function wake_part(watch, slot3, name, $eN) {
	const held2 = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot3), "std/src/reactive/lens.vl:1130:36"));
	});
	const $eO = held2[0];
	if ($eO[0] === 0) {
		notify($eO[1], $eN);
	}
	for (const part of held2[1]) {
		if (part[0] === name) {
			notify(part[1], $eN);
		}
	}
}
function landed(self, changes, late, $el) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index2 = 0;
	for (const change2 of changes) {
		const $em = change2;
		let $en = null;
		if ($em[0] === 0) {
			const _at = $em[1];
			const removed3 = $em[2];
			const inserted = $em[3];
			shaped = true;
			if (removed3 !== inserted) {
				resized = true;
			}
			$en = undefined;
		} else if ($em[0] === 1) {
			const _at2 = $em[1];
			const _count = $em[2];
			$en = resized = true;
		} else if ($em[0] === 3) {
			const _at3 = $em[1];
			const _count2 = $em[2];
			$en = resized = true;
		} else if ($em[0] === 6) {
			$en = resized = true;
		} else if ($em[0] === 5) {
			const _from = $em[1];
			const _to = $em[2];
			$en = shaped = true;
		} else {
			$en = undefined;
		}
		$en;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change2, wakes);
		}
		let $eD = null;
		if (self[0].v[4].length > 0) {
			let $eC = null;
			if (!(late)) {
				$eC = [ 0 ];
			} else if (!(several)) {
				$eC = [ 1 ];
			} else if (index2 === 0) {
				$eC = [ 2 ];
			} else {
				$eC = [ 3 ];
			}
			const stage = $eC;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$eD = undefined;
		}
		$eD;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $eE = null;
	if (shaped || resized) {
		$eE = watch.v[2];
	} else {
		$eE = [ 1 ];
	}
	const shape = __clone($eE);
	let $eF = null;
	if (resized) {
		$eF = watch.v[3];
	} else {
		$eF = [ 1 ];
	}
	const sized = __clone($eF);
	let $eG = undefined;
	let $eH = undefined;
	if (wakes.length === 0 && (($eG = shape), $eG[0] === 1) && (($eH = sized), $eH[0] === 1)) {
		notify(published, $el);
		return;
	}
	settle_together(($eI) => {
		for (const wake2 of wakes) {
			const $eJ = wake2;
			let $eK = null;
			if ($eJ[0] === 0) {
				const slot3 = $eJ[1];
				$eK = wake_all(watch, slot3, [ 0, $eI ]);
			} else if ($eJ[0] === 2) {
				const slot4 = $eJ[1];
				$eK = wake_all(watch, slot4, [ 0, $eI ]);
			} else {
				const slot5 = $eJ[1];
				const part = $eJ[2];
				$eK = wake_part(watch, slot5, part, [ 0, $eI ]);
			}
			$eK;
		}
		const $eP = sized;
		if ($eP[0] === 0) {
			notify($eP[1], [ 0, $eI ]);
		}
		const $eQ = shape;
		if ($eQ[0] === 0) {
			notify($eQ[1], [ 0, $eI ]);
		}
		notify(published, [ 0, $eI ]);
		return;
	}, $el);
}
function landed_one(self, change2, $ek) {
	landed(self, [ __clone(change2) ], false, $ek);
}
function splice2(self, at2, removed3, inserted, $ef) {
	const held2 = size(self);
	let $eg = null;
	if (at2 > held2) {
		$eg = held2;
	} else {
		$eg = at2;
	}
	const start5 = $eg;
	let $eh = null;
	if (start5 + removed3 > held2) {
		$eh = held2 - start5;
	} else {
		$eh = removed3;
	}
	const taking = $eh;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start5, taking, arriving ];
	coming_one(self, change2);
	mutate(self, (held3) => {
		return splice(held3, start5, taking, inserted);
	});
	landed_one(self, change2, $ef);
}
function splice3(self, at2, removed3, inserted, $ee) {
	splice2(self[0], at2, removed3, inserted, $ee);
}
function remove_at(self, at2, $ed) {
	return splice3(self, at2, 1, [  ], $ed);
}
function and_then(self, fn) {
	const $eY = self;
	let $eZ = null;
	if ($eY[0] === 0) {
		const x2 = $eY[1];
		$eZ = fn(x2);
	} else {
		$eZ = [ 1 ];
	}
	return $eZ;
}
function peek2(self, read) {
	return peek(self[0], read);
}
function has(self, at2) {
	const holds = self[0].v[1][0];
	return peek(self, (held2) => {
		return holds(held2, at2);
	});
}
function replace(self, at2, value, $fd) {
	const watch = self[0].v[3];
	let $fe = null;
	if (has(self, at2)) {
		$fe = [ 2, __clone(at2) ];
	} else if (viewing2(watch, (held2) => {
		return follows_items(held2[0]);
	})) {
		return false;
	} else {
		$fe = [ 1, __clone(at2), 1 ];
	}
	const change2 = $fe;
	coming_one(self, change2);
	const write = self[0].v[1][3];
	mutate(self, (held2) => {
		return write(held2, at2, value);
	});
	landed_one(self, change2, $fd);
	return true;
}
function replace2(self, at2, value, $fc) {
	return replace(self[0], at2, value, $fc);
}
function insert_at(self, at2, value, $ff) {
	return splice3(self, at2, 0, [ __clone(value) ], $ff);
}
function answer(self, id, accepted, reply, $dK) {
	const overlay2 = __clone(self);
	let answered = false;
	batch(($dL) => {
		const $dM = get6(overlay2[2][2].v, id);
		let $dN = null;
		if ($dM[0] === 0) {
			const key = $dM[1];
			$dN = landing(overlay2, key);
		} else {
			$dN = 0;
		}
		const landing2 = $dN;
		const $dW = close2(overlay2[2], id, accepted, [ 0, $dL ]);
		if ($dW[0] === 0) {

		} else {
			return;
		}
		answered = true;
		if (!($dW[1][3])) {
			return;
		}
		const $dX = reply;
		let $dY = null;
		if ($dX[0] === 0) {
			const item2 = $dX[1];
			$dY = [ 0, __clone(item2) ];
		} else {
			$dY = $dW[1][1];
		}
		const landed2 = __clone($dY);
		const lens2 = __clone(overlay2[0]);
		const $dZ = landed2;
		let $ea = null;
		if ($dZ[0] === 1) {
			const $eb = held(overlay2, $dW[1][0]);
			let $ec = undefined;
			if ($eb[0] === 0 && (($ec = at($eb[1])), $ec[0] === 0)) {
				remove_at(lens2, $ec[1], [ 0, $dL ]);
			}
			$ea = undefined;
		} else {
			const item3 = $dZ[1];
			const server_key = overlay2[1](item3);
			const $eW = held(overlay2, $dW[1][0]);
			let $eX = null;
			if ($eW[0] === 0) {
				$eX = at($eW[1]);
			} else {
				$eX = and_then(held(overlay2, server_key), (slot3) => {
					return at(slot3);
				});
			}
			const target = $eX;
			const $fa = target;
			let $fb = null;
			if ($fa[0] === 0) {
				const at2 = $fa[1];
				if (!(eq9(peek2(lens2, (list) => {
					return __list_get(list, at2);
				}), [ 0, __clone(item3) ]))) {
					replace2(lens2, at2, item3, [ 0, $dL ]);
				}
				$fb = undefined;
			} else {
				if ($dW[1][2]) {
					insert_at(lens2, landing2, item3, [ 0, $dL ]);
				}
				$fb = undefined;
			}
			$ea = $fb;
		}
		return $ea;
	}, $dK);
	return answered;
}
function reject(self, id, $dJ) {
	return answer(self, id, false, [ 1 ], $dJ);
}
function shown_rows(self) {
	return compose(self, false);
}
function propose_insert(self, at2, value, $fg) {
	const key = self[1](value);
	const $fh = held(self, key);
	if ($fh[0] === 0) {
		return open2(self[2], key, [ 0, __clone(value) ], false, [ 1 ], [ 1 ], $fg);
	}
	const shown2 = shown_rows(self);
	let $fi = null;
	if (at2 > 0 && at2 <= shown2.length) {
		$fi = [ 0, __clone(__at(shown2, at2 - 1, "std/src/reactive/lens/loaded.vl:403:53")[0]) ];
	} else {
		$fi = [ 1 ];
	}
	const above = $fi;
	const below = map(__list_get(shown2, at2), (row2) => {
		return __clone(row2[0]);
	});
	return open2(self[2], key, [ 0, __clone(value) ], true, above, below, $fg);
}
function shown(self) {
	let out = [  ];
	for (const row2 of compose(self, false)) {
		const $fl = row2[1];
		let $fm = null;
		if ($fl[0] === 0) {
			const slot3 = $fl[1];
			$fm = get4(slot3);
		} else {
			$fm = [ 1 ];
		}
		const held2 = $fm;
		const $fn = over3(self[2], row2[0], held2);
		if ($fn[0] === 0) {
			out.push($fn[1]);
		}
	}
	return out;
}
function all_of(storage) {
	return peek_storage2(storage, (held2) => {
		let out = [  ];
		let at2 = 0;
		const count2 = count(held2);
		while (at2 < count2) {
			const $fo = item(held2, at2);
			if ($fo[0] === 0) {
				out.push($fo[1]);
			}
			at2 = at2 + 1;
		}
		return out;
	});
}
function all(self) {
	return all_of(self[0].v[0]);
}
function all2(self) {
	return all(self[0]);
}
function identity4(self) {
	return identity2(shape_cell(self[0][0]));
}
function on_settle4(self, subscriber) {
	return on_settle(shape_cell(self[0][0]), subscriber);
}
function get7(self) {
	return at(self[0]);
}
function track4(self, $af) {
	const scope = $af;
	const identity5 = identity4(self);
	if (admits(scope, identity5)) {
		lists_of(scope[0]).v[0].push([ identity5, (subscriber) => {
			return on_settle4(self, subscriber);
		} ]);
	}
	return get7(self);
}
function view_key(self, key, $fw, $fx) {
	const ledger = __clone(self[2]);
	const index2 = __clone(self[3]);
	const shape = __clone(self[5]);
	return memo2(distinct(derive(($fy, $fz, $fA) => {
		track2(row(ledger, key)[0], $fz);
		track4(shape, $fz);
		const $fB = find(index2, key);
		let $fC = null;
		if ($fB[0] === 0) {
			const slot3 = $fB[1];
			$fC = track3(slot3, $fz);
		} else {
			$fC = [ 1 ];
		}
		const held2 = $fC;
		return over3(ledger, key, held2);
	}, "std/src/reactive/lens/loaded.vl:525:3"), "std/src/reactive/lens/loaded.vl:533:6"), $fw, $fx, "std/src/reactive/lens/loaded.vl:533:17");
}
function is_pending_key(self, key) {
	return [ __clone(row(self[2], key)[1]) ];
}
function shift(self, from, to) {
	const item2 = __remove_at(self, from, "std/src/reactive/lens.vl:2252:19");
	__insert_at(self, to, item2, "std/src/reactive/lens.vl:2253:8");
}
function move_item(self, from, to, $fG) {
	const held2 = size(self);
	if (from >= held2 || from === to) {
		return;
	}
	let $fH = null;
	if (to >= held2) {
		$fH = held2 - 1;
	} else {
		$fH = to;
	}
	const landing2 = $fH;
	const change2 = [ 5, from, landing2 ];
	coming_one(self, change2);
	mutate(self, (held3) => {
		return shift(held3, from, landing2, $fG);
	});
	landed_one(self, change2, $fG);
}
function move_item2(self, from, to, $fF) {
	return move_item(self[0], from, to, $fF);
}
function propose_delete(self, slot3, $fI) {
	const $fJ = key_of(self, slot3);
	if ($fJ[0] === 0) {

	} else {
		return [ 1 ];
	}
	return [ 0, open2(self[2], $fJ[1], [ 1 ], false, [ 1 ], [ 1 ], $fI) ];
}
function confirm(self, id, $fK) {
	return answer(self, id, true, [ 1 ], $fK);
}
function pending(self) {
	return [ __clone(self[2][4]) ];
}
function size2(self) {
	return size(self[0]);
}
function push(self, value, $fL) {
	return splice3(self, size2(self), 0, [ __clone(value) ], $fL);
}
function confirm_with(self, id, item2, $fQ) {
	return answer(self, id, true, [ 0, __clone(item2) ], $fQ);
}
function contains(self, value) {
	for (const item2 of self) {
		if (eq2(item2, value)) {
			return true;
		}
	}
	return false;
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
const trackers_allocated_count = __shared_new(0);
const delta_log_limit = 1024;
const block_size = 128;
const board = over2([ card("a", "A"), card("b", "B"), card("c", "C") ], [ 1 ]);
const overlay = beside(board, (item2) => {
	return item2[0];
}, [ 1 ]);
const rows4 = rows3(overlay, [ 1 ], [ 1 ]);
const rows_runs = __shared_new(0);
on_change(__clone(rows4), (_rows, $cC) => {
	const $cD = [ rows_runs, "v" ];
	return rows_runs.v = $cD[0][$cD[1]] + 1;
}, "lens-overlay.vl:43:19");
const b = slot2(board, 1, [ 1 ]);
const b_view = view(overlay, b, [ 1 ], [ 1 ]);
const b_runs = __shared_new(0);
on_change2(__clone(b_view), (_item, $de) => {
	const $df = [ b_runs, "v" ];
	return b_runs.v = $df[0][$df[1]] + 1;
}, "lens-overlay.vl:49:18");
const edit = unwrap(propose(overlay, b, card("b", "B2"), [ 1 ]), "lens-overlay.vl:50:49");
assert(eq9(get5(b_view), [ 0, card("b", "B2") ]) && eq9(get4(b), [ 0, card("b", "B") ]), "an edit shows, the lens is untouched", "lens-overlay.vl:51:2");
assert(b_runs.v === 1 && rows_runs.v === 0, "an edit wakes its view once and not the row feed", "lens-overlay.vl:55:2");
assert(reject(overlay, edit, [ 1 ]) && eq9(get5(b_view), [ 0, card("b", "B") ]) && b_runs.v === 2, "rejected", "lens-overlay.vl:59:2");
const insert3 = propose_insert(overlay, 2, card("x", "X"), [ 1 ]);
assert(eq(ids(shown(overlay)), [ "a", "b", "x", "c" ]), "the insert shows at position 2", "lens-overlay.vl:66:2");
assert(eq(ids(all2(board)), [ "a", "b", "c" ]), "and is not in the lens", "lens-overlay.vl:67:2");
assert(eq(row_keys(get5(rows4)), [ "a", "b", "(x)", "c" ]), "the row feed lists it, with no slot", "lens-overlay.vl:68:2");
assert(rows_runs.v === 1, "the row feed woke once for it", "lens-overlay.vl:69:2");
const x_view = view_key(overlay, "x", [ 1 ], [ 1 ]);
const x_runs = __shared_new(0);
on_change2(__clone(x_view), (_item, $fD) => {
	const $fE = [ x_runs, "v" ];
	return x_runs.v = $fE[0][$fE[1]] + 1;
}, "lens-overlay.vl:72:18");
assert(eq9(get5(x_view), [ 0, card("x", "X") ]) && get5(is_pending_key(overlay, "x")), "its view by key shows it, pending", "lens-overlay.vl:73:2");
insert_at(board, 0, card("z", "Z"), [ 1 ]);
move_item2(board, 2, 3, [ 1 ]);
assert(eq(ids(all2(board)), [ "z", "a", "c", "b" ]), "the server reordered", "lens-overlay.vl:82:2");
assert(eq(ids(shown(overlay)), [ "z", "a", "c", "b", "x" ]), "the provisional row followed the row above it", "lens-overlay.vl:83:2");
const hide_a = unwrap(propose_delete(overlay, slot2(board, 1, [ 1 ]), [ 1 ]), "lens-overlay.vl:90:53");
const after_hidden = propose_insert(overlay, 1, card("y", "Y"), [ 1 ]);
assert(eq(ids(shown(overlay)), [ "z", "y", "c", "b", "x" ]), "y shows where a was", "lens-overlay.vl:92:2");
assert(reject(overlay, hide_a, [ 1 ]), "the hide is refused", "lens-overlay.vl:93:2");
assert(eq(ids(shown(overlay)), [ "z", "y", "a", "c", "b", "x" ]), "a shows again in its place, y still right below z", "lens-overlay.vl:94:2");
assert(reject(overlay, after_hidden, [ 1 ]) && eq(ids(shown(overlay)), [ "z", "a", "c", "b", "x" ]), "a refused insert leaves no row", "lens-overlay.vl:98:2");
const runs_before = rows_runs.v;
assert(confirm(overlay, insert3, [ 1 ]), "the insert is confirmed", "lens-overlay.vl:106:2");
assert(eq(ids(all2(board)), [ "z", "a", "c", "b", "x" ]), "it landed in the lens where it was shown", "lens-overlay.vl:107:2");
assert(eq(row_keys(get5(rows4)), [ "z", "a", "c", "b", "x" ]), "the row has a slot now", "lens-overlay.vl:111:2");
assert(rows_runs.v === runs_before + 1, "the row feed woke once for the landing", "lens-overlay.vl:112:2");
assert(eq9(get5(x_view), [ 0, card("x", "X") ]) && x_runs.v === 0, "the row\'s view did not wake", "lens-overlay.vl:113:2");
assert(get5(pending(overlay)) === 0, "nothing pending", "lens-overlay.vl:117:2");
const echo = propose_insert(overlay, 0, card("e", "E"), [ 1 ]);
assert(eq(ids(shown(overlay)), [ "e", "z", "a", "c", "b", "x" ]), "e shows at the top", "lens-overlay.vl:122:2");
push(board, card("e", "E, stored"), [ 1 ]);
assert(eq(ids(shown(overlay)), [ "z", "a", "c", "b", "x", "e" ]), "the pushed row IS the row: one e, where the server put it", "lens-overlay.vl:124:2");
assert(eq9(get5(view_key(overlay, "e", [ 1 ], [ 1 ])), [ 0, card("e", "E") ]), "showing the proposal on top of the server\'s copy", "lens-overlay.vl:128:2");
assert(confirm(overlay, echo, [ 1 ]), "confirmed", "lens-overlay.vl:132:2");
assert(eq(ids(all2(board)), [ "z", "a", "c", "b", "x", "e" ]) && peek2(board, (list) => {
	return __at(list, 5, "lens-overlay.vl:134:75")[1];
}) === "E", "no second e; the confirmed value is the server\'s word", "lens-overlay.vl:133:2");
const temporary = propose_insert(overlay, 1, card("tmp-1", "New"), [ 1 ]);
assert(eq(ids(shown(overlay)), [ "z", "tmp-1", "a", "c", "b", "x", "e" ]), "the provisional row, under a temporary key", "lens-overlay.vl:141:2");
assert(confirm_with(overlay, temporary, card("s-9", "New"), [ 1 ]), "confirmed with the server\'s row", "lens-overlay.vl:145:2");
assert(eq(ids(all2(board)), [ "z", "s-9", "a", "c", "b", "x", "e" ]), "the server\'s row landed where the provisional one stood", "lens-overlay.vl:146:2");
assert(eq9(get5(view_key(overlay, "tmp-1", [ 1 ], [ 1 ])), [ 1 ]) && !(get5(is_pending_key(overlay, "tmp-1"))), "the temporary key is gone", "lens-overlay.vl:150:2");
const temporary2 = propose_insert(overlay, 0, card("tmp-2", "Other"), [ 1 ]);
push(board, card("s-10", "Other"), [ 1 ]);
assert(eq(ids(shown(overlay)), [ "tmp-2", "z", "s-9", "a", "c", "b", "x", "e", "s-10" ]), "both show until the confirmation", "lens-overlay.vl:158:2");
assert(confirm_with(overlay, temporary2, card("s-10", "Other"), [ 1 ]), "the confirmation names s-10", "lens-overlay.vl:162:2");
assert(eq(ids(all2(board)), [ "z", "s-9", "a", "c", "b", "x", "e", "s-10" ]), "no second s-10", "lens-overlay.vl:163:2");
assert(eq(ids(shown(overlay)), ids(all2(board))), "the provisional row is gone, nothing pending shows", "lens-overlay.vl:164:2");
const refused = propose_insert(overlay, 0, card("no", "No"), [ 1 ]);
assert(eq4(__at(shown(overlay), 0, "lens-overlay.vl:171:9"), card("no", "No")), "shown", "lens-overlay.vl:171:2");
assert(reject(overlay, refused, [ 1 ]) && eq4(__at(shown(overlay), 0, "lens-overlay.vl:174:7"), card("z", "Z")) && __at(ids(all2(board)), 0, "lens-overlay.vl:175:7") === "z", "gone, and never held", "lens-overlay.vl:172:2");
const c = slot2(board, 3, [ 1 ]);
const c_view = view(overlay, c, [ 1 ], [ 1 ]);
const doomed_edit = unwrap(propose(overlay, c, card("c", "C, edited"), [ 1 ]), "lens-overlay.vl:183:63");
remove_at(board, 3, [ 1 ]);
assert(eq9(get5(c_view), [ 1 ]) && eq9(get5(view_key(overlay, "c", [ 1 ], [ 1 ])), [ 1 ]), "the server\'s removal wins over a local edit", "lens-overlay.vl:185:2");
assert(confirm(overlay, doomed_edit, [ 1 ]), "the edit\'s answer is still taken", "lens-overlay.vl:189:2");
assert(get5(pending(overlay)) === 0 && eq9(get5(view_key(overlay, "c", [ 1 ], [ 1 ])), [ 1 ]) && !(contains(ids(all2(board)), "c")), "and does not bring the row back", "lens-overlay.vl:190:2");
const x = unwrap(held(overlay, "x"), "lens-overlay.vl:201:28");
const renamed = unwrap(propose(overlay, x, card("x", "X2"), [ 1 ]), "lens-overlay.vl:202:52");
const removed = unwrap(propose_delete(overlay, x, [ 1 ]), "lens-overlay.vl:203:42");
assert(eq9(get5(view(overlay, x, [ 1 ], [ 1 ])), [ 1 ]), "the newer change, the delete, shows", "lens-overlay.vl:204:2");
assert(reject(overlay, removed, [ 1 ]) && eq9(get5(view(overlay, x, [ 1 ], [ 1 ])), [ 0, card("x", "X2") ]), "the delete refused: the edit shows", "lens-overlay.vl:205:2");
assert(confirm(overlay, renamed, [ 1 ]) && eq9(get4(x), [ 0, card("x", "X2") ]), "the edit confirmed: the lens holds it", "lens-overlay.vl:209:2");
const renamed2 = unwrap(propose(overlay, x, card("x", "X3"), [ 1 ]), "lens-overlay.vl:213:52");
const removed2 = unwrap(propose_delete(overlay, x, [ 1 ]), "lens-overlay.vl:214:42");
assert(confirm(overlay, removed2, [ 1 ]), "the delete confirmed first", "lens-overlay.vl:215:2");
assert(eq9(get4(x), [ 1 ]) && eq6(held(overlay, "x"), [ 1 ]), "the lens let the row go", "lens-overlay.vl:216:2");
assert(reject(overlay, renamed2, [ 1 ]) && get5(pending(overlay)) === 0 && eq9(get5(view_key(overlay, "x", [ 1 ], [ 1 ])), [ 1 ]), "the late refusal of the older edit changes nothing", "lens-overlay.vl:217:2");
const gone = slot2(board, 0, [ 1 ]);
remove_at(board, 0, [ 1 ]);
assert(eq3(propose(overlay, gone, card("z", "Z"), [ 1 ]), [ 1 ]) && eq3(propose_delete(overlay, gone, [ 1 ]), [ 1 ]), "a slot whose item is gone takes no proposal", "lens-overlay.vl:228:2");
assert(!(confirm(overlay, 999, [ 1 ])) && !(reject(overlay, 999, [ 1 ])), "an unknown id is not pending", "lens-overlay.vl:232:2");
console.log("overlay: all passed");
//# sourceMappingURL=lens-overlay.mjs.map
