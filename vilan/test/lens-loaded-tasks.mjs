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
	let $cN = null;
	if (!(condition)) {
		$cN = (() => {
			throw __panic(message, caller);
		})();
	}
	return $cN;
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
	const $di = self;
	let $dl = null;
	if ($di[0] === 0) {
		const $dj = b;
		let $dk = null;
		if ($dj[0] === 0) {
			$dk = $di[1] === $dj[1];
		} else {
			$dk = false;
		}
		$dl = $dk;
	} else {
		const $dm = b;
		$dl = $dm[0] === 1;
	}
	return $dl;
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
		let $al = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$al = undefined;
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
		$al;
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
	const $fq = turn;
	let $fr = null;
	if ($fq[0] === 0) {
		const ambient = $fq[1];
		$fr = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $fs = last(draining_turns.v);
		let $ft = null;
		if ($fs[0] === 0) {
			const draining = $fs[1];
			$ft = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$ft = undefined;
		}
		$fr = $ft;
	}
	return $fr;
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
		const $fO = previous;
		let $fP = null;
		if ($fO[0] === 0) {
			const release7 = $fO[1];
			$fP = release7();
		} else {
			$fP = undefined;
		}
		return $fP;
	} ];
}
function detached(id, live, release7) {
	let subscribers = [  ];
	const empty = __shared_new(subscribers);
	return [ empty, id, live, __shared_new([ 0, release7 ]) ];
}
function dispose(self, $gd) {
	const $ge = $gd;
	let $gf = null;
	if ($ge[0] === 0) {
		const established = $ge[1];
		$gf = [ 0, established ];
	} else {
		$gf = last(draining_turns.v);
	}
	const ambient = $gf;
	release_under(self, ambient);
}
function detach(handle) {
	const $fx = last(releasing_turns.v);
	let $fy = null;
	if ($fx[0] === 0) {
		const at_release = $fx[1];
		$fy = at_release;
	} else {
		$fy = last(draining_turns.v);
	}
	const turn = $fy;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $fz = [ 0, handle[0] ];
	let $fA = null;
	if ($fz[0] === 0) {
		const subscribers = $fz[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$fA = undefined;
	} else {
		$fA = undefined;
	}
	$fA;
	const $fB = ambient;
	let $fC = null;
	if ($fB[0] === 0) {
		const turn = $fB[1];
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
		$fC = undefined;
	} else {
		$fC = undefined;
	}
	$fC;
	const $fD = handle[3].v;
	let $fE = null;
	if ($fD[0] === 0) {
		const release7 = $fD[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release7, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$fE = undefined;
	} else {
		$fE = undefined;
	}
	return $fE;
}
function new4() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $U = null;
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
		$U = undefined;
	}
	return $U;
}
function renew(self) {
	const $eQ = self[0].v[3];
	let $eR = null;
	if ($eQ[0] === 0) {
		const nursery2 = __clone($eQ[1]);
		let $eS = null;
		if (has_spawned(nursery2)) {
			$eS = [ 1 ];
		} else {
			$eS = [ 0, nursery2 ];
		}
		$eR = $eS;
	} else {
		$eR = [ 1 ];
	}
	const carried = $eR;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $fd = null;
	if (is_disposed(self)) {
		$fd = [ 1 ];
	} else {
		$fd = self[0].v[3];
	}
	return $fd;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $fc = null;
	if (!(is_disposed(self))) {
		const held3 = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $eT = held3[3];
		let $eU = null;
		if ($eT[0] === 0) {
			const nursery2 = $eT[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$eU = undefined;
		} else {
			$eU = undefined;
		}
		$eU;
		let $fb = null;
		if (held3[2]) {
			let failure = [ 1 ];
			for (const cleanup of held3[1].v) {
				const $eW = __guarded(cleanup);
				let $eX = null;
				if ($eW[0] === 0) {
					const message = $eW[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$eX = undefined;
				} else {
					$eX = undefined;
				}
				$eX;
			}
			const $eZ = failure;
			let $fa = null;
			if ($eZ[0] === 0) {
				const message2 = $eZ[1];
				$fa = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$fa = undefined;
			}
			$fb = $fa;
		}
		$fc = $fb;
	}
	return $fc;
}
function get_owner($iC) {
	return $iC;
}
function register_with_owner(subscription, $fY, $fZ) {
	const $ga = $fZ;
	let $gb = null;
	if ($ga[0] === 0) {
		const owner = $ga[1];
		$gb = take(owner, subscription, $fY);
	} else {
		$gb = __clone(subscription);
	}
	return $gb;
}
function defer_to_owner(cleanup, $R) {
	const $S = $R;
	let $T = null;
	if ($S[0] === 0) {
		const owner = $S[1];
		$T = defer(owner, cleanup);
	} else {
		$T = undefined;
	}
	return $T;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker(site) {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ], site ]) ];
}
function lists_of(tracker) {
	const $er = tracker[0].v[6];
	let $es = null;
	if ($er[0] === 0) {
		const lists = $er[1];
		$es = lists;
	} else {
		trackers_allocated_count.v = trackers_allocated_count.v + 1;
		const made = __shared_new([ [  ], [  ], [  ] ]);
		tracker[0].v[6] = [ 0, made ];
		$es = made;
	}
	return $es;
}
function has_run(tracker) {
	return tracker[0].v[0] > 0;
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $eL = tracker[0].v[6];
	let $eM = null;
	if ($eL[0] === 0) {
		const lists = $eL[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$eM = undefined;
	} else {
		$eM = undefined;
	}
	$eM;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $fi = tracker[0].v[6];
	let $fj = null;
	if ($fi[0] === 0) {
		const lists = $fi[1];
		$fj = lists;
	} else {
		return;
		$fj = undefined;
	}
	const lists2 = $fj;
	const $fk = tracker[0].v[5];
	let $fl = null;
	if ($fk[0] === 0) {
		const target = __clone($fk[1]);
		$fl = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$fl = undefined;
	}
	$fl;
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
		const $fo = reusable(held3, kept, dependency[0], position);
		let $fp = null;
		if ($fo[0] === 0) {
			const index2 = $fo[1];
			__at_put(kept, index2, true, "std/src/reactive.vl:1715:5");
			next.push(__clone(__at(held3, index2, "std/src/reactive.vl:1716:15")));
			$fp = undefined;
		} else {
			next.push([ __clone(dependency[0]), dependency[1](relay_for(tracker, target)) ]);
			$fp = undefined;
		}
		$fp;
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
	const $fm = identity3;
	let $fn = null;
	if ($fm[0] === 0) {
		const wanted = $fm[1];
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
		$fn = [ 1 ];
	} else {
		$fn = [ 1 ];
	}
	return $fn;
}
function same_identity(identity3, wanted) {
	const $ep = identity3;
	let $eq = null;
	if ($ep[0] === 0) {
		const held3 = $ep[1];
		$eq = held3 === wanted;
	} else {
		$eq = false;
	}
	return $eq;
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
	const $fu = tracker[0].v[7];
	let $fv = null;
	if ($fu[0] === 0) {
		const site = __clone($fu[1]);
		$fv = place_subscriber(relay[0], site);
	} else {
		$fv = undefined;
	}
	$fv;
	return relay;
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $fI = tracker[0].v[6];
	let $fJ = null;
	if ($fI[0] === 0) {
		const lists = $fI[1];
		$fJ = lists;
	} else {
		return;
		$fJ = undefined;
	}
	const lists2 = $fJ;
	let $fK = null;
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
		$fK = undefined;
	}
	return $fK;
}
function forget_reads(tracker) {
	const $fL = tracker[0].v[6];
	let $fM = null;
	if ($fL[0] === 0) {
		const lists = $fL[1];
		$fM = lists;
	} else {
		return;
		$fM = undefined;
	}
	const lists2 = $fM;
	let $fN = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$fN = undefined;
	}
	$fN;
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
	const $el = tracker[0].v[6];
	let $em = null;
	if ($el[0] === 0) {
		const lists = $el[1];
		$em = lists;
	} else {
		return true;
		$em = undefined;
	}
	const lists2 = $em;
	const $en = identity3;
	let $eo = null;
	if ($en[0] === 0) {
		const wanted = $en[1];
		const count2 = lists2.v[0].length;
		let index2 = 0;
		while (index2 < count2) {
			if (same_identity(__at(lists2.v[0], index2, "std/src/reactive.vl:1877:22")[0], wanted)) {
				return false;
			}
			index2 = index2 + 1;
		}
		$eo = true;
	} else {
		$eo = true;
	}
	return $eo;
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
		const $fW = previous;
		let $fX = null;
		if ($fW[0] === 0) {
			const earlier = $fW[1];
			$fX = earlier();
		} else {
			$fX = undefined;
		}
		return $fX;
	} ];
}
function placed_at(subscriber, site) {
	place_subscriber(subscriber[0], site);
	return subscriber;
}
function settle_together(body, $bZ) {
	const $ca = $bZ;
	let $cb = null;
	if ($ca[0] === 0) {
		const current2 = $ca[1];
		$cb = body(current2);
	} else {
		const $cc = last(draining_turns.v);
		let $cd = null;
		if ($cc[0] === 0) {
			const draining = $cc[1];
			$cd = body(draining);
		} else {
			$cd = batch(body, $bZ);
		}
		$cb = $cd;
	}
	return $cb;
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
	const $bu = seat(self, at2);
	const place = $bu[0];
	const index2 = $bu[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:265:15");
	let $bv = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:269:5"), index2, -(1), "std/src/reactive/lens.vl:269:35");
			made = made + 1;
		}
		$bv = undefined;
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
	$bv;
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
		const $bs = seat(self, at2);
		const place = $bs[0];
		const index2 = $bs[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:306:16");
		const held3 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:307:15").length;
		let $bt = null;
		if (left < held3 - index2) {
			$bt = left;
		} else {
			$bt = held3 - index2;
		}
		const take2 = $bt;
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
	const $bq = change2;
	let $br = null;
	if ($bq[0] === 0) {
		const at2 = $bq[1];
		const removed = $bq[2];
		const inserted = $bq[3];
		close(self, at2, removed, wakes);
		widen(self, at2, inserted);
		$br = undefined;
	} else if ($bq[0] === 1) {
		const at3 = $bq[1];
		const count2 = $bq[2];
		$br = widen(self, at3, count2);
	} else if ($bq[0] === 3) {
		const at4 = $bq[1];
		const count3 = $bq[2];
		$br = close(self, at4, count3, wakes);
	} else if ($bq[0] === 2) {
		const at5 = $bq[1];
		let $bx = null;
		if (at5 < self[6]) {
			const $bw = seat(self, at5);
			const place = $bw[0];
			const index2 = $bw[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:396:29")), "std/src/reactive/lens.vl:396:17"), index2, "std/src/reactive/lens.vl:396:17");
			if (slot3 !== -(1)) {
				wakes.push([ 0, slot3 ]);
			}
			$bx = undefined;
		}
		$br = $bx;
	} else if ($bq[0] === 4) {
		const at6 = $bq[1];
		const part = $bq[2];
		let $bz = null;
		if (at6 < self[6]) {
			const $by = seat(self, at6);
			const place2 = $by[0];
			const index3 = $by[1];
			const slot4 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:405:29")), "std/src/reactive/lens.vl:405:17"), index3, "std/src/reactive/lens.vl:405:17");
			if (slot4 !== -(1)) {
				wakes.push([ 1, slot4, part ]);
			}
			$bz = undefined;
		}
		$br = $bz;
	} else if ($bq[0] === 5) {
		const from = $bq[1];
		const to = $bq[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $bA = null;
		if (from > to) {
			$bA = from;
		} else {
			$bA = to;
		}
		const far = $bA;
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
		const $bB = seat(self, from);
		const left = $bB[0];
		const index4 = $bB[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:429:18");
		const slot5 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:430:16"), index4, "std/src/reactive/lens.vl:430:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:432:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:433:17");
		} else {
			renumber(self, source, index4);
		}
		restart(self);
		let $bC = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$bC = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:441:25")), "std/src/reactive/lens.vl:441:13").length ];
		} else {
			$bC = seat(self, to);
		}
		const $bD = $bC;
		const place3 = $bD[0];
		const landing2 = $bD[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:445:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:446:5"), landing2, slot5, "std/src/reactive/lens.vl:446:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing2);
		restart(self);
		$br = undefined;
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
		$br = undefined;
	}
	return $br;
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
	const $dU = [ self, other ];
	let $dV = null;
	if ($dU[0][0] === 0 && $dU[1][0] === 0) {
		$dV = true;
	} else if ($dU[0][0] === 1 && $dU[1][0] === 1) {
		$dV = true;
	} else if ($dU[0][0] === 2 && $dU[1][0] === 2) {
		$dV = true;
	} else if ($dU[0][0] === 3 && $dU[1][0] === 3) {
		const mine = $dU[0][1];
		const theirs = $dU[1][1];
		$dV = mine === theirs;
	} else {
		$dV = false;
	}
	return $dV;
}
function cancel(task) {
	const $ar = task;
	if ($ar[0] === 0) {
		$ar[1].cancel();
	}
}
function new7(task, $iJ, $iK) {
	const source = [ new8([ 0 ], "std/src/reactive/transient.vl:427:23") ];
	__task(async () => {
		const outcome = await (__guarded_async(async () => {
			return await (task);
		}));
		const $iL = outcome;
		let $iM = null;
		if ($iL[0] === 0) {
			const answered = $iL[1];
			const $iN = answered;
			let $iO = null;
			if ($iN[0] === 0) {
				const value = $iN[1];
				$iO = set3(source[0], [ 1, __clone(value) ], $iJ, "std/src/reactive/transient.vl:433:35");
			} else {
				const error = $iN[1];
				$iO = set3(source[0], [ 3, __clone(error), [ 1 ] ], $iJ, "std/src/reactive/transient.vl:434:36");
			}
			$iM = $iO;
		} else {
			const message = $iL[1];
			$iM = (() => {
				throw __panic(message, "std/src/reactive/transient.vl:436:25");
			})();
		}
		return $iM;
	}, "new", __nursery_of($iK));
	return source;
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function ambient_signal($ay) {
	const $az = $ay;
	let $aA = null;
	if ($az[0] === 0) {
		const n = $az[1];
		$aA = [ 0, n.signal_of() ];
	} else {
		$aA = [ 1 ];
	}
	return $aA;
}
function detached_nursery() {
	return __nursery_new_detached();
}
async function sleep(ms, $aw) {
	await (__sleep(clamp_delay(ms), ambient_signal($aw)));
}
function clamp_delay(ms) {
	let $ax = null;
	if (ms < 0) {
		$ax = 0;
	} else {
		$ax = ms;
	}
	return $ax;
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
	const $cM = state;
	return $cM[0] === 1;
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
async function search(calls, asked, $hs) {
	return await (answer(calls, 100 + asked, [ 0, [ m(asked, "result for " + asked), m(100, "shared row") ] ], $hs));
}
async function until(what, ready2, $cO) {
	let ticks2 = 0;
	while (!(ready2())) {
		if (ticks2 > 5000) {
			(() => {
				throw __panic("never happened: " + what, "lens-loaded-tasks.vl:92:4");
			})();
		}
		await (sleep(1, $cO));
		ticks2 = ticks2 + 1;
	}
}
async function ticks(count2, $cT) {
	let passed = 0;
	while (passed < count2) {
		await (sleep(1, $cT));
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
	const $t = as_usize(id);
	__at(watch.v[1], $t, "std/src/reactive/lens.vl:1058:2")[2] = __at(watch.v[1], $t, "std/src/reactive/lens.vl:1058:2")[2] + 1;
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
		const $w = __list_pop(keys2);
		let $x = null;
		if ($w[0] === 0) {
			const key = $w[1];
			const $y = __list_pop(values);
			let $z = null;
			if ($y[0] === 0) {
				const value = $y[1];
				$z = backwards.push([ key, value ]);
			} else {
				$z = undefined;
			}
			$x = $z;
		} else {
			$x = undefined;
		}
		$x;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $A = __list_pop(backwards);
		let $B = null;
		if ($A[0] === 0) {
			const pair = $A[1];
			$B = result.push(pair);
		} else {
			$B = undefined;
		}
		$B;
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
		const $Q = _;
		const _key = $Q[0];
		const id = $Q[1];
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
		const $u = before;
		if ($u[0] === 0) {
			release_id(watch, $u[1]);
		}
		return;
	};
	const unmap = (held3) => {
		const $v = get(found.v, held3);
		if ($v[0] === 0) {
			remove(found.v, held3);
			release_id(watch, $v[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $C = _;
			const _held = $C[0];
			const old = $C[1];
			release_id(watch, old);
		}
		found.v = new10();
		let at2 = 0;
		const count2 = peek_storage2(storage, (held3) => {
			return count(held3);
		});
		while (at2 < count2) {
			const $H = key_at(at2);
			if ($H[0] === 0) {
				map2($H[1], at2);
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
			const $I = key_at(index3);
			if ($I[0] === 0) {
				gone.push($I[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone;
		return;
	};
	const arrive = (at2, count2) => {
		let index3 = at2;
		while (index3 < at2 + count2) {
			const $J = key_at(index3);
			if ($J[0] === 0) {
				map2($J[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change2) => {
		const $K = change2;
		let $L = null;
		if ($K[0] === 0) {
			const at2 = $K[1];
			const removed = $K[2];
			const _inserted = $K[3];
			$L = note(at2, removed);
		} else if ($K[0] === 3) {
			const at3 = $K[1];
			const count2 = $K[2];
			$L = note(at3, count2);
		} else if ($K[0] === 2) {
			const at4 = $K[1];
			$L = note(at4, 1);
		} else if ($K[0] === 4) {
			const at5 = $K[1];
			const _part = $K[2];
			$L = note(at5, 1);
		} else {
			$L = undefined;
		}
		return $L;
	}, (change2, stage) => {
		const $M = stage;
		let $N = null;
		if ($M[0] === 0) {
			$N = undefined;
		} else if ($M[0] === 3) {
			return;
		} else {
			fill();
			return;
			$N = undefined;
		}
		$N;
		const $O = change2;
		let $P = null;
		if ($O[0] === 0) {
			const at2 = $O[1];
			const _removed = $O[2];
			const inserted = $O[3];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			arrive(at2, inserted);
			$P = undefined;
		} else if ($O[0] === 1) {
			const at3 = $O[1];
			const count2 = $O[2];
			$P = arrive(at3, count2);
		} else if ($O[0] === 3) {
			const _at = $O[1];
			const _count = $O[2];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			$P = undefined;
		} else if ($O[0] === 2) {
			const at4 = $O[1];
			for (const held5 of leaving.v) {
				unmap(held5);
			}
			arrive(at4, 1);
			$P = undefined;
		} else if ($O[0] === 4) {
			const at5 = $O[1];
			const _part = $O[2];
			for (const held6 of leaving.v) {
				unmap(held6);
			}
			arrive(at5, 1);
			$P = undefined;
		} else if ($O[0] === 5) {
			const _from = $O[1];
			const _to = $O[2];
			$P = undefined;
		} else {
			$P = fill();
		}
		return $P;
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
			const $Z = __list_get(held3, index2);
			let $aa = null;
			if ($Z[0] === 0) {
				const op = $Z[1];
				$aa = kept.push(op);
			} else {
				$aa = undefined;
			}
			$aa;
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
function rows(self, $W) {
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
			const id = __remove_at(held3.v, at2, "std/src/reactive/lens.vl:1836:27");
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
			__insert_at(held3.v, index2, id, "std/src/reactive/lens.vl:1848:18");
			arrived.push(id);
			index2 = index2 + 1;
		}
		return arrived;
	};
	held3.v = fill();
	const tap2 = tap(self, (_change) => {
		return;
	}, (change2, _stage) => {
		const $X = change2;
		let $Y = null;
		if ($X[0] === 0) {
			const at2 = $X[1];
			const removed = $X[2];
			const inserted = $X[3];
			const gone = leave(at2, removed);
			record(log, [ 0, at2, gone, arrive(at2, inserted) ]);
			$Y = undefined;
		} else if ($X[0] === 1) {
			const at3 = $X[1];
			const count2 = $X[2];
			$Y = record(log, [ 0, at3, [  ], arrive(at3, count2) ]);
		} else if ($X[0] === 3) {
			const at4 = $X[1];
			const count3 = $X[2];
			$Y = record(log, [ 0, at4, leave(at4, count3), [  ] ]);
		} else if ($X[0] === 5) {
			const from = $X[1];
			const to = $X[2];
			const id = __remove_at(held3.v, from, "std/src/reactive/lens.vl:1864:28");
			__insert_at(held3.v, to, id, "std/src/reactive/lens.vl:1865:19");
			record(log, [ 3, from, 1, to ]);
			$Y = undefined;
		} else if ($X[0] === 6) {
			for (const id2 of held3.v) {
				release_id(watch, id2);
			}
			held3.v = fill();
			record(log, [ 2, __clone(held3.v) ]);
			$Y = undefined;
		} else {
			$Y = undefined;
		}
		return $Y;
	});
	const feed = [ __clone(self), held3, __clone(log), tap2 ];
	defer_to_owner(() => {
		return release5(feed);
	}, $W);
	return feed;
}
function rows2(self, $V) {
	return rows(self[0], $V);
}
function lens(self) {
	return __clone(self[0]);
}
function slot_of(self, id) {
	const $ac = __clone(self);
	let $ab = null;
	if (id < 0) {
		$ab = -(1);
	} else {
		$ab = id;
	}
	return [ $ac, $ab ];
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
	let $am = null;
	if (is_empty(self)) {
		$am = [ 1 ];
	} else {
		$am = __list_get(self, self.length - 1);
	}
	return $am;
}
function notify(self, $ah) {
	const traced = tracing.v;
	let $ai = null;
	if (traced) {
		$ai = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$ai = "";
	}
	const cause = $ai;
	const $aj = $ah;
	let $ak = null;
	if ($aj[0] === 0) {
		const turn = $aj[1];
		$ak = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $an = last(draining_turns.v);
		let $ao = null;
		if ($an[0] === 0) {
			const draining = $an[1];
			$ao = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $ap = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$ap = undefined;
				}
				$ap;
			}
			$ao = undefined;
		}
		$ak = $ao;
	}
	return $ak;
}
function set(self, value, $ag, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $ag);
}
function drop_pages(self, $ae) {
	self[6].v[2] = 0;
	self[6].v[3] = 0;
	const $af = get2(self[4]);
	if ($af[0] === 1) {
		set(self[4], [ 0 ], $ae, "std/src/reactive/lens/loaded.vl:873:20");
	}
	const $aq = get2(self[5]);
	if ($aq[0] === 1) {
		set(self[5], [ 0 ], $ae, "std/src/reactive/lens/loaded.vl:876:20");
	}
}
function retire(self, $ad) {
	self[6].v[1] = 0;
	drop_pages(self, $ad);
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
	const $au = self;
	let $av = null;
	if ($au[0] === 0) {
		const x = $au[1];
		$av = x;
	} else {
		$av = __clone(__force(fallback));
	}
	return $av;
}
async function answer(self, call, reply, $at) {
	while (!(contains_key(self[0].v, call))) {
		insert(self[1].v, call, polled(self, call) + 1);
		await (sleep(1, $at));
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
	const $aD = self;
	let $aE = null;
	if ($aD[0] === 1) {
		const value = $aD[1];
		$aE = [ 0, __clone(value) ];
	} else if ($aD[0] === 2) {
		const value2 = $aD[1];
		$aE = [ 0, __clone(value2) ];
	} else if ($aD[0] === 3) {
		const _error = $aD[1];
		const stale = __clone($aD[2]);
		$aE = stale;
	} else {
		$aE = [ 1 ];
	}
	return $aE;
}
function refreshed(self) {
	const $aF = latest(self);
	let $aG = null;
	if ($aF[0] === 0) {
		const value = $aF[1];
		$aG = [ 2, value ];
	} else {
		$aG = [ 0 ];
	}
	return $aG;
}
function same_phase(first, second) {
	const $aI = [ first, second ];
	let $aJ = null;
	if ($aI[0][0] === 0 && $aI[1][0] === 0) {
		$aJ = true;
	} else if ($aI[0][0] === 4 && $aI[1][0] === 4) {
		$aJ = true;
	} else if ($aI[0][0] === 1 && $aI[1][0] === 1) {
		const mine = $aI[0][1];
		const theirs = $aI[1][1];
		$aJ = mine === theirs;
	} else if ($aI[0][0] === 2 && $aI[1][0] === 2) {
		const mine2 = $aI[0][1];
		const theirs2 = $aI[1][1];
		$aJ = mine2 === theirs2;
	} else {
		$aJ = false;
	}
	return $aJ;
}
function notify2(self, $ah) {
	const traced = tracing.v;
	let $aK = null;
	if (traced) {
		$aK = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$aK = "";
	}
	const cause = $aK;
	const $aL = $ah;
	let $aM = null;
	if ($aL[0] === 0) {
		const turn = $aL[1];
		$aM = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $aN = last(draining_turns.v);
		let $aO = null;
		if ($aN[0] === 0) {
			const draining = $aN[1];
			$aO = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $aP = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$aP = undefined;
				}
				$aP;
			}
			$aO = undefined;
		}
		$aM = $aO;
	}
	return $aM;
}
function set2(self, value, $ag, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $ag);
}
function set_phase(cell2, next, $aH) {
	if (!(same_phase(get2(cell2), next))) {
		set2(cell2, next, $aH, "std/src/reactive/lens/loaded.vl:716:8");
	}
}
function load(self, $aC) {
	const token = claim(self);
	self[6].v[1] = token;
	set_phase(self[3], refreshed(get2(self[3])), $aC);
	return token;
}
function all_of(storage) {
	return peek_storage2(storage, (held3) => {
		let out = [  ];
		let at2 = 0;
		const count2 = count(held3);
		while (at2 < count2) {
			const $aX = item(held3, at2);
			if ($aX[0] === 0) {
				out.push($aX[1]);
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
	const $bk = self[0].v[0];
	let $bl = null;
	if ($bk[0] === 0) {
		const cell2 = $bk[1];
		$bl = change2(cell2.v);
	} else {
		const port = __clone($bk[1]);
		$bl = port[1](change2);
	}
	return $bl;
}
function wake_all(watch, slot3, $bT) {
	const held3 = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot3), "std/src/reactive/lens.vl:1117:36"));
	});
	const $bU = held3[0];
	if ($bU[0] === 0) {
		notify2($bU[1], $bT);
	}
	for (const part of held3[1]) {
		notify2(part[1], $bT);
	}
}
function wake_part(watch, slot3, name, $bV) {
	const held3 = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot3), "std/src/reactive/lens.vl:1127:36"));
	});
	const $bW = held3[0];
	if ($bW[0] === 0) {
		notify2($bW[1], $bV);
	}
	for (const part of held3[1]) {
		if (part[0] === name) {
			notify2(part[1], $bV);
		}
	}
}
function batch(body, $ce) {
	const $cf = $ce;
	let $cg = null;
	if ($cf[0] === 0) {
		const current2 = $cf[1];
		$cg = body(current2);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$cg = result;
	}
	return $cg;
}
function landed(self, changes, late, $bn) {
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
		const $bo = change2;
		let $bp = null;
		if ($bo[0] === 0) {
			const _at = $bo[1];
			const removed = $bo[2];
			const inserted = $bo[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$bp = undefined;
		} else if ($bo[0] === 1) {
			const _at2 = $bo[1];
			const _count = $bo[2];
			$bp = resized = true;
		} else if ($bo[0] === 3) {
			const _at3 = $bo[1];
			const _count2 = $bo[2];
			$bp = resized = true;
		} else if ($bo[0] === 6) {
			$bp = resized = true;
		} else if ($bo[0] === 5) {
			const _from = $bo[1];
			const _to = $bo[2];
			$bp = shaped = true;
		} else {
			$bp = undefined;
		}
		$bp;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change2, wakes);
		}
		let $bF = null;
		if (self[0].v[4].length > 0) {
			let $bE = null;
			if (!(late)) {
				$bE = [ 0 ];
			} else if (!(several)) {
				$bE = [ 1 ];
			} else if (index2 === 0) {
				$bE = [ 2 ];
			} else {
				$bE = [ 3 ];
			}
			const stage = $bE;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$bF = undefined;
		}
		$bF;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $bG = null;
	if (shaped || resized) {
		$bG = watch.v[2];
	} else {
		$bG = [ 1 ];
	}
	const shape = __clone($bG);
	let $bH = null;
	if (resized) {
		$bH = watch.v[3];
	} else {
		$bH = [ 1 ];
	}
	const sized = __clone($bH);
	let $bI = undefined;
	let $bJ = undefined;
	if (wakes.length === 0 && (($bI = shape), $bI[0] === 1) && (($bJ = sized), $bJ[0] === 1)) {
		notify2(published, $bn);
		return;
	}
	settle_together(($bQ) => {
		for (const wake2 of wakes) {
			const $bR = wake2;
			let $bS = null;
			if ($bR[0] === 0) {
				const slot3 = $bR[1];
				$bS = wake_all(watch, slot3, [ 0, $bQ ]);
			} else if ($bR[0] === 2) {
				const slot4 = $bR[1];
				$bS = wake_all(watch, slot4, [ 0, $bQ ]);
			} else {
				const slot5 = $bR[1];
				const part = $bR[2];
				$bS = wake_part(watch, slot5, part, [ 0, $bQ ]);
			}
			$bS;
		}
		const $bX = sized;
		if ($bX[0] === 0) {
			notify2($bX[1], [ 0, $bQ ]);
		}
		const $bY = shape;
		if ($bY[0] === 0) {
			notify2($bY[1], [ 0, $bQ ]);
		}
		notify2(published, [ 0, $bQ ]);
		return;
	}, $bn);
}
function landed_one(self, change2, $bm) {
	landed(self, [ __clone(change2) ], false, $bm);
}
function splice2(self, at2, removed, inserted, $bd) {
	const held3 = size(self);
	let $bi = null;
	if (at2 > held3) {
		$bi = held3;
	} else {
		$bi = at2;
	}
	const start9 = $bi;
	let $bj = null;
	if (start9 + removed > held3) {
		$bj = held3 - start9;
	} else {
		$bj = removed;
	}
	const taking = $bj;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start9, taking, arriving ];
	coming_one(self, change2);
	mutate(self, (held4) => {
		return splice(held4, start9, taking, inserted);
	});
	landed_one(self, change2, $bd);
}
function remove_at(self, at2, $bc) {
	return splice2(self, at2, 1, [  ], $bc);
}
function eq6(self, b) {
	const $cl = self;
	let $co = null;
	if ($cl[0] === 0) {
		const $cm = b;
		let $cn = null;
		if ($cm[0] === 0) {
			$cn = eq5($cl[1], $cm[1]);
		} else {
			$cn = false;
		}
		$co = $cn;
	} else {
		const $cp = b;
		$co = $cp[0] === 1;
	}
	return $co;
}
function has(self, at2) {
	const holds = self[0].v[1][0];
	return peek(self, (held3) => {
		return holds(held3, at2);
	});
}
function landed2(self, changes, late, $bn) {
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
		const $cu = change2;
		let $cv = null;
		if ($cu[0] === 0) {
			const _at = $cu[1];
			const removed = $cu[2];
			const inserted = $cu[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$cv = undefined;
		} else if ($cu[0] === 1) {
			const _at2 = $cu[1];
			const _count = $cu[2];
			$cv = resized = true;
		} else if ($cu[0] === 3) {
			const _at3 = $cu[1];
			const _count2 = $cu[2];
			$cv = resized = true;
		} else if ($cu[0] === 6) {
			$cv = resized = true;
		} else if ($cu[0] === 5) {
			const _from = $cu[1];
			const _to = $cu[2];
			$cv = shaped = true;
		} else {
			$cv = undefined;
		}
		$cv;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change2, wakes);
		}
		let $cx = null;
		if (self[0].v[4].length > 0) {
			let $cw = null;
			if (!(late)) {
				$cw = [ 0 ];
			} else if (!(several)) {
				$cw = [ 1 ];
			} else if (index2 === 0) {
				$cw = [ 2 ];
			} else {
				$cw = [ 3 ];
			}
			const stage = $cw;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$cx = undefined;
		}
		$cx;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $cy = null;
	if (shaped || resized) {
		$cy = watch.v[2];
	} else {
		$cy = [ 1 ];
	}
	const shape = __clone($cy);
	let $cz = null;
	if (resized) {
		$cz = watch.v[3];
	} else {
		$cz = [ 1 ];
	}
	const sized = __clone($cz);
	let $cA = undefined;
	let $cB = undefined;
	if (wakes.length === 0 && (($cA = shape), $cA[0] === 1) && (($cB = sized), $cB[0] === 1)) {
		notify2(published, $bn);
		return;
	}
	settle_together(($bQ) => {
		for (const wake2 of wakes) {
			const $cC = wake2;
			let $cD = null;
			if ($cC[0] === 0) {
				const slot3 = $cC[1];
				$cD = wake_all(watch, slot3, [ 0, $bQ ]);
			} else if ($cC[0] === 2) {
				const slot4 = $cC[1];
				$cD = wake_all(watch, slot4, [ 0, $bQ ]);
			} else {
				const slot5 = $cC[1];
				const part = $cC[2];
				$cD = wake_part(watch, slot5, part, [ 0, $bQ ]);
			}
			$cD;
		}
		const $cE = sized;
		if ($cE[0] === 0) {
			notify2($cE[1], [ 0, $bQ ]);
		}
		const $cF = shape;
		if ($cF[0] === 0) {
			notify2($cF[1], [ 0, $bQ ]);
		}
		notify2(published, [ 0, $bQ ]);
		return;
	}, $bn);
}
function landed_one2(self, change2, $bm) {
	landed2(self, [ __clone(change2) ], false, $bm);
}
function replace(self, at2, value, $cq) {
	const watch = self[0].v[3];
	let $cr = null;
	if (has(self, at2)) {
		$cr = [ 2, __clone(at2) ];
	} else if (viewing2(watch, (held3) => {
		return follows_items(held3[0]);
	})) {
		return false;
	} else {
		$cr = [ 1, __clone(at2), 1 ];
	}
	const change2 = $cr;
	coming_one(self, change2);
	const write = self[0].v[1][3];
	mutate(self, (held3) => {
		return write(held3, at2, value);
	});
	landed_one2(self, change2, $cq);
	return true;
}
function size2(self) {
	return peek(self, (held3) => {
		return count(held3);
	});
}
function shift(self, from, to) {
	const item2 = __remove_at(self, from, "std/src/reactive/lens.vl:2249:19");
	__insert_at(self, to, item2, "std/src/reactive/lens.vl:2250:8");
}
function move_item(self, from, to, $cJ) {
	const held3 = size2(self);
	if (from >= held3 || from === to) {
		return;
	}
	let $cK = null;
	if (to >= held3) {
		$cK = held3 - 1;
	} else {
		$cK = to;
	}
	const landing2 = $cK;
	const change2 = [ 5, from, landing2 ];
	coming_one(self, change2);
	mutate(self, (held4) => {
		return shift(held4, from, landing2, $cJ);
	});
	landed_one2(self, change2, $cJ);
}
function insert_at(self, at2, value, $cL) {
	return splice2(self, at2, 0, [ __clone(value) ], $cL);
}
function reconcile_by_key(self, next, key, $aV) {
	const lens2 = __clone(self);
	batch(($aW) => {
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
			if (!(contains_key(wanted, __at(keys2, at2, "std/src/reactive/lens.vl:1907:29")))) {
				remove_at(lens2, at2, [ 0, $aW ]);
				__remove_at(keys2, at2, "std/src/reactive/lens.vl:1909:11");
			}
		}
		let index2 = 0;
		for (const item4 of next) {
			const want = key(item4);
			let $cG = null;
			if (index2 < keys2.length && __at(keys2, index2, "std/src/reactive/lens.vl:1915:30") === want) {
				if (!(eq6(peek(lens2, (list) => {
					return item(list, index2);
				}), [ 0, __clone(item4) ]))) {
					replace(lens2, index2, item4, [ 0, $aW ]);
				}
				$cG = undefined;
			} else {
				let found = [ 1 ];
				let search2 = index2 + 1;
				while (search2 < keys2.length) {
					if (__at(keys2, search2, "std/src/reactive/lens.vl:1923:10") === want) {
						found = [ 0, search2 ];
						search2 = keys2.length;
					}
					search2 = search2 + 1;
				}
				const $cH = found;
				let $cI = null;
				if ($cH[0] === 0) {
					const from = $cH[1];
					move_item(lens2, from, index2, [ 0, $aW ]);
					const moved = __remove_at(keys2, from, "std/src/reactive/lens.vl:1932:25");
					__insert_at(keys2, index2, moved, "std/src/reactive/lens.vl:1933:13");
					if (!(eq6(peek(lens2, (list) => {
						return item(list, index2);
					}), [ 0, __clone(item4) ]))) {
						replace(lens2, index2, item4, [ 0, $aW ]);
					}
					$cI = undefined;
				} else {
					insert_at(lens2, index2, item4, [ 0, $aW ]);
					__insert_at(keys2, index2, want, "std/src/reactive/lens.vl:1940:13");
					$cI = undefined;
				}
				$cG = $cI;
			}
			$cG;
			index2 = index2 + 1;
		}
		return;
	}, $aV);
}
function failed(self, error) {
	return [ 3, __clone(error), latest(self) ];
}
function settle(self, token, reply, $aR) {
	if (token === 0 || self[6].v[1] !== token) {
		return false;
	}
	self[6].v[1] = 0;
	const feed = __clone(self);
	const $aS = reply;
	let $aT = null;
	if ($aS[0] === 0) {
		const list = $aS[1];
		$aT = batch(($aU) => {
			reconcile_by_key(lens(feed[0]), list, feed[2], [ 0, $aU ]);
			drop_pages(feed, [ 0, $aU ]);
			set_phase(feed[3], [ 1, token ], [ 0, $aU ]);
			return;
		}, $aR);
	} else {
		const error = $aS[1];
		$aT = set_phase(self[3], failed(get2(self[3]), error), $aR);
	}
	$aT;
	return true;
}
function fetch2(self, fetch3, $aB) {
	const token = load(self, $aB);
	cancel(self[7].v[0]);
	const running = detached_nursery();
	self[7].v[0] = [ 0, __clone(running) ];
	const feed = __clone(self);
	(($aQ) => {
		__task(async () => {
			const reply = await (fetch3($aQ));
			settle(feed, token, reply, $aB);
			return;
		}, "fetch", $aQ);
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
function slot(self, at2, $cQ) {
	const handle = follow3(self, at2);
	defer_to_owner(() => {
		return release6(handle);
	}, $cQ);
	return handle;
}
function slot2(self, at2, $cP) {
	return slot(self[0], at2, $cP);
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
	const $cU = at(self);
	if ($cU[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held3) => {
		return read(held3, $cU[1]);
	});
}
function peek2(self, read) {
	return peek(self[0], read);
}
function eq7(self, b) {
	const $cZ = self;
	let $dc = null;
	if ($cZ[0] === 0) {
		const $da = b;
		let $db = null;
		if ($da[0] === 0) {
			$db = eq2($cZ[1], $da[1]);
		} else {
			$db = false;
		}
		$dc = $db;
	} else {
		const $dd = b;
		$dc = $dd[0] === 1;
	}
	return $dc;
}
function set_edge(cell2, next, $dt) {
	const $du = [ get2(cell2), __clone(next) ];
	let $dv = null;
	if ($du[0][0] === 0 && $du[1][0] === 0) {
		$dv = true;
	} else if ($du[0][0] === 1 && $du[1][0] === 1) {
		$dv = true;
	} else if ($du[0][0] === 2 && $du[1][0] === 2) {
		$dv = true;
	} else {
		$dv = false;
	}
	const same = $dv;
	if (!(same)) {
		set(cell2, next, $dt, "std/src/reactive/lens/loaded.vl:729:8");
	}
}
function request(self, older2, $dq) {
	let $dr = null;
	if (older2) {
		$dr = self[4];
	} else {
		$dr = self[5];
	}
	const edge = __clone($dr);
	const $ds = get2(edge);
	if ($ds[0] === 2) {
		return [ 1 ];
	}
	const token = claim(self);
	if (older2) {
		self[6].v[2] = token;
	} else {
		self[6].v[3] = token;
	}
	set_edge(edge, [ 1 ], $dq);
	return [ 0, token ];
}
function request_older(self, $dp) {
	return request(self, true, $dp);
}
function map(self, fn) {
	const $dF = self;
	let $dG = null;
	if ($dF[0] === 0) {
		const x = $dF[1];
		$dG = [ 0, fn(x) ];
	} else {
		$dG = [ 1 ];
	}
	return $dG;
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
	const $dJ = self;
	let $dK = null;
	if ($dJ[0] === 0) {
		const x = $dJ[1];
		$dK = fn(x);
	} else {
		$dK = [ 1 ];
	}
	return $dK;
}
function replace2(self, at2, value, $dN) {
	return replace(self[0], at2, value, $dN);
}
function splice3(self, at2, removed, inserted, $bd) {
	const held3 = size2(self);
	let $dQ = null;
	if (at2 > held3) {
		$dQ = held3;
	} else {
		$dQ = at2;
	}
	const start9 = $dQ;
	let $dR = null;
	if (start9 + removed > held3) {
		$dR = held3 - start9;
	} else {
		$dR = removed;
	}
	const taking = $dR;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start9, taking, arriving ];
	coming_one(self, change2);
	mutate(self, (held4) => {
		return splice(held4, start9, taking, inserted);
	});
	landed_one2(self, change2, $bd);
}
function splice4(self, at2, removed, inserted, $dP) {
	splice3(self[0], at2, removed, inserted, $dP);
}
function push(self, value, $dO) {
	return splice4(self, size3(self), 0, [ __clone(value) ], $dO);
}
function receive(self, item2, $dI) {
	const id = self[2](item2);
	const $dL = and_then(held2(self[1], id), (slot3) => {
		return at(slot3);
	});
	let $dM = null;
	if ($dL[0] === 0) {
		const at2 = $dL[1];
		if (!(eq6(peek2(self[0], (list) => {
			return __list_get(list, at2);
		}), [ 0, __clone(item2) ]))) {
			replace2(self[0], at2, item2, $dI);
		}
		$dM = undefined;
	} else {
		$dM = push(self[0], item2, $dI);
	}
	return $dM;
}
function settle_page(self, older2, token, reply, $dz) {
	let $dA = null;
	if (older2) {
		$dA = self[6].v[2];
	} else {
		$dA = self[6].v[3];
	}
	const mine = $dA;
	if (token === 0 || mine !== token) {
		return false;
	}
	if (older2) {
		self[6].v[2] = 0;
	} else {
		self[6].v[3] = 0;
	}
	let $dB = null;
	if (older2) {
		$dB = self[4];
	} else {
		$dB = self[5];
	}
	const edge = __clone($dB);
	const feed = __clone(self);
	const $dC = reply;
	let $dD = null;
	if ($dC[0] === 0) {
		const page = $dC[1];
		$dD = batch(($dE) => {
			const key = feed[2];
			let taken = new10();
			let fresh = [  ];
			for (const item2 of page[0]) {
				const id = key(item2);
				const $dH = held2(feed[1], id);
				if ($dH[0] === 0) {
					receive(feed, item2, [ 0, $dE ]);
				} else if (!(contains_key(taken, id))) {
					insert(taken, id, true);
					fresh.push(__clone(item2));
				}
			}
			let $dS = null;
			if (older2) {
				$dS = 0;
			} else {
				$dS = size3(feed[0]);
			}
			const at2 = $dS;
			splice4(feed[0], at2, 0, fresh, [ 0, $dE ]);
			let $dT = null;
			if (page[1]) {
				$dT = [ 0 ];
			} else {
				$dT = [ 2 ];
			}
			set_edge(edge, $dT, [ 0, $dE ]);
			return;
		}, $dz);
	} else {
		const error = $dC[1];
		$dD = set(edge, [ 3, __clone(error) ], $dz, "std/src/reactive/lens/loaded.vl:949:27");
	}
	$dD;
	return true;
}
function prepend_page(self, token, reply, $dy) {
	return settle_page(self, true, token, reply, $dy);
}
function fetch_older(self, fetch3, $do) {
	const $dw = request_older(self, $do);
	if ($dw[0] === 0) {

	} else {
		return;
	}
	cancel(self[7].v[1]);
	const running = detached_nursery();
	self[7].v[1] = [ 0, __clone(running) ];
	const feed = __clone(self);
	(($dx) => {
		__task(async () => {
			const reply = await (fetch3($dx));
			prepend_page(feed, $dw[1], reply, $do);
			return;
		}, "fetch_older", $dx);
		return;
	})(running);
}
function older(self) {
	return [ __clone(self[4]) ];
}
function request_newer(self, $dZ) {
	return request(self, false, $dZ);
}
function append_page(self, token, reply, $ec) {
	return settle_page(self, false, token, reply, $ec);
}
function fetch_newer(self, fetch3, $dY) {
	const $ea = request_newer(self, $dY);
	if ($ea[0] === 0) {

	} else {
		return;
	}
	cancel(self[7].v[2]);
	const running = detached_nursery();
	self[7].v[2] = [ 0, __clone(running) ];
	const feed = __clone(self);
	(($eb) => {
		__task(async () => {
			const reply = await (fetch3($eb));
			append_page(feed, $ea[1], reply, $dY);
			return;
		}, "fetch_newer", $eb);
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
		return new8(0, "std/src/reactive/lens.vl:1079:19");
	}
	const entry = as_usize(slot3);
	const $ek = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1082:5")[0];
	const cell2 = __clone($ek[1]);
	if ($ek[0] === 0) {
		return cell2;
	}
	const cell3 = new8(0, "std/src/reactive/lens.vl:1085:25");
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1086:2")[0] = [ 0, __clone(cell3) ];
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
function track(self, $ej) {
	const scope = $ej;
	const identity3 = identity2(self);
	if (admits(scope, identity3)) {
		lists_of(scope[0]).v[0].push([ identity3, (subscriber) => {
			return on_settle2(self, subscriber);
		} ]);
	}
	return get4(self);
}
function row(self, key) {
	const $ev = get(self[3].v, key);
	if ($ev[0] === 0) {
		return $ev[1];
	}
	const made = [ new8(0, "std/src/reactive/lens/loaded.vl:141:44"), new8(false, "std/src/reactive/lens/loaded.vl:141:74") ];
	insert(self[3].v, key, made);
	return made;
}
function track2(self, $ej) {
	const scope = $ej;
	const identity3 = identity(self);
	if (admits(scope, identity3)) {
		lists_of(scope[0]).v[0].push([ identity3, (subscriber) => {
			return on_settle(self, subscriber);
		} ]);
	}
	return get2(self);
}
function showing(self, key) {
	const $ew = get(self[0].v, key);
	if ($ew[0] === 0) {

	} else {
		return [ 1 ];
	}
	let out = [ 1 ];
	for (const proposal of $ew[1][0]) {
		if (proposal[0] > $ew[1][1]) {
			out = [ 0, __clone(proposal) ];
		}
	}
	return out;
}
function provisional(self, key) {
	const $eC = get(self[0].v, key);
	if ($eC[0] === 0) {

	} else {
		return false;
	}
	let out = false;
	for (const proposal of $eC[1][0]) {
		if (proposal[2]) {
			out = true;
		}
	}
	return out;
}
function over3(self, key, held3) {
	const $ex = showing(self, key);
	let $ey = null;
	if ($ex[0] === 1) {
		$ey = __clone(held3);
	} else {
		const proposal = $ex[1];
		const $ez = proposal[1];
		let $eA = null;
		if ($ez[0] === 1) {
			$eA = [ 1 ];
		} else {
			const value = $ez[1];
			const $eB = held3;
			let $eD = null;
			if ($eB[0] === 0 || provisional(self, key)) {
				$eD = [ 0, __clone(value) ];
			} else {
				$eD = [ 1 ];
			}
			$eA = $eD;
		}
		$ey = $eA;
	}
	return $ey;
}
function derive(body, caller) {
	return [ body, caller ];
}
function distinct(self, caller) {
	return [ self, caller ];
}
function is_none(self) {
	const $eV = self;
	return $eV[0] === 1;
}
function run_body(runs, body) {
	const run = renew(runs);
	const $fe = nursery(run);
	let $ff = null;
	if ($fe[0] === 0) {
		const nursery2 = $fe[1];
		$ff = (($fg) => {
			return (($fh) => {
				return body($fg, $fh);
			})(nursery2);
		})(run);
	} else {
		$ff = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $ff;
}
function run_once(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body(runs, ($eN, $eO) => {
		return (($eP) => {
			return body($eN, $eP, $eO);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs, tracker, body) {
	let value = run_once(runs, tracker, ($eI, $eJ, $eK) => {
		return body($eI, $eJ, $eK);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs, tracker, ($fF, $fG, $fH) => {
			return body($fF, $fG, $fH);
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
function set3(self, value, $ag, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $ag);
}
function take(self, item2, $gc) {
	defer(self, () => {
		dispose(item2, $gc);
		return;
	});
	return __clone(item2);
}
function cell(self, $eG, $eH, caller) {
	const instance = start2(self);
	const pull = instance[0];
	const cached = new8(pull(), caller);
	const refresh = subscriber_of(() => {
		set3(cached, pull(), $eG, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed3 = instance[1](refresh);
	also_releasing(refreshed3, instance[2]);
	register_with_owner(refreshed3, $eG, $eH);
	return cached;
}
function memo(self, $eE, $eF, caller) {
	return [ cell(self, $eE, $eF, caller) ];
}
function view(self, slot3, $ee, $ef) {
	const ledger = __clone(self[2]);
	const key = self[1];
	return memo(distinct(derive(($eg, $eh, $ei) => {
		const $et = track(slot3, $eh);
		let $eu = null;
		if ($et[0] === 0) {
			const item2 = $et[1];
			const id = key(item2);
			track2(row(ledger, id)[0], $eh);
			$eu = over3(ledger, id, [ 0, item2 ]);
		} else {
			$eu = [ 1 ];
		}
		return $eu;
	}, "std/src/reactive/lens/loaded.vl:507:3"), "std/src/reactive/lens/loaded.vl:516:6"), $ee, $ef, "std/src/reactive/lens/loaded.vl:516:17");
}
function observe(signal, observer) {
	const cell2 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $gi = [ 0, cell2 ];
		let $gj = null;
		if ($gi[0] === 0) {
			const live = $gi[1];
			$gj = observer(live.v);
		} else {
			$gj = undefined;
		}
		return $gj;
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
function open2(self, key, value, insert2, above, below, $go) {
	const id = self[6].v + 1;
	self[6].v = id;
	const ledger = __clone(self);
	batch(($gp) => {
		const cells = row(ledger, key);
		const found = get(ledger[0].v, key);
		const $gq = found;
		let $gr = null;
		if ($gq[0] === 0) {
			const held3 = $gq[1];
			$gr = held3;
		} else {
			ledger[1].v.push(__clone(key));
			set3(cells[1], true, [ 0, $gp ], "std/src/reactive/lens/loaded.vl:176:20");
			$gr = [ [  ], 0, __clone(above), __clone(below) ];
		}
		let entry = $gr;
		if (insert2) {
			entry[2] = __clone(above);
			entry[3] = __clone(below);
		}
		entry[0].push([ id, __clone(value), insert2 ]);
		insert(ledger[0].v, key, entry);
		insert(ledger[2].v, id, key);
		set3(ledger[4], get2(ledger[4]) + 1, [ 0, $gp ], "std/src/reactive/lens/loaded.vl:187:17");
		notify2(cells[0], [ 0, $gp ]);
		let $gy = undefined;
		if (insert2 || (($gy = value), $gy[0] === 1)) {
			notify2(ledger[5], [ 0, $gp ]);
		}
		return;
	}, $go);
	return id;
}
function propose(self, slot3, value, $gk) {
	const $gn = key_of(self, slot3);
	if ($gn[0] === 0) {

	} else {
		return [ 1 ];
	}
	return [ 0, open2(self[2], $gn[1], [ 0, __clone(value) ], false, [ 1 ], [ 1 ], $gk) ];
}
function unwrap(self, caller) {
	const $gz = self;
	let $gA = null;
	if ($gz[0] === 0) {
		const x = $gz[1];
		$gA = x;
	} else {
		$gA = (() => {
			throw __panic("expected Some but got None", caller);
		})();
	}
	return $gA;
}
async function answer2(self, call, reply, $at) {
	while (!(contains_key(self[0].v, call))) {
		insert(self[1].v, call, polled(self, call) + 1);
		await (sleep(1, $at));
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
	const $gJ = get(self[0].v, key);
	let $gK = null;
	if ($gJ[0] === 0) {
		const entry = $gJ[1];
		$gK = [ __clone(entry[2]), __clone(entry[3]) ];
	} else {
		$gK = [ [ 1 ], [ 1 ] ];
	}
	return $gK;
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
	const $gM = above;
	let $gN = undefined;
	if ($gM[0] === 0 && (($gN = position_of(rows3, $gM[1])), $gN[0] === 0)) {
		return $gN[1] + 1;
	}
	const $gO = below;
	let $gP = undefined;
	if ($gO[0] === 0 && (($gP = position_of(rows3, $gO[1])), $gP[0] === 0)) {
		return $gP[1];
	}
	const $gQ = above;
	if ($gQ[0] === 0) {
		return rows3.length;
	}
	return 0;
}
function hides(self, key) {
	const $gR = showing(self, key);
	if ($gR[0] === 0) {
		const $gS = $gR[1][1];
		return $gS[0] === 1;
	}
	return false;
}
function compose(self, with_hidden) {
	const key = self[1];
	let all3 = [  ];
	for (const slot3 of get5(self[4])) {
		const $gG = get4(slot3);
		if ($gG[0] === 0) {
			all3.push([ key($gG[1]), [ 0, __clone(slot3) ] ]);
		}
	}
	for (const row_key of keys(self[2])) {
		let $gH = undefined;
		let $gI = undefined;
		if (provisional(self[2], row_key) && (($gH = find(self[3], row_key)), $gH[0] === 1) && (($gI = over3(self[2], row_key, [ 1 ])), $gI[0] === 0)) {
			const $gL = anchors(self[2], row_key);
			const above = $gL[0];
			const below = $gL[1];
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
		const $gT = row2[1];
		if ($gT[0] === 0) {
			at2 = at2 + 1;
		}
	}
	return at2;
}
function close2(self, id, accepted, $gU) {
	const $gV = get(self[2].v, id);
	let $gW = null;
	if ($gV[0] === 0) {
		const held3 = $gV[1];
		$gW = held3;
	} else {
		return [ 1 ];
	}
	const key = $gW;
	const found = get(self[0].v, key);
	const $gX = found;
	if ($gX[0] === 0) {

	} else {
		return [ 1 ];
	}
	let entry = __clone($gX[1]);
	let ended = [ 1 ];
	let kept = [  ];
	for (const proposal of entry[0]) {
		if (proposal[0] === id) {
			ended = [ 0, __clone(proposal) ];
		} else {
			kept.push(__clone(proposal));
		}
	}
	const $gY = ended;
	if ($gY[0] === 0) {

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
	batch(($gZ) => {
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
			set3(cells[1], false, [ 0, $gZ ], "std/src/reactive/lens/loaded.vl:235:19");
		} else {
			insert(ledger[0].v, key, entry);
		}
		set3(ledger[4], get2(ledger[4]) - 1, [ 0, $gZ ], "std/src/reactive/lens/loaded.vl:239:17");
		notify2(cells[0], [ 0, $gZ ]);
		let $ha = undefined;
		if ($gY[1][2] || (($ha = $gY[1][1]), $ha[0] === 1)) {
			notify2(ledger[5], [ 0, $gZ ]);
		}
		return;
	}, $gU);
	return [ 0, [ __clone(key), __clone($gY[1][1]), $gY[1][2], lands ] ];
}
function remove_at2(self, at2, $bc) {
	return splice4(self, at2, 1, [  ], $bc);
}
function insert_at2(self, at2, value, $cL) {
	return splice4(self, at2, 0, [ __clone(value) ], $cL);
}
function answer3(self, id, accepted, reply, $gC) {
	const overlay2 = __clone(self);
	let answered = false;
	batch(($gD) => {
		const $gE = get(overlay2[2][2].v, id);
		let $gF = null;
		if ($gE[0] === 0) {
			const key = $gE[1];
			$gF = landing(overlay2, key);
		} else {
			$gF = 0;
		}
		const landing2 = $gF;
		const $hb = close2(overlay2[2], id, accepted, [ 0, $gD ]);
		if ($hb[0] === 0) {

		} else {
			return;
		}
		answered = true;
		if (!($hb[1][3])) {
			return;
		}
		const $hc = reply;
		let $hd = null;
		if ($hc[0] === 0) {
			const item2 = $hc[1];
			$hd = [ 0, __clone(item2) ];
		} else {
			$hd = $hb[1][1];
		}
		const landed3 = __clone($hd);
		const lens2 = __clone(overlay2[0]);
		const $he = landed3;
		let $hf = null;
		if ($he[0] === 1) {
			const $hg = held2(overlay2, $hb[1][0]);
			let $hh = undefined;
			if ($hg[0] === 0 && (($hh = at($hg[1])), $hh[0] === 0)) {
				remove_at2(lens2, $hh[1], [ 0, $gD ]);
			}
			$hf = undefined;
		} else {
			const item3 = $he[1];
			const server_key = overlay2[1](item3);
			const $hi = held2(overlay2, $hb[1][0]);
			let $hj = null;
			if ($hi[0] === 0) {
				$hj = at($hi[1]);
			} else {
				$hj = and_then(held2(overlay2, server_key), (slot3) => {
					return at(slot3);
				});
			}
			const target = $hj;
			const $hk = target;
			let $hl = null;
			if ($hk[0] === 0) {
				const at2 = $hk[1];
				if (!(eq6(peek2(lens2, (list) => {
					return __list_get(list, at2);
				}), [ 0, __clone(item3) ]))) {
					replace2(lens2, at2, item3, [ 0, $gD ]);
				}
				$hl = undefined;
			} else {
				if ($hb[1][2]) {
					insert_at2(lens2, landing2, item3, [ 0, $gD ]);
				}
				$hl = undefined;
			}
			$hf = $hl;
		}
		return $hf;
	}, $gC);
	return answered;
}
function confirm(self, id, $gB) {
	return answer3(self, id, true, [ 1 ], $gB);
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
	const $hD = latest(self);
	let $hE = null;
	if ($hD[0] === 0) {
		const value = $hD[1];
		$hE = [ 2, value ];
	} else {
		$hE = [ 0 ];
	}
	return $hE;
}
function claim2(self, $hA) {
	const mine = self[1].v + 1;
	self[1].v = mine;
	set3(self[0], refreshed2(get2(self[0])), $hA, "std/src/reactive/transient.vl:247:13");
	return mine;
}
function current(self, mine) {
	return self[1].v === mine;
}
function settle2(self, mine, outcome, $hN) {
	let $hQ = null;
	if (current(self, mine)) {
		const $hO = outcome;
		let $hP = null;
		if ($hO[0] === 0) {
			const value = $hO[1];
			$hP = set3(self[0], [ 1, __clone(value) ], $hN, "std/src/reactive/transient.vl:266:32");
		} else {
			const error = $hO[1];
			$hP = set3(self[0], failed(get2(self[0]), error), $hN, "std/src/reactive/transient.vl:267:33");
		}
		$hQ = $hP;
	}
	return $hQ;
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
	const $hV = nursery(run);
	let $hW = null;
	if ($hV[0] === 0) {
		const nursery2 = $hV[1];
		$hW = (($fg) => {
			return (($fh) => {
				return body($fg, $fh);
			})(nursery2);
		})(run);
	} else {
		$hW = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $hW;
}
function run_once2(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body2(runs, ($eN, $eO) => {
		return (($eP) => {
			return body($eN, $eP, $eO);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked2(runs, tracker, body) {
	let value = run_once2(runs, tracker, ($eI, $eJ, $eK) => {
		return body($eI, $eJ, $eK);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once2(runs, tracker, ($fF, $fG, $fH) => {
			return body($fF, $fG, $fH);
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
		return run_tracked2(runs, tracker, ($hS, $hT, $hU) => {
			return transform(value, $hS, $hT, $hU);
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
function hold(self, subscription, $hX, $hY) {
	const previous = __clone(subscription[3].v);
	subscription[3].v = [ 0, () => {
		retire2(self);
		const $hZ = previous;
		let $ia = null;
		if ($hZ[0] === 0) {
			const earlier = $hZ[1];
			$ia = earlier();
		} else {
			$ia = undefined;
		}
		return $ia;
	} ];
	register_with_owner(subscription, $hX, $hY);
}
function seal_results(pipe, global, $hw, $hx, $hy) {
	const seal = pending2();
	as_derivation();
	const subscription = sub(pipe, (task, $hz) => {
		const mine = claim2(seal, $hw);
		__task(async () => {
			const outcome = await (__guarded_async(async () => {
				return await (task);
			}));
			let $hR = null;
			if (current(seal, mine)) {
				const $hL = outcome;
				let $hM = null;
				if ($hL[0] === 0) {
					const answered = $hL[1];
					$hM = settle2(seal, mine, answered, $hw);
				} else {
					const message = $hL[1];
					$hM = (() => {
						throw __panic(message, "std/src/reactive/transient.vl:381:26");
					})();
				}
				$hR = $hM;
			}
			return $hR;
		}, "seal_results", __nursery_of($hy));
		return;
	}, "std/src/reactive/transient.vl:372:26");
	if (global) {
		hold_for_the_program(seal, subscription);
	} else {
		hold(seal, subscription, $hw, $hx);
	}
	return seal;
}
function transient(self, $ht, $hu, $hv) {
	return seal_results(self, false, $ht, $hu, $hv);
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
	const value = run_body2(runs, ($eN, $eO) => {
		return (($eP) => {
			return body($eN, $eP, $eO);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked3(runs, tracker, body) {
	let value = run_once3(runs, tracker, ($eI, $eJ, $eK) => {
		return body($eI, $eJ, $eK);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once3(runs, tracker, ($fF, $fG, $fH) => {
			return body($fF, $fG, $fH);
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
		return run_tracked3(runs, tracker, ($hS, $hT, $hU) => {
			return transform(value, $hS, $hT, $hU);
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
	let value = run_once3(runs, tracker, ($eI, $eJ, $eK) => {
		return body($eI, $eJ, $eK);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once3(runs, tracker, ($fF, $fG, $fH) => {
			return body($fF, $fG, $fH);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function effect(self, body, $ij, $ik, caller) {
	const runs = new4();
	const tracker = new_tracker([ 0, caller ]);
	let latest2 = [ 1 ];
	const subscription = sub2(self, (value2, $il) => {
		latest2 = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			run_tracked4(runs, tracker, ($im, $in, $io) => {
				return body(value2, $im, $in, $io);
			});
		}
		return;
	}, caller);
	const rerun = subscriber_of(() => {
		let $iw = null;
		if (tracker[0].v[2]) {
			const $ir = latest2;
			let $is = null;
			if ($ir[0] === 0) {
				const value2 = __clone($ir[1]);
				$is = run_tracked4(runs, tracker, ($it, $iu, $iv) => {
					return body(value2, $it, $iu, $iv);
				});
			} else {
				$is = undefined;
			}
			$iw = $is;
		}
		return $iw;
	}, false);
	if (tracing.v) {
		trace_hooks.v[8](rerun[0], caller);
	}
	attach_tracker(tracker, rerun);
	const $ix = latest2;
	let $iy = null;
	if ($ix[0] === 0) {
		const value = __clone($ix[1]);
		$iy = run_tracked4(runs, tracker, ($iz, $iA, $iB) => {
			return body(value, $iz, $iA, $iB);
		});
	} else {
		$iy = undefined;
	}
	$iy;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	take(get_owner($ik), subscription, $ij);
}
const $ig = Object.create({start: start6, on_change: on_change2, sub: sub2, effect: effect});
function states(self) {
	const pipe = [ derive2(__clone(self[0]), (state, $id, $ie, $if) => {
		return __clone(state);
	}, "std/src/reactive/transient.vl:314:56"), $ig ];
	return pipe;
}
function follow4(self, source, $ib, $ic) {
	const feed = __clone(self);
	const mine = __shared_new(0);
	const watching = (($iG) => {
		return $iG[1].on_change($iG[0], (state, $iD) => {
			const $iE = state;
			let $iF = null;
			if ($iE[0] === 0) {
				$iF = mine.v = load(feed, $ib);
			} else if ($iE[0] === 2) {
				const _old = $iE[1];
				$iF = mine.v = load(feed, $ib);
			} else if ($iE[0] === 1) {
				const list = $iE[1];
				if (mine.v === 0) {
					mine.v = load(feed, $ib);
				}
				settle(feed, mine.v, [ 0, __clone(list) ], $ib);
				mine.v = 0;
				$iF = undefined;
			} else if ($iE[0] === 3) {
				const error = $iE[1];
				const _stale = $iE[2];
				if (mine.v === 0) {
					mine.v = load(feed, $ib);
				}
				settle(feed, mine.v, [ 1, __clone(error) ], $ib);
				mine.v = 0;
				$iF = undefined;
			} else {
				$iF = undefined;
			}
			return $iF;
		}, "std/src/reactive/lens/loaded.vl:1035:34");
	})(states(source));
	return register_with_owner(watching, $ib, $ic);
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
		return run_tracked4(runs, tracker, ($hS, $hT, $hU) => {
			return transform(value, $hS, $hT, $hU);
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
function effect2(self, body, $ij, $ik, caller) {
	const runs = new4();
	const tracker = new_tracker([ 0, caller ]);
	let latest2 = [ 1 ];
	const subscription = sub3(self, (value2, $il) => {
		latest2 = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			run_tracked4(runs, tracker, ($im, $in, $io) => {
				return body(value2, $im, $in, $io);
			});
		}
		return;
	}, caller);
	const rerun = subscriber_of(() => {
		let $jd = null;
		if (tracker[0].v[2]) {
			const $jb = latest2;
			let $jc = null;
			if ($jb[0] === 0) {
				const value2 = __clone($jb[1]);
				$jc = run_tracked4(runs, tracker, ($it, $iu, $iv) => {
					return body(value2, $it, $iu, $iv);
				});
			} else {
				$jc = undefined;
			}
			$jd = $jc;
		}
		return $jd;
	}, false);
	if (tracing.v) {
		trace_hooks.v[8](rerun[0], caller);
	}
	attach_tracker(tracker, rerun);
	const $je = latest2;
	let $jf = null;
	if ($je[0] === 0) {
		const value = __clone($je[1]);
		$jf = run_tracked4(runs, tracker, ($iz, $iA, $iB) => {
			return body(value, $iz, $iA, $iB);
		});
	} else {
		$jf = undefined;
	}
	$jf;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	take(get_owner($ik), subscription, $ij);
}
const $iY = Object.create({start: start8, on_change: on_change4, sub: sub3, effect: effect2});
function states2(self) {
	const pipe = [ derive2(__clone(self[0]), (state, $iV, $iW, $iX) => {
		return __clone(state);
	}, "std/src/reactive/transient.vl:479:56"), $iY ];
	return pipe;
}
function follow5(self, source, $ib, $ic) {
	const feed = __clone(self);
	const mine = __shared_new(0);
	const watching = (($ji) => {
		return $ji[1].on_change($ji[0], (state, $iD) => {
			const $jg = state;
			let $jh = null;
			if ($jg[0] === 0) {
				$jh = mine.v = load(feed, $ib);
			} else if ($jg[0] === 2) {
				const _old = $jg[1];
				$jh = mine.v = load(feed, $ib);
			} else if ($jg[0] === 1) {
				const list = $jg[1];
				if (mine.v === 0) {
					mine.v = load(feed, $ib);
				}
				settle(feed, mine.v, [ 0, __clone(list) ], $ib);
				mine.v = 0;
				$jh = undefined;
			} else if ($jg[0] === 3) {
				const error = $jg[1];
				const _stale = $jg[2];
				if (mine.v === 0) {
					mine.v = load(feed, $ib);
				}
				settle(feed, mine.v, [ 1, __clone(error) ], $ib);
				mine.v = 0;
				$jh = undefined;
			} else {
				$jh = undefined;
			}
			return $jh;
		}, "std/src/reactive/lens/loaded.vl:1035:34");
	})(states2(source));
	return register_with_owner(watching, $ib, $ic);
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
	fetch2(feed, async ($as) => {
		return await (answer(calls, 1, [ 0, [ m(1, "a"), m(2, "b") ] ], [ 0, $as ]));
	}, [ 1 ]);
	assert(size3(items(feed)) === 0 && !(ready(get3(phase(feed)))), "nothing yet", "lens-loaded-tasks.vl:117:2");
	release3(calls, 1);
	await (until("the first load lands", () => {
		return ready(get3(phase(feed)));
	}, [ 1 ]));
	assert(eq(held(feed), [ 1, 2 ]), "the first load\'s rows", "lens-loaded-tasks.vl:120:2");
	const second = slot2(items(feed), 1, [ 1 ]);
	fetch2(feed, async ($cR) => {
		return await (answer(calls, 2, [ 0, [ m(1, "SLOW") ] ], [ 0, $cR ]));
	}, [ 1 ]);
	await (until("call 2 is waiting", () => {
		return polled(calls, 2) > 0;
	}, [ 1 ]));
	fetch2(feed, async ($cS) => {
		return await (answer(calls, 3, [ 0, [ m(1, "a"), m(2, "b2") ] ], [ 0, $cS ]));
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
	fetch2(feed, async ($de) => {
		return await (answer(calls, 4, [ 1, "server error" ], [ 0, $de ]));
	}, [ 1 ]);
	release3(calls, 4);
	await (until("the failure lands", () => {
		const $df = get3(phase(feed));
		return $df[0] === 3;
	}, [ 1 ]));
	const $dg = get3(phase(feed));
	let $dh = null;
	if ($dg[0] === 3) {
		const error = $dg[1];
		const stale = $dg[2];
		$dh = assert(error === "server error" && !(eq3(stale, [ 1 ])) && eq(held(feed), [ 1, 2 ]), "Failed, with the stale token, the rows kept", "lens-loaded-tasks.vl:141:51");
	} else {
		$dh = assert(false, "the phase is Failed", "lens-loaded-tasks.vl:145:8");
	}
	$dh;
	fetch_older(feed, async ($dn) => {
		return await (answer(calls, 5, [ 1, "timeout" ], [ 0, $dn ]));
	}, [ 1 ]);
	assert(eq4(get3(older(feed)), [ 1 ]), "the older end is loading", "lens-loaded-tasks.vl:150:2");
	release3(calls, 5);
	await (until("the page fails", () => {
		return eq4(get3(older(feed)), [ 3, "timeout" ]);
	}, [ 1 ]));
	assert(eq(held(feed), [ 1, 2 ]), "the page failed, the rows stayed", "lens-loaded-tasks.vl:153:2");
	fetch_older(feed, async ($dW) => {
		return await (answer(calls, 6, [ 0, [ [ m(-(1), "older"), m(1, "a") ], false ] ], [ 0, $dW ]));
	}, [ 1 ]);
	release3(calls, 6);
	await (until("the retry lands", () => {
		return eq4(get3(older(feed)), [ 2 ]);
	}, [ 1 ]));
	assert(eq(held(feed), [ -(1), 1, 2 ]), "the retry landed once, no duplicate", "lens-loaded-tasks.vl:160:2");
	fetch_newer(feed, async ($dX) => {
		return await (answer(calls, 7, [ 0, [ [ m(9, "late") ], true ] ], [ 0, $dX ]));
	}, [ 1 ]);
	await (until("call 7 is waiting", () => {
		return polled(calls, 7) > 0;
	}, [ 1 ]));
	fetch_newer(feed, async ($ed) => {
		return await (answer(calls, 8, [ 0, [ [ m(3, "c") ], false ] ], [ 0, $ed ]));
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
	on_change(__clone(view2), (_item, $gg) => {
		const $gh = [ runs, "v" ];
		return runs.v = $gh[0][$gh[1]] + 1;
	}, "lens-loaded-tasks.vl:180:20");
	const edit = unwrap(propose(overlay(feed), row2, m(2, "edited"), [ 1 ]), "lens-loaded-tasks.vl:181:57");
	const overlay2 = overlay(feed);
	__task(async () => {
		await (answer2(calls, 9, true, [ 1 ]));
		confirm(overlay2, edit, [ 1 ]);
		return;
	}, "main");
	fetch2(feed, async ($hm) => {
		return await (answer(calls, 10, [ 0, [ m(-(1), "older"), m(1, "a"), m(2, "b2 (before the edit)"), m(3, "c") ] ], [ 0, $hm ]));
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
	const on_page = run_with_owner(page, ($hn) => {
		return new12((item2) => {
			return item2[0];
		}, [ 1 ], [ 0, $hn ]);
	});
	fetch2(on_page, async ($ho) => {
		return await (answer(calls, 11, [ 0, [ m(9, "late") ] ], [ 0, $ho ]));
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
	const results = transient(derive2(__clone(query), (asked, $hp, $hq, $hr) => {
		return __task(async () => {
			return await (search(calls, asked, [ 0, $hr ]));
		}, "main", $hr);
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
	on_change3(__clone(shared), (_item, $iH) => {
		const $iI = [ shared_runs, "v" ];
		return shared_runs.v = $iI[0][$iI[1]] + 1;
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
})().catch(($jj) => {
	console.error(String($jj));
	process.exit(1);
});
//# sourceMappingURL=lens-loaded-tasks.mjs.map
