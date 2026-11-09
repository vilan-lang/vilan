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
async function __guarded_async(body) {
	try {
		return [ 0, await body() ];
	} catch (error) {
		let reason = error && error.message ? error.message : String(error);
		if (error && error.cause && error.cause.message) {
			reason = reason + ": " + error.cause.message;
		}
		return [ 1, reason ];
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
function __nmap_get(table, hash) {
	const entry = table[0].get(hash);
	return entry !== undefined && entry[1][0] === 0 ? [ 0, __clone(entry[1][1]) ] : [ 1 ];
}
function __nmap_has(table, hash) {
	if (hash !== table[1]) return table[0].has(hash);
	const entry = table[0].get(hash);
	return entry !== undefined && entry[1][0] === 0;
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
function __nursery_of(option) {
	return option[0] === 0 ? option[1] : undefined;
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
function __sleep(ms, signal) {
	const sig = signal && signal[0] === 0 ? signal[1] : undefined;
	return new Promise((resolve, reject) => {
		if (sig && sig.aborted) {
			reject(sig.reason);
			return;
		}
		const timer = setTimeout(() => resolve(), ms);
		if (sig) sig.addEventListener("abort", () => {
			clearTimeout(timer);
			reject(sig.reason);
		}, { once: true });
	});
}
class __Task {
	constructor(run, origin, nursery) {
		this.origin = origin;
		this.observed = false;
		this.nursery = nursery;
		this.owned = !!nursery;
		this.rejected = false;
		this.error = undefined;
		this.promise = run();
		this.promise.then(null, (error) => {
			this.rejected = true;
			this.error = error;
			if (this.owned && !__nursery_is_cancel(error)) this.nursery.__fail(this);
			if (!this.observed && !this.owned) {
				globalThis.setTimeout(() => {
					if (!this.observed) console.error("unhandled task error (spawned in " + this.origin + "): " + String(error));
				}, 0);
			}
		});
		if (nursery) nursery.children.push(this);
	}
	then(onFulfilled, onRejected) {
		this.observed = true;
		return this.promise.then(onFulfilled, onRejected);
	}
}
function __task(run, origin, nursery) {
	return new __Task(run, origin, nursery);
}
function __with_finally(body, after) {
	try {
		body();
	} finally {
		after();
	}
}
function eq(self, b) {
	if (self.length !== b.length) {
		return false;
	}
	let index2 = 0;
	for (const item2 of self) {
		if (!(item2 === __at(b, index2, "std/src/compare.vl:147:17"))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function hash(self) {
	return __hash(self);
}
function new2() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function assert(condition, message, caller) {
	let $cM = null;
	if (!(condition)) {
		$cM = (() => {
			throw __panic(message, caller);
		})();
	}
	return $cM;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $q = null;
	if (wrapped < 0) {
		$q = wrapped + modulus;
	} else {
		$q = wrapped;
	}
	return $q;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $p = null;
	if (truncated > 0) {
		$p = truncated;
	} else {
		$p = 0;
	}
	return $p;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $r = null;
	if (wrapped >= half) {
		$r = wrapped - modulus;
	} else {
		$r = wrapped;
	}
	return $r;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function eq2(self, b) {
	return self === b;
}
function eq3(self, b) {
	const $dh = self;
	let $dk = null;
	if ($dh[0] === 0) {
		const $di = b;
		let $dj = null;
		if ($di[0] === 0) {
			$dj = $dh[1] === $di[1];
		} else {
			$dj = false;
		}
		$dk = $dj;
	} else {
		const $dl = b;
		$dk = $dl[0] === 1;
	}
	return $dk;
}
function as_derivation() {
	minting_derivation.v = true;
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
		const key = hash(subscriber[0]);
		let $ak = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$ak = undefined;
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
		$ak;
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
	const $fp = turn;
	let $fq = null;
	if ($fp[0] === 0) {
		const ambient = $fp[1];
		$fq = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $fr = last(draining_turns.v);
		let $fs = null;
		if ($fr[0] === 0) {
			const draining = $fr[1];
			$fs = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$fs = undefined;
		}
		$fq = $fs;
	}
	return $fq;
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
function retiring(subscriber, release7) {
	return detached(subscriber[0], subscriber[2], release7);
}
function also_retiring(handle, subscriber) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		subscriber[2].v = false;
		const $fN = previous;
		let $fO = null;
		if ($fN[0] === 0) {
			const release7 = $fN[1];
			$fO = release7();
		} else {
			$fO = undefined;
		}
		return $fO;
	} ];
}
function detached(id, live, release7) {
	let subscribers = [  ];
	const empty = __shared_new(subscribers);
	return [ empty, id, live, __shared_new([ 0, release7 ]) ];
}
function dispose(self, $gc) {
	const $gd = $gc;
	let $ge = null;
	if ($gd[0] === 0) {
		const established = $gd[1];
		$ge = [ 0, established ];
	} else {
		$ge = last(draining_turns.v);
	}
	const ambient = $ge;
	release_under(self, ambient);
}
function detach(handle) {
	const $fw = last(releasing_turns.v);
	let $fx = null;
	if ($fw[0] === 0) {
		const at_release = $fw[1];
		$fx = at_release;
	} else {
		$fx = last(draining_turns.v);
	}
	const turn = $fx;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $fy = [ 0, handle[0] ];
	let $fz = null;
	if ($fy[0] === 0) {
		const subscribers = $fy[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$fz = undefined;
	} else {
		$fz = undefined;
	}
	$fz;
	const $fA = ambient;
	let $fB = null;
	if ($fA[0] === 0) {
		const turn = $fA[1];
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
		$fB = undefined;
	} else {
		$fB = undefined;
	}
	$fB;
	const $fC = handle[3].v;
	let $fD = null;
	if ($fC[0] === 0) {
		const release7 = $fC[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release7, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$fD = undefined;
	} else {
		$fD = undefined;
	}
	return $fD;
}
function new4() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $T = null;
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
		$T = undefined;
	}
	return $T;
}
function renew(self) {
	const $eP = self[0].v[3];
	let $eQ = null;
	if ($eP[0] === 0) {
		const nursery2 = __clone($eP[1]);
		let $eR = null;
		if (has_spawned(nursery2)) {
			$eR = [ 1 ];
		} else {
			$eR = [ 0, nursery2 ];
		}
		$eQ = $eR;
	} else {
		$eQ = [ 1 ];
	}
	const carried = $eQ;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $fc = null;
	if (is_disposed(self)) {
		$fc = [ 1 ];
	} else {
		$fc = self[0].v[3];
	}
	return $fc;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $fb = null;
	if (!(is_disposed(self))) {
		const held3 = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $eS = held3[3];
		let $eT = null;
		if ($eS[0] === 0) {
			const nursery2 = $eS[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$eT = undefined;
		} else {
			$eT = undefined;
		}
		$eT;
		let $fa = null;
		if (held3[2]) {
			let failure = [ 1 ];
			for (const cleanup of held3[1].v) {
				const $eV = __guarded(cleanup);
				let $eW = null;
				if ($eV[0] === 0) {
					const message = $eV[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$eW = undefined;
				} else {
					$eW = undefined;
				}
				$eW;
			}
			const $eY = failure;
			let $eZ = null;
			if ($eY[0] === 0) {
				const message2 = $eY[1];
				$eZ = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$eZ = undefined;
			}
			$fa = $eZ;
		}
		$fb = $fa;
	}
	return $fb;
}
function get_owner($iB) {
	return $iB;
}
function register_with_owner(subscription, $fX, $fY) {
	const $fZ = $fY;
	let $ga = null;
	if ($fZ[0] === 0) {
		const owner = $fZ[1];
		$ga = take(owner, subscription, $fX);
	} else {
		$ga = __clone(subscription);
	}
	return $ga;
}
function defer_to_owner(cleanup, $Q) {
	const $R = $Q;
	let $S = null;
	if ($R[0] === 0) {
		const owner = $R[1];
		$S = defer(owner, cleanup);
	} else {
		$S = undefined;
	}
	return $S;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker(site) {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ], site ]) ];
}
function lists_of(tracker) {
	const $eq = tracker[0].v[6];
	let $er = null;
	if ($eq[0] === 0) {
		const lists = $eq[1];
		$er = lists;
	} else {
		trackers_allocated_count.v = trackers_allocated_count.v + 1;
		const made = __shared_new([ [  ], [  ], [  ] ]);
		tracker[0].v[6] = [ 0, made ];
		$er = made;
	}
	return $er;
}
function has_run(tracker) {
	return tracker[0].v[0] > 0;
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $eK = tracker[0].v[6];
	let $eL = null;
	if ($eK[0] === 0) {
		const lists = $eK[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$eL = undefined;
	} else {
		$eL = undefined;
	}
	$eL;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $fh = tracker[0].v[6];
	let $fi = null;
	if ($fh[0] === 0) {
		const lists = $fh[1];
		$fi = lists;
	} else {
		return;
		$fi = undefined;
	}
	const lists2 = $fi;
	const $fj = tracker[0].v[5];
	let $fk = null;
	if ($fj[0] === 0) {
		const target = __clone($fj[1]);
		$fk = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$fk = undefined;
	}
	$fk;
	if (!(is_empty(lists2.v[0]))) {
		lists2.v[0] = [  ];
	}
}
function reconnect(tracker, lists, target) {
	if (is_empty(lists.v[0]) && is_empty(lists.v[2])) {
		return;
	}
	const held3 = __clone(lists.v[2]);
	const reading = __clone(lists.v[0]);
	let kept = [  ];
	for (const _edge of held3) {
		kept.push(false);
	}
	let next = [  ];
	tracker[0].v[3] = true;
	let position = 0;
	for (const dependency of reading) {
		const $fn = reusable(held3, kept, dependency[0], position);
		let $fo = null;
		if ($fn[0] === 0) {
			const index2 = $fn[1];
			__at_put(kept, index2, true, "std/src/reactive.vl:1715:5");
			next.push(__clone(__at(held3, index2, "std/src/reactive.vl:1716:15")));
			$fo = undefined;
		} else {
			next.push([ __clone(dependency[0]), dependency[1](relay_for(tracker, target)) ]);
			$fo = undefined;
		}
		$fo;
		position = position + 1;
	}
	lists.v[2] = next;
	let index3 = 0;
	for (const edge of held3) {
		if (!(__at(kept, index3, "std/src/reactive.vl:1730:7"))) {
			detach(edge[1]);
		}
		index3 = index3 + 1;
	}
	tracker[0].v[3] = false;
}
function reusable(held3, kept, identity3, position) {
	const $fl = identity3;
	let $fm = null;
	if ($fl[0] === 0) {
		const wanted = $fl[1];
		if (position < held3.length && !(__at(kept, position, "std/src/reactive.vl:1753:9")) && same_identity(__at(held3, position, "std/src/reactive.vl:1754:22")[0], wanted)) {
			return [ 0, position ];
		}
		let index2 = 0;
		while (index2 < held3.length) {
			if (!(__at(kept, index2, "std/src/reactive.vl:1759:9")) && same_identity(__at(held3, index2, "std/src/reactive.vl:1759:38")[0], wanted)) {
				return [ 0, index2 ];
			}
			index2 = index2 + 1;
		}
		$fm = [ 1 ];
	} else {
		$fm = [ 1 ];
	}
	return $fm;
}
function same_identity(identity3, wanted) {
	const $eo = identity3;
	let $ep = null;
	if ($eo[0] === 0) {
		const held3 = $eo[1];
		$ep = held3 === wanted;
	} else {
		$ep = false;
	}
	return $ep;
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
	const $ft = tracker[0].v[7];
	let $fu = null;
	if ($ft[0] === 0) {
		const site = __clone($ft[1]);
		$fu = place_subscriber(relay[0], site);
	} else {
		$fu = undefined;
	}
	$fu;
	return relay;
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $fH = tracker[0].v[6];
	let $fI = null;
	if ($fH[0] === 0) {
		const lists = $fH[1];
		$fI = lists;
	} else {
		return;
		$fI = undefined;
	}
	const lists2 = $fI;
	let $fJ = null;
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
		$fJ = undefined;
	}
	return $fJ;
}
function forget_reads(tracker) {
	const $fK = tracker[0].v[6];
	let $fL = null;
	if ($fK[0] === 0) {
		const lists = $fK[1];
		$fL = lists;
	} else {
		return;
		$fL = undefined;
	}
	const lists2 = $fL;
	let $fM = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$fM = undefined;
	}
	$fM;
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
	const $ek = tracker[0].v[6];
	let $el = null;
	if ($ek[0] === 0) {
		const lists = $ek[1];
		$el = lists;
	} else {
		return true;
		$el = undefined;
	}
	const lists2 = $el;
	const $em = identity3;
	let $en = null;
	if ($em[0] === 0) {
		const wanted = $em[1];
		const count2 = lists2.v[0].length;
		let index2 = 0;
		while (index2 < count2) {
			if (same_identity(__at(lists2.v[0], index2, "std/src/reactive.vl:1877:22")[0], wanted)) {
				return false;
			}
			index2 = index2 + 1;
		}
		$en = true;
	} else {
		$en = true;
	}
	return $en;
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
function also_releasing(handle, release7) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release7();
		const $fV = previous;
		let $fW = null;
		if ($fV[0] === 0) {
			const earlier = $fV[1];
			$fW = earlier();
		} else {
			$fW = undefined;
		}
		return $fW;
	} ];
}
function placed_at(subscriber, site) {
	place_subscriber(subscriber[0], site);
	return subscriber;
}
function settle_together(body, $bY) {
	const $bZ = $bY;
	let $ca = null;
	if ($bZ[0] === 0) {
		const current2 = $bZ[1];
		$ca = body(current2);
	} else {
		const $cb = last(draining_turns.v);
		let $cc = null;
		if ($cb[0] === 0) {
			const draining = $cb[1];
			$cc = body(draining);
		} else {
			$cc = batch(body, $bY);
		}
		$ca = $cc;
	}
	return $ca;
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
	const held3 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:239:14").length;
	while (index2 < held3) {
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
	const $bt = seat(self, at2);
	const place = $bt[0];
	const index2 = $bt[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:265:15");
	let $bu = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:269:5"), index2, -(1), "std/src/reactive/lens.vl:269:35");
			made = made + 1;
		}
		$bu = undefined;
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
		const held3 = before.length;
		while (copied < held3) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:287:16"));
			copied = copied + 1;
		}
		__at_put(self[0], as_usize(block), grown, "std/src/reactive/lens.vl:290:4");
	}
	$bu;
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
		const $br = seat(self, at2);
		const place = $br[0];
		const index2 = $br[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:306:16");
		const held3 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:307:15").length;
		let $bs = null;
		if (left < held3 - index2) {
			$bs = left;
		} else {
			$bs = held3 - index2;
		}
		const take2 = $bs;
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
	const held3 = all3.length;
	while (index2 < held3) {
		const fresh = open(self, next);
		let filled = 0;
		while (filled < block_size && index2 < held3) {
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
	const $s = seat(self, at2);
	const place = $s[0];
	const index2 = $s[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:371:15");
	const held3 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:372:14"), index2, "std/src/reactive/lens.vl:372:14");
	if (held3 !== -(1)) {
		return held3;
	}
	const slot3 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index2);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:379:3"), index2, slot3, "std/src/reactive/lens.vl:379:3");
	return slot3;
}
function apply2(self, change2, wakes) {
	const $bp = change2;
	let $bq = null;
	if ($bp[0] === 0) {
		const at2 = $bp[1];
		const removed = $bp[2];
		const inserted = $bp[3];
		close(self, at2, removed, wakes);
		widen(self, at2, inserted);
		$bq = undefined;
	} else if ($bp[0] === 1) {
		const at3 = $bp[1];
		const count2 = $bp[2];
		$bq = widen(self, at3, count2);
	} else if ($bp[0] === 3) {
		const at4 = $bp[1];
		const count3 = $bp[2];
		$bq = close(self, at4, count3, wakes);
	} else if ($bp[0] === 2) {
		const at5 = $bp[1];
		let $bw = null;
		if (at5 < self[6]) {
			const $bv = seat(self, at5);
			const place = $bv[0];
			const index2 = $bv[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:396:29")), "std/src/reactive/lens.vl:396:17"), index2, "std/src/reactive/lens.vl:396:17");
			if (slot3 !== -(1)) {
				wakes.push([ 0, slot3 ]);
			}
			$bw = undefined;
		}
		$bq = $bw;
	} else if ($bp[0] === 4) {
		const at6 = $bp[1];
		const part = $bp[2];
		let $by = null;
		if (at6 < self[6]) {
			const $bx = seat(self, at6);
			const place2 = $bx[0];
			const index3 = $bx[1];
			const slot4 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:405:29")), "std/src/reactive/lens.vl:405:17"), index3, "std/src/reactive/lens.vl:405:17");
			if (slot4 !== -(1)) {
				wakes.push([ 1, slot4, part ]);
			}
			$by = undefined;
		}
		$bq = $by;
	} else if ($bp[0] === 5) {
		const from = $bp[1];
		const to = $bp[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $bz = null;
		if (from > to) {
			$bz = from;
		} else {
			$bz = to;
		}
		const far = $bz;
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
		const $bA = seat(self, from);
		const left = $bA[0];
		const index4 = $bA[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:429:18");
		const slot5 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:430:16"), index4, "std/src/reactive/lens.vl:430:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:432:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:433:17");
		} else {
			renumber(self, source, index4);
		}
		restart(self);
		let $bB = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$bB = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:441:25")), "std/src/reactive/lens.vl:441:13").length ];
		} else {
			$bB = seat(self, to);
		}
		const $bC = $bB;
		const place3 = $bC[0];
		const landing2 = $bC[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:445:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:446:5"), landing2, slot5, "std/src/reactive/lens.vl:446:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing2);
		restart(self);
		$bq = undefined;
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
		$bq = undefined;
	}
	return $bq;
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
function eq4(self, other) {
	const $dT = [ self, other ];
	let $dU = null;
	if ($dT[0][0] === 0 && $dT[1][0] === 0) {
		$dU = true;
	} else if ($dT[0][0] === 1 && $dT[1][0] === 1) {
		$dU = true;
	} else if ($dT[0][0] === 2 && $dT[1][0] === 2) {
		$dU = true;
	} else if ($dT[0][0] === 3 && $dT[1][0] === 3) {
		const mine = $dT[0][1];
		const theirs = $dT[1][1];
		$dU = mine === theirs;
	} else {
		$dU = false;
	}
	return $dU;
}
function cancel(task) {
	const $aq = task;
	if ($aq[0] === 0) {
		$aq[1].cancel();
	}
}
function new7(task, $iI, $iJ) {
	const source = [ new8([ 0 ], "std/src/reactive/transient.vl:427:23") ];
	__task(async () => {
		const outcome = await (__guarded_async(async () => {
			return await (task);
		}));
		const $iK = outcome;
		let $iL = null;
		if ($iK[0] === 0) {
			const answered = $iK[1];
			const $iM = answered;
			let $iN = null;
			if ($iM[0] === 0) {
				const value = $iM[1];
				$iN = set3(source[0], [ 1, __clone(value) ], $iI, "std/src/reactive/transient.vl:433:35");
			} else {
				const error = $iM[1];
				$iN = set3(source[0], [ 3, __clone(error), [ 1 ] ], $iI, "std/src/reactive/transient.vl:434:36");
			}
			$iL = $iN;
		} else {
			const message = $iK[1];
			$iL = (() => {
				throw __panic(message, "std/src/reactive/transient.vl:436:25");
			})();
		}
		return $iL;
	}, "new", __nursery_of($iJ));
	return source;
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function ambient_signal($ax) {
	const $ay = $ax;
	let $az = null;
	if ($ay[0] === 0) {
		const n = $ay[1];
		$az = [ 0, n.signal_of() ];
	} else {
		$az = [ 1 ];
	}
	return $az;
}
function detached_nursery() {
	return __nursery_new_detached();
}
async function sleep(ms, $av) {
	await (__sleep(clamp_delay(ms), ambient_signal($av)));
}
function clamp_delay(ms) {
	let $aw = null;
	if (ms < 0) {
		$aw = 0;
	} else {
		$aw = ms;
	}
	return $aw;
}
function m(id, text) {
	return [ id, text ];
}
function held(feed) {
	let out = [  ];
	for (const item2 of all2(items(feed))) {
		out.push(item2[0]);
	}
	return out;
}
function ready(state) {
	const $cL = state;
	return $cL[0] === 1;
}
function server() {
	return [ __shared_new(new2()), __shared_new(new2()), __shared_new(new2()) ];
}
function release3(self, call) {
	insert(self[0].v, call, true);
}
function polled(self, call) {
	return unwrap_or(get(self[1].v, call), __lazy("fallback", () => {
		return 0;
	}));
}
function done(self, call) {
	return contains_key(self[2].v, call);
}
async function search(calls, asked, $hr) {
	return await (answer(calls, 100 + asked, [ 0, [ m(asked, "result for " + asked), m(100, "shared row") ] ], $hr));
}
async function until(what, ready2, $cN) {
	let ticks2 = 0;
	while (!(ready2())) {
		if (ticks2 > 5000) {
			(() => {
				throw __panic("never happened: " + what, "lens-loaded-tasks.vl:92:4");
			})();
		}
		await (sleep(1, $cN));
		ticks2 = ticks2 + 1;
	}
}
async function ticks(count2, $cS) {
	let passed = 0;
	while (passed < count2) {
		await (sleep(1, $cS));
		passed = passed + 1;
	}
}
function eq5(self, other) {
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
function new8(value, caller) {
	let subscribers = [  ];
	const cell2 = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell2[0]), caller);
	}
	return cell2;
}
function lens_over(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new8(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over(value, handler, access) {
	return lens_over([ 0, __shared_new(value) ], handler, access);
}
function sequence(value, $d) {
	return over(value, new5(), [ (held3, at2) => {
		return at2 < count(held3);
	}, (held3, at2) => {
		return item(held3, at2);
	}, (held3, at2, read) => {
		return lend(held3, at2, read);
	}, (held3, at2, value2) => {
		return put(held3, at2, value2, $d);
	}, (held3, at2, with2) => {
		return change(held3, at2, with2, $d);
	} ]);
}
function over2(value, $c) {
	return [ sequence(value, $c) ];
}
function new9() {
	return [ __shared_new(new2()), __shared_new([  ]), __shared_new(new2()), __shared_new(new2()), new8(0, "std/src/reactive/lens/loaded.vl:130:24"), new8(0, "std/src/reactive/lens/loaded.vl:131:25"), __shared_new(0) ];
}
function lend2(value, read) {
	return read(value);
}
function viewing(cell2, read) {
	return lend2(cell2.v, read);
}
function peek_storage(storage, read) {
	const $h = storage;
	let $i = null;
	if ($h[0] === 0) {
		const cell2 = $h[1];
		$i = viewing(cell2, read);
	} else {
		const port = $h[1];
		let out = [ 1 ];
		port[0]((held3) => {
			return out = [ 0, read(held3) ];
		});
		const $j = out;
		let $k = null;
		if ($j[0] === 0) {
			const value = __clone($j[1]);
			$k = value;
		} else {
			$k = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$i = $k;
	}
	return $i;
}
function viewing2(cell2, read) {
	return lend2(cell2.v, read);
}
function peek_storage2(storage, read) {
	const $l = storage;
	let $m = null;
	if ($l[0] === 0) {
		const cell2 = $l[1];
		$m = viewing2(cell2, read);
	} else {
		const port = $l[1];
		let out = [ 1 ];
		port[0]((held3) => {
			return out = [ 0, read(held3) ];
		});
		const $n = out;
		let $o = null;
		if ($n[0] === 0) {
			const value = __clone($n[1]);
			$o = value;
		} else {
			$o = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$m = $o;
	}
	return $m;
}
function follow_id(storage, watch, holds, at2) {
	if (viewing2(watch, (held3) => {
		return follows_items(held3[0]);
	}) && !(peek_storage2(storage, (held3) => {
		return holds(held3, at2);
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
		const $v = __list_pop(keys2);
		let $w = null;
		if ($v[0] === 0) {
			const key = $v[1];
			const $x = __list_pop(values);
			let $y = null;
			if ($x[0] === 0) {
				const value = $x[1];
				$y = backwards.push([ key, value ]);
			} else {
				$y = undefined;
			}
			$w = $y;
		} else {
			$w = undefined;
		}
		$w;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $z = __list_pop(backwards);
		let $A = null;
		if ($z[0] === 0) {
			const pair = $z[1];
			$A = result.push(pair);
		} else {
			$A = undefined;
		}
		$A;
	}
	return result;
}
function new10() {
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
function release4(self) {
	untap(self[0], self[2]);
	for (const _ of entries(self[1].v)) {
		const $P = _;
		const _key = $P[0];
		const id = $P[1];
		release_id(self[0][0].v[3], id);
	}
	self[1].v = new10();
}
function index_by(self, key, $g) {
	const found = __shared_new(new2());
	const leaving = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const view2 = self[0].v[1][2];
	const key_at = (at2) => {
		let out = [ 1 ];
		peek_storage(storage, (held3) => {
			return view2(held3, at2, (item2) => {
				return out = [ 0, key(item2) ];
			});
		});
		return out;
	};
	const map2 = (held3, at2) => {
		const id = follow_id(storage, watch, holds, at2);
		const before = get(found.v, held3);
		insert(found.v, held3, id);
		const $t = before;
		if ($t[0] === 0) {
			release_id(watch, $t[1]);
		}
		return;
	};
	const unmap = (held3) => {
		const $u = get(found.v, held3);
		if ($u[0] === 0) {
			remove(found.v, held3);
			release_id(watch, $u[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $B = _;
			const _held = $B[0];
			const old = $B[1];
			release_id(watch, old);
		}
		found.v = new10();
		let at2 = 0;
		const count2 = peek_storage2(storage, (held3) => {
			return count(held3);
		});
		while (at2 < count2) {
			const $G = key_at(at2);
			if ($G[0] === 0) {
				map2($G[1], at2);
			}
			at2 = at2 + 1;
		}
		return;
	};
	fill();
	const note = (at2, count2) => {
		let gone = [  ];
		let index3 = at2;
		while (index3 < at2 + count2) {
			const $H = key_at(index3);
			if ($H[0] === 0) {
				gone.push($H[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone;
		return;
	};
	const arrive = (at2, count2) => {
		let index3 = at2;
		while (index3 < at2 + count2) {
			const $I = key_at(index3);
			if ($I[0] === 0) {
				map2($I[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change2) => {
		const $J = change2;
		let $K = null;
		if ($J[0] === 0) {
			const at2 = $J[1];
			const removed = $J[2];
			const _inserted = $J[3];
			$K = note(at2, removed);
		} else if ($J[0] === 3) {
			const at3 = $J[1];
			const count2 = $J[2];
			$K = note(at3, count2);
		} else if ($J[0] === 2) {
			const at4 = $J[1];
			$K = note(at4, 1);
		} else if ($J[0] === 4) {
			const at5 = $J[1];
			const _part = $J[2];
			$K = note(at5, 1);
		} else {
			$K = undefined;
		}
		return $K;
	}, (change2, stage) => {
		const $L = stage;
		let $M = null;
		if ($L[0] === 0) {
			$M = undefined;
		} else if ($L[0] === 3) {
			return;
		} else {
			fill();
			return;
			$M = undefined;
		}
		$M;
		const $N = change2;
		let $O = null;
		if ($N[0] === 0) {
			const at2 = $N[1];
			const _removed = $N[2];
			const inserted = $N[3];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			arrive(at2, inserted);
			$O = undefined;
		} else if ($N[0] === 1) {
			const at3 = $N[1];
			const count2 = $N[2];
			$O = arrive(at3, count2);
		} else if ($N[0] === 3) {
			const _at = $N[1];
			const _count = $N[2];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			$O = undefined;
		} else if ($N[0] === 2) {
			const at4 = $N[1];
			for (const held5 of leaving.v) {
				unmap(held5);
			}
			arrive(at4, 1);
			$O = undefined;
		} else if ($N[0] === 4) {
			const at5 = $N[1];
			const _part = $N[2];
			for (const held6 of leaving.v) {
				unmap(held6);
			}
			arrive(at5, 1);
			$O = undefined;
		} else if ($N[0] === 5) {
			const _from = $N[1];
			const _to = $N[2];
			$O = undefined;
		} else {
			$O = fill();
		}
		return $O;
	});
	const index2 = [ __clone(self), found, tap2 ];
	defer_to_owner(() => {
		return release4(index2);
	}, $g);
	return index2;
}
function index_by2(self, key, $f) {
	return index_by(self[0], key, $f);
}
function new11() {
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
		const held3 = __clone(self[0].v);
		const length = held3.length;
		while (index2 < length) {
			const $Y = __list_get(held3, index2);
			let $Z = null;
			if ($Y[0] === 0) {
				const op = $Y[1];
				$Z = kept.push(op);
			} else {
				$Z = undefined;
			}
			$Z;
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
function release5(self) {
	untap(self[0], self[3]);
	for (const id of self[1].v) {
		release_id(self[0][0].v[3], id);
	}
	self[1].v = [  ];
}
function rows(self, $V) {
	const log = new11();
	const held3 = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const fill = () => {
		let ids = [  ];
		let at2 = 0;
		const count2 = peek_storage2(storage, (list) => {
			return count(list);
		});
		while (at2 < count2) {
			ids.push(follow_id(storage, watch, holds, at2));
			at2 = at2 + 1;
		}
		return ids;
	};
	const leave = (at2, count2) => {
		let gone = [  ];
		let taken = 0;
		while (taken < count2) {
			const id = __remove_at(held3.v, at2, "std/src/reactive/lens.vl:1839:27");
			release_id(watch, id);
			gone.push(id);
			taken = taken + 1;
		}
		return gone;
	};
	const arrive = (at2, count2) => {
		let arrived = [  ];
		let index2 = at2;
		while (index2 < at2 + count2) {
			const id = follow_id(storage, watch, holds, index2);
			__insert_at(held3.v, index2, id, "std/src/reactive/lens.vl:1851:18");
			arrived.push(id);
			index2 = index2 + 1;
		}
		return arrived;
	};
	held3.v = fill();
	const tap2 = tap(self, (_change) => {
		return;
	}, (change2, _stage) => {
		const $W = change2;
		let $X = null;
		if ($W[0] === 0) {
			const at2 = $W[1];
			const removed = $W[2];
			const inserted = $W[3];
			const gone = leave(at2, removed);
			record(log, [ 0, at2, gone, arrive(at2, inserted) ]);
			$X = undefined;
		} else if ($W[0] === 1) {
			const at3 = $W[1];
			const count2 = $W[2];
			$X = record(log, [ 0, at3, [  ], arrive(at3, count2) ]);
		} else if ($W[0] === 3) {
			const at4 = $W[1];
			const count3 = $W[2];
			$X = record(log, [ 0, at4, leave(at4, count3), [  ] ]);
		} else if ($W[0] === 5) {
			const from = $W[1];
			const to = $W[2];
			const id = __remove_at(held3.v, from, "std/src/reactive/lens.vl:1867:28");
			__insert_at(held3.v, to, id, "std/src/reactive/lens.vl:1868:19");
			record(log, [ 3, from, 1, to ]);
			$X = undefined;
		} else if ($W[0] === 6) {
			for (const id2 of held3.v) {
				release_id(watch, id2);
			}
			held3.v = fill();
			record(log, [ 2, __clone(held3.v) ]);
			$X = undefined;
		} else {
			$X = undefined;
		}
		return $X;
	});
	const feed = [ __clone(self), held3, __clone(log), tap2 ];
	defer_to_owner(() => {
		return release5(feed);
	}, $V);
	return feed;
}
function rows2(self, $U) {
	return rows(self[0], $U);
}
function lens(self) {
	return __clone(self[0]);
}
function slot_of(self, id) {
	const $ab = __clone(self);
	let $aa = null;
	if (id < 0) {
		$aa = -(1);
	} else {
		$aa = id;
	}
	return [ $ab, $aa ];
}
function index(self) {
	return [ __clone(self) ];
}
function beside(lens2, key, $e) {
	const ledger = new9();
	return [ __clone(lens2), key, ledger, index_by2(lens2, key, $e), rows2(lens2, $e), index(slot_of(lens(lens2), -(1))) ];
}
function get2(self) {
	return __clone(self[0].v);
}
function last(self) {
	let $al = null;
	if (is_empty(self)) {
		$al = [ 1 ];
	} else {
		$al = __list_get(self, self.length - 1);
	}
	return $al;
}
function notify(self, $ag) {
	const traced = tracing.v;
	let $ah = null;
	if (traced) {
		$ah = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$ah = "";
	}
	const cause = $ah;
	const $ai = $ag;
	let $aj = null;
	if ($ai[0] === 0) {
		const turn = $ai[1];
		$aj = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $am = last(draining_turns.v);
		let $an = null;
		if ($am[0] === 0) {
			const draining = $am[1];
			$an = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $ao = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$ao = undefined;
				}
				$ao;
			}
			$an = undefined;
		}
		$aj = $an;
	}
	return $aj;
}
function set(self, value, $af, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $af);
}
function drop_pages(self, $ad) {
	self[6].v[2] = 0;
	self[6].v[3] = 0;
	const $ae = get2(self[4]);
	if ($ae[0] === 1) {
		set(self[4], [ 0 ], $ad, "std/src/reactive/lens/loaded.vl:873:20");
	}
	const $ap = get2(self[5]);
	if ($ap[0] === 1) {
		set(self[5], [ 0 ], $ad, "std/src/reactive/lens/loaded.vl:876:20");
	}
}
function retire(self, $ac) {
	self[6].v[1] = 0;
	drop_pages(self, $ac);
	const running = self[7].v;
	cancel(running[0]);
	cancel(running[1]);
	cancel(running[2]);
	self[7].v = [ [ 1 ], [ 1 ], [ 1 ] ];
}
function new12(key, $a, $b) {
	const empty = [  ];
	const lens2 = over2(empty, $a);
	const feed = [ __clone(lens2), beside(lens2, key, $b), key, new8([ 0 ], "std/src/reactive/lens/loaded.vl:767:24"), new8([ 0 ], "std/src/reactive/lens/loaded.vl:768:29"), new8([ 0 ], "std/src/reactive/lens/loaded.vl:769:29"), __shared_new([ 0, 0, 0, 0 ]), __shared_new([ [ 1 ], [ 1 ], [ 1 ] ]) ];
	defer_to_owner(() => {
		return retire(feed, $a);
	}, $b);
	return feed;
}
function contains_key(self, key) {
	return __nmap_has(self[0], hash(key));
}
function unwrap_or(self, fallback) {
	const $at = self;
	let $au = null;
	if ($at[0] === 0) {
		const x = $at[1];
		$au = x;
	} else {
		$au = __clone(__force(fallback));
	}
	return $au;
}
async function answer(self, call, reply, $as) {
	while (!(contains_key(self[0].v, call))) {
		insert(self[1].v, call, polled(self, call) + 1);
		await (sleep(1, $as));
	}
	insert(self[2].v, call, true);
	return __clone(reply);
}
function claim(self) {
	const token = self[6].v[0] + 1;
	self[6].v[0] = token;
	return token;
}
function latest(self) {
	const $aC = self;
	let $aD = null;
	if ($aC[0] === 1) {
		const value = $aC[1];
		$aD = [ 0, __clone(value) ];
	} else if ($aC[0] === 2) {
		const value2 = $aC[1];
		$aD = [ 0, __clone(value2) ];
	} else if ($aC[0] === 3) {
		const _error = $aC[1];
		const stale = __clone($aC[2]);
		$aD = stale;
	} else {
		$aD = [ 1 ];
	}
	return $aD;
}
function refreshed(self) {
	const $aE = latest(self);
	let $aF = null;
	if ($aE[0] === 0) {
		const value = $aE[1];
		$aF = [ 2, value ];
	} else {
		$aF = [ 0 ];
	}
	return $aF;
}
function same_phase(first, second) {
	const $aH = [ first, second ];
	let $aI = null;
	if ($aH[0][0] === 0 && $aH[1][0] === 0) {
		$aI = true;
	} else if ($aH[0][0] === 4 && $aH[1][0] === 4) {
		$aI = true;
	} else if ($aH[0][0] === 1 && $aH[1][0] === 1) {
		const mine = $aH[0][1];
		const theirs = $aH[1][1];
		$aI = mine === theirs;
	} else if ($aH[0][0] === 2 && $aH[1][0] === 2) {
		const mine2 = $aH[0][1];
		const theirs2 = $aH[1][1];
		$aI = mine2 === theirs2;
	} else {
		$aI = false;
	}
	return $aI;
}
function notify2(self, $ag) {
	const traced = tracing.v;
	let $aJ = null;
	if (traced) {
		$aJ = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$aJ = "";
	}
	const cause = $aJ;
	const $aK = $ag;
	let $aL = null;
	if ($aK[0] === 0) {
		const turn = $aK[1];
		$aL = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $aM = last(draining_turns.v);
		let $aN = null;
		if ($aM[0] === 0) {
			const draining = $aM[1];
			$aN = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $aO = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$aO = undefined;
				}
				$aO;
			}
			$aN = undefined;
		}
		$aL = $aN;
	}
	return $aL;
}
function set2(self, value, $af, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $af);
}
function set_phase(cell2, next, $aG) {
	if (!(same_phase(get2(cell2), next))) {
		set2(cell2, next, $aG, "std/src/reactive/lens/loaded.vl:716:8");
	}
}
function load(self, $aB) {
	const token = claim(self);
	self[6].v[1] = token;
	set_phase(self[3], refreshed(get2(self[3])), $aB);
	return token;
}
function all_of(storage) {
	return peek_storage2(storage, (held3) => {
		let out = [  ];
		let at2 = 0;
		const count2 = count(held3);
		while (at2 < count2) {
			const $aW = item(held3, at2);
			if ($aW[0] === 0) {
				out.push($aW[1]);
			}
			at2 = at2 + 1;
		}
		return out;
	});
}
function all(self) {
	return all_of(self[0].v[0]);
}
function peek(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function size(self) {
	return peek(self, (held3) => {
		return count(held3);
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
function splice(self, at2, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
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
	const $bj = self[0].v[0];
	let $bk = null;
	if ($bj[0] === 0) {
		const cell2 = $bj[1];
		$bk = change2(cell2.v);
	} else {
		const port = __clone($bj[1]);
		$bk = port[1](change2);
	}
	return $bk;
}
function wake_all(watch, slot3, $bS) {
	const held3 = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot3), "std/src/reactive/lens.vl:1120:36"));
	});
	const $bT = held3[0];
	if ($bT[0] === 0) {
		notify2($bT[1], $bS);
	}
	for (const part of held3[1]) {
		notify2(part[1], $bS);
	}
}
function wake_part(watch, slot3, name, $bU) {
	const held3 = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot3), "std/src/reactive/lens.vl:1130:36"));
	});
	const $bV = held3[0];
	if ($bV[0] === 0) {
		notify2($bV[1], $bU);
	}
	for (const part of held3[1]) {
		if (part[0] === name) {
			notify2(part[1], $bU);
		}
	}
}
function batch(body, $cd) {
	const $ce = $cd;
	let $cf = null;
	if ($ce[0] === 0) {
		const current2 = $ce[1];
		$cf = body(current2);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$cf = result;
	}
	return $cf;
}
function landed(self, changes, late, $bm) {
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
		const $bn = change2;
		let $bo = null;
		if ($bn[0] === 0) {
			const _at = $bn[1];
			const removed = $bn[2];
			const inserted = $bn[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$bo = undefined;
		} else if ($bn[0] === 1) {
			const _at2 = $bn[1];
			const _count = $bn[2];
			$bo = resized = true;
		} else if ($bn[0] === 3) {
			const _at3 = $bn[1];
			const _count2 = $bn[2];
			$bo = resized = true;
		} else if ($bn[0] === 6) {
			$bo = resized = true;
		} else if ($bn[0] === 5) {
			const _from = $bn[1];
			const _to = $bn[2];
			$bo = shaped = true;
		} else {
			$bo = undefined;
		}
		$bo;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change2, wakes);
		}
		let $bE = null;
		if (self[0].v[4].length > 0) {
			let $bD = null;
			if (!(late)) {
				$bD = [ 0 ];
			} else if (!(several)) {
				$bD = [ 1 ];
			} else if (index2 === 0) {
				$bD = [ 2 ];
			} else {
				$bD = [ 3 ];
			}
			const stage = $bD;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$bE = undefined;
		}
		$bE;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $bF = null;
	if (shaped || resized) {
		$bF = watch.v[2];
	} else {
		$bF = [ 1 ];
	}
	const shape = __clone($bF);
	let $bG = null;
	if (resized) {
		$bG = watch.v[3];
	} else {
		$bG = [ 1 ];
	}
	const sized = __clone($bG);
	let $bH = undefined;
	let $bI = undefined;
	if (wakes.length === 0 && (($bH = shape), $bH[0] === 1) && (($bI = sized), $bI[0] === 1)) {
		notify2(published, $bm);
		return;
	}
	settle_together(($bP) => {
		for (const wake2 of wakes) {
			const $bQ = wake2;
			let $bR = null;
			if ($bQ[0] === 0) {
				const slot3 = $bQ[1];
				$bR = wake_all(watch, slot3, [ 0, $bP ]);
			} else if ($bQ[0] === 2) {
				const slot4 = $bQ[1];
				$bR = wake_all(watch, slot4, [ 0, $bP ]);
			} else {
				const slot5 = $bQ[1];
				const part = $bQ[2];
				$bR = wake_part(watch, slot5, part, [ 0, $bP ]);
			}
			$bR;
		}
		const $bW = sized;
		if ($bW[0] === 0) {
			notify2($bW[1], [ 0, $bP ]);
		}
		const $bX = shape;
		if ($bX[0] === 0) {
			notify2($bX[1], [ 0, $bP ]);
		}
		notify2(published, [ 0, $bP ]);
		return;
	}, $bm);
}
function landed_one(self, change2, $bl) {
	landed(self, [ __clone(change2) ], false, $bl);
}
function splice2(self, at2, removed, inserted, $bc) {
	const held3 = size(self);
	let $bh = null;
	if (at2 > held3) {
		$bh = held3;
	} else {
		$bh = at2;
	}
	const start9 = $bh;
	let $bi = null;
	if (start9 + removed > held3) {
		$bi = held3 - start9;
	} else {
		$bi = removed;
	}
	const taking = $bi;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start9, taking, arriving ];
	coming_one(self, change2);
	mutate(self, (held4) => {
		return splice(held4, start9, taking, inserted);
	});
	landed_one(self, change2, $bc);
}
function remove_at(self, at2, $bb) {
	return splice2(self, at2, 1, [  ], $bb);
}
function eq6(self, b) {
	const $ck = self;
	let $cn = null;
	if ($ck[0] === 0) {
		const $cl = b;
		let $cm = null;
		if ($cl[0] === 0) {
			$cm = eq5($ck[1], $cl[1]);
		} else {
			$cm = false;
		}
		$cn = $cm;
	} else {
		const $co = b;
		$cn = $co[0] === 1;
	}
	return $cn;
}
function has(self, at2) {
	const holds = self[0].v[1][0];
	return peek(self, (held3) => {
		return holds(held3, at2);
	});
}
function landed2(self, changes, late, $bm) {
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
		const $ct = change2;
		let $cu = null;
		if ($ct[0] === 0) {
			const _at = $ct[1];
			const removed = $ct[2];
			const inserted = $ct[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$cu = undefined;
		} else if ($ct[0] === 1) {
			const _at2 = $ct[1];
			const _count = $ct[2];
			$cu = resized = true;
		} else if ($ct[0] === 3) {
			const _at3 = $ct[1];
			const _count2 = $ct[2];
			$cu = resized = true;
		} else if ($ct[0] === 6) {
			$cu = resized = true;
		} else if ($ct[0] === 5) {
			const _from = $ct[1];
			const _to = $ct[2];
			$cu = shaped = true;
		} else {
			$cu = undefined;
		}
		$cu;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change2, wakes);
		}
		let $cw = null;
		if (self[0].v[4].length > 0) {
			let $cv = null;
			if (!(late)) {
				$cv = [ 0 ];
			} else if (!(several)) {
				$cv = [ 1 ];
			} else if (index2 === 0) {
				$cv = [ 2 ];
			} else {
				$cv = [ 3 ];
			}
			const stage = $cv;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$cw = undefined;
		}
		$cw;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $cx = null;
	if (shaped || resized) {
		$cx = watch.v[2];
	} else {
		$cx = [ 1 ];
	}
	const shape = __clone($cx);
	let $cy = null;
	if (resized) {
		$cy = watch.v[3];
	} else {
		$cy = [ 1 ];
	}
	const sized = __clone($cy);
	let $cz = undefined;
	let $cA = undefined;
	if (wakes.length === 0 && (($cz = shape), $cz[0] === 1) && (($cA = sized), $cA[0] === 1)) {
		notify2(published, $bm);
		return;
	}
	settle_together(($bP) => {
		for (const wake2 of wakes) {
			const $cB = wake2;
			let $cC = null;
			if ($cB[0] === 0) {
				const slot3 = $cB[1];
				$cC = wake_all(watch, slot3, [ 0, $bP ]);
			} else if ($cB[0] === 2) {
				const slot4 = $cB[1];
				$cC = wake_all(watch, slot4, [ 0, $bP ]);
			} else {
				const slot5 = $cB[1];
				const part = $cB[2];
				$cC = wake_part(watch, slot5, part, [ 0, $bP ]);
			}
			$cC;
		}
		const $cD = sized;
		if ($cD[0] === 0) {
			notify2($cD[1], [ 0, $bP ]);
		}
		const $cE = shape;
		if ($cE[0] === 0) {
			notify2($cE[1], [ 0, $bP ]);
		}
		notify2(published, [ 0, $bP ]);
		return;
	}, $bm);
}
function landed_one2(self, change2, $bl) {
	landed2(self, [ __clone(change2) ], false, $bl);
}
function replace(self, at2, value, $cp) {
	const watch = self[0].v[3];
	let $cq = null;
	if (has(self, at2)) {
		$cq = [ 2, __clone(at2) ];
	} else if (viewing2(watch, (held3) => {
		return follows_items(held3[0]);
	})) {
		return false;
	} else {
		$cq = [ 1, __clone(at2), 1 ];
	}
	const change2 = $cq;
	coming_one(self, change2);
	const write = self[0].v[1][3];
	mutate(self, (held3) => {
		return write(held3, at2, value);
	});
	landed_one2(self, change2, $cp);
	return true;
}
function size2(self) {
	return peek(self, (held3) => {
		return count(held3);
	});
}
function shift(self, from, to) {
	const item2 = __remove_at(self, from, "std/src/reactive/lens.vl:2252:19");
	__insert_at(self, to, item2, "std/src/reactive/lens.vl:2253:8");
}
function move_item(self, from, to, $cI) {
	const held3 = size2(self);
	if (from >= held3 || from === to) {
		return;
	}
	let $cJ = null;
	if (to >= held3) {
		$cJ = held3 - 1;
	} else {
		$cJ = to;
	}
	const landing2 = $cJ;
	const change2 = [ 5, from, landing2 ];
	coming_one(self, change2);
	mutate(self, (held4) => {
		return shift(held4, from, landing2, $cI);
	});
	landed_one2(self, change2, $cI);
}
function insert_at(self, at2, value, $cK) {
	return splice2(self, at2, 0, [ __clone(value) ], $cK);
}
function reconcile_by_key(self, next, key, $aU) {
	const lens2 = __clone(self);
	batch(($aV) => {
		let wanted = new10();
		for (const item2 of next) {
			insert(wanted, key(item2), true);
		}
		let keys2 = [  ];
		for (const item3 of all(lens2)) {
			keys2.push(key(item3));
		}
		let at2 = keys2.length;
		while (at2 > 0) {
			at2 = at2 - 1;
			if (!(contains_key(wanted, __at(keys2, at2, "std/src/reactive/lens.vl:1910:29")))) {
				remove_at(lens2, at2, [ 0, $aV ]);
				__remove_at(keys2, at2, "std/src/reactive/lens.vl:1912:11");
			}
		}
		let index2 = 0;
		for (const item4 of next) {
			const want = key(item4);
			let $cF = null;
			if (index2 < keys2.length && __at(keys2, index2, "std/src/reactive/lens.vl:1918:30") === want) {
				if (!(eq6(peek(lens2, (list) => {
					return item(list, index2);
				}), [ 0, __clone(item4) ]))) {
					replace(lens2, index2, item4, [ 0, $aV ]);
				}
				$cF = undefined;
			} else {
				let found = [ 1 ];
				let search2 = index2 + 1;
				while (search2 < keys2.length) {
					if (__at(keys2, search2, "std/src/reactive/lens.vl:1926:10") === want) {
						found = [ 0, search2 ];
						search2 = keys2.length;
					}
					search2 = search2 + 1;
				}
				const $cG = found;
				let $cH = null;
				if ($cG[0] === 0) {
					const from = $cG[1];
					move_item(lens2, from, index2, [ 0, $aV ]);
					const moved = __remove_at(keys2, from, "std/src/reactive/lens.vl:1935:25");
					__insert_at(keys2, index2, moved, "std/src/reactive/lens.vl:1936:13");
					if (!(eq6(peek(lens2, (list) => {
						return item(list, index2);
					}), [ 0, __clone(item4) ]))) {
						replace(lens2, index2, item4, [ 0, $aV ]);
					}
					$cH = undefined;
				} else {
					insert_at(lens2, index2, item4, [ 0, $aV ]);
					__insert_at(keys2, index2, want, "std/src/reactive/lens.vl:1943:13");
					$cH = undefined;
				}
				$cF = $cH;
			}
			$cF;
			index2 = index2 + 1;
		}
		return;
	}, $aU);
}
function failed(self, error) {
	return [ 3, __clone(error), latest(self) ];
}
function settle(self, token, reply, $aQ) {
	if (token === 0 || self[6].v[1] !== token) {
		return false;
	}
	self[6].v[1] = 0;
	const feed = __clone(self);
	const $aR = reply;
	let $aS = null;
	if ($aR[0] === 0) {
		const list = $aR[1];
		$aS = batch(($aT) => {
			reconcile_by_key(lens(feed[0]), list, feed[2], [ 0, $aT ]);
			drop_pages(feed, [ 0, $aT ]);
			set_phase(feed[3], [ 1, token ], [ 0, $aT ]);
			return;
		}, $aQ);
	} else {
		const error = $aR[1];
		$aS = set_phase(self[3], failed(get2(self[3]), error), $aQ);
	}
	$aS;
	return true;
}
function fetch2(self, fetch3, $aA) {
	const token = load(self, $aA);
	cancel(self[7].v[0]);
	const running = detached_nursery();
	self[7].v[0] = [ 0, __clone(running) ];
	const feed = __clone(self);
	(($aP) => {
		__task(async () => {
			const reply = await (fetch3($aP));
			settle(feed, token, reply, $aA);
			return;
		}, "fetch", $aP);
		return;
	})(running);
}
function items(self) {
	return __clone(self[0]);
}
function size3(self) {
	return size2(self[0]);
}
function phase(self) {
	return [ __clone(self[3]) ];
}
function get3(self) {
	return get2(self[0]);
}
function all2(self) {
	return all(self[0]);
}
function follow3(self, at2) {
	const core = self[0];
	return slot_of(self, follow_id(core.v[0], core.v[3], core.v[1][0], at2));
}
function release6(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot(self, at2, $cP) {
	const handle = follow3(self, at2);
	defer_to_owner(() => {
		return release6(handle);
	}, $cP);
	return handle;
}
function slot2(self, at2, $cO) {
	return slot(self[0], at2, $cO);
}
function at(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][0].v[3], (held3) => {
		return locate(held3[0], self[1]);
	});
}
function get4(self) {
	const $cT = at(self);
	if ($cT[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held3) => {
		return read(held3, $cT[1]);
	});
}
function peek2(self, read) {
	return peek(self[0], read);
}
function eq7(self, b) {
	const $cY = self;
	let $db = null;
	if ($cY[0] === 0) {
		const $cZ = b;
		let $da = null;
		if ($cZ[0] === 0) {
			$da = eq2($cY[1], $cZ[1]);
		} else {
			$da = false;
		}
		$db = $da;
	} else {
		const $dc = b;
		$db = $dc[0] === 1;
	}
	return $db;
}
function set_edge(cell2, next, $ds) {
	const $dt = [ get2(cell2), __clone(next) ];
	let $du = null;
	if ($dt[0][0] === 0 && $dt[1][0] === 0) {
		$du = true;
	} else if ($dt[0][0] === 1 && $dt[1][0] === 1) {
		$du = true;
	} else if ($dt[0][0] === 2 && $dt[1][0] === 2) {
		$du = true;
	} else {
		$du = false;
	}
	const same = $du;
	if (!(same)) {
		set(cell2, next, $ds, "std/src/reactive/lens/loaded.vl:729:8");
	}
}
function request(self, older2, $dp) {
	let $dq = null;
	if (older2) {
		$dq = self[4];
	} else {
		$dq = self[5];
	}
	const edge = __clone($dq);
	const $dr = get2(edge);
	if ($dr[0] === 2) {
		return [ 1 ];
	}
	const token = claim(self);
	if (older2) {
		self[6].v[2] = token;
	} else {
		self[6].v[3] = token;
	}
	set_edge(edge, [ 1 ], $dp);
	return [ 0, token ];
}
function request_older(self, $do) {
	return request(self, true, $do);
}
function map(self, fn) {
	const $dE = self;
	let $dF = null;
	if ($dE[0] === 0) {
		const x = $dE[1];
		$dF = [ 0, fn(x) ];
	} else {
		$dF = [ 1 ];
	}
	return $dF;
}
function find(self, key) {
	const lens2 = __clone(self[0]);
	return map(viewing2(self[1], (held3) => {
		return get(held3, key);
	}), (id) => {
		return slot_of(lens2, id);
	});
}
function held2(self, key) {
	return find(self[3], key);
}
function and_then(self, fn) {
	const $dI = self;
	let $dJ = null;
	if ($dI[0] === 0) {
		const x = $dI[1];
		$dJ = fn(x);
	} else {
		$dJ = [ 1 ];
	}
	return $dJ;
}
function replace2(self, at2, value, $dM) {
	return replace(self[0], at2, value, $dM);
}
function splice3(self, at2, removed, inserted, $bc) {
	const held3 = size2(self);
	let $dP = null;
	if (at2 > held3) {
		$dP = held3;
	} else {
		$dP = at2;
	}
	const start9 = $dP;
	let $dQ = null;
	if (start9 + removed > held3) {
		$dQ = held3 - start9;
	} else {
		$dQ = removed;
	}
	const taking = $dQ;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start9, taking, arriving ];
	coming_one(self, change2);
	mutate(self, (held4) => {
		return splice(held4, start9, taking, inserted);
	});
	landed_one2(self, change2, $bc);
}
function splice4(self, at2, removed, inserted, $dO) {
	splice3(self[0], at2, removed, inserted, $dO);
}
function push(self, value, $dN) {
	return splice4(self, size3(self), 0, [ __clone(value) ], $dN);
}
function receive(self, item2, $dH) {
	const id = self[2](item2);
	const $dK = and_then(held2(self[1], id), (slot3) => {
		return at(slot3);
	});
	let $dL = null;
	if ($dK[0] === 0) {
		const at2 = $dK[1];
		if (!(eq6(peek2(self[0], (list) => {
			return __list_get(list, at2);
		}), [ 0, __clone(item2) ]))) {
			replace2(self[0], at2, item2, $dH);
		}
		$dL = undefined;
	} else {
		$dL = push(self[0], item2, $dH);
	}
	return $dL;
}
function settle_page(self, older2, token, reply, $dy) {
	let $dz = null;
	if (older2) {
		$dz = self[6].v[2];
	} else {
		$dz = self[6].v[3];
	}
	const mine = $dz;
	if (token === 0 || mine !== token) {
		return false;
	}
	if (older2) {
		self[6].v[2] = 0;
	} else {
		self[6].v[3] = 0;
	}
	let $dA = null;
	if (older2) {
		$dA = self[4];
	} else {
		$dA = self[5];
	}
	const edge = __clone($dA);
	const feed = __clone(self);
	const $dB = reply;
	let $dC = null;
	if ($dB[0] === 0) {
		const page = $dB[1];
		$dC = batch(($dD) => {
			const key = feed[2];
			let taken = new10();
			let fresh = [  ];
			for (const item2 of page[0]) {
				const id = key(item2);
				const $dG = held2(feed[1], id);
				if ($dG[0] === 0) {
					receive(feed, item2, [ 0, $dD ]);
				} else if (!(contains_key(taken, id))) {
					insert(taken, id, true);
					fresh.push(__clone(item2));
				}
			}
			let $dR = null;
			if (older2) {
				$dR = 0;
			} else {
				$dR = size3(feed[0]);
			}
			const at2 = $dR;
			splice4(feed[0], at2, 0, fresh, [ 0, $dD ]);
			let $dS = null;
			if (page[1]) {
				$dS = [ 0 ];
			} else {
				$dS = [ 2 ];
			}
			set_edge(edge, $dS, [ 0, $dD ]);
			return;
		}, $dy);
	} else {
		const error = $dB[1];
		$dC = set(edge, [ 3, __clone(error) ], $dy, "std/src/reactive/lens/loaded.vl:949:27");
	}
	$dC;
	return true;
}
function prepend_page(self, token, reply, $dx) {
	return settle_page(self, true, token, reply, $dx);
}
function fetch_older(self, fetch3, $dn) {
	const $dv = request_older(self, $dn);
	if ($dv[0] === 0) {

	} else {
		return;
	}
	cancel(self[7].v[1]);
	const running = detached_nursery();
	self[7].v[1] = [ 0, __clone(running) ];
	const feed = __clone(self);
	(($dw) => {
		__task(async () => {
			const reply = await (fetch3($dw));
			prepend_page(feed, $dv[1], reply, $dn);
			return;
		}, "fetch_older", $dw);
		return;
	})(running);
}
function older(self) {
	return [ __clone(self[4]) ];
}
function request_newer(self, $dY) {
	return request(self, false, $dY);
}
function append_page(self, token, reply, $eb) {
	return settle_page(self, false, token, reply, $eb);
}
function fetch_newer(self, fetch3, $dX) {
	const $dZ = request_newer(self, $dX);
	if ($dZ[0] === 0) {

	} else {
		return;
	}
	cancel(self[7].v[2]);
	const running = detached_nursery();
	self[7].v[2] = [ 0, __clone(running) ];
	const feed = __clone(self);
	(($ea) => {
		__task(async () => {
			const reply = await (fetch3($ea));
			append_page(feed, $dZ[1], reply, $dX);
			return;
		}, "fetch_newer", $ea);
		return;
	})(running);
}
function newer(self) {
	return [ __clone(self[5]) ];
}
function overlay(self) {
	return __clone(self[1]);
}
function slot_cell(watch, slot3) {
	if (slot3 < 0) {
		return new8(0, "std/src/reactive/lens.vl:1082:19");
	}
	const entry = as_usize(slot3);
	const $ej = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1085:5")[0];
	const cell2 = __clone($ej[1]);
	if ($ej[0] === 0) {
		return cell2;
	}
	const cell3 = new8(0, "std/src/reactive/lens.vl:1088:25");
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1089:2")[0] = [ 0, __clone(cell3) ];
	return cell3;
}
function identity(self) {
	return [ 0, __shared_identity(self[0]) ];
}
function identity2(self) {
	return identity(slot_cell(self[0][0].v[3], self[1]));
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
	return on_settle(slot_cell(self[0][0].v[3], self[1]), subscriber);
}
function track(self, $ei) {
	const scope = $ei;
	const identity3 = identity2(self);
	if (admits(scope, identity3)) {
		lists_of(scope[0]).v[0].push([ identity3, (subscriber) => {
			return on_settle2(self, subscriber);
		} ]);
	}
	return get4(self);
}
function row(self, key) {
	const $eu = get(self[3].v, key);
	if ($eu[0] === 0) {
		return $eu[1];
	}
	const made = [ new8(0, "std/src/reactive/lens/loaded.vl:141:44"), new8(false, "std/src/reactive/lens/loaded.vl:141:74") ];
	insert(self[3].v, key, made);
	return made;
}
function track2(self, $ei) {
	const scope = $ei;
	const identity3 = identity(self);
	if (admits(scope, identity3)) {
		lists_of(scope[0]).v[0].push([ identity3, (subscriber) => {
			return on_settle(self, subscriber);
		} ]);
	}
	return get2(self);
}
function showing(self, key) {
	const $ev = get(self[0].v, key);
	if ($ev[0] === 0) {

	} else {
		return [ 1 ];
	}
	let out = [ 1 ];
	for (const proposal of $ev[1][0]) {
		if (proposal[0] > $ev[1][1]) {
			out = [ 0, __clone(proposal) ];
		}
	}
	return out;
}
function provisional(self, key) {
	const $eB = get(self[0].v, key);
	if ($eB[0] === 0) {

	} else {
		return false;
	}
	let out = false;
	for (const proposal of $eB[1][0]) {
		if (proposal[2]) {
			out = true;
		}
	}
	return out;
}
function over3(self, key, held3) {
	const $ew = showing(self, key);
	let $ex = null;
	if ($ew[0] === 1) {
		$ex = __clone(held3);
	} else {
		const proposal = $ew[1];
		const $ey = proposal[1];
		let $ez = null;
		if ($ey[0] === 1) {
			$ez = [ 1 ];
		} else {
			const value = $ey[1];
			const $eA = held3;
			let $eC = null;
			if ($eA[0] === 0 || provisional(self, key)) {
				$eC = [ 0, __clone(value) ];
			} else {
				$eC = [ 1 ];
			}
			$ez = $eC;
		}
		$ex = $ez;
	}
	return $ex;
}
function derive(body, caller) {
	return [ body, caller ];
}
function distinct(self, caller) {
	return [ self, caller ];
}
function is_none(self) {
	const $eU = self;
	return $eU[0] === 1;
}
function run_body(runs, body) {
	const run = renew(runs);
	const $fd = nursery(run);
	let $fe = null;
	if ($fd[0] === 0) {
		const nursery2 = $fd[1];
		$fe = (($ff) => {
			return (($fg) => {
				return body($ff, $fg);
			})(nursery2);
		})(run);
	} else {
		$fe = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $fe;
}
function run_once(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body(runs, ($eM, $eN) => {
		return (($eO) => {
			return body($eM, $eO, $eN);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs, tracker, body) {
	let value = run_once(runs, tracker, ($eH, $eI, $eJ) => {
		return body($eH, $eI, $eJ);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs, tracker, ($fE, $fF, $fG) => {
			return body($fE, $fF, $fG);
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
			if (!(eq6(now, last2.v))) {
				last2.v = now;
				wake(subscriber);
			}
			return;
		}, true), site));
		also_retiring(relayed, subscriber);
		return relayed;
	}, upstream[2] ];
}
function set3(self, value, $af, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $af);
}
function take(self, item2, $gb) {
	defer(self, () => {
		dispose(item2, $gb);
		return;
	});
	return __clone(item2);
}
function cell(self, $eF, $eG, caller) {
	const instance = start2(self);
	const pull = instance[0];
	const cached = new8(pull(), caller);
	const refresh = subscriber_of(() => {
		set3(cached, pull(), $eF, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed3 = instance[1](refresh);
	also_releasing(refreshed3, instance[2]);
	register_with_owner(refreshed3, $eF, $eG);
	return cached;
}
function memo(self, $eD, $eE, caller) {
	return [ cell(self, $eD, $eE, caller) ];
}
function view(self, slot3, $ed, $ee) {
	const ledger = __clone(self[2]);
	const key = self[1];
	return memo(distinct(derive(($ef, $eg, $eh) => {
		const $es = track(slot3, $eg);
		let $et = null;
		if ($es[0] === 0) {
			const item2 = $es[1];
			const id = key(item2);
			track2(row(ledger, id)[0], $eg);
			$et = over3(ledger, id, [ 0, item2 ]);
		} else {
			$et = [ 1 ];
		}
		return $et;
	}, "std/src/reactive/lens/loaded.vl:507:3"), "std/src/reactive/lens/loaded.vl:516:6"), $ed, $ee, "std/src/reactive/lens/loaded.vl:516:17");
}
function observe(signal, observer) {
	const cell2 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $gh = [ 0, cell2 ];
		let $gi = null;
		if ($gh[0] === 0) {
			const live = $gh[1];
			$gi = observer(live.v);
		} else {
			$gi = undefined;
		}
		return $gi;
	}));
}
function attach_observer(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get2(self));
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
function key_of(self, slot3) {
	const key = self[1];
	return map(get4(slot3), (item2) => {
		return key(item2);
	});
}
function open2(self, key, value, insert2, above, below, $gn) {
	const id = self[6].v + 1;
	self[6].v = id;
	const ledger = __clone(self);
	batch(($go) => {
		const cells = row(ledger, key);
		const found = get(ledger[0].v, key);
		const $gp = found;
		let $gq = null;
		if ($gp[0] === 0) {
			const held3 = $gp[1];
			$gq = held3;
		} else {
			ledger[1].v.push(__clone(key));
			set3(cells[1], true, [ 0, $go ], "std/src/reactive/lens/loaded.vl:176:20");
			$gq = [ [  ], 0, __clone(above), __clone(below) ];
		}
		let entry = $gq;
		if (insert2) {
			entry[2] = __clone(above);
			entry[3] = __clone(below);
		}
		entry[0].push([ id, __clone(value), insert2 ]);
		insert(ledger[0].v, key, entry);
		insert(ledger[2].v, id, key);
		set3(ledger[4], get2(ledger[4]) + 1, [ 0, $go ], "std/src/reactive/lens/loaded.vl:187:17");
		notify2(cells[0], [ 0, $go ]);
		let $gx = undefined;
		if (insert2 || (($gx = value), $gx[0] === 1)) {
			notify2(ledger[5], [ 0, $go ]);
		}
		return;
	}, $gn);
	return id;
}
function propose(self, slot3, value, $gj) {
	const $gm = key_of(self, slot3);
	if ($gm[0] === 0) {

	} else {
		return [ 1 ];
	}
	return [ 0, open2(self[2], $gm[1], [ 0, __clone(value) ], false, [ 1 ], [ 1 ], $gj) ];
}
function unwrap(self, caller) {
	const $gy = self;
	let $gz = null;
	if ($gy[0] === 0) {
		const x = $gy[1];
		$gz = x;
	} else {
		$gz = (() => {
			throw __panic("expected Some but got None", caller);
		})();
	}
	return $gz;
}
async function answer2(self, call, reply, $as) {
	while (!(contains_key(self[0].v, call))) {
		insert(self[1].v, call, polled(self, call) + 1);
		await (sleep(1, $as));
	}
	insert(self[2].v, call, true);
	return __clone(reply);
}
function handles(self, ids) {
	let out = [  ];
	for (const id of ids) {
		out.push(slot_of(self[0], id));
	}
	return out;
}
function get5(self) {
	return handles(self, self[1].v);
}
function keys(self) {
	return __clone(self[1].v);
}
function anchors(self, key) {
	const $gI = get(self[0].v, key);
	let $gJ = null;
	if ($gI[0] === 0) {
		const entry = $gI[1];
		$gJ = [ __clone(entry[2]), __clone(entry[3]) ];
	} else {
		$gJ = [ [ 1 ], [ 1 ] ];
	}
	return $gJ;
}
function position_of(rows3, key) {
	let at2 = 0;
	for (const row2 of rows3) {
		if (row2[0] === key) {
			return [ 0, at2 ];
		}
		at2 = at2 + 1;
	}
	return [ 1 ];
}
function landing_among(rows3, above, below) {
	const $gL = above;
	let $gM = undefined;
	if ($gL[0] === 0 && (($gM = position_of(rows3, $gL[1])), $gM[0] === 0)) {
		return $gM[1] + 1;
	}
	const $gN = below;
	let $gO = undefined;
	if ($gN[0] === 0 && (($gO = position_of(rows3, $gN[1])), $gO[0] === 0)) {
		return $gO[1];
	}
	const $gP = above;
	if ($gP[0] === 0) {
		return rows3.length;
	}
	return 0;
}
function hides(self, key) {
	const $gQ = showing(self, key);
	if ($gQ[0] === 0) {
		const $gR = $gQ[1][1];
		return $gR[0] === 1;
	}
	return false;
}
function compose(self, with_hidden) {
	const key = self[1];
	let all3 = [  ];
	for (const slot3 of get5(self[4])) {
		const $gF = get4(slot3);
		if ($gF[0] === 0) {
			all3.push([ key($gF[1]), [ 0, __clone(slot3) ] ]);
		}
	}
	for (const row_key of keys(self[2])) {
		let $gG = undefined;
		let $gH = undefined;
		if (provisional(self[2], row_key) && (($gG = find(self[3], row_key)), $gG[0] === 1) && (($gH = over3(self[2], row_key, [ 1 ])), $gH[0] === 0)) {
			const $gK = anchors(self[2], row_key);
			const above = $gK[0];
			const below = $gK[1];
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
function landing(self, key) {
	let at2 = 0;
	for (const row2 of compose(self, true)) {
		if (row2[0] === key) {
			return at2;
		}
		const $gS = row2[1];
		if ($gS[0] === 0) {
			at2 = at2 + 1;
		}
	}
	return at2;
}
function close2(self, id, accepted, $gT) {
	const $gU = get(self[2].v, id);
	let $gV = null;
	if ($gU[0] === 0) {
		const held3 = $gU[1];
		$gV = held3;
	} else {
		return [ 1 ];
	}
	const key = $gV;
	const found = get(self[0].v, key);
	const $gW = found;
	if ($gW[0] === 0) {

	} else {
		return [ 1 ];
	}
	let entry = __clone($gW[1]);
	let ended = [ 1 ];
	let kept = [  ];
	for (const proposal of entry[0]) {
		if (proposal[0] === id) {
			ended = [ 0, __clone(proposal) ];
		} else {
			kept.push(__clone(proposal));
		}
	}
	const $gX = ended;
	if ($gX[0] === 0) {

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
	batch(($gY) => {
		const cells = row(ledger, key);
		remove(ledger[2].v, id);
		if (settled) {
			remove(ledger[0].v, key);
			let order = [  ];
			for (const held_key of ledger[1].v) {
				if (held_key !== key) {
					order.push(__clone(held_key));
				}
			}
			ledger[1].v = order;
			set3(cells[1], false, [ 0, $gY ], "std/src/reactive/lens/loaded.vl:235:19");
		} else {
			insert(ledger[0].v, key, entry);
		}
		set3(ledger[4], get2(ledger[4]) - 1, [ 0, $gY ], "std/src/reactive/lens/loaded.vl:239:17");
		notify2(cells[0], [ 0, $gY ]);
		let $gZ = undefined;
		if ($gX[1][2] || (($gZ = $gX[1][1]), $gZ[0] === 1)) {
			notify2(ledger[5], [ 0, $gY ]);
		}
		return;
	}, $gT);
	return [ 0, [ __clone(key), __clone($gX[1][1]), $gX[1][2], lands ] ];
}
function remove_at2(self, at2, $bb) {
	return splice4(self, at2, 1, [  ], $bb);
}
function insert_at2(self, at2, value, $cK) {
	return splice4(self, at2, 0, [ __clone(value) ], $cK);
}
function answer3(self, id, accepted, reply, $gB) {
	const overlay2 = __clone(self);
	let answered = false;
	batch(($gC) => {
		const $gD = get(overlay2[2][2].v, id);
		let $gE = null;
		if ($gD[0] === 0) {
			const key = $gD[1];
			$gE = landing(overlay2, key);
		} else {
			$gE = 0;
		}
		const landing2 = $gE;
		const $ha = close2(overlay2[2], id, accepted, [ 0, $gC ]);
		if ($ha[0] === 0) {

		} else {
			return;
		}
		answered = true;
		if (!($ha[1][3])) {
			return;
		}
		const $hb = reply;
		let $hc = null;
		if ($hb[0] === 0) {
			const item2 = $hb[1];
			$hc = [ 0, __clone(item2) ];
		} else {
			$hc = $ha[1][1];
		}
		const landed3 = __clone($hc);
		const lens2 = __clone(overlay2[0]);
		const $hd = landed3;
		let $he = null;
		if ($hd[0] === 1) {
			const $hf = held2(overlay2, $ha[1][0]);
			let $hg = undefined;
			if ($hf[0] === 0 && (($hg = at($hf[1])), $hg[0] === 0)) {
				remove_at2(lens2, $hg[1], [ 0, $gC ]);
			}
			$he = undefined;
		} else {
			const item3 = $hd[1];
			const server_key = overlay2[1](item3);
			const $hh = held2(overlay2, $ha[1][0]);
			let $hi = null;
			if ($hh[0] === 0) {
				$hi = at($hh[1]);
			} else {
				$hi = and_then(held2(overlay2, server_key), (slot3) => {
					return at(slot3);
				});
			}
			const target = $hi;
			const $hj = target;
			let $hk = null;
			if ($hj[0] === 0) {
				const at2 = $hj[1];
				if (!(eq6(peek2(lens2, (list) => {
					return __list_get(list, at2);
				}), [ 0, __clone(item3) ]))) {
					replace2(lens2, at2, item3, [ 0, $gC ]);
				}
				$hk = undefined;
			} else {
				if ($ha[1][2]) {
					insert_at2(lens2, landing2, item3, [ 0, $gC ]);
				}
				$hk = undefined;
			}
			$he = $hk;
		}
		return $he;
	}, $gB);
	return answered;
}
function confirm(self, id, $gA) {
	return answer3(self, id, true, [ 1 ], $gA);
}
function pending(self) {
	return [ __clone(self[2][4]) ];
}
function run_with_owner(owner, body) {
	return body(owner);
}
function derive2(self, transform, caller) {
	return [ self, transform, caller ];
}
function pending2() {
	return [ new8([ 0 ], "std/src/reactive/transient.vl:239:34"), __shared_new(0) ];
}
function refreshed2(self) {
	const $hC = latest(self);
	let $hD = null;
	if ($hC[0] === 0) {
		const value = $hC[1];
		$hD = [ 2, value ];
	} else {
		$hD = [ 0 ];
	}
	return $hD;
}
function claim2(self, $hz) {
	const mine = self[1].v + 1;
	self[1].v = mine;
	set3(self[0], refreshed2(get2(self[0])), $hz, "std/src/reactive/transient.vl:247:13");
	return mine;
}
function current(self, mine) {
	return self[1].v === mine;
}
function settle2(self, mine, outcome, $hM) {
	let $hP = null;
	if (current(self, mine)) {
		const $hN = outcome;
		let $hO = null;
		if ($hN[0] === 0) {
			const value = $hN[1];
			$hO = set3(self[0], [ 1, __clone(value) ], $hM, "std/src/reactive/transient.vl:266:32");
		} else {
			const error = $hN[1];
			$hO = set3(self[0], failed(get2(self[0]), error), $hM, "std/src/reactive/transient.vl:267:33");
		}
		$hP = $hO;
	}
	return $hP;
}
function start3(self) {
	return [ () => {
		return get2(self);
	}, (subscriber) => {
		return on_settle(self, subscriber);
	}, () => {
		return;
	} ];
}
function run_body2(runs, body) {
	const run = renew(runs);
	const $hU = nursery(run);
	let $hV = null;
	if ($hU[0] === 0) {
		const nursery2 = $hU[1];
		$hV = (($ff) => {
			return (($fg) => {
				return body($ff, $fg);
			})(nursery2);
		})(run);
	} else {
		$hV = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $hV;
}
function run_once2(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body2(runs, ($eM, $eN) => {
		return (($eO) => {
			return body($eM, $eO, $eN);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked2(runs, tracker, body) {
	let value = run_once2(runs, tracker, ($eH, $eI, $eJ) => {
		return body($eH, $eI, $eJ);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once2(runs, tracker, ($fE, $fF, $fG) => {
			return body($fE, $fF, $fG);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function start4(self) {
	const transform = self[1];
	const site = __clone(self[2]);
	const upstream = start3(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new4();
	const tracker = new_tracker([ 0, site ]);
	return [ () => {
		const value = pull();
		return run_tracked2(runs, tracker, ($hR, $hS, $hT) => {
			return transform(value, $hR, $hS, $hT);
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
function observe_flow(flow, observer, immediately, caller) {
	const instance = start4(flow);
	const pull = instance[0];
	if (!(immediately)) {
		pull();
	}
	const subscriber = mint_subscriber(() => {
		return observer(pull());
	});
	if (tracing.v) {
		trace_hooks.v[8](subscriber[0], caller);
	}
	const subscription = instance[1](subscriber);
	also_releasing(subscription, instance[2]);
	if (immediately) {
		observer(pull());
	}
	return subscription;
}
function sub(self, observer, caller) {
	return observe_flow(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true, caller);
}
function hold_for_the_program(self, subscription) {

}
function retire2(self) {
	self[1].v = self[1].v + 1;
}
function hold(self, subscription, $hW, $hX) {
	const previous = __clone(subscription[3].v);
	subscription[3].v = [ 0, () => {
		retire2(self);
		const $hY = previous;
		let $hZ = null;
		if ($hY[0] === 0) {
			const earlier = $hY[1];
			$hZ = earlier();
		} else {
			$hZ = undefined;
		}
		return $hZ;
	} ];
	register_with_owner(subscription, $hW, $hX);
}
function seal_results(pipe, global, $hv, $hw, $hx) {
	const seal = pending2();
	as_derivation();
	const subscription = sub(pipe, (task, $hy) => {
		const mine = claim2(seal, $hv);
		__task(async () => {
			const outcome = await (__guarded_async(async () => {
				return await (task);
			}));
			let $hQ = null;
			if (current(seal, mine)) {
				const $hK = outcome;
				let $hL = null;
				if ($hK[0] === 0) {
					const answered = $hK[1];
					$hL = settle2(seal, mine, answered, $hv);
				} else {
					const message = $hK[1];
					$hL = (() => {
						throw __panic(message, "std/src/reactive/transient.vl:381:26");
					})();
				}
				$hQ = $hL;
			}
			return $hQ;
		}, "seal_results", __nursery_of($hx));
		return;
	}, "std/src/reactive/transient.vl:372:26");
	if (global) {
		hold_for_the_program(seal, subscription);
	} else {
		hold(seal, subscription, $hv, $hw);
	}
	return seal;
}
function transient(self, $hs, $ht, $hu) {
	return seal_results(self, false, $hs, $ht, $hu);
}
function on_settle3(self, subscriber) {
	return attach(self, subscriber);
}
function start5(self) {
	return [ () => {
		return get2(self);
	}, (subscriber) => {
		return on_settle3(self, subscriber);
	}, () => {
		return;
	} ];
}
function run_once3(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body2(runs, ($eM, $eN) => {
		return (($eO) => {
			return body($eM, $eO, $eN);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked3(runs, tracker, body) {
	let value = run_once3(runs, tracker, ($eH, $eI, $eJ) => {
		return body($eH, $eI, $eJ);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once3(runs, tracker, ($fE, $fF, $fG) => {
			return body($fE, $fF, $fG);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function start6(self) {
	const transform = self[1];
	const site = __clone(self[2]);
	const upstream = start5(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new4();
	const tracker = new_tracker([ 0, site ]);
	return [ () => {
		const value = pull();
		return run_tracked3(runs, tracker, ($hR, $hS, $hT) => {
			return transform(value, $hR, $hS, $hT);
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
function observe_flow2(flow, observer, immediately, caller) {
	const instance = start6(flow);
	const pull = instance[0];
	if (!(immediately)) {
		pull();
	}
	const subscriber = mint_subscriber(() => {
		return observer(pull());
	});
	if (tracing.v) {
		trace_hooks.v[8](subscriber[0], caller);
	}
	const subscription = instance[1](subscriber);
	also_releasing(subscription, instance[2]);
	if (immediately) {
		observer(pull());
	}
	return subscription;
}
function on_change2(self, observer, caller) {
	return observe_flow2(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false, caller);
}
function sub2(self, observer, caller) {
	return observe_flow2(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true, caller);
}
function run_tracked4(runs, tracker, body) {
	let value = run_once3(runs, tracker, ($eH, $eI, $eJ) => {
		return body($eH, $eI, $eJ);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once3(runs, tracker, ($fE, $fF, $fG) => {
			return body($fE, $fF, $fG);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function effect(self, body, $ii, $ij, caller) {
	const runs = new4();
	const tracker = new_tracker([ 0, caller ]);
	let latest2 = [ 1 ];
	const subscription = sub2(self, (value2, $ik) => {
		latest2 = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			run_tracked4(runs, tracker, ($il, $im, $in) => {
				return body(value2, $il, $im, $in);
			});
		}
		return;
	}, caller);
	const rerun = subscriber_of(() => {
		let $iv = null;
		if (tracker[0].v[2]) {
			const $iq = latest2;
			let $ir = null;
			if ($iq[0] === 0) {
				const value2 = __clone($iq[1]);
				$ir = run_tracked4(runs, tracker, ($is, $it, $iu) => {
					return body(value2, $is, $it, $iu);
				});
			} else {
				$ir = undefined;
			}
			$iv = $ir;
		}
		return $iv;
	}, false);
	if (tracing.v) {
		trace_hooks.v[8](rerun[0], caller);
	}
	attach_tracker(tracker, rerun);
	const $iw = latest2;
	let $ix = null;
	if ($iw[0] === 0) {
		const value = __clone($iw[1]);
		$ix = run_tracked4(runs, tracker, ($iy, $iz, $iA) => {
			return body(value, $iy, $iz, $iA);
		});
	} else {
		$ix = undefined;
	}
	$ix;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	take(get_owner($ij), subscription, $ii);
}
const $if = Object.create({start: start6, on_change: on_change2, sub: sub2, effect: effect});
function states(self) {
	const pipe = [ derive2(__clone(self[0]), (state, $ic, $id, $ie) => {
		return __clone(state);
	}, "std/src/reactive/transient.vl:314:56"), $if ];
	return pipe;
}
function follow4(self, source, $ia, $ib) {
	const feed = __clone(self);
	const mine = __shared_new(0);
	const watching = (($iF) => {
		return $iF[1].on_change($iF[0], (state, $iC) => {
			const $iD = state;
			let $iE = null;
			if ($iD[0] === 0) {
				$iE = mine.v = load(feed, $ia);
			} else if ($iD[0] === 2) {
				const _old = $iD[1];
				$iE = mine.v = load(feed, $ia);
			} else if ($iD[0] === 1) {
				const list = $iD[1];
				if (mine.v === 0) {
					mine.v = load(feed, $ia);
				}
				settle(feed, mine.v, [ 0, __clone(list) ], $ia);
				mine.v = 0;
				$iE = undefined;
			} else if ($iD[0] === 3) {
				const error = $iD[1];
				const _stale = $iD[2];
				if (mine.v === 0) {
					mine.v = load(feed, $ia);
				}
				settle(feed, mine.v, [ 1, __clone(error) ], $ia);
				mine.v = 0;
				$iE = undefined;
			} else {
				$iE = undefined;
			}
			return $iE;
		}, "std/src/reactive/lens/loaded.vl:1035:34");
	})(states(source));
	return register_with_owner(watching, $ia, $ib);
}
function attach_observer3(self, observer, immediately) {
	const subscription = on_settle2(self, mint_subscriber(() => {
		return observer(get4(self));
	}));
	if (immediately) {
		observer(get4(self));
	}
	return subscription;
}
function on_change3(self, observer, caller) {
	return place_subscription(attach_observer3(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function start7(self) {
	return [ () => {
		return get2(self);
	}, (subscriber) => {
		return on_settle3(self, subscriber);
	}, () => {
		return;
	} ];
}
function start8(self) {
	const transform = self[1];
	const site = __clone(self[2]);
	const upstream = start7(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new4();
	const tracker = new_tracker([ 0, site ]);
	return [ () => {
		const value = pull();
		return run_tracked4(runs, tracker, ($hR, $hS, $hT) => {
			return transform(value, $hR, $hS, $hT);
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
function observe_flow3(flow, observer, immediately, caller) {
	const instance = start8(flow);
	const pull = instance[0];
	if (!(immediately)) {
		pull();
	}
	const subscriber = mint_subscriber(() => {
		return observer(pull());
	});
	if (tracing.v) {
		trace_hooks.v[8](subscriber[0], caller);
	}
	const subscription = instance[1](subscriber);
	also_releasing(subscription, instance[2]);
	if (immediately) {
		observer(pull());
	}
	return subscription;
}
function on_change4(self, observer, caller) {
	return observe_flow3(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false, caller);
}
function sub3(self, observer, caller) {
	return observe_flow3(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true, caller);
}
function effect2(self, body, $ii, $ij, caller) {
	const runs = new4();
	const tracker = new_tracker([ 0, caller ]);
	let latest2 = [ 1 ];
	const subscription = sub3(self, (value2, $ik) => {
		latest2 = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			run_tracked4(runs, tracker, ($il, $im, $in) => {
				return body(value2, $il, $im, $in);
			});
		}
		return;
	}, caller);
	const rerun = subscriber_of(() => {
		let $jc = null;
		if (tracker[0].v[2]) {
			const $ja = latest2;
			let $jb = null;
			if ($ja[0] === 0) {
				const value2 = __clone($ja[1]);
				$jb = run_tracked4(runs, tracker, ($is, $it, $iu) => {
					return body(value2, $is, $it, $iu);
				});
			} else {
				$jb = undefined;
			}
			$jc = $jb;
		}
		return $jc;
	}, false);
	if (tracing.v) {
		trace_hooks.v[8](rerun[0], caller);
	}
	attach_tracker(tracker, rerun);
	const $jd = latest2;
	let $je = null;
	if ($jd[0] === 0) {
		const value = __clone($jd[1]);
		$je = run_tracked4(runs, tracker, ($iy, $iz, $iA) => {
			return body(value, $iy, $iz, $iA);
		});
	} else {
		$je = undefined;
	}
	$je;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	take(get_owner($ij), subscription, $ii);
}
const $iX = Object.create({start: start8, on_change: on_change4, sub: sub3, effect: effect2});
function states2(self) {
	const pipe = [ derive2(__clone(self[0]), (state, $iU, $iV, $iW) => {
		return __clone(state);
	}, "std/src/reactive/transient.vl:479:56"), $iX ];
	return pipe;
}
function follow5(self, source, $ia, $ib) {
	const feed = __clone(self);
	const mine = __shared_new(0);
	const watching = (($jh) => {
		return $jh[1].on_change($jh[0], (state, $iC) => {
			const $jf = state;
			let $jg = null;
			if ($jf[0] === 0) {
				$jg = mine.v = load(feed, $ia);
			} else if ($jf[0] === 2) {
				const _old = $jf[1];
				$jg = mine.v = load(feed, $ia);
			} else if ($jf[0] === 1) {
				const list = $jf[1];
				if (mine.v === 0) {
					mine.v = load(feed, $ia);
				}
				settle(feed, mine.v, [ 0, __clone(list) ], $ia);
				mine.v = 0;
				$jg = undefined;
			} else if ($jf[0] === 3) {
				const error = $jf[1];
				const _stale = $jf[2];
				if (mine.v === 0) {
					mine.v = load(feed, $ia);
				}
				settle(feed, mine.v, [ 1, __clone(error) ], $ia);
				mine.v = 0;
				$jg = undefined;
			} else {
				$jg = undefined;
			}
			return $jg;
		}, "std/src/reactive/lens/loaded.vl:1035:34");
	})(states2(source));
	return register_with_owner(watching, $ia, $ib);
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
(async () => {
	const calls = server();
	const feed = new12((item2) => {
		return item2[0];
	}, [ 1 ], [ 1 ]);
	fetch2(feed, async ($ar) => {
		return await (answer(calls, 1, [ 0, [ m(1, "a"), m(2, "b") ] ], [ 0, $ar ]));
	}, [ 1 ]);
	assert(size3(items(feed)) === 0 && !(ready(get3(phase(feed)))), "nothing yet", "lens-loaded-tasks.vl:117:2");
	release3(calls, 1);
	await (until("the first load lands", () => {
		return ready(get3(phase(feed)));
	}, [ 1 ]));
	assert(eq(held(feed), [ 1, 2 ]), "the first load\'s rows", "lens-loaded-tasks.vl:120:2");
	const second = slot2(items(feed), 1, [ 1 ]);
	fetch2(feed, async ($cQ) => {
		return await (answer(calls, 2, [ 0, [ m(1, "SLOW") ] ], [ 0, $cQ ]));
	}, [ 1 ]);
	await (until("call 2 is waiting", () => {
		return polled(calls, 2) > 0;
	}, [ 1 ]));
	fetch2(feed, async ($cR) => {
		return await (answer(calls, 3, [ 0, [ m(1, "a"), m(2, "b2") ] ], [ 0, $cR ]));
	}, [ 1 ]);
	const stopped = polled(calls, 2);
	await (ticks(20, [ 1 ]));
	assert(polled(calls, 2) === stopped, "the superseded call stopped polling: it was cancelled", "lens-loaded-tasks.vl:127:2");
	release3(calls, 3);
	await (until("the newer load lands", () => {
		return eq6(get4(second), [ 0, m(2, "b2") ]);
	}, [ 1 ]));
	release3(calls, 2);
	await (ticks(20, [ 1 ]));
	assert(!(done(calls, 2)), "the superseded call never finished, its gate open or not", "lens-loaded-tasks.vl:132:2");
	assert(peek2(items(feed), (list) => {
		return __at(list, 0, "lens-loaded-tasks.vl:134:28")[1];
	}) === "a" && eq7(at(second), [ 0, 1 ]), "the newer load landed; the slot survived", "lens-loaded-tasks.vl:133:2");
	fetch2(feed, async ($dd) => {
		return await (answer(calls, 4, [ 1, "server error" ], [ 0, $dd ]));
	}, [ 1 ]);
	release3(calls, 4);
	await (until("the failure lands", () => {
		const $de = get3(phase(feed));
		return $de[0] === 3;
	}, [ 1 ]));
	const $df = get3(phase(feed));
	let $dg = null;
	if ($df[0] === 3) {
		const error = $df[1];
		const stale = $df[2];
		$dg = assert(error === "server error" && !(eq3(stale, [ 1 ])) && eq(held(feed), [ 1, 2 ]), "Failed, with the stale token, the rows kept", "lens-loaded-tasks.vl:141:51");
	} else {
		$dg = assert(false, "the phase is Failed", "lens-loaded-tasks.vl:145:8");
	}
	$dg;
	fetch_older(feed, async ($dm) => {
		return await (answer(calls, 5, [ 1, "timeout" ], [ 0, $dm ]));
	}, [ 1 ]);
	assert(eq4(get3(older(feed)), [ 1 ]), "the older end is loading", "lens-loaded-tasks.vl:150:2");
	release3(calls, 5);
	await (until("the page fails", () => {
		return eq4(get3(older(feed)), [ 3, "timeout" ]);
	}, [ 1 ]));
	assert(eq(held(feed), [ 1, 2 ]), "the page failed, the rows stayed", "lens-loaded-tasks.vl:153:2");
	fetch_older(feed, async ($dV) => {
		return await (answer(calls, 6, [ 0, [ [ m(-(1), "older"), m(1, "a") ], false ] ], [ 0, $dV ]));
	}, [ 1 ]);
	release3(calls, 6);
	await (until("the retry lands", () => {
		return eq4(get3(older(feed)), [ 2 ]);
	}, [ 1 ]));
	assert(eq(held(feed), [ -(1), 1, 2 ]), "the retry landed once, no duplicate", "lens-loaded-tasks.vl:160:2");
	fetch_newer(feed, async ($dW) => {
		return await (answer(calls, 7, [ 0, [ [ m(9, "late") ], true ] ], [ 0, $dW ]));
	}, [ 1 ]);
	await (until("call 7 is waiting", () => {
		return polled(calls, 7) > 0;
	}, [ 1 ]));
	fetch_newer(feed, async ($ec) => {
		return await (answer(calls, 8, [ 0, [ [ m(3, "c") ], false ] ], [ 0, $ec ]));
	}, [ 1 ]);
	const stopped2 = polled(calls, 7);
	release3(calls, 8);
	release3(calls, 7);
	await (until("the newer page lands", () => {
		return eq4(get3(newer(feed)), [ 2 ]);
	}, [ 1 ]));
	await (ticks(20, [ 1 ]));
	assert(eq(held(feed), [ -(1), 1, 2, 3 ]) && polled(calls, 7) === stopped2 && !(done(calls, 7)), "the superseded page was cancelled; only the newer one landed", "lens-loaded-tasks.vl:169:2");
	const row2 = slot2(items(feed), 2, [ 1 ]);
	const view2 = view(overlay(feed), row2, [ 1 ], [ 1 ]);
	const runs = __shared_new(0);
	on_change(__clone(view2), (_item, $gf) => {
		const $gg = [ runs, "v" ];
		return runs.v = $gg[0][$gg[1]] + 1;
	}, "lens-loaded-tasks.vl:180:20");
	const edit = unwrap(propose(overlay(feed), row2, m(2, "edited"), [ 1 ]), "lens-loaded-tasks.vl:181:57");
	const overlay2 = overlay(feed);
	__task(async () => {
		await (answer2(calls, 9, true, [ 1 ]));
		confirm(overlay2, edit, [ 1 ]);
		return;
	}, "main");
	fetch2(feed, async ($hl) => {
		return await (answer(calls, 10, [ 0, [ m(-(1), "older"), m(1, "a"), m(2, "b2 (before the edit)"), m(3, "c") ] ], [ 0, $hl ]));
	}, [ 1 ]);
	release3(calls, 10);
	await (until("the reload lands", () => {
		return eq6(get4(row2), [ 0, m(2, "b2 (before the edit)") ]);
	}, [ 1 ]));
	assert(eq6(get3(view2), [ 0, m(2, "edited") ]) && get3(pending(overlay(feed))) === 1, "the reload landed under the pending edit", "lens-loaded-tasks.vl:195:2");
	release3(calls, 9);
	await (until("the confirmation lands", () => {
		return get3(pending(overlay(feed))) === 0;
	}, [ 1 ]));
	assert(eq6(get4(row2), [ 0, m(2, "edited") ]) && eq6(get3(view2), [ 0, m(2, "edited") ]), "confirmed", "lens-loaded-tasks.vl:201:2");
	assert(runs.v === 1, "the view woke once: the edit; not the reload, not the confirm", "lens-loaded-tasks.vl:202:2");
	const page = new4();
	const on_page = run_with_owner(page, ($hm) => {
		return new12((item2) => {
			return item2[0];
		}, [ 1 ], [ 0, $hm ]);
	});
	fetch2(on_page, async ($hn) => {
		return await (answer(calls, 11, [ 0, [ m(9, "late") ] ], [ 0, $hn ]));
	}, [ 1 ]);
	await (until("call 11 is waiting", () => {
		return polled(calls, 11) > 0;
	}, [ 1 ]));
	dispose2(page);
	const stopped3 = polled(calls, 11);
	release3(calls, 11);
	await (ticks(20, [ 1 ]));
	assert(polled(calls, 11) === stopped3 && !(done(calls, 11)) && size3(items(on_page)) === 0, "the owner\'s end cancelled the load and dropped its reply", "lens-loaded-tasks.vl:214:2");
	const query = new8(1, "lens-loaded-tasks.vl:222:26");
	const results = transient(derive2(__clone(query), (asked, $ho, $hp, $hq) => {
		return __task(async () => {
			return await (search(calls, asked, [ 0, $hq ]));
		}, "main", $hq);
	}, "lens-loaded-tasks.vl:224:4"), [ 1 ], [ 1 ], [ 1 ]);
	const found = new12((item2) => {
		return item2[0];
	}, [ 1 ], [ 1 ]);
	follow4(found, results, [ 1 ], [ 1 ]);
	release3(calls, 101);
	await (until("the first search lands", () => {
		return eq(held(found), [ 1, 100 ]);
	}, [ 1 ]));
	const shared = slot2(items(found), 1, [ 1 ]);
	const shared_runs = __shared_new(0);
	on_change3(__clone(shared), (_item, $iG) => {
		const $iH = [ shared_runs, "v" ];
		return shared_runs.v = $iH[0][$iH[1]] + 1;
	}, "lens-loaded-tasks.vl:232:18");
	set3(query, 2, [ 1 ], "lens-loaded-tasks.vl:233:8");
	await (until("search 2 is waiting", () => {
		return polled(calls, 102) > 0;
	}, [ 1 ]));
	set3(query, 3, [ 1 ], "lens-loaded-tasks.vl:235:8");
	release3(calls, 103);
	await (until("the newest search lands", () => {
		return eq(held(found), [ 3, 100 ]);
	}, [ 1 ]));
	release3(calls, 102);
	await (ticks(20, [ 1 ]));
	assert(eq(held(found), [ 3, 100 ]), "the superseded search never landed", "lens-loaded-tasks.vl:240:2");
	assert(!(done(calls, 102)), "the superseded search was cancelled with its run", "lens-loaded-tasks.vl:241:2");
	assert(eq7(at(shared), [ 0, 1 ]) && eq6(get4(shared), [ 0, m(100, "shared row") ]) && shared_runs.v === 0, "the shared row kept its slot and never woke", "lens-loaded-tasks.vl:242:2");
	assert(ready(get3(phase(found))), "Ready", "lens-loaded-tasks.vl:248:2");
	const once = new12((item2) => {
		return item2[0];
	}, [ 1 ], [ 1 ]);
	follow5(once, new7(__task(async () => {
		return await (answer(calls, 200, [ 0, [ m(2, "two"), m(100, "shared row") ] ], [ 1 ]));
	}, "main"), [ 1 ], [ 1 ]), [ 1 ], [ 1 ]);
	assert(!(ready(get3(phase(once)))) && size3(items(once)) === 0, "the task is pending", "lens-loaded-tasks.vl:256:2");
	release3(calls, 200);
	await (until("the task lands", () => {
		return ready(get3(phase(once)));
	}, [ 1 ]));
	assert(eq(held(once), [ 2, 100 ]), "a TaskSource lands too", "lens-loaded-tasks.vl:259:2");
	console.log("loaded tasks: all passed");
})().catch(($ji) => {
	console.error(String($ji));
	process.exit(1);
});
//# sourceMappingURL=lens-loaded-tasks.mjs.map
