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
	let $dJ = null;
	if (!(condition)) {
		$dJ = (() => {
			throw __panic(message, caller);
		})();
	}
	return $dJ;
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
	const $fU = self;
	let $fX = null;
	if ($fU[0] === 0) {
		const $fV = b2;
		let $fW = null;
		if ($fV[0] === 0) {
			$fW = $fU[1] === $fV[1];
		} else {
			$fW = false;
		}
		$fX = $fW;
	} else {
		const $fY = b2;
		$fX = $fY[0] === 1;
	}
	return $fX;
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
		let $bH = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$bH = undefined;
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
		$bH;
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
	const $bF = turn;
	let $bG = null;
	if ($bF[0] === 0) {
		const ambient = $bF[1];
		$bG = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $bI = last(draining_turns.v);
		let $bJ = null;
		if ($bI[0] === 0) {
			const draining = $bI[1];
			$bJ = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$bJ = undefined;
		}
		$bG = $bJ;
	}
	return $bG;
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
		const $cj = previous;
		let $ck = null;
		if ($cj[0] === 0) {
			const release6 = $cj[1];
			$ck = release6();
		} else {
			$ck = undefined;
		}
		return $ck;
	} ];
}
function detached(id, live, release6) {
	let subscribers = [  ];
	const empty = __shared_new(subscribers);
	return [ empty, id, live, __shared_new([ 0, release6 ]) ];
}
function dispose(self, $cA) {
	const $cB = $cA;
	let $cC = null;
	if ($cB[0] === 0) {
		const established = $cB[1];
		$cC = [ 0, established ];
	} else {
		$cC = last(draining_turns.v);
	}
	const ambient = $cC;
	release_under(self, ambient);
}
function detach(handle) {
	const $bN = last(releasing_turns.v);
	let $bO = null;
	if ($bN[0] === 0) {
		const at_release = $bN[1];
		$bO = at_release;
	} else {
		$bO = last(draining_turns.v);
	}
	const turn = $bO;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $bP = [ 0, handle[0] ];
	let $bQ = null;
	if ($bP[0] === 0) {
		const subscribers = $bP[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$bQ = undefined;
	} else {
		$bQ = undefined;
	}
	$bQ;
	const $bR = ambient;
	let $bS = null;
	if ($bR[0] === 0) {
		const turn = $bR[1];
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
		$bS = undefined;
	} else {
		$bS = undefined;
	}
	$bS;
	const $bT = handle[3].v;
	let $bU = null;
	if ($bT[0] === 0) {
		const release6 = $bT[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release6, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$bU = undefined;
	} else {
		$bU = undefined;
	}
	return $bU;
}
function new4() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $S = null;
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
		$S = undefined;
	}
	return $S;
}
function renew(self) {
	const $be = self[0].v[3];
	let $bf = null;
	if ($be[0] === 0) {
		const nursery2 = __clone($be[1]);
		let $bg = null;
		if (has_spawned(nursery2)) {
			$bg = [ 1 ];
		} else {
			$bg = [ 0, nursery2 ];
		}
		$bf = $bg;
	} else {
		$bf = [ 1 ];
	}
	const carried = $bf;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $br = null;
	if (is_disposed(self)) {
		$br = [ 1 ];
	} else {
		$br = self[0].v[3];
	}
	return $br;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $bq = null;
	if (!(is_disposed(self))) {
		const held2 = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $bh = held2[3];
		let $bi = null;
		if ($bh[0] === 0) {
			const nursery2 = $bh[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$bi = undefined;
		} else {
			$bi = undefined;
		}
		$bi;
		let $bp = null;
		if (held2[2]) {
			let failure = [ 1 ];
			for (const cleanup of held2[1].v) {
				const $bk = __guarded(cleanup);
				let $bl = null;
				if ($bk[0] === 0) {
					const message = $bk[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$bl = undefined;
				} else {
					$bl = undefined;
				}
				$bl;
			}
			const $bn = failure;
			let $bo = null;
			if ($bn[0] === 0) {
				const message2 = $bn[1];
				$bo = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$bo = undefined;
			}
			$bp = $bo;
		}
		$bq = $bp;
	}
	return $bq;
}
function register_with_owner(subscription, $cv, $cw) {
	const $cx = $cw;
	let $cy = null;
	if ($cx[0] === 0) {
		const owner = $cx[1];
		$cy = take(owner, subscription, $cv);
	} else {
		$cy = __clone(subscription);
	}
	return $cy;
}
function defer_to_owner(cleanup, $P) {
	const $Q = $P;
	let $R = null;
	if ($Q[0] === 0) {
		const owner = $Q[1];
		$R = defer(owner, cleanup);
	} else {
		$R = undefined;
	}
	return $R;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker(site) {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ], site ]) ];
}
function lists_of(tracker) {
	const $an = tracker[0].v[6];
	let $ao = null;
	if ($an[0] === 0) {
		const lists = $an[1];
		$ao = lists;
	} else {
		trackers_allocated_count.v = trackers_allocated_count.v + 1;
		const made = __shared_new([ [  ], [  ], [  ] ]);
		tracker[0].v[6] = [ 0, made ];
		$ao = made;
	}
	return $ao;
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $aZ = tracker[0].v[6];
	let $ba = null;
	if ($aZ[0] === 0) {
		const lists = $aZ[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$ba = undefined;
	} else {
		$ba = undefined;
	}
	$ba;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $bw = tracker[0].v[6];
	let $bx = null;
	if ($bw[0] === 0) {
		const lists = $bw[1];
		$bx = lists;
	} else {
		return;
		$bx = undefined;
	}
	const lists2 = $bx;
	const $by = tracker[0].v[5];
	let $bz = null;
	if ($by[0] === 0) {
		const target = __clone($by[1]);
		$bz = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$bz = undefined;
	}
	$bz;
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
		const $bC = reusable(held2, kept, dependency[0], position);
		let $bD = null;
		if ($bC[0] === 0) {
			const index2 = $bC[1];
			__at_put(kept, index2, true, "std/src/reactive.vl:1715:5");
			next.push(__clone(__at(held2, index2, "std/src/reactive.vl:1716:15")));
			$bD = undefined;
		} else {
			next.push([ __clone(dependency[0]), dependency[1](relay_for(tracker, target)) ]);
			$bD = undefined;
		}
		$bD;
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
	const $bA = identity5;
	let $bB = null;
	if ($bA[0] === 0) {
		const wanted = $bA[1];
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
		$bB = [ 1 ];
	} else {
		$bB = [ 1 ];
	}
	return $bB;
}
function same_identity(identity5, wanted) {
	const $al = identity5;
	let $am = null;
	if ($al[0] === 0) {
		const held2 = $al[1];
		$am = held2 === wanted;
	} else {
		$am = false;
	}
	return $am;
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
	const $bK = tracker[0].v[7];
	let $bL = null;
	if ($bK[0] === 0) {
		const site = __clone($bK[1]);
		$bL = place_subscriber(relay[0], site);
	} else {
		$bL = undefined;
	}
	$bL;
	return relay;
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $bY = tracker[0].v[6];
	let $bZ = null;
	if ($bY[0] === 0) {
		const lists = $bY[1];
		$bZ = lists;
	} else {
		return;
		$bZ = undefined;
	}
	const lists2 = $bZ;
	let $ca = null;
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
		$ca = undefined;
	}
	return $ca;
}
function forget_reads(tracker) {
	const $cb = tracker[0].v[6];
	let $cc = null;
	if ($cb[0] === 0) {
		const lists = $cb[1];
		$cc = lists;
	} else {
		return;
		$cc = undefined;
	}
	const lists2 = $cc;
	let $cd = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$cd = undefined;
	}
	$cd;
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
	const $ah = tracker[0].v[6];
	let $ai = null;
	if ($ah[0] === 0) {
		const lists = $ah[1];
		$ai = lists;
	} else {
		return true;
		$ai = undefined;
	}
	const lists2 = $ai;
	const $aj = identity5;
	let $ak = null;
	if ($aj[0] === 0) {
		const wanted = $aj[1];
		const count2 = lists2.v[0].length;
		let index2 = 0;
		while (index2 < count2) {
			if (same_identity(__at(lists2.v[0], index2, "std/src/reactive.vl:1877:22")[0], wanted)) {
				return false;
			}
			index2 = index2 + 1;
		}
		$ak = true;
	} else {
		$ak = true;
	}
	return $ak;
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
		const $ct = previous;
		let $cu = null;
		if ($ct[0] === 0) {
			const earlier = $ct[1];
			$cu = earlier();
		} else {
			$cu = undefined;
		}
		return $cu;
	} ];
}
function placed_at(subscriber, site) {
	place_subscriber(subscriber[0], site);
	return subscriber;
}
function settle_together(body, $eS) {
	const $eT = $eS;
	let $eU = null;
	if ($eT[0] === 0) {
		const current = $eT[1];
		$eU = body(current);
	} else {
		const $eV = last(draining_turns.v);
		let $eW = null;
		if ($eV[0] === 0) {
			const draining = $eV[1];
			$eW = body(draining);
		} else {
			$eW = batch(body, $eS);
		}
		$eU = $eW;
	}
	return $eU;
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
	const $et = seat(self, at2);
	const place = $et[0];
	const index2 = $et[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:265:15");
	let $eu = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:269:5"), index2, -(1), "std/src/reactive/lens.vl:269:35");
			made = made + 1;
		}
		$eu = undefined;
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
	$eu;
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
		const $er = seat(self, at2);
		const place = $er[0];
		const index2 = $er[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:306:16");
		const held2 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:307:15").length;
		let $es = null;
		if (left < held2 - index2) {
			$es = left;
		} else {
			$es = held2 - index2;
		}
		const take2 = $es;
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
	const $ep = change2;
	let $eq = null;
	if ($ep[0] === 0) {
		const at2 = $ep[1];
		const removed3 = $ep[2];
		const inserted = $ep[3];
		close(self, at2, removed3, wakes);
		widen(self, at2, inserted);
		$eq = undefined;
	} else if ($ep[0] === 1) {
		const at3 = $ep[1];
		const count2 = $ep[2];
		$eq = widen(self, at3, count2);
	} else if ($ep[0] === 3) {
		const at4 = $ep[1];
		const count3 = $ep[2];
		$eq = close(self, at4, count3, wakes);
	} else if ($ep[0] === 2) {
		const at5 = $ep[1];
		let $ew = null;
		if (at5 < self[6]) {
			const $ev = seat(self, at5);
			const place = $ev[0];
			const index2 = $ev[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:396:29")), "std/src/reactive/lens.vl:396:17"), index2, "std/src/reactive/lens.vl:396:17");
			if (slot3 !== -(1)) {
				wakes.push([ 0, slot3 ]);
			}
			$ew = undefined;
		}
		$eq = $ew;
	} else if ($ep[0] === 4) {
		const at6 = $ep[1];
		const part = $ep[2];
		let $ey = null;
		if (at6 < self[6]) {
			const $ex = seat(self, at6);
			const place2 = $ex[0];
			const index3 = $ex[1];
			const slot4 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:405:29")), "std/src/reactive/lens.vl:405:17"), index3, "std/src/reactive/lens.vl:405:17");
			if (slot4 !== -(1)) {
				wakes.push([ 1, slot4, part ]);
			}
			$ey = undefined;
		}
		$eq = $ey;
	} else if ($ep[0] === 5) {
		const from = $ep[1];
		const to = $ep[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $ez = null;
		if (from > to) {
			$ez = from;
		} else {
			$ez = to;
		}
		const far = $ez;
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
		const $eA = seat(self, from);
		const left = $eA[0];
		const index4 = $eA[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:429:18");
		const slot5 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:430:16"), index4, "std/src/reactive/lens.vl:430:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:432:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:433:17");
		} else {
			renumber(self, source, index4);
		}
		restart(self);
		let $eB = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$eB = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:441:25")), "std/src/reactive/lens.vl:441:13").length ];
		} else {
			$eB = seat(self, to);
		}
		const $eC = $eB;
		const place3 = $eC[0];
		const landing2 = $eC[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:445:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:446:5"), landing2, slot5, "std/src/reactive/lens.vl:446:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing2);
		restart(self);
		$eq = undefined;
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
		$eq = undefined;
	}
	return $eq;
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
		const $fw = out;
		const $fu = row2[1];
		let $fv = null;
		if ($fu[0] === 0) {
			$fv = row2[0];
		} else {
			$fv = "(" + row2[0] + ")";
		}
		$fw.push($fv);
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
		read(__at(self, at2, "std/src/reactive/lens.vl:2219:10"));
	}
}
function put(self, at2, value) {
	__at_put(self, at2, __clone(value), "std/src/reactive/lens.vl:2239:3");
}
function change(self, at2, with2) {
	if (at2 < self.length) {
		with2(__at(self, at2, "std/src/reactive/lens.vl:2244:14"));
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
	const $r = as_usize(id);
	__at(watch.v[1], $r, "std/src/reactive/lens.vl:1058:2")[2] = __at(watch.v[1], $r, "std/src/reactive/lens.vl:1058:2")[2] + 1;
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
	const left = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1068:13")[2] - 1;
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1069:2")[2] = left;
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
		const $u = __list_pop(keys2);
		let $v = null;
		if ($u[0] === 0) {
			const key = $u[1];
			const $w = __list_pop(values);
			let $x = null;
			if ($w[0] === 0) {
				const value = $w[1];
				$x = backwards.push([ key, value ]);
			} else {
				$x = undefined;
			}
			$v = $x;
		} else {
			$v = undefined;
		}
		$v;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $y = __list_pop(backwards);
		let $z = null;
		if ($y[0] === 0) {
			const pair = $y[1];
			$z = result.push(pair);
		} else {
			$z = undefined;
		}
		$z;
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
		const $O = _;
		const _key = $O[0];
		const id = $O[1];
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
		const $s = before;
		if ($s[0] === 0) {
			release_id(watch, $s[1]);
		}
		return;
	};
	const unmap = (held2) => {
		const $t = get(found.v, held2);
		if ($t[0] === 0) {
			remove(found.v, held2);
			release_id(watch, $t[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $A = _;
			const _held = $A[0];
			const old = $A[1];
			release_id(watch, old);
		}
		found.v = new9();
		let at2 = 0;
		const count2 = peek_storage2(storage, (held2) => {
			return count(held2);
		});
		while (at2 < count2) {
			const $F = key_at(at2);
			if ($F[0] === 0) {
				map2($F[1], at2);
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
			const $G = key_at(index3);
			if ($G[0] === 0) {
				gone2.push($G[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone2;
		return;
	};
	const arrive = (at2, count2) => {
		let index3 = at2;
		while (index3 < at2 + count2) {
			const $H = key_at(index3);
			if ($H[0] === 0) {
				map2($H[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change2) => {
		const $I = change2;
		let $J = null;
		if ($I[0] === 0) {
			const at2 = $I[1];
			const removed3 = $I[2];
			const _inserted = $I[3];
			$J = note(at2, removed3);
		} else if ($I[0] === 3) {
			const at3 = $I[1];
			const count2 = $I[2];
			$J = note(at3, count2);
		} else if ($I[0] === 2) {
			const at4 = $I[1];
			$J = note(at4, 1);
		} else if ($I[0] === 4) {
			const at5 = $I[1];
			const _part = $I[2];
			$J = note(at5, 1);
		} else {
			$J = undefined;
		}
		return $J;
	}, (change2, stage) => {
		const $K = stage;
		let $L = null;
		if ($K[0] === 0) {
			$L = undefined;
		} else if ($K[0] === 3) {
			return;
		} else {
			fill();
			return;
			$L = undefined;
		}
		$L;
		const $M = change2;
		let $N = null;
		if ($M[0] === 0) {
			const at2 = $M[1];
			const _removed = $M[2];
			const inserted = $M[3];
			for (const held2 of leaving.v) {
				unmap(held2);
			}
			arrive(at2, inserted);
			$N = undefined;
		} else if ($M[0] === 1) {
			const at3 = $M[1];
			const count2 = $M[2];
			$N = arrive(at3, count2);
		} else if ($M[0] === 3) {
			const _at = $M[1];
			const _count = $M[2];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			$N = undefined;
		} else if ($M[0] === 2) {
			const at4 = $M[1];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			arrive(at4, 1);
			$N = undefined;
		} else if ($M[0] === 4) {
			const at5 = $M[1];
			const _part = $M[2];
			for (const held5 of leaving.v) {
				unmap(held5);
			}
			arrive(at5, 1);
			$N = undefined;
		} else if ($M[0] === 5) {
			const _from = $M[1];
			const _to = $M[2];
			$N = undefined;
		} else {
			$N = fill();
		}
		return $N;
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
			const $X = __list_get(held2, index2);
			let $Y = null;
			if ($X[0] === 0) {
				const op = $X[1];
				$Y = kept.push(op);
			} else {
				$Y = undefined;
			}
			$Y;
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
function rows(self, $U) {
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
			const id = __remove_at(held2.v, at2, "std/src/reactive/lens.vl:1836:27");
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
			__insert_at(held2.v, index2, id, "std/src/reactive/lens.vl:1848:18");
			arrived.push(id);
			index2 = index2 + 1;
		}
		return arrived;
	};
	held2.v = fill();
	const tap2 = tap(self, (_change) => {
		return;
	}, (change2, _stage) => {
		const $V = change2;
		let $W = null;
		if ($V[0] === 0) {
			const at2 = $V[1];
			const removed3 = $V[2];
			const inserted = $V[3];
			const gone2 = leave(at2, removed3);
			record(log, [ 0, at2, gone2, arrive(at2, inserted) ]);
			$W = undefined;
		} else if ($V[0] === 1) {
			const at3 = $V[1];
			const count2 = $V[2];
			$W = record(log, [ 0, at3, [  ], arrive(at3, count2) ]);
		} else if ($V[0] === 3) {
			const at4 = $V[1];
			const count3 = $V[2];
			$W = record(log, [ 0, at4, leave(at4, count3), [  ] ]);
		} else if ($V[0] === 5) {
			const from = $V[1];
			const to = $V[2];
			const id = __remove_at(held2.v, from, "std/src/reactive/lens.vl:1864:28");
			__insert_at(held2.v, to, id, "std/src/reactive/lens.vl:1865:19");
			record(log, [ 3, from, 1, to ]);
			$W = undefined;
		} else if ($V[0] === 6) {
			for (const id2 of held2.v) {
				release_id(watch, id2);
			}
			held2.v = fill();
			record(log, [ 2, __clone(held2.v) ]);
			$W = undefined;
		} else {
			$W = undefined;
		}
		return $W;
	});
	const feed = [ __clone(self), held2, __clone(log), tap2 ];
	defer_to_owner(() => {
		return release4(feed);
	}, $U);
	return feed;
}
function rows2(self, $T) {
	return rows(self[0], $T);
}
function lens(self) {
	return __clone(self[0]);
}
function slot_of(self, id) {
	const $aa = __clone(self);
	let $Z = null;
	if (id < 0) {
		$Z = -(1);
	} else {
		$Z = id;
	}
	return [ $aa, $Z ];
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
	const $ap = watch.v[2];
	const cell3 = __clone($ap[1]);
	if ($ap[0] === 0) {
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
function track(self, $ag) {
	const scope = $ag;
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
function track2(self, $ag) {
	const scope = $ag;
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
	const $aq = at(self);
	if ($aq[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held2) => {
		return read(held2, $aq[1]);
	});
}
function keys(self) {
	return __clone(self[1].v);
}
function provisional(self, key) {
	const $aw = get(self[0].v, key);
	if ($aw[0] === 0) {

	} else {
		return false;
	}
	let out = false;
	for (const proposal of $aw[1][0]) {
		if (proposal[2]) {
			out = true;
		}
	}
	return out;
}
function map(self, fn) {
	const $ax = self;
	let $ay = null;
	if ($ax[0] === 0) {
		const x2 = $ax[1];
		$ay = [ 0, fn(x2) ];
	} else {
		$ay = [ 1 ];
	}
	return $ay;
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
	const $aA = get(self[0].v, key);
	if ($aA[0] === 0) {

	} else {
		return [ 1 ];
	}
	let out = [ 1 ];
	for (const proposal of $aA[1][0]) {
		if (proposal[0] > $aA[1][1]) {
			out = [ 0, __clone(proposal) ];
		}
	}
	return out;
}
function over3(self, key, held2) {
	const $aB = showing(self, key);
	let $aC = null;
	if ($aB[0] === 1) {
		$aC = __clone(held2);
	} else {
		const proposal = $aB[1];
		const $aD = proposal[1];
		let $aE = null;
		if ($aD[0] === 1) {
			$aE = [ 1 ];
		} else {
			const value = $aD[1];
			const $aF = held2;
			let $aG = null;
			if ($aF[0] === 0 || provisional(self, key)) {
				$aG = [ 0, __clone(value) ];
			} else {
				$aG = [ 1 ];
			}
			$aE = $aG;
		}
		$aC = $aE;
	}
	return $aC;
}
function anchors(self, key) {
	const $aI = get(self[0].v, key);
	let $aJ = null;
	if ($aI[0] === 0) {
		const entry = $aI[1];
		$aJ = [ __clone(entry[2]), __clone(entry[3]) ];
	} else {
		$aJ = [ [ 1 ], [ 1 ] ];
	}
	return $aJ;
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
	const $aL = above;
	let $aM = undefined;
	if ($aL[0] === 0 && (($aM = position_of(rows5, $aL[1])), $aM[0] === 0)) {
		return $aM[1] + 1;
	}
	const $aN = below;
	let $aO = undefined;
	if ($aN[0] === 0 && (($aO = position_of(rows5, $aN[1])), $aO[0] === 0)) {
		return $aO[1];
	}
	const $aP = above;
	if ($aP[0] === 0) {
		return rows5.length;
	}
	return 0;
}
function hides(self, key) {
	const $aQ = showing(self, key);
	if ($aQ[0] === 0) {
		const $aR = $aQ[1][1];
		return $aR[0] === 1;
	}
	return false;
}
function compose(self, with_hidden) {
	const key = self[1];
	let all3 = [  ];
	for (const slot3 of get2(self[4])) {
		const $av = get4(slot3);
		if ($av[0] === 0) {
			all3.push([ key($av[1]), [ 0, __clone(slot3) ] ]);
		}
	}
	for (const row_key of keys(self[2])) {
		let $az = undefined;
		let $aH = undefined;
		if (provisional(self[2], row_key) && (($az = find(self[3], row_key)), $az[0] === 1) && (($aH = over3(self[2], row_key, [ 1 ])), $aH[0] === 0)) {
			const $aK = anchors(self[2], row_key);
			const above = $aK[0];
			const below = $aK[1];
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
	const $bj = self;
	return $bj[0] === 1;
}
function run_body(runs, body) {
	const run = renew(runs);
	const $bs = nursery(run);
	let $bt = null;
	if ($bs[0] === 0) {
		const nursery2 = $bs[1];
		$bt = (($bu) => {
			return (($bv) => {
				return body($bu, $bv);
			})(nursery2);
		})(run);
	} else {
		$bt = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $bt;
}
function last(self) {
	let $bE = null;
	if (is_empty(self)) {
		$bE = [ 1 ];
	} else {
		$bE = __list_get(self, self.length - 1);
	}
	return $bE;
}
function run_once(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body(runs, ($bb, $bc) => {
		return (($bd) => {
			return body($bb, $bd, $bc);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs, tracker, body) {
	let value = run_once(runs, tracker, ($aW, $aX, $aY) => {
		return body($aW, $aX, $aY);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs, tracker, ($bV, $bW, $bX) => {
			return body($bV, $bW, $bX);
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
	const $ce = self;
	let $ch = null;
	if ($ce[0] === 0) {
		const $cf = b2;
		let $cg = null;
		if ($cf[0] === 0) {
			$cg = eq5($ce[1], $cf[1]);
		} else {
			$cg = false;
		}
		$ch = $cg;
	} else {
		const $ci = b2;
		$ch = $ci[0] === 1;
	}
	return $ch;
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
function notify(self, $cm) {
	const traced = tracing.v;
	let $cn = null;
	if (traced) {
		$cn = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$cn = "";
	}
	const cause = $cn;
	const $co = $cm;
	let $cp = null;
	if ($co[0] === 0) {
		const turn = $co[1];
		$cp = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $cq = last(draining_turns.v);
		let $cr = null;
		if ($cq[0] === 0) {
			const draining = $cq[1];
			$cr = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $cs = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$cs = undefined;
				}
				$cs;
			}
			$cr = undefined;
		}
		$cp = $cr;
	}
	return $cp;
}
function set(self, value, $cl, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $cl);
}
function take(self, item2, $cz) {
	defer(self, () => {
		dispose(item2, $cz);
		return;
	});
	return __clone(item2);
}
function cell(self, $aU, $aV, caller) {
	const instance = start2(self);
	const pull = instance[0];
	const cached = new7(pull(), caller);
	const refresh = subscriber_of(() => {
		set(cached, pull(), $aU, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed = instance[1](refresh);
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $aU, $aV);
	return cached;
}
function memo(self, $aS, $aT, caller) {
	return [ cell(self, $aS, $aT, caller) ];
}
function rows3(self, $ab, $ac) {
	const overlay2 = __clone(self);
	const feed = __clone(self[4]);
	const layout = __clone(self[2][5]);
	return memo(distinct(derive(($ad, $ae, $af) => {
		track(feed, $ae);
		track2(layout, $ae);
		return compose(overlay2, false);
	}, "std/src/reactive/lens/loaded.vl:544:3"), "std/src/reactive/lens/loaded.vl:548:6"), $ab, $ac, "std/src/reactive/lens/loaded.vl:548:17");
}
function observe(signal, observer) {
	const cell3 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $cF = [ 0, cell3 ];
		let $cG = null;
		if ($cF[0] === 0) {
			const live = $cF[1];
			$cG = observer(live.v);
		} else {
			$cG = undefined;
		}
		return $cG;
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
function slot(self, at2, $cI) {
	const handle = follow3(self, at2);
	defer_to_owner(() => {
		return release5(handle);
	}, $cI);
	return handle;
}
function slot2(self, at2, $cH) {
	return slot(self[0], at2, $cH);
}
function slot_cell(watch, slot3) {
	if (slot3 < 0) {
		return new7(0, "std/src/reactive/lens.vl:1079:19");
	}
	const entry = as_usize(slot3);
	const $cO = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1082:5")[0];
	const cell3 = __clone($cO[1]);
	if ($cO[0] === 0) {
		return cell3;
	}
	const cell4 = new7(0, "std/src/reactive/lens.vl:1085:25");
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1086:2")[0] = [ 0, __clone(cell4) ];
	return cell4;
}
function identity3(self) {
	return identity2(slot_cell(self[0][0].v[3], self[1]));
}
function on_settle3(self, subscriber) {
	return on_settle(slot_cell(self[0][0].v[3], self[1]), subscriber);
}
function track3(self, $ag) {
	const scope = $ag;
	const identity5 = identity3(self);
	if (admits(scope, identity5)) {
		lists_of(scope[0]).v[0].push([ identity5, (subscriber) => {
			return on_settle3(self, subscriber);
		} ]);
	}
	return get4(self);
}
function row(self, key) {
	const $cR = get(self[3].v, key);
	if ($cR[0] === 0) {
		return $cR[1];
	}
	const made = [ new7(0, "std/src/reactive/lens/loaded.vl:141:44"), new7(false, "std/src/reactive/lens/loaded.vl:141:74") ];
	insert(self[3].v, key, made);
	return made;
}
function run_body2(runs, body) {
	const run = renew(runs);
	const $cS = nursery(run);
	let $cT = null;
	if ($cS[0] === 0) {
		const nursery2 = $cS[1];
		$cT = (($bu) => {
			return (($bv) => {
				return body($bu, $bv);
			})(nursery2);
		})(run);
	} else {
		$cT = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $cT;
}
function run_once2(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body2(runs, ($bb, $bc) => {
		return (($bd) => {
			return body($bb, $bd, $bc);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked2(runs, tracker, body) {
	let value = run_once2(runs, tracker, ($aW, $aX, $aY) => {
		return body($aW, $aX, $aY);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once2(runs, tracker, ($bV, $bW, $bX) => {
			return body($bV, $bW, $bX);
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
	const $cU = self;
	let $cX = null;
	if ($cU[0] === 0) {
		const $cV = b2;
		let $cW = null;
		if ($cV[0] === 0) {
			$cW = eq4($cU[1], $cV[1]);
		} else {
			$cW = false;
		}
		$cX = $cW;
	} else {
		const $cY = b2;
		$cX = $cY[0] === 1;
	}
	return $cX;
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
function set2(self, value, $cl, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $cl);
}
function cell2(self, $aU, $aV, caller) {
	const instance = start4(self);
	const pull = instance[0];
	const cached = new7(pull(), caller);
	const refresh = subscriber_of(() => {
		set2(cached, pull(), $aU, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed = instance[1](refresh);
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $aU, $aV);
	return cached;
}
function memo2(self, $aS, $aT, caller) {
	return [ cell2(self, $aS, $aT, caller) ];
}
function view(self, slot3, $cJ, $cK) {
	const ledger = __clone(self[2]);
	const key = self[1];
	return memo2(distinct(derive(($cL, $cM, $cN) => {
		const $cP = track3(slot3, $cM);
		let $cQ = null;
		if ($cP[0] === 0) {
			const item2 = $cP[1];
			const id = key(item2);
			track2(row(ledger, id)[0], $cM);
			$cQ = over3(ledger, id, [ 0, item2 ]);
		} else {
			$cQ = [ 1 ];
		}
		return $cQ;
	}, "std/src/reactive/lens/loaded.vl:507:3"), "std/src/reactive/lens/loaded.vl:516:6"), $cJ, $cK, "std/src/reactive/lens/loaded.vl:516:17");
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
function batch(body, $dE) {
	const $dF = $dE;
	let $dG = null;
	if ($dF[0] === 0) {
		const current = $dF[1];
		$dG = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$dG = result;
	}
	return $dG;
}
function open2(self, key, value, insert4, above, below, $dn) {
	const id = self[6].v + 1;
	self[6].v = id;
	const ledger = __clone(self);
	batch(($do) => {
		const cells = row(ledger, key);
		const found = get(ledger[0].v, key);
		const $dp = found;
		let $dq = null;
		if ($dp[0] === 0) {
			const held2 = $dp[1];
			$dq = held2;
		} else {
			ledger[1].v.push(__clone(key));
			set2(cells[1], true, [ 0, $do ], "std/src/reactive/lens/loaded.vl:176:20");
			$dq = [ [  ], 0, __clone(above), __clone(below) ];
		}
		let entry = $dq;
		if (insert4) {
			entry[2] = __clone(above);
			entry[3] = __clone(below);
		}
		entry[0].push([ id, __clone(value), insert4 ]);
		insert(ledger[0].v, key, entry);
		insert2(ledger[2].v, id, key);
		set2(ledger[4], get3(ledger[4]) + 1, [ 0, $do ], "std/src/reactive/lens/loaded.vl:187:17");
		notify(cells[0], [ 0, $do ]);
		let $dD = undefined;
		if (insert4 || (($dD = value), $dD[0] === 1)) {
			notify(ledger[5], [ 0, $do ]);
		}
		return;
	}, $dn);
	return id;
}
function propose(self, slot3, value, $dj) {
	const $dm = key_of(self, slot3);
	if ($dm[0] === 0) {

	} else {
		return [ 1 ];
	}
	return [ 0, open2(self[2], $dm[1], [ 0, __clone(value) ], false, [ 1 ], [ 1 ], $dj) ];
}
function unwrap(self, caller) {
	const $dH = self;
	let $dI = null;
	if ($dH[0] === 0) {
		const x2 = $dH[1];
		$dI = x2;
	} else {
		$dI = (() => {
			throw __panic("expected Some but got None", caller);
		})();
	}
	return $dI;
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
		const $dP = row2[1];
		if ($dP[0] === 0) {
			at2 = at2 + 1;
		}
	}
	return at2;
}
function remove2(self, key) {
	__nmap_remove(self[0], hash2(key));
}
function close2(self, id, accepted, $dQ) {
	const $dR = get6(self[2].v, id);
	let $dS = null;
	if ($dR[0] === 0) {
		const held2 = $dR[1];
		$dS = held2;
	} else {
		return [ 1 ];
	}
	const key = $dS;
	const found = get(self[0].v, key);
	const $dT = found;
	if ($dT[0] === 0) {

	} else {
		return [ 1 ];
	}
	let entry = __clone($dT[1]);
	let ended = [ 1 ];
	let kept = [  ];
	for (const proposal of entry[0]) {
		if (proposal[0] === id) {
			ended = [ 0, __clone(proposal) ];
		} else {
			kept.push(__clone(proposal));
		}
	}
	const $dU = ended;
	if ($dU[0] === 0) {

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
	batch(($dV) => {
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
			set2(cells[1], false, [ 0, $dV ], "std/src/reactive/lens/loaded.vl:235:19");
		} else {
			insert(ledger[0].v, key, entry);
		}
		set2(ledger[4], get3(ledger[4]) - 1, [ 0, $dV ], "std/src/reactive/lens/loaded.vl:239:17");
		notify(cells[0], [ 0, $dV ]);
		let $dW = undefined;
		if ($dU[1][2] || (($dW = $dU[1][1]), $dW[0] === 1)) {
			notify(ledger[5], [ 0, $dV ]);
		}
		return;
	}, $dQ);
	return [ 0, [ __clone(key), __clone($dU[1][1]), $dU[1][2], lands ] ];
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
		__remove_at(self, at2, "std/src/reactive/lens.vl:2228:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at2 + offset, __clone(value), "std/src/reactive/lens.vl:2233:9");
		offset = offset + 1;
	}
}
function mutate(self, change2) {
	const $ej = self[0].v[0];
	let $ek = null;
	if ($ej[0] === 0) {
		const cell3 = $ej[1];
		$ek = change2(cell3.v);
	} else {
		const port = __clone($ej[1]);
		$ek = port[1](change2);
	}
	return $ek;
}
function wake_all(watch, slot3, $eM) {
	const held2 = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot3), "std/src/reactive/lens.vl:1117:36"));
	});
	const $eN = held2[0];
	if ($eN[0] === 0) {
		notify($eN[1], $eM);
	}
	for (const part of held2[1]) {
		notify(part[1], $eM);
	}
}
function wake_part(watch, slot3, name, $eO) {
	const held2 = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot3), "std/src/reactive/lens.vl:1127:36"));
	});
	const $eP = held2[0];
	if ($eP[0] === 0) {
		notify($eP[1], $eO);
	}
	for (const part of held2[1]) {
		if (part[0] === name) {
			notify(part[1], $eO);
		}
	}
}
function landed(self, changes, late, $em) {
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
		const $en = change2;
		let $eo = null;
		if ($en[0] === 0) {
			const _at = $en[1];
			const removed3 = $en[2];
			const inserted = $en[3];
			shaped = true;
			if (removed3 !== inserted) {
				resized = true;
			}
			$eo = undefined;
		} else if ($en[0] === 1) {
			const _at2 = $en[1];
			const _count = $en[2];
			$eo = resized = true;
		} else if ($en[0] === 3) {
			const _at3 = $en[1];
			const _count2 = $en[2];
			$eo = resized = true;
		} else if ($en[0] === 6) {
			$eo = resized = true;
		} else if ($en[0] === 5) {
			const _from = $en[1];
			const _to = $en[2];
			$eo = shaped = true;
		} else {
			$eo = undefined;
		}
		$eo;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change2, wakes);
		}
		let $eE = null;
		if (self[0].v[4].length > 0) {
			let $eD = null;
			if (!(late)) {
				$eD = [ 0 ];
			} else if (!(several)) {
				$eD = [ 1 ];
			} else if (index2 === 0) {
				$eD = [ 2 ];
			} else {
				$eD = [ 3 ];
			}
			const stage = $eD;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$eE = undefined;
		}
		$eE;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $eF = null;
	if (shaped || resized) {
		$eF = watch.v[2];
	} else {
		$eF = [ 1 ];
	}
	const shape = __clone($eF);
	let $eG = null;
	if (resized) {
		$eG = watch.v[3];
	} else {
		$eG = [ 1 ];
	}
	const sized = __clone($eG);
	let $eH = undefined;
	let $eI = undefined;
	if (wakes.length === 0 && (($eH = shape), $eH[0] === 1) && (($eI = sized), $eI[0] === 1)) {
		notify(published, $em);
		return;
	}
	settle_together(($eJ) => {
		for (const wake2 of wakes) {
			const $eK = wake2;
			let $eL = null;
			if ($eK[0] === 0) {
				const slot3 = $eK[1];
				$eL = wake_all(watch, slot3, [ 0, $eJ ]);
			} else if ($eK[0] === 2) {
				const slot4 = $eK[1];
				$eL = wake_all(watch, slot4, [ 0, $eJ ]);
			} else {
				const slot5 = $eK[1];
				const part = $eK[2];
				$eL = wake_part(watch, slot5, part, [ 0, $eJ ]);
			}
			$eL;
		}
		const $eQ = sized;
		if ($eQ[0] === 0) {
			notify($eQ[1], [ 0, $eJ ]);
		}
		const $eR = shape;
		if ($eR[0] === 0) {
			notify($eR[1], [ 0, $eJ ]);
		}
		notify(published, [ 0, $eJ ]);
		return;
	}, $em);
}
function landed_one(self, change2, $el) {
	landed(self, [ __clone(change2) ], false, $el);
}
function splice2(self, at2, removed3, inserted, $eg) {
	const held2 = size(self);
	let $eh = null;
	if (at2 > held2) {
		$eh = held2;
	} else {
		$eh = at2;
	}
	const start5 = $eh;
	let $ei = null;
	if (start5 + removed3 > held2) {
		$ei = held2 - start5;
	} else {
		$ei = removed3;
	}
	const taking = $ei;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start5, taking, arriving ];
	coming_one(self, change2);
	mutate(self, (held3) => {
		return splice(held3, start5, taking, inserted);
	});
	landed_one(self, change2, $eg);
}
function splice3(self, at2, removed3, inserted, $ef) {
	splice2(self[0], at2, removed3, inserted, $ef);
}
function remove_at(self, at2, $ee) {
	return splice3(self, at2, 1, [  ], $ee);
}
function and_then(self, fn) {
	const $eZ = self;
	let $fa = null;
	if ($eZ[0] === 0) {
		const x2 = $eZ[1];
		$fa = fn(x2);
	} else {
		$fa = [ 1 ];
	}
	return $fa;
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
function replace(self, at2, value, $fe) {
	const watch = self[0].v[3];
	let $ff = null;
	if (has(self, at2)) {
		$ff = [ 2, __clone(at2) ];
	} else if (viewing2(watch, (held2) => {
		return follows_items(held2[0]);
	})) {
		return false;
	} else {
		$ff = [ 1, __clone(at2), 1 ];
	}
	const change2 = $ff;
	coming_one(self, change2);
	const write = self[0].v[1][3];
	mutate(self, (held2) => {
		return write(held2, at2, value);
	});
	landed_one(self, change2, $fe);
	return true;
}
function replace2(self, at2, value, $fd) {
	return replace(self[0], at2, value, $fd);
}
function insert_at(self, at2, value, $fg) {
	return splice3(self, at2, 0, [ __clone(value) ], $fg);
}
function answer(self, id, accepted, reply, $dL) {
	const overlay2 = __clone(self);
	let answered = false;
	batch(($dM) => {
		const $dN = get6(overlay2[2][2].v, id);
		let $dO = null;
		if ($dN[0] === 0) {
			const key = $dN[1];
			$dO = landing(overlay2, key);
		} else {
			$dO = 0;
		}
		const landing2 = $dO;
		const $dX = close2(overlay2[2], id, accepted, [ 0, $dM ]);
		if ($dX[0] === 0) {

		} else {
			return;
		}
		answered = true;
		if (!($dX[1][3])) {
			return;
		}
		const $dY = reply;
		let $dZ = null;
		if ($dY[0] === 0) {
			const item2 = $dY[1];
			$dZ = [ 0, __clone(item2) ];
		} else {
			$dZ = $dX[1][1];
		}
		const landed2 = __clone($dZ);
		const lens2 = __clone(overlay2[0]);
		const $ea = landed2;
		let $eb = null;
		if ($ea[0] === 1) {
			const $ec = held(overlay2, $dX[1][0]);
			let $ed = undefined;
			if ($ec[0] === 0 && (($ed = at($ec[1])), $ed[0] === 0)) {
				remove_at(lens2, $ed[1], [ 0, $dM ]);
			}
			$eb = undefined;
		} else {
			const item3 = $ea[1];
			const server_key = overlay2[1](item3);
			const $eX = held(overlay2, $dX[1][0]);
			let $eY = null;
			if ($eX[0] === 0) {
				$eY = at($eX[1]);
			} else {
				$eY = and_then(held(overlay2, server_key), (slot3) => {
					return at(slot3);
				});
			}
			const target = $eY;
			const $fb = target;
			let $fc = null;
			if ($fb[0] === 0) {
				const at2 = $fb[1];
				if (!(eq9(peek2(lens2, (list) => {
					return __list_get(list, at2);
				}), [ 0, __clone(item3) ]))) {
					replace2(lens2, at2, item3, [ 0, $dM ]);
				}
				$fc = undefined;
			} else {
				if ($dX[1][2]) {
					insert_at(lens2, landing2, item3, [ 0, $dM ]);
				}
				$fc = undefined;
			}
			$eb = $fc;
		}
		return $eb;
	}, $dL);
	return answered;
}
function reject(self, id, $dK) {
	return answer(self, id, false, [ 1 ], $dK);
}
function shown_rows(self) {
	return compose(self, false);
}
function propose_insert(self, at2, value, $fh) {
	const key = self[1](value);
	const $fi = held(self, key);
	if ($fi[0] === 0) {
		return open2(self[2], key, [ 0, __clone(value) ], false, [ 1 ], [ 1 ], $fh);
	}
	const shown2 = shown_rows(self);
	let $fj = null;
	if (at2 > 0 && at2 <= shown2.length) {
		$fj = [ 0, __clone(__at(shown2, at2 - 1, "std/src/reactive/lens/loaded.vl:403:53")[0]) ];
	} else {
		$fj = [ 1 ];
	}
	const above = $fj;
	const below = map(__list_get(shown2, at2), (row2) => {
		return __clone(row2[0]);
	});
	return open2(self[2], key, [ 0, __clone(value) ], true, above, below, $fh);
}
function shown(self) {
	let out = [  ];
	for (const row2 of compose(self, false)) {
		const $fm = row2[1];
		let $fn = null;
		if ($fm[0] === 0) {
			const slot3 = $fm[1];
			$fn = get4(slot3);
		} else {
			$fn = [ 1 ];
		}
		const held2 = $fn;
		const $fo = over3(self[2], row2[0], held2);
		if ($fo[0] === 0) {
			out.push($fo[1]);
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
			const $fp = item(held2, at2);
			if ($fp[0] === 0) {
				out.push($fp[1]);
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
function track4(self, $ag) {
	const scope = $ag;
	const identity5 = identity4(self);
	if (admits(scope, identity5)) {
		lists_of(scope[0]).v[0].push([ identity5, (subscriber) => {
			return on_settle4(self, subscriber);
		} ]);
	}
	return get7(self);
}
function view_key(self, key, $fx, $fy) {
	const ledger = __clone(self[2]);
	const index2 = __clone(self[3]);
	const shape = __clone(self[5]);
	return memo2(distinct(derive(($fz, $fA, $fB) => {
		track2(row(ledger, key)[0], $fA);
		track4(shape, $fA);
		const $fC = find(index2, key);
		let $fD = null;
		if ($fC[0] === 0) {
			const slot3 = $fC[1];
			$fD = track3(slot3, $fA);
		} else {
			$fD = [ 1 ];
		}
		const held2 = $fD;
		return over3(ledger, key, held2);
	}, "std/src/reactive/lens/loaded.vl:525:3"), "std/src/reactive/lens/loaded.vl:533:6"), $fx, $fy, "std/src/reactive/lens/loaded.vl:533:17");
}
function is_pending_key(self, key) {
	return [ __clone(row(self[2], key)[1]) ];
}
function shift(self, from, to) {
	const item2 = __remove_at(self, from, "std/src/reactive/lens.vl:2249:19");
	__insert_at(self, to, item2, "std/src/reactive/lens.vl:2250:8");
}
function move_item(self, from, to, $fH) {
	const held2 = size(self);
	if (from >= held2 || from === to) {
		return;
	}
	let $fI = null;
	if (to >= held2) {
		$fI = held2 - 1;
	} else {
		$fI = to;
	}
	const landing2 = $fI;
	const change2 = [ 5, from, landing2 ];
	coming_one(self, change2);
	mutate(self, (held3) => {
		return shift(held3, from, landing2, $fH);
	});
	landed_one(self, change2, $fH);
}
function move_item2(self, from, to, $fG) {
	return move_item(self[0], from, to, $fG);
}
function propose_delete(self, slot3, $fJ) {
	const $fK = key_of(self, slot3);
	if ($fK[0] === 0) {

	} else {
		return [ 1 ];
	}
	return [ 0, open2(self[2], $fK[1], [ 1 ], false, [ 1 ], [ 1 ], $fJ) ];
}
function confirm(self, id, $fL) {
	return answer(self, id, true, [ 1 ], $fL);
}
function pending(self) {
	return [ __clone(self[2][4]) ];
}
function size2(self) {
	return size(self[0]);
}
function push(self, value, $fM) {
	return splice3(self, size2(self), 0, [ __clone(value) ], $fM);
}
function confirm_with(self, id, item2, $fR) {
	return answer(self, id, true, [ 0, __clone(item2) ], $fR);
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
on_change(__clone(rows4), (_rows, $cD) => {
	const $cE = [ rows_runs, "v" ];
	return rows_runs.v = $cE[0][$cE[1]] + 1;
}, "lens-overlay.vl:43:19");
const b = slot2(board, 1, [ 1 ]);
const b_view = view(overlay, b, [ 1 ], [ 1 ]);
const b_runs = __shared_new(0);
on_change2(__clone(b_view), (_item, $df) => {
	const $dg = [ b_runs, "v" ];
	return b_runs.v = $dg[0][$dg[1]] + 1;
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
on_change2(__clone(x_view), (_item, $fE) => {
	const $fF = [ x_runs, "v" ];
	return x_runs.v = $fF[0][$fF[1]] + 1;
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
