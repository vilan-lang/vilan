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
function hash2(self) {
	return __hash(self);
}
function new2() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function assert(condition, message, caller) {
	let $ay = null;
	if (!(condition)) {
		$ay = (() => {
			throw __panic(message, caller);
		})();
	}
	return $ay;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $s = null;
	if (wrapped < 0) {
		$s = wrapped + modulus;
	} else {
		$s = wrapped;
	}
	return $s;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $r = null;
	if (truncated > 0) {
		$r = truncated;
	} else {
		$r = 0;
	}
	return $r;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $t = null;
	if (wrapped >= half) {
		$t = wrapped - modulus;
	} else {
		$t = wrapped;
	}
	return $t;
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
	const $hG = self;
	let $hJ = null;
	if ($hG[0] === 0) {
		const $hH = b;
		let $hI = null;
		if ($hH[0] === 0) {
			$hI = $hG[1] === $hH[1];
		} else {
			$hI = false;
		}
		$hJ = $hI;
	} else {
		const $hK = b;
		$hJ = $hK[0] === 1;
	}
	return $hJ;
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
		const key = hash2(subscriber[0]);
		let $an = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$an = undefined;
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
		$an;
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
	const $dX = turn;
	let $dY = null;
	if ($dX[0] === 0) {
		const ambient = $dX[1];
		$dY = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $dZ = last(draining_turns.v);
		let $ea = null;
		if ($dZ[0] === 0) {
			const draining = $dZ[1];
			$ea = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$ea = undefined;
		}
		$dY = $ea;
	}
	return $dY;
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
function retiring(subscriber, release8) {
	return detached(subscriber[0], subscriber[2], release8);
}
function also_retiring(handle, subscriber) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		subscriber[2].v = false;
		const $ev = previous;
		let $ew = null;
		if ($ev[0] === 0) {
			const release8 = $ev[1];
			$ew = release8();
		} else {
			$ew = undefined;
		}
		return $ew;
	} ];
}
function detached(id, live, release8) {
	let subscribers = [  ];
	const empty = __shared_new(subscribers);
	return [ empty, id, live, __shared_new([ 0, release8 ]) ];
}
function dispose(self, $eK) {
	const $eL = $eK;
	let $eM = null;
	if ($eL[0] === 0) {
		const established = $eL[1];
		$eM = [ 0, established ];
	} else {
		$eM = last(draining_turns.v);
	}
	const ambient = $eM;
	release_under(self, ambient);
}
function detach(handle) {
	const $ee = last(releasing_turns.v);
	let $ef = null;
	if ($ee[0] === 0) {
		const at_release = $ee[1];
		$ef = at_release;
	} else {
		$ef = last(draining_turns.v);
	}
	const turn = $ef;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $eg = [ 0, handle[0] ];
	let $eh = null;
	if ($eg[0] === 0) {
		const subscribers = $eg[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$eh = undefined;
	} else {
		$eh = undefined;
	}
	$eh;
	const $ei = ambient;
	let $ej = null;
	if ($ei[0] === 0) {
		const turn = $ei[1];
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
		$ej = undefined;
	} else {
		$ej = undefined;
	}
	$ej;
	const $ek = handle[3].v;
	let $el = null;
	if ($ek[0] === 0) {
		const release8 = $ek[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release8, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$el = undefined;
	} else {
		$el = undefined;
	}
	return $el;
}
function new4() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $W = null;
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
		$W = undefined;
	}
	return $W;
}
function renew(self) {
	const $dx = self[0].v[3];
	let $dy = null;
	if ($dx[0] === 0) {
		const nursery2 = $dx[1];
		let $dz = null;
		if (has_spawned(nursery2)) {
			$dz = [ 1 ];
		} else {
			$dz = [ 0, __clone(nursery2) ];
		}
		$dy = $dz;
	} else {
		$dy = [ 1 ];
	}
	const carried = $dy;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $dK = null;
	if (is_disposed(self)) {
		$dK = [ 1 ];
	} else {
		$dK = self[0].v[3];
	}
	return $dK;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $dJ = null;
	if (!(is_disposed(self))) {
		const held3 = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $dA = held3[3];
		let $dB = null;
		if ($dA[0] === 0) {
			const nursery2 = $dA[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$dB = undefined;
		} else {
			$dB = undefined;
		}
		$dB;
		let $dI = null;
		if (held3[2]) {
			let failure = [ 1 ];
			for (const cleanup of held3[1].v) {
				const $dD = __guarded(cleanup);
				let $dE = null;
				if ($dD[0] === 0) {
					const message = $dD[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$dE = undefined;
				} else {
					$dE = undefined;
				}
				$dE;
			}
			const $dG = failure;
			let $dH = null;
			if ($dG[0] === 0) {
				const message2 = $dG[1];
				$dH = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$dH = undefined;
			}
			$dI = $dH;
		}
		$dJ = $dI;
	}
	return $dJ;
}
function register_with_owner(subscription, $eF, $eG) {
	const $eH = $eG;
	let $eI = null;
	if ($eH[0] === 0) {
		const owner = $eH[1];
		$eI = take(owner, subscription, $eF);
	} else {
		$eI = __clone(subscription);
	}
	return $eI;
}
function defer_to_owner(cleanup, $T) {
	const $U = $T;
	let $V = null;
	if ($U[0] === 0) {
		const owner = $U[1];
		$V = defer(owner, cleanup);
	} else {
		$V = undefined;
	}
	return $V;
}
function release_runs(runs2) {
	dispose2([ runs2[0], runs2[0].v[0] ]);
}
function new_tracker(site) {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ], site ]) ];
}
function lists_of(tracker) {
	const $cX = tracker[0].v[6];
	let $cY = null;
	if ($cX[0] === 0) {
		const lists = $cX[1];
		$cY = lists;
	} else {
		trackers_allocated_count.v = trackers_allocated_count.v + 1;
		const made = __shared_new([ [  ], [  ], [  ] ]);
		tracker[0].v[6] = [ 0, made ];
		$cY = made;
	}
	return $cY;
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $ds = tracker[0].v[6];
	let $dt = null;
	if ($ds[0] === 0) {
		const lists = $ds[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$dt = undefined;
	} else {
		$dt = undefined;
	}
	$dt;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $dP = tracker[0].v[6];
	let $dQ = null;
	if ($dP[0] === 0) {
		const lists = $dP[1];
		$dQ = lists;
	} else {
		return;
		$dQ = undefined;
	}
	const lists2 = $dQ;
	const $dR = tracker[0].v[5];
	let $dS = null;
	if ($dR[0] === 0) {
		const target = __clone($dR[1]);
		$dS = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$dS = undefined;
	}
	$dS;
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
		const $dV = reusable(held3, kept, dependency[0], position);
		let $dW = null;
		if ($dV[0] === 0) {
			const index2 = $dV[1];
			__at_put(kept, index2, true, "std/src/reactive.vl:1715:5");
			next.push(__clone(__at(held3, index2, "std/src/reactive.vl:1716:15")));
			$dW = undefined;
		} else {
			next.push([ __clone(dependency[0]), dependency[1](relay_for(tracker, target)) ]);
			$dW = undefined;
		}
		$dW;
		position = position + 1;
	}
	lists.v[2] = next;
	let index3 = 0;
	for (const edge2 of held3) {
		if (!(__at(kept, index3, "std/src/reactive.vl:1730:7"))) {
			detach(edge2[1]);
		}
		index3 = index3 + 1;
	}
	tracker[0].v[3] = false;
}
function reusable(held3, kept, identity5, position) {
	const $dT = identity5;
	let $dU = null;
	if ($dT[0] === 0) {
		const wanted = $dT[1];
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
		$dU = [ 1 ];
	} else {
		$dU = [ 1 ];
	}
	return $dU;
}
function same_identity(identity5, wanted) {
	const $cV = identity5;
	let $cW = null;
	if ($cV[0] === 0) {
		const held3 = $cV[1];
		$cW = held3 === wanted;
	} else {
		$cW = false;
	}
	return $cW;
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
	const $eb = tracker[0].v[7];
	let $ec = null;
	if ($eb[0] === 0) {
		const site = __clone($eb[1]);
		$ec = place_subscriber(relay[0], site);
	} else {
		$ec = undefined;
	}
	$ec;
	return relay;
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $ep = tracker[0].v[6];
	let $eq = null;
	if ($ep[0] === 0) {
		const lists = $ep[1];
		$eq = lists;
	} else {
		return;
		$eq = undefined;
	}
	const lists2 = $eq;
	let $er = null;
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
		$er = undefined;
	}
	return $er;
}
function forget_reads(tracker) {
	const $es = tracker[0].v[6];
	let $et = null;
	if ($es[0] === 0) {
		const lists = $es[1];
		$et = lists;
	} else {
		return;
		$et = undefined;
	}
	const lists2 = $et;
	let $eu = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge2 of edges) {
			detach(edge2[1]);
		}
		$eu = undefined;
	}
	$eu;
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
	const $cR = tracker[0].v[6];
	let $cS = null;
	if ($cR[0] === 0) {
		const lists = $cR[1];
		$cS = lists;
	} else {
		return true;
		$cS = undefined;
	}
	const lists2 = $cS;
	const $cT = identity5;
	let $cU = null;
	if ($cT[0] === 0) {
		const wanted = $cT[1];
		const count2 = lists2.v[0].length;
		let index2 = 0;
		while (index2 < count2) {
			if (same_identity(__at(lists2.v[0], index2, "std/src/reactive.vl:1877:22")[0], wanted)) {
				return false;
			}
			index2 = index2 + 1;
		}
		$cU = true;
	} else {
		$cU = true;
	}
	return $cU;
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
function also_releasing(handle, release8) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release8();
		const $eD = previous;
		let $eE = null;
		if ($eD[0] === 0) {
			const earlier = $eD[1];
			$eE = earlier();
		} else {
			$eE = undefined;
		}
		return $eE;
	} ];
}
function placed_at(subscriber, site) {
	place_subscriber(subscriber[0], site);
	return subscriber;
}
function settle_together(body, $bV) {
	const $bW = $bV;
	let $bX = null;
	if ($bW[0] === 0) {
		const current = $bW[1];
		$bX = body(current);
	} else {
		const $bY = last(draining_turns.v);
		let $bZ = null;
		if ($bY[0] === 0) {
			const draining = $bY[1];
			$bZ = body(draining);
		} else {
			$bZ = batch(body, $bV);
		}
		$bX = $bZ;
	}
	return $bX;
}
function new5() {
	return [ [  ] ];
}
function follow(self, at3) {
	if (self[0].length === 0) {
		self[0].push(new6());
	}
	return follow2(__at(self[0], 0, "std/src/reactive/lens.vl:145:3"), at3);
}
function apply(self, change2, wakes) {
	if (self[0].length > 0) {
		apply2(__at(self[0], 0, "std/src/reactive/lens.vl:150:4"), change2, wakes);
	}
}
function locate(self, slot5) {
	if (self[0].length === 0) {
		return [ 1 ];
	}
	return locate2(__at(self[0], 0, "std/src/reactive/lens.vl:158:3"), slot5);
}
function follows_items(self) {
	return true;
}
function release(self, slot5) {
	if (self[0].length > 0) {
		release2(__at(self[0], 0, "std/src/reactive/lens.vl:167:4"), slot5);
	}
}
function new6() {
	return [ [  ], [  ], [  ], [  ], [  ], [  ], 0 ];
}
function seat(self, at3) {
	let low = 0;
	let high = self[3].length;
	while (high - low > 1) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[3], middle, "std/src/reactive/lens.vl:213:7") <= at3) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at3 - __at(self[3], low, "std/src/reactive/lens.vl:219:14") ];
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
		const slot5 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:241:15"), index2, "std/src/reactive/lens.vl:241:15");
		if (slot5 !== -(1)) {
			__at_put(self[4], as_usize(slot5), block, "std/src/reactive/lens.vl:243:5");
			__at_put(self[5], as_usize(slot5), index2, "std/src/reactive/lens.vl:244:5");
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
function widen(self, at3, count2) {
	if (count2 === 0 || at3 >= self[6]) {
		return;
	}
	const $bq = seat(self, at3);
	const place = $bq[0];
	const index2 = $bq[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:265:15");
	let $br = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:269:5"), index2, -(1), "std/src/reactive/lens.vl:269:35");
			made = made + 1;
		}
		$br = undefined;
	} else {
		const before2 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:273:17"));
		let grown = [  ];
		let copied = 0;
		while (copied < index2) {
			grown.push(__at(before2, copied, "std/src/reactive/lens.vl:277:16"));
			copied = copied + 1;
		}
		let made2 = 0;
		while (made2 < count2) {
			grown.push(-(1));
			made2 = made2 + 1;
		}
		const held3 = before2.length;
		while (copied < held3) {
			grown.push(__at(before2, copied, "std/src/reactive/lens.vl:287:16"));
			copied = copied + 1;
		}
		__at_put(self[0], as_usize(block), grown, "std/src/reactive/lens.vl:290:4");
	}
	$br;
	self[6] = self[6] + count2;
	renumber(self, block, index2 + count2);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:294:6").length > 2 * block_size) {
		rebalance(self, place);
	}
	restart(self);
}
function close(self, at3, count2, wakes) {
	let left = count2;
	while (left > 0 && at3 < self[6]) {
		const $bo = seat(self, at3);
		const place = $bo[0];
		const index2 = $bo[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:306:16");
		const held3 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:307:15").length;
		let $bp = null;
		if (left < held3 - index2) {
			$bp = left;
		} else {
			$bp = held3 - index2;
		}
		const take2 = $bp;
		let taken = 0;
		while (taken < take2) {
			const slot5 = __remove_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:311:16"), index2, "std/src/reactive/lens.vl:311:46");
			if (slot5 !== -(1)) {
				__at_put(self[4], as_usize(slot5), -(1), "std/src/reactive/lens.vl:313:6");
				wakes.push([ 2, slot5 ]);
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
	let first5 = [  ];
	let index2 = 0;
	while (index2 < block_size) {
		first5.push(__at(all3, index2, "std/src/reactive/lens.vl:336:15"));
		index2 = index2 + 1;
	}
	__at_put(self[0], as_usize(block), first5, "std/src/reactive/lens.vl:339:3");
	let next = place + 1;
	const held3 = all3.length;
	while (index2 < held3) {
		const fresh2 = open(self, next);
		let filled = 0;
		while (filled < block_size && index2 < held3) {
			__at(self[0], as_usize(fresh2), "std/src/reactive/lens.vl:346:5").push(__at(all3, index2, "std/src/reactive/lens.vl:346:40"));
			index2 = index2 + 1;
			filled = filled + 1;
		}
		renumber(self, fresh2, 0);
		next = next + 1;
	}
}
function follow2(self, at3) {
	while (self[6] <= at3) {
		const places = self[1].length;
		if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:361:34")), "std/src/reactive/lens.vl:361:22").length >= block_size) {
			const fresh2 = open(self, places);
			__at_put(self[2], as_usize(fresh2), places, "std/src/reactive/lens.vl:363:5");
			self[3].push(self[6]);
		}
		const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:366:15");
		__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:367:4").push(-(1));
		self[6] = self[6] + 1;
	}
	const $u = seat(self, at3);
	const place = $u[0];
	const index2 = $u[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:371:15");
	const held3 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:372:14"), index2, "std/src/reactive/lens.vl:372:14");
	if (held3 !== -(1)) {
		return held3;
	}
	const slot5 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index2);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:379:3"), index2, slot5, "std/src/reactive/lens.vl:379:3");
	return slot5;
}
function apply2(self, change2, wakes) {
	const $bm = change2;
	let $bn = null;
	if ($bm[0] === 0) {
		const at3 = $bm[1];
		const removed = $bm[2];
		const inserted = $bm[3];
		close(self, at3, removed, wakes);
		widen(self, at3, inserted);
		$bn = undefined;
	} else if ($bm[0] === 1) {
		const at4 = $bm[1];
		const count2 = $bm[2];
		$bn = widen(self, at4, count2);
	} else if ($bm[0] === 3) {
		const at5 = $bm[1];
		const count3 = $bm[2];
		$bn = close(self, at5, count3, wakes);
	} else if ($bm[0] === 2) {
		const at6 = $bm[1];
		let $bt = null;
		if (at6 < self[6]) {
			const $bs = seat(self, at6);
			const place = $bs[0];
			const index2 = $bs[1];
			const slot5 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:396:29")), "std/src/reactive/lens.vl:396:17"), index2, "std/src/reactive/lens.vl:396:17");
			if (slot5 !== -(1)) {
				wakes.push([ 0, slot5 ]);
			}
			$bt = undefined;
		}
		$bn = $bt;
	} else if ($bm[0] === 4) {
		const at7 = $bm[1];
		const part = $bm[2];
		let $bv = null;
		if (at7 < self[6]) {
			const $bu = seat(self, at7);
			const place2 = $bu[0];
			const index3 = $bu[1];
			const slot6 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:405:29")), "std/src/reactive/lens.vl:405:17"), index3, "std/src/reactive/lens.vl:405:17");
			if (slot6 !== -(1)) {
				wakes.push([ 1, slot6, part ]);
			}
			$bv = undefined;
		}
		$bn = $bv;
	} else if ($bm[0] === 5) {
		const from = $bm[1];
		const to = $bm[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $bw = null;
		if (from > to) {
			$bw = from;
		} else {
			$bw = to;
		}
		const far = $bw;
		while (self[6] <= far) {
			const places = self[1].length;
			if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:419:36")), "std/src/reactive/lens.vl:419:24").length >= block_size) {
				const fresh2 = open(self, places);
				__at_put(self[2], as_usize(fresh2), places, "std/src/reactive/lens.vl:421:7");
				self[3].push(self[6]);
			}
			const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:424:17");
			__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:425:6").push(-(1));
			self[6] = self[6] + 1;
		}
		const $bx = seat(self, from);
		const left = $bx[0];
		const index4 = $bx[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:429:18");
		const slot7 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:430:16"), index4, "std/src/reactive/lens.vl:430:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:432:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:433:17");
		} else {
			renumber(self, source, index4);
		}
		restart(self);
		let $by = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$by = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:441:25")), "std/src/reactive/lens.vl:441:13").length ];
		} else {
			$by = seat(self, to);
		}
		const $bz = $by;
		const place3 = $bz[0];
		const landing2 = $bz[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:445:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:446:5"), landing2, slot7, "std/src/reactive/lens.vl:446:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing2);
		restart(self);
		$bn = undefined;
	} else {
		let slot8 = 0;
		const slots = as_i32(self[4].length);
		while (slot8 < slots) {
			if (__at(self[4], as_usize(slot8), "std/src/reactive/lens.vl:455:9") !== -(1)) {
				__at_put(self[4], as_usize(slot8), -(1), "std/src/reactive/lens.vl:456:7");
				wakes.push([ 2, slot8 ]);
			}
			slot8 = slot8 + 1;
		}
		self[0] = [  ];
		self[1] = [  ];
		self[2] = [  ];
		self[3] = [  ];
		self[6] = 0;
		$bn = undefined;
	}
	return $bn;
}
function release2(self, slot5) {
	const block = __at(self[4], as_usize(slot5), "std/src/reactive/lens.vl:471:15");
	if (block === -(1)) {
		return;
	}
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:475:3"), __at(self[5], as_usize(slot5), "std/src/reactive/lens.vl:475:33"), -(1), "std/src/reactive/lens.vl:475:3");
	__at_put(self[4], as_usize(slot5), -(1), "std/src/reactive/lens.vl:476:3");
}
function locate2(self, slot5) {
	const block = __at(self[4], as_usize(slot5), "std/src/reactive/lens.vl:480:15");
	if (block === -(1)) {
		return [ 1 ];
	}
	return [ 0, __at(self[3], __at(self[2], as_usize(block), "std/src/reactive/lens.vl:484:20"), "std/src/reactive/lens.vl:484:8") + __at(self[5], as_usize(slot5), "std/src/reactive/lens.vl:484:51") ];
}
function new7() {
	return [ new10(), [  ] ];
}
function cancel(task) {
	const $at = task;
	if ($at[0] === 0) {
		$at[1].cancel();
	}
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function m(id, text) {
	return [ id, text ];
}
function fresh($a, $b) {
	return new12((item2) => {
		return item2[0];
	}, $a, $b);
}
function shown(feed8) {
	let out = [  ];
	for (const item2 of shown2(overlay(feed8))) {
		out.push(item2[0]);
	}
	return out;
}
function held(feed8) {
	let out = [  ];
	for (const item2 of all2(items(feed8))) {
		out.push(item2[0]);
	}
	return out;
}
function said(state) {
	const $au = state;
	let $av = null;
	if ($au[0] === 0) {
		$av = "Pending";
	} else if ($au[0] === 1) {
		const token = $au[1];
		$av = "Ready(" + token + ")";
	} else if ($au[0] === 2) {
		const token2 = $au[1];
		$av = "Refreshing(" + token2 + ")";
	} else if ($au[0] === 3) {
		const error = $au[1];
		const stale = $au[2];
		const $aw = stale;
		let $ax = null;
		if ($aw[0] === 0) {
			const token3 = $aw[1];
			$ax = "Failed(" + error + ", " + token3 + ")";
		} else {
			$ax = "Failed(" + error + ")";
		}
		$av = $ax;
	} else {
		$av = "Absent";
	}
	return $av;
}
function edge(state) {
	const $fA = state;
	let $fB = null;
	if ($fA[0] === 0) {
		$fB = "Idle";
	} else if ($fA[0] === 1) {
		$fB = "Loading";
	} else if ($fA[0] === 2) {
		$fB = "End";
	} else {
		const error = $fA[1];
		$fB = "Failed(" + error + ")";
	}
	return $fB;
}
function eq4(self, other) {
	return self[0] === other[0] && self[1] === other[1];
}
function count(self) {
	return self.length;
}
function item(self, at3) {
	return __list_get(self, at3);
}
function lend(self, at3, read) {
	if (at3 < self.length) {
		read(__at(self, at3, "std/src/reactive/lens.vl:2219:10"));
	}
}
function put(self, at3, value) {
	__at_put(self, at3, __clone(value), "std/src/reactive/lens.vl:2239:3");
}
function change(self, at3, with2) {
	if (at3 < self.length) {
		with2(__at(self, at3, "std/src/reactive/lens.vl:2244:14"));
	}
}
function new8(value, caller) {
	let subscribers = [  ];
	const cell4 = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell4[0]), caller);
	}
	return cell4;
}
function lens_over(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new8(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over(value, handler, access) {
	return lens_over([ 0, __shared_new(value) ], handler, access);
}
function sequence(value, $f) {
	return over(value, new5(), [ (held3, at3) => {
		return at3 < count(held3);
	}, (held3, at3) => {
		return item(held3, at3);
	}, (held3, at3, read) => {
		return lend(held3, at3, read);
	}, (held3, at3, value2) => {
		return put(held3, at3, value2, $f);
	}, (held3, at3, with2) => {
		return change(held3, at3, with2, $f);
	} ]);
}
function over2(value, $e) {
	return [ sequence(value, $e) ];
}
function new9() {
	return [ __shared_new(new2()), __shared_new([  ]), __shared_new(new2()), __shared_new(new2()), new8(0, "std/src/reactive/lens/loaded.vl:130:24"), new8(0, "std/src/reactive/lens/loaded.vl:131:25"), __shared_new(0) ];
}
function lend2(value, read) {
	return read(value);
}
function viewing(cell4, read) {
	return lend2(cell4.v, read);
}
function peek_storage(storage, read) {
	const $j = storage;
	let $k = null;
	if ($j[0] === 0) {
		const cell4 = $j[1];
		$k = viewing(cell4, read);
	} else {
		const port = $j[1];
		let out = [ 1 ];
		port[0]((held3) => {
			return out = [ 0, read(held3) ];
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
function viewing2(cell4, read) {
	return lend2(cell4.v, read);
}
function peek_storage2(storage, read) {
	const $n = storage;
	let $o = null;
	if ($n[0] === 0) {
		const cell4 = $n[1];
		$o = viewing2(cell4, read);
	} else {
		const port = $n[1];
		let out = [ 1 ];
		port[0]((held3) => {
			return out = [ 0, read(held3) ];
		});
		const $p = out;
		let $q = null;
		if ($p[0] === 0) {
			const value = __clone($p[1]);
			$q = value;
		} else {
			$q = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$o = $q;
	}
	return $o;
}
function follow_id(storage, watch, holds2, at3) {
	if (viewing2(watch, (held3) => {
		return follows_items(held3[0]);
	}) && !(peek_storage2(storage, (held3) => {
		return holds2(held3, at3);
	}))) {
		return -(1);
	}
	const id = follow(watch.v[0], at3);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	const $v = as_usize(id);
	__at(watch.v[1], $v, "std/src/reactive/lens.vl:1058:2")[2] = __at(watch.v[1], $v, "std/src/reactive/lens.vl:1058:2")[2] + 1;
	return id;
}
function get(self, key) {
	return __nmap_get(self[0], hash2(key));
}
function insert(self, key, value) {
	__nmap_insert(self[0], hash2(key), __clone(key), __clone(value));
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
	__nmap_remove(self[0], hash2(key));
}
function is_empty(self) {
	return self.length === 0;
}
function entries(self) {
	let keys2 = __nmap_keys(self[0]);
	let values = __nmap_values(self[0]);
	let backwards = [  ];
	while (!(is_empty(keys2))) {
		const $y = __list_pop(keys2);
		let $z = null;
		if ($y[0] === 0) {
			const key = $y[1];
			const $A = __list_pop(values);
			let $B = null;
			if ($A[0] === 0) {
				const value = $A[1];
				$B = backwards.push([ key, value ]);
			} else {
				$B = undefined;
			}
			$z = $B;
		} else {
			$z = undefined;
		}
		$z;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $C = __list_pop(backwards);
		let $D = null;
		if ($C[0] === 0) {
			const pair = $C[1];
			$D = result.push(pair);
		} else {
			$D = undefined;
		}
		$D;
	}
	return result;
}
function new10() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function tap(self, before2, after) {
	const id = self[0].v[5];
	self[0].v[5] = id + 1;
	self[0].v[4].push([ id, before2, after ]);
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
		const $S = _;
		const _key = $S[0];
		const id = $S[1];
		release_id(self[0][0].v[3], id);
	}
	self[1].v = new10();
}
function index_by(self, key, $i) {
	const found = __shared_new(new2());
	const leaving = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds2 = self[0].v[1][0];
	const view6 = self[0].v[1][2];
	const key_at = (at3) => {
		let out = [ 1 ];
		peek_storage(storage, (held3) => {
			return view6(held3, at3, (item2) => {
				return out = [ 0, key(item2) ];
			});
		});
		return out;
	};
	const map2 = (held3, at3) => {
		const id = follow_id(storage, watch, holds2, at3);
		const before2 = get(found.v, held3);
		insert(found.v, held3, id);
		const $w = before2;
		if ($w[0] === 0) {
			release_id(watch, $w[1]);
		}
		return;
	};
	const unmap = (held3) => {
		const $x = get(found.v, held3);
		if ($x[0] === 0) {
			remove(found.v, held3);
			release_id(watch, $x[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $E = _;
			const _held = $E[0];
			const old = $E[1];
			release_id(watch, old);
		}
		found.v = new10();
		let at3 = 0;
		const count2 = peek_storage2(storage, (held3) => {
			return count(held3);
		});
		while (at3 < count2) {
			const $J = key_at(at3);
			if ($J[0] === 0) {
				map2($J[1], at3);
			}
			at3 = at3 + 1;
		}
		return;
	};
	fill();
	const note = (at3, count2) => {
		let gone2 = [  ];
		let index3 = at3;
		while (index3 < at3 + count2) {
			const $K = key_at(index3);
			if ($K[0] === 0) {
				gone2.push($K[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone2;
		return;
	};
	const arrive = (at3, count2) => {
		let index3 = at3;
		while (index3 < at3 + count2) {
			const $L = key_at(index3);
			if ($L[0] === 0) {
				map2($L[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change2) => {
		const $M = change2;
		let $N = null;
		if ($M[0] === 0) {
			const at3 = $M[1];
			const removed = $M[2];
			const _inserted = $M[3];
			$N = note(at3, removed);
		} else if ($M[0] === 3) {
			const at4 = $M[1];
			const count2 = $M[2];
			$N = note(at4, count2);
		} else if ($M[0] === 2) {
			const at5 = $M[1];
			$N = note(at5, 1);
		} else if ($M[0] === 4) {
			const at6 = $M[1];
			const _part = $M[2];
			$N = note(at6, 1);
		} else {
			$N = undefined;
		}
		return $N;
	}, (change2, stage) => {
		const $O = stage;
		let $P = null;
		if ($O[0] === 0) {
			$P = undefined;
		} else if ($O[0] === 3) {
			return;
		} else {
			fill();
			return;
			$P = undefined;
		}
		$P;
		const $Q = change2;
		let $R = null;
		if ($Q[0] === 0) {
			const at3 = $Q[1];
			const _removed = $Q[2];
			const inserted = $Q[3];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			arrive(at3, inserted);
			$R = undefined;
		} else if ($Q[0] === 1) {
			const at4 = $Q[1];
			const count2 = $Q[2];
			$R = arrive(at4, count2);
		} else if ($Q[0] === 3) {
			const _at = $Q[1];
			const _count = $Q[2];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			$R = undefined;
		} else if ($Q[0] === 2) {
			const at5 = $Q[1];
			for (const held5 of leaving.v) {
				unmap(held5);
			}
			arrive(at5, 1);
			$R = undefined;
		} else if ($Q[0] === 4) {
			const at6 = $Q[1];
			const _part = $Q[2];
			for (const held6 of leaving.v) {
				unmap(held6);
			}
			arrive(at6, 1);
			$R = undefined;
		} else if ($Q[0] === 5) {
			const _from = $Q[1];
			const _to = $Q[2];
			$R = undefined;
		} else {
			$R = fill();
		}
		return $R;
	});
	const index2 = [ __clone(self), found, tap2 ];
	defer_to_owner(() => {
		return release3(index2);
	}, $i);
	return index2;
}
function index_by2(self, key, $h) {
	return index_by(self[0], key, $h);
}
function new11() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function trim(self) {
	let lowest = self[1].v;
	for (const cursor of self[3].v) {
		const at3 = cursor[1].v;
		if (at3 < lowest) {
			lowest = at3;
		}
	}
	const base = self[2].v;
	if (lowest > base) {
		const dropped = lowest - base;
		let kept = [  ];
		let index2 = as_usize(dropped);
		const held3 = __clone(self[0].v);
		const length3 = held3.length;
		while (index2 < length3) {
			const $ab = __list_get(held3, index2);
			let $ac = null;
			if ($ab[0] === 0) {
				const op = $ab[1];
				$ac = kept.push(op);
			} else {
				$ac = undefined;
			}
			$ac;
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
function rows(self, $Y) {
	const log = new11();
	const held3 = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds2 = self[0].v[1][0];
	const fill = () => {
		let ids = [  ];
		let at3 = 0;
		const count2 = peek_storage2(storage, (list) => {
			return count(list);
		});
		while (at3 < count2) {
			ids.push(follow_id(storage, watch, holds2, at3));
			at3 = at3 + 1;
		}
		return ids;
	};
	const leave = (at3, count2) => {
		let gone2 = [  ];
		let taken = 0;
		while (taken < count2) {
			const id = __remove_at(held3.v, at3, "std/src/reactive/lens.vl:1836:27");
			release_id(watch, id);
			gone2.push(id);
			taken = taken + 1;
		}
		return gone2;
	};
	const arrive = (at3, count2) => {
		let arrived = [  ];
		let index2 = at3;
		while (index2 < at3 + count2) {
			const id = follow_id(storage, watch, holds2, index2);
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
		const $Z = change2;
		let $aa = null;
		if ($Z[0] === 0) {
			const at3 = $Z[1];
			const removed = $Z[2];
			const inserted = $Z[3];
			const gone2 = leave(at3, removed);
			record(log, [ 0, at3, gone2, arrive(at3, inserted) ]);
			$aa = undefined;
		} else if ($Z[0] === 1) {
			const at4 = $Z[1];
			const count2 = $Z[2];
			$aa = record(log, [ 0, at4, [  ], arrive(at4, count2) ]);
		} else if ($Z[0] === 3) {
			const at5 = $Z[1];
			const count3 = $Z[2];
			$aa = record(log, [ 0, at5, leave(at5, count3), [  ] ]);
		} else if ($Z[0] === 5) {
			const from = $Z[1];
			const to = $Z[2];
			const id = __remove_at(held3.v, from, "std/src/reactive/lens.vl:1864:28");
			__insert_at(held3.v, to, id, "std/src/reactive/lens.vl:1865:19");
			record(log, [ 3, from, 1, to ]);
			$aa = undefined;
		} else if ($Z[0] === 6) {
			for (const id2 of held3.v) {
				release_id(watch, id2);
			}
			held3.v = fill();
			record(log, [ 2, __clone(held3.v) ]);
			$aa = undefined;
		} else {
			$aa = undefined;
		}
		return $aa;
	});
	const feed8 = [ __clone(self), held3, __clone(log), tap2 ];
	defer_to_owner(() => {
		return release4(feed8);
	}, $Y);
	return feed8;
}
function rows2(self, $X) {
	return rows(self[0], $X);
}
function lens(self) {
	return __clone(self[0]);
}
function slot_of(self, id) {
	const $ae = __clone(self);
	let $ad = null;
	if (id < 0) {
		$ad = -(1);
	} else {
		$ad = id;
	}
	return [ $ae, $ad ];
}
function index(self) {
	return [ __clone(self) ];
}
function beside(lens2, key, $g) {
	const ledger = new9();
	return [ __clone(lens2), key, ledger, index_by2(lens2, key, $g), rows2(lens2, $g), index(slot_of(lens(lens2), -(1))) ];
}
function get2(self) {
	return __clone(self[0].v);
}
function last(self) {
	let $ao = null;
	if (is_empty(self)) {
		$ao = [ 1 ];
	} else {
		$ao = __list_get(self, self.length - 1);
	}
	return $ao;
}
function notify(self, $aj) {
	const traced = tracing.v;
	let $ak = null;
	if (traced) {
		$ak = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$ak = "";
	}
	const cause = $ak;
	const $al = $aj;
	let $am = null;
	if ($al[0] === 0) {
		const turn = $al[1];
		$am = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $ap = last(draining_turns.v);
		let $aq = null;
		if ($ap[0] === 0) {
			const draining = $ap[1];
			$aq = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $ar = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$ar = undefined;
				}
				$ar;
			}
			$aq = undefined;
		}
		$am = $aq;
	}
	return $am;
}
function set(self, value, $ai, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $ai);
}
function drop_pages(self, $ag) {
	self[6].v[2] = 0;
	self[6].v[3] = 0;
	const $ah = get2(self[4]);
	if ($ah[0] === 1) {
		set(self[4], [ 0 ], $ag, "std/src/reactive/lens/loaded.vl:873:20");
	}
	const $as = get2(self[5]);
	if ($as[0] === 1) {
		set(self[5], [ 0 ], $ag, "std/src/reactive/lens/loaded.vl:876:20");
	}
}
function retire(self, $af) {
	self[6].v[1] = 0;
	drop_pages(self, $af);
	const running = self[7].v;
	cancel(running[0]);
	cancel(running[1]);
	cancel(running[2]);
	self[7].v = [ [ 1 ], [ 1 ], [ 1 ] ];
}
function new12(key, $c, $d) {
	const empty = [  ];
	const lens2 = over2(empty, $c);
	const feed8 = [ __clone(lens2), beside(lens2, key, $d), key, new8([ 0 ], "std/src/reactive/lens/loaded.vl:767:24"), new8([ 0 ], "std/src/reactive/lens/loaded.vl:768:29"), new8([ 0 ], "std/src/reactive/lens/loaded.vl:769:29"), __shared_new([ 0, 0, 0, 0 ]), __shared_new([ [ 1 ], [ 1 ], [ 1 ] ]) ];
	defer_to_owner(() => {
		return retire(feed8, $c);
	}, $d);
	return feed8;
}
function phase(self) {
	return [ __clone(self[3]) ];
}
function get3(self) {
	return get2(self[0]);
}
function items(self) {
	return __clone(self[0]);
}
function peek(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function size(self) {
	return peek(self, (held3) => {
		return count(held3);
	});
}
function size2(self) {
	return size(self[0]);
}
function claim(self) {
	const token = self[6].v[0] + 1;
	self[6].v[0] = token;
	return token;
}
function latest(self) {
	const $aA = self;
	let $aB = null;
	if ($aA[0] === 1) {
		const value = $aA[1];
		$aB = [ 0, __clone(value) ];
	} else if ($aA[0] === 2) {
		const value2 = $aA[1];
		$aB = [ 0, __clone(value2) ];
	} else if ($aA[0] === 3) {
		const _error = $aA[1];
		const stale = __clone($aA[2]);
		$aB = stale;
	} else {
		$aB = [ 1 ];
	}
	return $aB;
}
function refreshed(self) {
	const $aC = latest(self);
	let $aD = null;
	if ($aC[0] === 0) {
		const value = $aC[1];
		$aD = [ 2, value ];
	} else {
		$aD = [ 0 ];
	}
	return $aD;
}
function same_phase(first5, second4) {
	const $aF = [ first5, second4 ];
	let $aG = null;
	if ($aF[0][0] === 0 && $aF[1][0] === 0) {
		$aG = true;
	} else if ($aF[0][0] === 4 && $aF[1][0] === 4) {
		$aG = true;
	} else if ($aF[0][0] === 1 && $aF[1][0] === 1) {
		const mine2 = $aF[0][1];
		const theirs = $aF[1][1];
		$aG = mine2 === theirs;
	} else if ($aF[0][0] === 2 && $aF[1][0] === 2) {
		const mine3 = $aF[0][1];
		const theirs2 = $aF[1][1];
		$aG = mine3 === theirs2;
	} else {
		$aG = false;
	}
	return $aG;
}
function notify2(self, $aj) {
	const traced = tracing.v;
	let $aH = null;
	if (traced) {
		$aH = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$aH = "";
	}
	const cause = $aH;
	const $aI = $aj;
	let $aJ = null;
	if ($aI[0] === 0) {
		const turn = $aI[1];
		$aJ = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $aK = last(draining_turns.v);
		let $aL = null;
		if ($aK[0] === 0) {
			const draining = $aK[1];
			$aL = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $aM = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$aM = undefined;
				}
				$aM;
			}
			$aL = undefined;
		}
		$aJ = $aL;
	}
	return $aJ;
}
function set2(self, value, $ai, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $ai);
}
function set_phase(cell4, next, $aE) {
	if (!(same_phase(get2(cell4), next))) {
		set2(cell4, next, $aE, "std/src/reactive/lens/loaded.vl:716:8");
	}
}
function load(self, $az) {
	const token = claim(self);
	self[6].v[1] = token;
	set_phase(self[3], refreshed(get2(self[3])), $az);
	return token;
}
function all_of(storage) {
	return peek_storage2(storage, (held3) => {
		let out = [  ];
		let at3 = 0;
		const count2 = count(held3);
		while (at3 < count2) {
			const $aT = item(held3, at3);
			if ($aT[0] === 0) {
				out.push($aT[1]);
			}
			at3 = at3 + 1;
		}
		return out;
	});
}
function all(self) {
	return all_of(self[0].v[0]);
}
function contains_key(self, key) {
	return __nmap_has(self[0], hash2(key));
}
function size3(self) {
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
function splice(self, at3, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self, at3, "std/src/reactive/lens.vl:2228:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at3 + offset, __clone(value), "std/src/reactive/lens.vl:2233:9");
		offset = offset + 1;
	}
}
function mutate(self, change2) {
	const $bg = self[0].v[0];
	let $bh = null;
	if ($bg[0] === 0) {
		const cell4 = $bg[1];
		$bh = change2(cell4.v);
	} else {
		const port = __clone($bg[1]);
		$bh = port[1](change2);
	}
	return $bh;
}
function wake_all(watch, slot5, $bP) {
	const held3 = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot5), "std/src/reactive/lens.vl:1117:36"));
	});
	const $bQ = held3[0];
	if ($bQ[0] === 0) {
		notify2($bQ[1], $bP);
	}
	for (const part of held3[1]) {
		notify2(part[1], $bP);
	}
}
function wake_part(watch, slot5, name, $bR) {
	const held3 = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot5), "std/src/reactive/lens.vl:1127:36"));
	});
	const $bS = held3[0];
	if ($bS[0] === 0) {
		notify2($bS[1], $bR);
	}
	for (const part of held3[1]) {
		if (part[0] === name) {
			notify2(part[1], $bR);
		}
	}
}
function batch(body, $ca) {
	const $cb = $ca;
	let $cc = null;
	if ($cb[0] === 0) {
		const current = $cb[1];
		$cc = body(current);
	} else {
		const fresh2 = new3();
		const result = body(fresh2);
		drain(fresh2);
		fresh2[5].v = true;
		$cc = result;
	}
	return $cc;
}
function landed(self, changes, late, $bj) {
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
		const $bk = change2;
		let $bl = null;
		if ($bk[0] === 0) {
			const _at = $bk[1];
			const removed = $bk[2];
			const inserted = $bk[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$bl = undefined;
		} else if ($bk[0] === 1) {
			const _at2 = $bk[1];
			const _count = $bk[2];
			$bl = resized = true;
		} else if ($bk[0] === 3) {
			const _at3 = $bk[1];
			const _count2 = $bk[2];
			$bl = resized = true;
		} else if ($bk[0] === 6) {
			$bl = resized = true;
		} else if ($bk[0] === 5) {
			const _from = $bk[1];
			const _to = $bk[2];
			$bl = shaped = true;
		} else {
			$bl = undefined;
		}
		$bl;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change2, wakes);
		}
		let $bB = null;
		if (self[0].v[4].length > 0) {
			let $bA = null;
			if (!(late)) {
				$bA = [ 0 ];
			} else if (!(several)) {
				$bA = [ 1 ];
			} else if (index2 === 0) {
				$bA = [ 2 ];
			} else {
				$bA = [ 3 ];
			}
			const stage = $bA;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$bB = undefined;
		}
		$bB;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $bC = null;
	if (shaped || resized) {
		$bC = watch.v[2];
	} else {
		$bC = [ 1 ];
	}
	const shape = __clone($bC);
	let $bD = null;
	if (resized) {
		$bD = watch.v[3];
	} else {
		$bD = [ 1 ];
	}
	const sized = __clone($bD);
	let $bE = undefined;
	let $bF = undefined;
	if (wakes.length === 0 && (($bE = shape), $bE[0] === 1) && (($bF = sized), $bF[0] === 1)) {
		notify2(published, $bj);
		return;
	}
	settle_together(($bM) => {
		for (const wake2 of wakes) {
			const $bN = wake2;
			let $bO = null;
			if ($bN[0] === 0) {
				const slot5 = $bN[1];
				$bO = wake_all(watch, slot5, [ 0, $bM ]);
			} else if ($bN[0] === 2) {
				const slot6 = $bN[1];
				$bO = wake_all(watch, slot6, [ 0, $bM ]);
			} else {
				const slot7 = $bN[1];
				const part = $bN[2];
				$bO = wake_part(watch, slot7, part, [ 0, $bM ]);
			}
			$bO;
		}
		const $bT = sized;
		if ($bT[0] === 0) {
			notify2($bT[1], [ 0, $bM ]);
		}
		const $bU = shape;
		if ($bU[0] === 0) {
			notify2($bU[1], [ 0, $bM ]);
		}
		notify2(published, [ 0, $bM ]);
		return;
	}, $bj);
}
function landed_one(self, change2, $bi) {
	landed(self, [ __clone(change2) ], false, $bi);
}
function splice2(self, at3, removed, inserted, $aZ) {
	const held3 = size3(self);
	let $be = null;
	if (at3 > held3) {
		$be = held3;
	} else {
		$be = at3;
	}
	const start7 = $be;
	let $bf = null;
	if (start7 + removed > held3) {
		$bf = held3 - start7;
	} else {
		$bf = removed;
	}
	const taking = $bf;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start7, taking, arriving ];
	coming_one(self, change2);
	mutate(self, (held4) => {
		return splice(held4, start7, taking, inserted);
	});
	landed_one(self, change2, $aZ);
}
function remove_at(self, at3, $aY) {
	return splice2(self, at3, 1, [  ], $aY);
}
function eq5(self, b) {
	const $ch = self;
	let $ck = null;
	if ($ch[0] === 0) {
		const $ci = b;
		let $cj = null;
		if ($ci[0] === 0) {
			$cj = eq4($ch[1], $ci[1]);
		} else {
			$cj = false;
		}
		$ck = $cj;
	} else {
		const $cl = b;
		$ck = $cl[0] === 1;
	}
	return $ck;
}
function has(self, at3) {
	const holds2 = self[0].v[1][0];
	return peek(self, (held3) => {
		return holds2(held3, at3);
	});
}
function landed2(self, changes, late, $bj) {
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
		const $cq = change2;
		let $cr = null;
		if ($cq[0] === 0) {
			const _at = $cq[1];
			const removed = $cq[2];
			const inserted = $cq[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$cr = undefined;
		} else if ($cq[0] === 1) {
			const _at2 = $cq[1];
			const _count = $cq[2];
			$cr = resized = true;
		} else if ($cq[0] === 3) {
			const _at3 = $cq[1];
			const _count2 = $cq[2];
			$cr = resized = true;
		} else if ($cq[0] === 6) {
			$cr = resized = true;
		} else if ($cq[0] === 5) {
			const _from = $cq[1];
			const _to = $cq[2];
			$cr = shaped = true;
		} else {
			$cr = undefined;
		}
		$cr;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change2, wakes);
		}
		let $ct = null;
		if (self[0].v[4].length > 0) {
			let $cs = null;
			if (!(late)) {
				$cs = [ 0 ];
			} else if (!(several)) {
				$cs = [ 1 ];
			} else if (index2 === 0) {
				$cs = [ 2 ];
			} else {
				$cs = [ 3 ];
			}
			const stage = $cs;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$ct = undefined;
		}
		$ct;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $cu = null;
	if (shaped || resized) {
		$cu = watch.v[2];
	} else {
		$cu = [ 1 ];
	}
	const shape = __clone($cu);
	let $cv = null;
	if (resized) {
		$cv = watch.v[3];
	} else {
		$cv = [ 1 ];
	}
	const sized = __clone($cv);
	let $cw = undefined;
	let $cx = undefined;
	if (wakes.length === 0 && (($cw = shape), $cw[0] === 1) && (($cx = sized), $cx[0] === 1)) {
		notify2(published, $bj);
		return;
	}
	settle_together(($bM) => {
		for (const wake2 of wakes) {
			const $cy = wake2;
			let $cz = null;
			if ($cy[0] === 0) {
				const slot5 = $cy[1];
				$cz = wake_all(watch, slot5, [ 0, $bM ]);
			} else if ($cy[0] === 2) {
				const slot6 = $cy[1];
				$cz = wake_all(watch, slot6, [ 0, $bM ]);
			} else {
				const slot7 = $cy[1];
				const part = $cy[2];
				$cz = wake_part(watch, slot7, part, [ 0, $bM ]);
			}
			$cz;
		}
		const $cA = sized;
		if ($cA[0] === 0) {
			notify2($cA[1], [ 0, $bM ]);
		}
		const $cB = shape;
		if ($cB[0] === 0) {
			notify2($cB[1], [ 0, $bM ]);
		}
		notify2(published, [ 0, $bM ]);
		return;
	}, $bj);
}
function landed_one2(self, change2, $bi) {
	landed2(self, [ __clone(change2) ], false, $bi);
}
function replace(self, at3, value, $cm) {
	const watch = self[0].v[3];
	let $cn = null;
	if (has(self, at3)) {
		$cn = [ 2, __clone(at3) ];
	} else if (viewing2(watch, (held3) => {
		return follows_items(held3[0]);
	})) {
		return false;
	} else {
		$cn = [ 1, __clone(at3), 1 ];
	}
	const change2 = $cn;
	coming_one(self, change2);
	const write = self[0].v[1][3];
	mutate(self, (held3) => {
		return write(held3, at3, value);
	});
	landed_one2(self, change2, $cm);
	return true;
}
function shift(self, from, to) {
	const item2 = __remove_at(self, from, "std/src/reactive/lens.vl:2249:19");
	__insert_at(self, to, item2, "std/src/reactive/lens.vl:2250:8");
}
function move_item(self, from, to, $cF) {
	const held3 = size(self);
	if (from >= held3 || from === to) {
		return;
	}
	let $cG = null;
	if (to >= held3) {
		$cG = held3 - 1;
	} else {
		$cG = to;
	}
	const landing2 = $cG;
	const change2 = [ 5, from, landing2 ];
	coming_one(self, change2);
	mutate(self, (held4) => {
		return shift(held4, from, landing2, $cF);
	});
	landed_one2(self, change2, $cF);
}
function insert_at(self, at3, value, $cH) {
	return splice2(self, at3, 0, [ __clone(value) ], $cH);
}
function reconcile_by_key(self, next, key, $aR) {
	const lens2 = __clone(self);
	batch(($aS) => {
		let wanted = new10();
		for (const item2 of next) {
			insert(wanted, key(item2), true);
		}
		let keys2 = [  ];
		for (const item3 of all(lens2)) {
			keys2.push(key(item3));
		}
		let at3 = keys2.length;
		while (at3 > 0) {
			at3 = at3 - 1;
			if (!(contains_key(wanted, __at(keys2, at3, "std/src/reactive/lens.vl:1907:29")))) {
				remove_at(lens2, at3, [ 0, $aS ]);
				__remove_at(keys2, at3, "std/src/reactive/lens.vl:1909:11");
			}
		}
		let index2 = 0;
		for (const item4 of next) {
			const want = key(item4);
			let $cC = null;
			if (index2 < keys2.length && __at(keys2, index2, "std/src/reactive/lens.vl:1915:30") === want) {
				if (!(eq5(peek(lens2, (list) => {
					return item(list, index2);
				}), [ 0, __clone(item4) ]))) {
					replace(lens2, index2, item4, [ 0, $aS ]);
				}
				$cC = undefined;
			} else {
				let found = [ 1 ];
				let search = index2 + 1;
				while (search < keys2.length) {
					if (__at(keys2, search, "std/src/reactive/lens.vl:1923:10") === want) {
						found = [ 0, search ];
						search = keys2.length;
					}
					search = search + 1;
				}
				const $cD = found;
				let $cE = null;
				if ($cD[0] === 0) {
					const from = $cD[1];
					move_item(lens2, from, index2, [ 0, $aS ]);
					const moved = __remove_at(keys2, from, "std/src/reactive/lens.vl:1932:25");
					__insert_at(keys2, index2, moved, "std/src/reactive/lens.vl:1933:13");
					if (!(eq5(peek(lens2, (list) => {
						return item(list, index2);
					}), [ 0, __clone(item4) ]))) {
						replace(lens2, index2, item4, [ 0, $aS ]);
					}
					$cE = undefined;
				} else {
					insert_at(lens2, index2, item4, [ 0, $aS ]);
					__insert_at(keys2, index2, want, "std/src/reactive/lens.vl:1940:13");
					$cE = undefined;
				}
				$cC = $cE;
			}
			$cC;
			index2 = index2 + 1;
		}
		return;
	}, $aR);
}
function failed(self, error) {
	return [ 3, __clone(error), latest(self) ];
}
function settle(self, token, reply, $aN) {
	if (token === 0 || self[6].v[1] !== token) {
		return false;
	}
	self[6].v[1] = 0;
	const feed8 = __clone(self);
	const $aO = reply;
	let $aP = null;
	if ($aO[0] === 0) {
		const list = $aO[1];
		$aP = batch(($aQ) => {
			reconcile_by_key(lens(feed8[0]), list, feed8[2], [ 0, $aQ ]);
			drop_pages(feed8, [ 0, $aQ ]);
			set_phase(feed8[3], [ 1, token ], [ 0, $aQ ]);
			return;
		}, $aN);
	} else {
		const error = $aO[1];
		$aP = set_phase(self[3], failed(get2(self[3]), error), $aN);
	}
	$aP;
	return true;
}
function follow3(self, at3) {
	const core = self[0];
	return slot_of(self, follow_id(core.v[0], core.v[3], core.v[1][0], at3));
}
function release5(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot(self, at3, $cJ) {
	const handle = follow3(self, at3);
	defer_to_owner(() => {
		return release5(handle);
	}, $cJ);
	return handle;
}
function slot2(self, at3, $cI) {
	return slot(self[0], at3, $cI);
}
function overlay(self) {
	return __clone(self[1]);
}
function slot_cell(watch, slot5) {
	if (slot5 < 0) {
		return new8(0, "std/src/reactive/lens.vl:1079:19");
	}
	const entry = as_usize(slot5);
	const $cQ = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1082:5")[0];
	const cell4 = __clone($cQ[1]);
	if ($cQ[0] === 0) {
		return cell4;
	}
	const cell5 = new8(0, "std/src/reactive/lens.vl:1085:25");
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1086:2")[0] = [ 0, __clone(cell5) ];
	return cell5;
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
function at(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][0].v[3], (held3) => {
		return locate(held3[0], self[1]);
	});
}
function get4(self) {
	const $cZ = at(self);
	if ($cZ[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held3) => {
		return read(held3, $cZ[1]);
	});
}
function track(self, $cP) {
	const scope = $cP;
	const identity5 = identity2(self);
	if (admits(scope, identity5)) {
		lists_of(scope[0]).v[0].push([ identity5, (subscriber) => {
			return on_settle2(self, subscriber);
		} ]);
	}
	return get4(self);
}
function row(self, key) {
	const $dc = get(self[3].v, key);
	if ($dc[0] === 0) {
		return $dc[1];
	}
	const made = [ new8(0, "std/src/reactive/lens/loaded.vl:141:44"), new8(false, "std/src/reactive/lens/loaded.vl:141:74") ];
	insert(self[3].v, key, made);
	return made;
}
function track2(self, $cP) {
	const scope = $cP;
	const identity5 = identity(self);
	if (admits(scope, identity5)) {
		lists_of(scope[0]).v[0].push([ identity5, (subscriber) => {
			return on_settle(self, subscriber);
		} ]);
	}
	return get2(self);
}
function showing(self, key) {
	const $dd = get(self[0].v, key);
	if ($dd[0] === 0) {

	} else {
		return [ 1 ];
	}
	let out = [ 1 ];
	for (const proposal of $dd[1][0]) {
		if (proposal[0] > $dd[1][1]) {
			out = [ 0, __clone(proposal) ];
		}
	}
	return out;
}
function provisional(self, key) {
	const $dj = get(self[0].v, key);
	if ($dj[0] === 0) {

	} else {
		return false;
	}
	let out = false;
	for (const proposal of $dj[1][0]) {
		if (proposal[2]) {
			out = true;
		}
	}
	return out;
}
function over3(self, key, held3) {
	const $de = showing(self, key);
	let $df = null;
	if ($de[0] === 1) {
		$df = __clone(held3);
	} else {
		const proposal = $de[1];
		const $dg = proposal[1];
		let $dh = null;
		if ($dg[0] === 1) {
			$dh = [ 1 ];
		} else {
			const value = $dg[1];
			const $di = held3;
			let $dk = null;
			if ($di[0] === 0 || provisional(self, key)) {
				$dk = [ 0, __clone(value) ];
			} else {
				$dk = [ 1 ];
			}
			$dh = $dk;
		}
		$df = $dh;
	}
	return $df;
}
function derive(body, caller) {
	return [ body, caller ];
}
function distinct(self, caller) {
	return [ self, caller ];
}
function is_none(self) {
	const $dC = self;
	return $dC[0] === 1;
}
function run_body(runs2, body) {
	const run = renew(runs2);
	const $dL = nursery(run);
	let $dM = null;
	if ($dL[0] === 0) {
		const nursery2 = $dL[1];
		$dM = (($dN) => {
			return (($dO) => {
				return body($dN, $dO);
			})(nursery2);
		})(run);
	} else {
		$dM = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $dM;
}
function run_once(runs2, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body(runs2, ($du, $dv) => {
		return (($dw) => {
			return body($du, $dw, $dv);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs2, tracker, body) {
	let value = run_once(runs2, tracker, ($dp, $dq, $dr) => {
		return body($dp, $dq, $dr);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs2, tracker, ($em, $en, $eo) => {
			return body($em, $en, $eo);
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
	const runs2 = new4();
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
			if (!(eq5(now, last2.v))) {
				last2.v = now;
				wake(subscriber);
			}
			return;
		}, true), site));
		also_retiring(relayed, subscriber);
		return relayed;
	}, upstream[2] ];
}
function set3(self, value, $ai, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $ai);
}
function take(self, item2, $eJ) {
	defer(self, () => {
		dispose(item2, $eJ);
		return;
	});
	return __clone(item2);
}
function cell(self, $dn, $do, caller) {
	const instance = start2(self);
	const pull = instance[0];
	const cached = new8(pull(), caller);
	const refresh = subscriber_of(() => {
		set3(cached, pull(), $dn, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed2 = instance[1](refresh);
	also_releasing(refreshed2, instance[2]);
	register_with_owner(refreshed2, $dn, $do);
	return cached;
}
function memo(self, $dl, $dm, caller) {
	return [ cell(self, $dl, $dm, caller) ];
}
function view(self, slot5, $cK, $cL) {
	const ledger = __clone(self[2]);
	const key = self[1];
	return memo(distinct(derive(($cM, $cN, $cO) => {
		const $da = track(slot5, $cN);
		let $db = null;
		if ($da[0] === 0) {
			const item2 = $da[1];
			const id = key(item2);
			track2(row(ledger, id)[0], $cN);
			$db = over3(ledger, id, [ 0, item2 ]);
		} else {
			$db = [ 1 ];
		}
		return $db;
	}, "std/src/reactive/lens/loaded.vl:507:3"), "std/src/reactive/lens/loaded.vl:516:6"), $cK, $cL, "std/src/reactive/lens/loaded.vl:516:17");
}
function observe(signal, observer) {
	const cell4 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $eP = [ 0, cell4 ];
		let $eQ = null;
		if ($eP[0] === 0) {
			const live = $eP[1];
			$eQ = observer(live.v);
		} else {
			$eQ = undefined;
		}
		return $eQ;
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
function attach_observer3(self, observer, immediately) {
	const subscription = on_settle2(self, mint_subscriber(() => {
		return observer(get4(self));
	}));
	if (immediately) {
		observer(get4(self));
	}
	return subscription;
}
function on_change2(self, observer, caller) {
	return place_subscription(attach_observer3(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function length(self) {
	return [ __clone(self) ];
}
function length2(self) {
	return length(self[0]);
}
function get5(self) {
	return peek(self[0], (held3) => {
		return count(held3);
	});
}
function sized_cell(self) {
	const watch = self[0].v[3];
	const $eV = watch.v[3];
	const cell4 = __clone($eV[1]);
	if ($eV[0] === 0) {
		return cell4;
	}
	const cell5 = new8(0, "std/src/reactive/lens.vl:1019:26");
	watch.v[3] = [ 0, __clone(cell5) ];
	return cell5;
}
function on_settle3(self, subscriber) {
	return on_settle(sized_cell(self[0]), subscriber);
}
function attach_observer4(self, observer, immediately) {
	const subscription = on_settle3(self, mint_subscriber(() => {
		return observer(get5(self));
	}));
	if (immediately) {
		observer(get5(self));
	}
	return subscription;
}
function on_change3(self, observer, caller) {
	return place_subscription(attach_observer4(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function map(self, fn) {
	const $eW = self;
	let $eX = null;
	if ($eW[0] === 0) {
		const x = $eW[1];
		$eX = [ 0, fn(x) ];
	} else {
		$eX = [ 1 ];
	}
	return $eX;
}
function key_of(self, slot5) {
	const key = self[1];
	return map(get4(slot5), (item2) => {
		return key(item2);
	});
}
function is_pending_key(self, key) {
	return [ __clone(row(self[2], key)[1]) ];
}
function is_pending(self, slot5) {
	const $eY = key_of(self, slot5);
	let $eZ = null;
	if ($eY[0] === 0) {
		const key = $eY[1];
		$eZ = is_pending_key(self, key);
	} else {
		$eZ = [ new8(false, "std/src/reactive/lens/loaded.vl:609:42") ];
	}
	return $eZ;
}
function attach_observer5(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get2(self));
	}
	return subscription;
}
function attach_observer6(self, observer, immediately) {
	return attach_observer5(self[0], observer, immediately);
}
function on_change4(self, observer, caller) {
	return place_subscription(attach_observer6(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function open2(self, key, value, insert3, above, below, $fg) {
	const id = self[6].v + 1;
	self[6].v = id;
	const ledger = __clone(self);
	batch(($fh) => {
		const cells = row(ledger, key);
		const found = get(ledger[0].v, key);
		const $fi = found;
		let $fj = null;
		if ($fi[0] === 0) {
			const held3 = $fi[1];
			$fj = held3;
		} else {
			ledger[1].v.push(__clone(key));
			set3(cells[1], true, [ 0, $fh ], "std/src/reactive/lens/loaded.vl:176:20");
			$fj = [ [  ], 0, __clone(above), __clone(below) ];
		}
		let entry = $fj;
		if (insert3) {
			entry[2] = __clone(above);
			entry[3] = __clone(below);
		}
		entry[0].push([ id, __clone(value), insert3 ]);
		insert(ledger[0].v, key, entry);
		insert(ledger[2].v, id, key);
		set3(ledger[4], get2(ledger[4]) + 1, [ 0, $fh ], "std/src/reactive/lens/loaded.vl:187:17");
		notify2(cells[0], [ 0, $fh ]);
		let $fq = undefined;
		if (insert3 || (($fq = value), $fq[0] === 1)) {
			notify2(ledger[5], [ 0, $fh ]);
		}
		return;
	}, $fg);
	return id;
}
function propose(self, slot5, value, $fe) {
	const $ff = key_of(self, slot5);
	if ($ff[0] === 0) {

	} else {
		return [ 1 ];
	}
	return [ 0, open2(self[2], $ff[1], [ 0, __clone(value) ], false, [ 1 ], [ 1 ], $fe) ];
}
function unwrap(self, caller) {
	const $fr = self;
	let $fs = null;
	if ($fr[0] === 0) {
		const x = $fr[1];
		$fs = x;
	} else {
		$fs = (() => {
			throw __panic("expected Some but got None", caller);
		})();
	}
	return $fs;
}
function pending(self) {
	return [ __clone(self[2][4]) ];
}
function set_edge(cell4, next, $fx) {
	const $fy = [ get2(cell4), __clone(next) ];
	let $fz = null;
	if ($fy[0][0] === 0 && $fy[1][0] === 0) {
		$fz = true;
	} else if ($fy[0][0] === 1 && $fy[1][0] === 1) {
		$fz = true;
	} else if ($fy[0][0] === 2 && $fy[1][0] === 2) {
		$fz = true;
	} else {
		$fz = false;
	}
	const same = $fz;
	if (!(same)) {
		set(cell4, next, $fx, "std/src/reactive/lens/loaded.vl:729:8");
	}
}
function request(self, older3, $fu) {
	let $fv = null;
	if (older3) {
		$fv = self[4];
	} else {
		$fv = self[5];
	}
	const edge2 = __clone($fv);
	const $fw = get2(edge2);
	if ($fw[0] === 2) {
		return [ 1 ];
	}
	const token = claim(self);
	if (older3) {
		self[6].v[2] = token;
	} else {
		self[6].v[3] = token;
	}
	set_edge(edge2, [ 1 ], $fu);
	return [ 0, token ];
}
function request_older(self, $ft) {
	return request(self, true, $ft);
}
function older(self) {
	return [ __clone(self[4]) ];
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
	const $fN = self;
	let $fO = null;
	if ($fN[0] === 0) {
		const x = $fN[1];
		$fO = fn(x);
	} else {
		$fO = [ 1 ];
	}
	return $fO;
}
function peek2(self, read) {
	return peek(self[0], read);
}
function replace2(self, at3, value, $fR) {
	return replace(self[0], at3, value, $fR);
}
function splice3(self, at3, removed, inserted, $aZ) {
	const held3 = size(self);
	let $fU = null;
	if (at3 > held3) {
		$fU = held3;
	} else {
		$fU = at3;
	}
	const start7 = $fU;
	let $fV = null;
	if (start7 + removed > held3) {
		$fV = held3 - start7;
	} else {
		$fV = removed;
	}
	const taking = $fV;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start7, taking, arriving ];
	coming_one(self, change2);
	mutate(self, (held4) => {
		return splice(held4, start7, taking, inserted);
	});
	landed_one2(self, change2, $aZ);
}
function splice4(self, at3, removed, inserted, $fT) {
	splice3(self[0], at3, removed, inserted, $fT);
}
function push(self, value, $fS) {
	return splice4(self, size2(self), 0, [ __clone(value) ], $fS);
}
function receive(self, item2, $fM) {
	const id = self[2](item2);
	const $fP = and_then(held2(self[1], id), (slot5) => {
		return at(slot5);
	});
	let $fQ = null;
	if ($fP[0] === 0) {
		const at3 = $fP[1];
		if (!(eq5(peek2(self[0], (list) => {
			return __list_get(list, at3);
		}), [ 0, __clone(item2) ]))) {
			replace2(self[0], at3, item2, $fM);
		}
		$fQ = undefined;
	} else {
		$fQ = push(self[0], item2, $fM);
	}
	return $fQ;
}
function settle_page(self, older3, token, reply, $fD) {
	let $fE = null;
	if (older3) {
		$fE = self[6].v[2];
	} else {
		$fE = self[6].v[3];
	}
	const mine2 = $fE;
	if (token === 0 || mine2 !== token) {
		return false;
	}
	if (older3) {
		self[6].v[2] = 0;
	} else {
		self[6].v[3] = 0;
	}
	let $fF = null;
	if (older3) {
		$fF = self[4];
	} else {
		$fF = self[5];
	}
	const edge2 = __clone($fF);
	const feed8 = __clone(self);
	const $fG = reply;
	let $fH = null;
	if ($fG[0] === 0) {
		const page3 = $fG[1];
		$fH = batch(($fI) => {
			const key = feed8[2];
			let taken = new10();
			let fresh2 = [  ];
			for (const item2 of page3[0]) {
				const id = key(item2);
				const $fL = held2(feed8[1], id);
				if ($fL[0] === 0) {
					receive(feed8, item2, [ 0, $fI ]);
				} else if (!(contains_key(taken, id))) {
					insert(taken, id, true);
					fresh2.push(__clone(item2));
				}
			}
			let $fW = null;
			if (older3) {
				$fW = 0;
			} else {
				$fW = size2(feed8[0]);
			}
			const at3 = $fW;
			splice4(feed8[0], at3, 0, fresh2, [ 0, $fI ]);
			let $fX = null;
			if (page3[1]) {
				$fX = [ 0 ];
			} else {
				$fX = [ 2 ];
			}
			set_edge(edge2, $fX, [ 0, $fI ]);
			return;
		}, $fD);
	} else {
		const error = $fG[1];
		$fH = set(edge2, [ 3, __clone(error) ], $fD, "std/src/reactive/lens/loaded.vl:949:27");
	}
	$fH;
	return true;
}
function prepend_page(self, token, reply, $fC) {
	return settle_page(self, true, token, reply, $fC);
}
function all2(self) {
	return all(self[0]);
}
function eq6(self, b) {
	const $fY = self;
	let $gb = null;
	if ($fY[0] === 0) {
		const $fZ = b;
		let $ga = null;
		if ($fZ[0] === 0) {
			$ga = eq2($fY[1], $fZ[1]);
		} else {
			$ga = false;
		}
		$gb = $ga;
	} else {
		const $gc = b;
		$gb = $gc[0] === 1;
	}
	return $gb;
}
function handles(self, ids) {
	let out = [  ];
	for (const id of ids) {
		out.push(slot_of(self[0], id));
	}
	return out;
}
function get6(self) {
	return handles(self, self[1].v);
}
function keys(self) {
	return __clone(self[1].v);
}
function anchors(self, key) {
	const $gl = get(self[0].v, key);
	let $gm = null;
	if ($gl[0] === 0) {
		const entry = $gl[1];
		$gm = [ __clone(entry[2]), __clone(entry[3]) ];
	} else {
		$gm = [ [ 1 ], [ 1 ] ];
	}
	return $gm;
}
function position_of(rows5, key) {
	let at3 = 0;
	for (const row6 of rows5) {
		if (row6[0] === key) {
			return [ 0, at3 ];
		}
		at3 = at3 + 1;
	}
	return [ 1 ];
}
function landing_among(rows5, above, below) {
	const $go = above;
	let $gp = undefined;
	if ($go[0] === 0 && (($gp = position_of(rows5, $go[1])), $gp[0] === 0)) {
		return $gp[1] + 1;
	}
	const $gq = below;
	let $gr = undefined;
	if ($gq[0] === 0 && (($gr = position_of(rows5, $gq[1])), $gr[0] === 0)) {
		return $gr[1];
	}
	const $gs = above;
	if ($gs[0] === 0) {
		return rows5.length;
	}
	return 0;
}
function hides(self, key) {
	const $gt = showing(self, key);
	if ($gt[0] === 0) {
		const $gu = $gt[1][1];
		return $gu[0] === 1;
	}
	return false;
}
function compose(self, with_hidden) {
	const key = self[1];
	let all3 = [  ];
	for (const slot5 of get6(self[4])) {
		const $gi = get4(slot5);
		if ($gi[0] === 0) {
			all3.push([ key($gi[1]), [ 0, __clone(slot5) ] ]);
		}
	}
	for (const row_key of keys(self[2])) {
		let $gj = undefined;
		let $gk = undefined;
		if (provisional(self[2], row_key) && (($gj = find(self[3], row_key)), $gj[0] === 1) && (($gk = over3(self[2], row_key, [ 1 ])), $gk[0] === 0)) {
			const $gn = anchors(self[2], row_key);
			const above = $gn[0];
			const below = $gn[1];
			const at3 = landing_among(all3, above, below);
			__insert_at(all3, at3, [ __clone(row_key), [ 1 ] ], "std/src/reactive/lens/loaded.vl:588:9");
		}
	}
	if (with_hidden) {
		return all3;
	}
	let out = [  ];
	for (const row6 of all3) {
		if (!(hides(self[2], row6[0]))) {
			out.push(__clone(row6));
		}
	}
	return out;
}
function landing(self, key) {
	let at3 = 0;
	for (const row6 of compose(self, true)) {
		if (row6[0] === key) {
			return at3;
		}
		const $gv = row6[1];
		if ($gv[0] === 0) {
			at3 = at3 + 1;
		}
	}
	return at3;
}
function close2(self, id, accepted, $gw) {
	const $gx = get(self[2].v, id);
	let $gy = null;
	if ($gx[0] === 0) {
		const held3 = $gx[1];
		$gy = held3;
	} else {
		return [ 1 ];
	}
	const key = $gy;
	const found = get(self[0].v, key);
	const $gz = found;
	if ($gz[0] === 0) {

	} else {
		return [ 1 ];
	}
	let entry = __clone($gz[1]);
	let ended = [ 1 ];
	let kept = [  ];
	for (const proposal of entry[0]) {
		if (proposal[0] === id) {
			ended = [ 0, __clone(proposal) ];
		} else {
			kept.push(__clone(proposal));
		}
	}
	const $gA = ended;
	if ($gA[0] === 0) {

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
	batch(($gB) => {
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
			set3(cells[1], false, [ 0, $gB ], "std/src/reactive/lens/loaded.vl:235:19");
		} else {
			insert(ledger[0].v, key, entry);
		}
		set3(ledger[4], get2(ledger[4]) - 1, [ 0, $gB ], "std/src/reactive/lens/loaded.vl:239:17");
		notify2(cells[0], [ 0, $gB ]);
		let $gC = undefined;
		if ($gA[1][2] || (($gC = $gA[1][1]), $gC[0] === 1)) {
			notify2(ledger[5], [ 0, $gB ]);
		}
		return;
	}, $gw);
	return [ 0, [ __clone(key), __clone($gA[1][1]), $gA[1][2], lands ] ];
}
function remove_at2(self, at3, $aY) {
	return splice4(self, at3, 1, [  ], $aY);
}
function insert_at2(self, at3, value, $cH) {
	return splice4(self, at3, 0, [ __clone(value) ], $cH);
}
function answer(self, id, accepted, reply, $ge) {
	const overlay3 = __clone(self);
	let answered = false;
	batch(($gf) => {
		const $gg = get(overlay3[2][2].v, id);
		let $gh = null;
		if ($gg[0] === 0) {
			const key = $gg[1];
			$gh = landing(overlay3, key);
		} else {
			$gh = 0;
		}
		const landing2 = $gh;
		const $gD = close2(overlay3[2], id, accepted, [ 0, $gf ]);
		if ($gD[0] === 0) {

		} else {
			return;
		}
		answered = true;
		if (!($gD[1][3])) {
			return;
		}
		const $gE = reply;
		let $gF = null;
		if ($gE[0] === 0) {
			const item2 = $gE[1];
			$gF = [ 0, __clone(item2) ];
		} else {
			$gF = $gD[1][1];
		}
		const landed4 = __clone($gF);
		const lens2 = __clone(overlay3[0]);
		const $gG = landed4;
		let $gH = null;
		if ($gG[0] === 1) {
			const $gI = held2(overlay3, $gD[1][0]);
			let $gJ = undefined;
			if ($gI[0] === 0 && (($gJ = at($gI[1])), $gJ[0] === 0)) {
				remove_at2(lens2, $gJ[1], [ 0, $gf ]);
			}
			$gH = undefined;
		} else {
			const item3 = $gG[1];
			const server_key = overlay3[1](item3);
			const $gK = held2(overlay3, $gD[1][0]);
			let $gL = null;
			if ($gK[0] === 0) {
				$gL = at($gK[1]);
			} else {
				$gL = and_then(held2(overlay3, server_key), (slot5) => {
					return at(slot5);
				});
			}
			const target = $gL;
			const $gM = target;
			let $gN = null;
			if ($gM[0] === 0) {
				const at3 = $gM[1];
				if (!(eq5(peek2(lens2, (list) => {
					return __list_get(list, at3);
				}), [ 0, __clone(item3) ]))) {
					replace2(lens2, at3, item3, [ 0, $gf ]);
				}
				$gN = undefined;
			} else {
				if ($gD[1][2]) {
					insert_at2(lens2, landing2, item3, [ 0, $gf ]);
				}
				$gN = undefined;
			}
			$gH = $gN;
		}
		return $gH;
	}, $ge);
	return answered;
}
function confirm(self, id, $gd) {
	return answer(self, id, true, [ 1 ], $gd);
}
function resync(self, list, $gO) {
	const token = load(self, $gO);
	settle(self, token, [ 0, __clone(list) ], $gO);
}
function identity3(self) {
	return [ 1 ];
}
function shape_cell(self) {
	const watch = self[0].v[3];
	const $he = watch.v[2];
	const cell4 = __clone($he[1]);
	if ($he[0] === 0) {
		return cell4;
	}
	const cell5 = new8(0, "std/src/reactive/lens.vl:1031:26");
	watch.v[2] = [ 0, __clone(cell5) ];
	return cell5;
}
function on_settle4(self, subscriber) {
	return on_settle(shape_cell(self[0]), subscriber);
}
function track3(self, $cP) {
	const scope = $cP;
	const identity5 = identity3(self);
	if (admits(scope, identity5)) {
		lists_of(scope[0]).v[0].push([ identity5, (subscriber) => {
			return on_settle4(self, subscriber);
		} ]);
	}
	return get6(self);
}
function run_body2(runs2, body) {
	const run = renew(runs2);
	const $hf = nursery(run);
	let $hg = null;
	if ($hf[0] === 0) {
		const nursery2 = $hf[1];
		$hg = (($dN) => {
			return (($dO) => {
				return body($dN, $dO);
			})(nursery2);
		})(run);
	} else {
		$hg = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $hg;
}
function run_once2(runs2, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body2(runs2, ($du, $dv) => {
		return (($dw) => {
			return body($du, $dw, $dv);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked2(runs2, tracker, body) {
	let value = run_once2(runs2, tracker, ($dp, $dq, $dr) => {
		return body($dp, $dq, $dr);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once2(runs2, tracker, ($em, $en, $eo) => {
			return body($em, $en, $eo);
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
	const runs2 = new4();
	const tracker = new_tracker([ 0, __clone(self[1]) ]);
	return [ () => {
		return run_tracked2(runs2, tracker, body);
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
function eq7(self, other) {
	return self[1] === other[1];
}
function eq8(self, b) {
	const $hh = self;
	let $hk = null;
	if ($hh[0] === 0) {
		const $hi = b;
		let $hj = null;
		if ($hi[0] === 0) {
			$hj = eq7($hh[1], $hi[1]);
		} else {
			$hj = false;
		}
		$hk = $hj;
	} else {
		const $hl = b;
		$hk = $hl[0] === 1;
	}
	return $hk;
}
function eq9(self, other) {
	return self[0] === other[0] && eq8(self[1], other[1]);
}
function eq10(self, b) {
	if (self.length !== b.length) {
		return false;
	}
	let index2 = 0;
	for (const item2 of self) {
		if (!(eq9(item2, __at(b, index2, "std/src/compare.vl:147:17")))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
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
			if (!(eq10(now, last2.v))) {
				last2.v = now;
				wake(subscriber);
			}
			return;
		}, true), site));
		also_retiring(relayed, subscriber);
		return relayed;
	}, upstream[2] ];
}
function cell2(self, $dn, $do, caller) {
	const instance = start4(self);
	const pull = instance[0];
	const cached = new8(pull(), caller);
	const refresh = subscriber_of(() => {
		set3(cached, pull(), $dn, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed2 = instance[1](refresh);
	also_releasing(refreshed2, instance[2]);
	register_with_owner(refreshed2, $dn, $do);
	return cached;
}
function memo2(self, $dl, $dm, caller) {
	return [ cell2(self, $dl, $dm, caller) ];
}
function rows3(self, $gZ, $ha) {
	const overlay3 = __clone(self);
	const feed8 = __clone(self[4]);
	const layout = __clone(self[2][5]);
	return memo2(distinct(derive(($hb, $hc, $hd) => {
		track3(feed8, $hc);
		track2(layout, $hc);
		return compose(overlay3, false);
	}, "std/src/reactive/lens/loaded.vl:544:3"), "std/src/reactive/lens/loaded.vl:548:6"), $gZ, $ha, "std/src/reactive/lens/loaded.vl:548:17");
}
function attach_observer7(self, observer, immediately) {
	return attach_observer5(self[0], observer, immediately);
}
function on_change5(self, observer, caller) {
	return place_subscription(attach_observer7(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function propose_delete(self, slot5, $hw) {
	const $hx = key_of(self, slot5);
	if ($hx[0] === 0) {

	} else {
		return [ 1 ];
	}
	return [ 0, open2(self[2], $hx[1], [ 1 ], false, [ 1 ], [ 1 ], $hw) ];
}
function shown2(self) {
	let out = [  ];
	for (const row6 of compose(self, false)) {
		const $hy = row6[1];
		let $hz = null;
		if ($hy[0] === 0) {
			const slot5 = $hy[1];
			$hz = get4(slot5);
		} else {
			$hz = [ 1 ];
		}
		const held3 = $hz;
		const $hA = over3(self[2], row6[0], held3);
		if ($hA[0] === 0) {
			out.push($hA[1]);
		}
	}
	return out;
}
function reject(self, id, $hB) {
	return answer(self, id, false, [ 1 ], $hB);
}
function shown_rows(self) {
	return compose(self, false);
}
function request_newer(self, $hL) {
	return request(self, false, $hL);
}
function append_page(self, token, reply, $hM) {
	return settle_page(self, false, token, reply, $hM);
}
function newer(self) {
	return [ __clone(self[5]) ];
}
function get7(self, key) {
	return __nmap_get(self[0], hash(key));
}
function find2(self, key) {
	return get7(self, key);
}
function insert2(self, key, value) {
	__nmap_insert(self[0], hash(key), __clone(key), __clone(value));
}
function store(self, key, value) {
	insert2(self, key, value);
}
function lens_over2(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new8(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over4(value, handler, access) {
	return lens_over2([ 0, __shared_new(value) ], handler, access);
}
function keyed(value) {
	return over4(value, new7(), [ (held3, key) => {
		const $hO = find2(held3, key);
		return $hO[0] === 0;
	}, (held3, key) => {
		return find2(held3, key);
	}, (held3, key, read) => {
		const $hP = find2(held3, key);
		const item2 = [ $hP[1] ];
		if ($hP[0] === 0) {
			read([ item2, 0 ]);
		}
		return;
	}, (held3, key, value2) => {
		return store(held3, key, value2);
	}, (held3, key, with2) => {
		const $hQ = find2(held3, key);
		if ($hQ[0] === 0) {

		} else {
			return;
		}
		let item2 = [ $hQ[1] ];
		with2([ item2, 0 ]);
		store(held3, key, item2[0]);
		return;
	} ]);
}
function over5(value) {
	return [ keyed(value) ];
}
function beside2(lens2) {
	const ledger = new9();
	return [ __clone(lens2), ledger ];
}
function retire2(self) {
	self[3].v = new10();
}
function new13($hN) {
	const empty = new10();
	const lens2 = over5(empty);
	const records = [ __clone(lens2), beside2(lens2), __shared_new(new2()), __shared_new(new2()), __shared_new(0) ];
	defer_to_owner(() => {
		return retire2(records);
	}, $hN);
	return records;
}
function overlay2(self) {
	return __clone(self[1]);
}
function follows_items2(self) {
	return false;
}
function follow4(self, at3) {
	const $hY = get7(self[0], at3);
	if ($hY[0] === 0) {
		return $hY[1];
	}
	const slot5 = as_i32(self[1].length);
	self[1].push(__clone(at3));
	insert2(self[0], at3, slot5);
	return slot5;
}
function follow_id2(storage, watch, holds2, at3) {
	if (viewing2(watch, (held3) => {
		return follows_items2(held3[0]);
	}) && !(peek_storage2(storage, (held3) => {
		return holds2(held3, at3);
	}))) {
		return -(1);
	}
	const id = follow4(watch.v[0], at3);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	const $hZ = as_usize(id);
	__at(watch.v[1], $hZ, "std/src/reactive/lens.vl:1058:2")[2] = __at(watch.v[1], $hZ, "std/src/reactive/lens.vl:1058:2")[2] + 1;
	return id;
}
function follow5(self, at3) {
	const core = self[0];
	return slot_of(self, follow_id2(core.v[0], core.v[3], core.v[1][0], at3));
}
function remove2(self, key) {
	__nmap_remove(self[0], hash(key));
}
function release6(self, slot5) {
	const $ic = __list_get(self[1], as_usize(slot5));
	if ($ic[0] === 0) {
		remove2(self[0], $ic[1]);
	}
}
function release_id2(watch, id) {
	if (id < 0) {
		return;
	}
	const entry = as_usize(id);
	const left = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1068:13")[2] - 1;
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1069:2")[2] = left;
	if (left <= 0) {
		release6(watch.v[0], id);
	}
}
function release7(self) {
	release_id2(self[0][0].v[3], self[1]);
}
function slot3(self, at3, $cJ) {
	const handle = follow5(self, at3);
	defer_to_owner(() => {
		return release7(handle);
	}, $cJ);
	return handle;
}
function slot4(self, key, $hT) {
	return slot3(self[0], key, $hT);
}
function row2(self, key) {
	const $ig = get7(self[3].v, key);
	if ($ig[0] === 0) {
		return $ig[1];
	}
	const made = [ new8(0, "std/src/reactive/lens/loaded.vl:141:44"), new8(false, "std/src/reactive/lens/loaded.vl:141:74") ];
	insert2(self[3].v, key, made);
	return made;
}
function identity4(self) {
	return identity(slot_cell(self[0][0].v[3], self[1]));
}
function on_settle5(self, subscriber) {
	return on_settle(slot_cell(self[0][0].v[3], self[1]), subscriber);
}
function locate3(self, slot5) {
	return __list_get(self[1], as_usize(slot5));
}
function at2(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][0].v[3], (held3) => {
		return locate3(held3[0], self[1]);
	});
}
function get8(self) {
	const $ii = at2(self);
	if ($ii[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held3) => {
		return read(held3, $ii[1]);
	});
}
function track4(self, $cP) {
	const scope = $cP;
	const identity5 = identity4(self);
	if (admits(scope, identity5)) {
		lists_of(scope[0]).v[0].push([ identity5, (subscriber) => {
			return on_settle5(self, subscriber);
		} ]);
	}
	return get8(self);
}
function showing2(self, key) {
	const $in = get7(self[0].v, key);
	if ($in[0] === 0) {

	} else {
		return [ 1 ];
	}
	let out = [ 1 ];
	for (const proposal of $in[1][0]) {
		if (proposal[0] > $in[1][1]) {
			out = [ 0, __clone(proposal) ];
		}
	}
	return out;
}
function provisional2(self, key) {
	const $it = get7(self[0].v, key);
	if ($it[0] === 0) {

	} else {
		return false;
	}
	let out = false;
	for (const proposal of $it[1][0]) {
		if (proposal[2]) {
			out = true;
		}
	}
	return out;
}
function over6(self, key, held3) {
	const $io = showing2(self, key);
	let $ip = null;
	if ($io[0] === 1) {
		$ip = __clone(held3);
	} else {
		const proposal = $io[1];
		const $iq = proposal[1];
		let $ir = null;
		if ($iq[0] === 1) {
			$ir = [ 1 ];
		} else {
			const value = $iq[1];
			const $is = held3;
			let $iu = null;
			if ($is[0] === 0 || provisional2(self, key)) {
				$iu = [ 0, __clone(value) ];
			} else {
				$iu = [ 1 ];
			}
			$ir = $iu;
		}
		$ip = $ir;
	}
	return $ip;
}
function run_once3(runs2, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body2(runs2, ($du, $dv) => {
		return (($dw) => {
			return body($du, $dw, $dv);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked3(runs2, tracker, body) {
	let value = run_once3(runs2, tracker, ($dp, $dq, $dr) => {
		return body($dp, $dq, $dr);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once3(runs2, tracker, ($em, $en, $eo) => {
			return body($em, $en, $eo);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function start5(self) {
	const body = self[0];
	const runs2 = new4();
	const tracker = new_tracker([ 0, __clone(self[1]) ]);
	return [ () => {
		return run_tracked3(runs2, tracker, body);
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
function eq11(self, b) {
	const $ix = self;
	let $iA = null;
	if ($ix[0] === 0) {
		const $iy = b;
		let $iz = null;
		if ($iy[0] === 0) {
			$iz = $ix[1] === $iy[1];
		} else {
			$iz = false;
		}
		$iA = $iz;
	} else {
		const $iB = b;
		$iA = $iB[0] === 1;
	}
	return $iA;
}
function start6(self) {
	const site = __clone(self[1]);
	const upstream = start5(self[0]);
	const pull = upstream[0];
	const last2 = __shared_new(pull());
	return [ () => {
		return __clone(last2.v);
	}, (subscriber) => {
		const relayed = upstream[1](placed_at(subscriber_of(() => {
			const now = pull();
			if (!(eq11(now, last2.v))) {
				last2.v = now;
				wake(subscriber);
			}
			return;
		}, true), site));
		also_retiring(relayed, subscriber);
		return relayed;
	}, upstream[2] ];
}
function cell3(self, $dn, $do, caller) {
	const instance = start6(self);
	const pull = instance[0];
	const cached = new8(pull(), caller);
	const refresh = subscriber_of(() => {
		set3(cached, pull(), $dn, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed2 = instance[1](refresh);
	also_releasing(refreshed2, instance[2]);
	register_with_owner(refreshed2, $dn, $do);
	return cached;
}
function memo3(self, $dl, $dm, caller) {
	return [ cell3(self, $dl, $dm, caller) ];
}
function view2(self, key, $hR, $hS) {
	const ledger = __clone(self[1]);
	const slot5 = slot4(self[0], key, $hS);
	return memo3(distinct(derive(($id, $ie, $if) => {
		track2(row2(ledger, key)[0], $ie);
		return over6(ledger, key, track4(slot5, $ie));
	}, "std/src/reactive/lens/loaded.vl:1150:3"), "std/src/reactive/lens/loaded.vl:1153:6"), $hR, $hS, "std/src/reactive/lens/loaded.vl:1153:17");
}
function on_change6(self, observer, caller) {
	return place_subscription(attach_observer7(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function phase_cell(self, key) {
	const $iN = get7(self[2].v, key);
	if ($iN[0] === 0) {
		return $iN[1];
	}
	const made = new8([ 0 ], "std/src/reactive/lens/loaded.vl:1214:62");
	insert2(self[2].v, key, made);
	return made;
}
function load2(self, key, $iM) {
	const token = self[4].v + 1;
	self[4].v = token;
	insert2(self[3].v, key, token);
	const phase3 = phase_cell(self, key);
	set_phase(phase3, refreshed(get2(phase3)), $iM);
	return token;
}
function phase2(self, key) {
	return [ phase_cell(self, key) ];
}
function peek3(self, read) {
	return peek(self[0], read);
}
function apply3(self, change2, wakes) {
	const $jd = change2;
	let $je = null;
	if ($jd[0] === 0) {
		const key = $jd[1];
		const _removed = $jd[2];
		const _inserted = $jd[3];
		const $jf = get7(self[0], key);
		if ($jf[0] === 0) {
			wakes.push([ 0, $jf[1] ]);
		}
		$je = undefined;
	} else if ($jd[0] === 1) {
		const key2 = $jd[1];
		const _count = $jd[2];
		const $jg = get7(self[0], key2);
		if ($jg[0] === 0) {
			wakes.push([ 0, $jg[1] ]);
		}
		$je = undefined;
	} else if ($jd[0] === 2) {
		const key3 = $jd[1];
		const $jh = get7(self[0], key3);
		if ($jh[0] === 0) {
			wakes.push([ 0, $jh[1] ]);
		}
		$je = undefined;
	} else if ($jd[0] === 3) {
		const key4 = $jd[1];
		const _count2 = $jd[2];
		const $ji = get7(self[0], key4);
		if ($ji[0] === 0) {
			wakes.push([ 0, $ji[1] ]);
		}
		$je = undefined;
	} else if ($jd[0] === 4) {
		const key5 = $jd[1];
		const part = $jd[2];
		const $jj = get7(self[0], key5);
		if ($jj[0] === 0) {
			wakes.push([ 1, $jj[1], part ]);
		}
		$je = undefined;
	} else if ($jd[0] === 5) {
		const _from = $jd[1];
		const _to = $jd[2];
		$je = undefined;
	} else {
		let slot5 = 0;
		const held3 = as_i32(self[1].length);
		while (slot5 < held3) {
			wakes.push([ 0, slot5 ]);
			slot5 = slot5 + 1;
		}
		$je = undefined;
	}
	return $je;
}
function landed3(self, changes, late, $bj) {
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
		const $jb = change2;
		let $jc = null;
		if ($jb[0] === 0) {
			const _at = $jb[1];
			const removed = $jb[2];
			const inserted = $jb[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$jc = undefined;
		} else if ($jb[0] === 1) {
			const _at2 = $jb[1];
			const _count = $jb[2];
			$jc = resized = true;
		} else if ($jb[0] === 3) {
			const _at3 = $jb[1];
			const _count2 = $jb[2];
			$jc = resized = true;
		} else if ($jb[0] === 6) {
			$jc = resized = true;
		} else if ($jb[0] === 5) {
			const _from = $jb[1];
			const _to = $jb[2];
			$jc = shaped = true;
		} else {
			$jc = undefined;
		}
		$jc;
		if (watch.v[1].length > 0) {
			apply3(watch.v[0], change2, wakes);
		}
		let $jl = null;
		if (self[0].v[4].length > 0) {
			let $jk = null;
			if (!(late)) {
				$jk = [ 0 ];
			} else if (!(several)) {
				$jk = [ 1 ];
			} else if (index2 === 0) {
				$jk = [ 2 ];
			} else {
				$jk = [ 3 ];
			}
			const stage = $jk;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$jl = undefined;
		}
		$jl;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $jm = null;
	if (shaped || resized) {
		$jm = watch.v[2];
	} else {
		$jm = [ 1 ];
	}
	const shape = __clone($jm);
	let $jn = null;
	if (resized) {
		$jn = watch.v[3];
	} else {
		$jn = [ 1 ];
	}
	const sized = __clone($jn);
	let $jo = undefined;
	let $jp = undefined;
	if (wakes.length === 0 && (($jo = shape), $jo[0] === 1) && (($jp = sized), $jp[0] === 1)) {
		notify2(published, $bj);
		return;
	}
	settle_together(($bM) => {
		for (const wake2 of wakes) {
			const $jq = wake2;
			let $jr = null;
			if ($jq[0] === 0) {
				const slot5 = $jq[1];
				$jr = wake_all(watch, slot5, [ 0, $bM ]);
			} else if ($jq[0] === 2) {
				const slot6 = $jq[1];
				$jr = wake_all(watch, slot6, [ 0, $bM ]);
			} else {
				const slot7 = $jq[1];
				const part = $jq[2];
				$jr = wake_part(watch, slot7, part, [ 0, $bM ]);
			}
			$jr;
		}
		const $ju = sized;
		if ($ju[0] === 0) {
			notify2($ju[1], [ 0, $bM ]);
		}
		const $jv = shape;
		if ($jv[0] === 0) {
			notify2($jv[1], [ 0, $bM ]);
		}
		notify2(published, [ 0, $bM ]);
		return;
	}, $bj);
}
function landed_one3(self, change2, $bi) {
	landed3(self, [ __clone(change2) ], false, $bi);
}
function put2(self, key, value, $iW) {
	const had = peek(self, (held3) => {
		const $iX = find2(held3, key);
		return $iX[0] === 0;
	});
	let $iY = null;
	if (had) {
		$iY = [ 2, __clone(key) ];
	} else {
		$iY = [ 1, __clone(key), 1 ];
	}
	const change2 = $iY;
	coming_one(self, change2);
	mutate(self, (held3) => {
		return store(held3, key, value);
	});
	landed_one3(self, change2, $iW);
}
function put3(self, key, value, $iV) {
	return put2(self[0], key, value, $iV);
}
function drop(self, key) {
	remove2(self, key);
}
function remove3(self, key, $jx) {
	const had = peek(self, (held3) => {
		const $jy = find2(held3, key);
		return $jy[0] === 0;
	});
	if (!(had)) {
		return;
	}
	coming_one(self, [ 3, __clone(key), 1 ]);
	mutate(self, (held3) => {
		return drop(held3, key);
	});
	landed_one3(self, [ 3, __clone(key), 1 ], $jx);
}
function remove4(self, key, $jw) {
	return remove3(self[0], key, $jw);
}
function receive2(self, key, value, $iS) {
	const held3 = peek3(self[0], (map2) => {
		return get7(map2, key);
	});
	if (eq3(held3, value)) {
		return;
	}
	const $iT = value;
	let $iU = null;
	if ($iT[0] === 0) {
		const record2 = $iT[1];
		$iU = put3(self[0], key, record2, $iS);
	} else {
		$iU = remove4(self[0], key, $iS);
	}
	return $iU;
}
function settle2(self, key, token, reply, $iO) {
	if (!(eq3(get7(self[3].v, key), [ 0, token ]))) {
		return false;
	}
	remove2(self[3].v, key);
	const phase3 = phase_cell(self, key);
	const records = __clone(self);
	const $iP = reply;
	let $iQ = null;
	if ($iP[0] === 0) {
		const found = $iP[1];
		$iQ = batch(($iR) => {
			receive2(records, key, found, [ 0, $iR ]);
			const $jz = found;
			let $jA = null;
			if ($jz[0] === 0) {
				$jA = [ 1, token ];
			} else {
				$jA = [ 4 ];
			}
			set_phase(phase3, $jA, [ 0, $iR ]);
			return;
		}, $iO);
	} else {
		const error = $iP[1];
		$iQ = set_phase(phase3, failed(get2(phase3), error), $iO);
	}
	$iQ;
	return true;
}
function contains_key2(self, key) {
	return __nmap_has(self[0], hash(key));
}
function holds(self, key) {
	return peek3(self[0], (map2) => {
		return contains_key2(map2, key);
	});
}
function open3(self, key, value, insert3, above, below, $fg) {
	const id = self[6].v + 1;
	self[6].v = id;
	const ledger = __clone(self);
	batch(($fh) => {
		const cells = row2(ledger, key);
		const found = get7(ledger[0].v, key);
		const $jC = found;
		let $jD = null;
		if ($jC[0] === 0) {
			const held3 = $jC[1];
			$jD = held3;
		} else {
			ledger[1].v.push(__clone(key));
			set3(cells[1], true, [ 0, $fh ], "std/src/reactive/lens/loaded.vl:176:20");
			$jD = [ [  ], 0, __clone(above), __clone(below) ];
		}
		let entry = $jD;
		if (insert3) {
			entry[2] = __clone(above);
			entry[3] = __clone(below);
		}
		entry[0].push([ id, __clone(value), insert3 ]);
		insert2(ledger[0].v, key, entry);
		insert(ledger[2].v, id, key);
		set3(ledger[4], get2(ledger[4]) + 1, [ 0, $fh ], "std/src/reactive/lens/loaded.vl:187:17");
		notify2(cells[0], [ 0, $fh ]);
		let $jE = undefined;
		if (insert3 || (($jE = value), $jE[0] === 1)) {
			notify2(ledger[5], [ 0, $fh ]);
		}
		return;
	}, $fg);
	return id;
}
function propose2(self, key, value, $jB) {
	return open3(self[1], key, [ 0, __clone(value) ], !(holds(self, key)), [ 1 ], [ 1 ], $jB);
}
function is_pending2(self, key) {
	return [ __clone(row2(self[1], key)[1]) ];
}
function push2(self, key, value, $jF) {
	const phase3 = phase_cell(self, key);
	const records = __clone(self);
	batch(($jG) => {
		receive2(records, key, value, [ 0, $jG ]);
		const $jH = value;
		let $jI = null;
		if ($jH[0] === 0) {
			$jI = [ 1, 0 ];
		} else {
			$jI = [ 4 ];
		}
		set_phase(phase3, $jI, [ 0, $jG ]);
		return;
	}, $jF);
}
function items2(self) {
	return __clone(self[0]);
}
function close3(self, id, accepted, $gw) {
	const $jM = get(self[2].v, id);
	let $jN = null;
	if ($jM[0] === 0) {
		const held3 = $jM[1];
		$jN = held3;
	} else {
		return [ 1 ];
	}
	const key = $jN;
	const found = get7(self[0].v, key);
	const $jO = found;
	if ($jO[0] === 0) {

	} else {
		return [ 1 ];
	}
	let entry = __clone($jO[1]);
	let ended = [ 1 ];
	let kept = [  ];
	for (const proposal of entry[0]) {
		if (proposal[0] === id) {
			ended = [ 0, __clone(proposal) ];
		} else {
			kept.push(__clone(proposal));
		}
	}
	const $jP = ended;
	if ($jP[0] === 0) {

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
	batch(($gB) => {
		const cells = row2(ledger, key);
		remove(ledger[2].v, id);
		if (settled) {
			remove2(ledger[0].v, key);
			let order = [  ];
			for (const held_key of ledger[1].v) {
				if (held_key !== key) {
					order.push(__clone(held_key));
				}
			}
			ledger[1].v = order;
			set3(cells[1], false, [ 0, $gB ], "std/src/reactive/lens/loaded.vl:235:19");
		} else {
			insert2(ledger[0].v, key, entry);
		}
		set3(ledger[4], get2(ledger[4]) - 1, [ 0, $gB ], "std/src/reactive/lens/loaded.vl:239:17");
		notify2(cells[0], [ 0, $gB ]);
		let $jQ = undefined;
		if ($jP[1][2] || (($jQ = $jP[1][1]), $jQ[0] === 1)) {
			notify2(ledger[5], [ 0, $gB ]);
		}
		return;
	}, $gw);
	return [ 0, [ __clone(key), __clone($jP[1][1]), $jP[1][2], lands ] ];
}
function answer2(self, id, accepted, reply, $jK) {
	const overlay3 = __clone(self);
	let answered = false;
	batch(($jL) => {
		const $jR = close3(overlay3[1], id, accepted, [ 0, $jL ]);
		if ($jR[0] === 0) {

		} else {
			return;
		}
		answered = true;
		if (!($jR[1][3])) {
			return;
		}
		const $jS = reply;
		let $jT = null;
		if ($jS[0] === 0) {
			const value = $jS[1];
			$jT = [ 0, __clone(value) ];
		} else {
			$jT = $jR[1][1];
		}
		const landed4 = __clone($jT);
		const held3 = peek3(overlay3[0], (map2) => {
			return get7(map2, $jR[1][0]);
		});
		const $jU = landed4;
		let $jV = null;
		if ($jU[0] === 1) {
			const $jW = held3;
			if ($jW[0] === 0) {
				remove4(overlay3[0], $jR[1][0], [ 0, $jL ]);
			}
			$jV = undefined;
		} else {
			const value2 = $jU[1];
			const $jX = held3;
			let $jY = null;
			if ($jX[0] === 0 || $jR[1][2]) {
				if (!(eq3(held3, [ 0, __clone(value2) ]))) {
					put3(overlay3[0], $jR[1][0], value2, [ 0, $jL ]);
				}
				$jY = undefined;
			}
			$jV = $jY;
		}
		return $jV;
	}, $jK);
	return answered;
}
function reject2(self, id, $jJ) {
	return answer2(self, id, false, [ 1 ], $jJ);
}
function confirm2(self, id, $kb) {
	return answer2(self, id, true, [ 1 ], $kb);
}
function pending2(self) {
	return [ __clone(self[1][4]) ];
}
function attach_observer8(self, observer, immediately) {
	const subscription = on_settle5(self, mint_subscriber(() => {
		return observer(get8(self));
	}));
	if (immediately) {
		observer(get8(self));
	}
	return subscription;
}
function on_change7(self, observer, caller) {
	return place_subscription(attach_observer8(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function propose_delete2(self, key, $ke) {
	return open3(self[1], key, [ 1 ], false, [ 1 ], [ 1 ], $ke);
}
function disconnected(self, error, $kj) {
	retire(self, $kj);
	set_phase(self[3], failed(get2(self[3]), error), $kj);
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
const feed = fresh([ 1 ], [ 1 ]);
assert(said(get3(phase(feed))) === "Pending" && size2(items(feed)) === 0, "1: nothing yet is Pending, not empty", "lens-loaded.vl:70:2");
const first = load(feed, [ 1 ]);
assert(settle(feed, first, [ 0, [ m(10, "j"), m(11, "k"), m(12, "l") ] ], [ 1 ]), "1: page 1 lands", "lens-loaded.vl:75:2");
assert(said(get3(phase(feed))) === "Ready(1)", "1: Ready, with the load\'s token", "lens-loaded.vl:76:2");
const row3 = slot2(items(feed), 1, [ 1 ]);
const view3 = view(overlay(feed), row3, [ 1 ], [ 1 ]);
const view_runs = __shared_new(0);
const slot_runs = __shared_new(0);
const length_runs = __shared_new(0);
const flag_runs = __shared_new(0);
on_change(__clone(view3), (_item, $eN) => {
	const $eO = [ view_runs, "v" ];
	return view_runs.v = $eO[0][$eO[1]] + 1;
}, "lens-loaded.vl:83:17");
on_change2(__clone(row3), (_item, $eR) => {
	const $eS = [ slot_runs, "v" ];
	return slot_runs.v = $eS[0][$eS[1]] + 1;
}, "lens-loaded.vl:84:16");
on_change3(length2(items(feed)), (_count, $eT) => {
	const $eU = [ length_runs, "v" ];
	return length_runs.v = $eU[0][$eU[1]] + 1;
}, "lens-loaded.vl:85:34");
const flag = is_pending(overlay(feed), row3);
on_change4(__clone(flag), (_on, $fa) => {
	const $fb = [ flag_runs, "v" ];
	return flag_runs.v = $fb[0][$fb[1]] + 1;
}, "lens-loaded.vl:87:17");
const edit = unwrap(propose(overlay(feed), row3, m(11, "k, edited"), [ 1 ]), "lens-loaded.vl:88:61");
assert(eq5(get3(view3), [ 0, m(11, "k, edited") ]) && get3(flag) && get3(pending(overlay(feed))) === 1, "1: the edit shows at once through the overlay", "lens-loaded.vl:89:2");
assert(eq5(get4(row3), [ 0, m(11, "k") ]), "1: the lens is not written (R12)", "lens-loaded.vl:93:2");
assert(view_runs.v === 1 && flag_runs.v === 1 && slot_runs.v === 0, "1: the view and its flag woke once; the lens\'s slot not at all", "lens-loaded.vl:94:2");
const page = unwrap(request_older(feed, [ 1 ]), "lens-loaded.vl:98:34");
assert(edge(get3(older(feed))) === "Loading", "1: the older end is loading", "lens-loaded.vl:99:2");
assert(prepend_page(feed, page, [ 0, [ [ m(7, "g"), m(8, "h"), m(9, "i"), m(10, "j") ], true ] ], [ 1 ]), "1: page 2 lands", "lens-loaded.vl:101:2");
assert(eq(held(feed), [ 7, 8, 9, 10, 11, 12 ]), "1: page 2 is spliced in at the top, the overlap once", "lens-loaded.vl:108:2");
assert(eq6(at(row3), [ 0, 4 ]) && eq5(get3(view3), [ 0, m(11, "k, edited") ]), "1: the edited row moved down and kept its edit", "lens-loaded.vl:112:2");
assert(view_runs.v === 1 && slot_runs.v === 0 && length_runs.v === 1, "1: a page above the row did not wake it; the length woke once", "lens-loaded.vl:116:2");
assert(edge(get3(older(feed))) === "Idle", "1: the older end may ask again", "lens-loaded.vl:120:2");
assert(confirm(overlay(feed), edit, [ 1 ]), "1: the confirmation is applied", "lens-loaded.vl:121:2");
assert(eq5(get4(row3), [ 0, m(11, "k, edited") ]) && eq5(get3(view3), [ 0, m(11, "k, edited") ]), "1: confirmed: the lens holds the server\'s word now", "lens-loaded.vl:122:2");
assert(!(get3(flag)) && get3(pending(overlay(feed))) === 0, "1: nothing pending", "lens-loaded.vl:126:2");
assert(view_runs.v === 1 && flag_runs.v === 2 && slot_runs.v === 1, "1: a confirm of the value shown woke the flag and the lens\'s slot, not the view", "lens-loaded.vl:127:2");
const feed2 = fresh([ 1 ], [ 1 ]);
resync(feed2, [ m(1, "a"), m(2, "b"), m(3, "c") ], [ 1 ]);
const one = slot2(items(feed2), 0, [ 1 ]);
const two = slot2(items(feed2), 1, [ 1 ]);
const three = slot2(items(feed2), 2, [ 1 ]);
const one_runs = __shared_new(0);
const three_runs = __shared_new(0);
on_change2(__clone(one), (_item, $gP) => {
	const $gQ = [ one_runs, "v" ];
	return one_runs.v = $gQ[0][$gQ[1]] + 1;
}, "lens-loaded.vl:141:16");
on_change2(__clone(three), (_item, $gR) => {
	const $gS = [ three_runs, "v" ];
	return three_runs.v = $gS[0][$gS[1]] + 1;
}, "lens-loaded.vl:142:18");
const slow = load(feed2, [ 1 ]);
assert(said(get3(phase(feed2))) === "Refreshing(1)", "2: a refresh keeps the rows and says so", "lens-loaded.vl:144:2");
const quick = load(feed2, [ 1 ]);
assert(settle(feed2, quick, [ 0, [ m(1, "a"), m(3, "c2"), m(4, "d") ] ], [ 1 ]), "2: the newer reply lands", "lens-loaded.vl:146:2");
assert(!(settle(feed2, slow, [ 0, [ m(1, "OLD") ] ], [ 1 ])), "2: the superseded reply is dropped", "lens-loaded.vl:147:2");
assert(eq(held(feed2), [ 1, 3, 4 ]) && peek2(items(feed2), (list) => {
	return __at(list, 1, "lens-loaded.vl:149:55")[1];
}) === "c2", "2: the newer reply\'s rows", "lens-loaded.vl:148:2");
assert(said(get3(phase(feed2))) === "Ready(" + quick + ")", "2: Ready with the newer token", "lens-loaded.vl:152:2");
assert(eq6(at(one), [ 0, 0 ]) && eq6(at(three), [ 0, 1 ]) && eq5(get4(three), [ 0, m(3, "c2") ]), "2: kept rows kept their slots", "lens-loaded.vl:153:2");
assert(eq5(get4(two), [ 1 ]), "2: a row that left ends its slot", "lens-loaded.vl:157:2");
assert(one_runs.v === 0 && three_runs.v === 1, "2: an unchanged row woke nothing; a changed one woke once", "lens-loaded.vl:158:2");
const failing = load(feed2, [ 1 ]);
assert(settle(feed2, failing, [ 1, "500" ], [ 1 ]), "3: the failure is applied", "lens-loaded.vl:166:2");
assert(said(get3(phase(feed2))) === "Failed(500, " + quick + ")", "3: Failed, naming the load whose rows are held", "lens-loaded.vl:167:2");
assert(eq(held(feed2), [ 1, 3, 4 ]) && eq5(get4(three), [ 0, m(3, "c2") ]), "3: the rows stay", "lens-loaded.vl:171:2");
const retry = load(feed2, [ 1 ]);
assert(said(get3(phase(feed2))) === "Refreshing(" + quick + ")", "3: a retry refreshes from the stale rows", "lens-loaded.vl:173:2");
assert(settle(feed2, retry, [ 0, [ m(1, "a"), m(3, "c3"), m(4, "d") ] ], [ 1 ]), "3: the retry lands", "lens-loaded.vl:177:2");
assert(said(get3(phase(feed2))) === "Ready(" + retry + ")" && eq5(get4(three), [ 0, m(3, "c3") ]), "3: Ready again, the slot survived", "lens-loaded.vl:178:2");
const feed3 = fresh([ 1 ], [ 1 ]);
resync(feed3, [ m(1, "a"), m(2, "b"), m(3, "c"), m(4, "d") ], [ 1 ]);
const doomed = slot2(items(feed3), 2, [ 1 ]);
const doomed_view = view(overlay(feed3), doomed, [ 1 ], [ 1 ]);
const doomed_runs = __shared_new(0);
on_change2(__clone(doomed), (_item, $gX) => {
	const $gY = [ doomed_runs, "v" ];
	return doomed_runs.v = $gY[0][$gY[1]] + 1;
}, "lens-loaded.vl:192:19");
const rows4 = rows3(overlay(feed3), [ 1 ], [ 1 ]);
const rows_runs = __shared_new(0);
on_change5(rows4, (_rows, $hs) => {
	const $ht = [ rows_runs, "v" ];
	return rows_runs.v = $ht[0][$ht[1]] + 1;
}, "lens-loaded.vl:195:17");
const delete2 = unwrap(propose_delete(overlay(feed3), doomed, [ 1 ]), "lens-loaded.vl:196:53");
assert(eq(shown(feed3), [ 1, 2, 4 ]) && eq5(get3(doomed_view), [ 1 ]), "4: the delete shows at once", "lens-loaded.vl:197:2");
assert(eq(held(feed3), [ 1, 2, 3, 4 ]) && eq5(get4(doomed), [ 0, m(3, "c") ]), "4: the lens keeps the row (it is hidden, not deleted)", "lens-loaded.vl:198:2");
assert(get3(is_pending_key(overlay(feed3), 3)) && get3(pending(overlay(feed3))) === 1, "4: and is pending", "lens-loaded.vl:202:2");
assert(rows_runs.v === 1, "4: the row feed woke once for the hide", "lens-loaded.vl:206:2");
const reload = load(feed3, [ 1 ]);
assert(settle(feed3, reload, [ 0, [ m(0, "new"), m(4, "d"), m(3, "c"), m(2, "b"), m(1, "a") ] ], [ 1 ]), "4: the reload lands", "lens-loaded.vl:209:2");
assert(eq(shown(feed3), [ 0, 4, 2, 1 ]), "4: the reload is shown with the pending delete still applied", "lens-loaded.vl:213:2");
assert(eq6(at(doomed), [ 0, 2 ]), "4: the hidden row\'s slot moved with the reload", "lens-loaded.vl:217:2");
assert(reject(overlay(feed3), delete2, [ 1 ]), "4: the rejection is applied", "lens-loaded.vl:218:2");
assert(eq(shown(feed3), [ 0, 4, 3, 2, 1 ]), "4: the rejected row came back where the server now has it", "lens-loaded.vl:219:2");
assert(eq8(__at(shown_rows(overlay(feed3)), 2, "lens-loaded.vl:224:3")[1], [ 0, doomed ]) && eq5(get3(doomed_view), [ 0, m(3, "c") ]), "4: in its OWN slot: a view keyed by slot does not rebuild the row", "lens-loaded.vl:223:2");
assert(!(get3(is_pending_key(overlay(feed3), 3))) && get3(pending(overlay(feed3))) === 0, "4: nothing pending", "lens-loaded.vl:231:2");
assert(doomed_runs.v === 0, "4: the hidden row\'s slot never woke: hide, reload and un-hide", "lens-loaded.vl:235:2");
assert(rows_runs.v === 3, "4: the row feed woke once each for the hide, the reload and the un-hide", "lens-loaded.vl:239:2");
const doomed2 = slot2(items(feed3), 1, [ 1 ]);
const delete3 = unwrap(propose_delete(overlay(feed3), doomed2, [ 1 ]), "lens-loaded.vl:245:53");
const reload2 = load(feed3, [ 1 ]);
assert(reject(overlay(feed3), delete3, [ 1 ]), "4b: rejected", "lens-loaded.vl:247:2");
assert(eq(shown(feed3), [ 0, 4, 3, 2, 1 ]), "4b: rejected before the reload: back in its place", "lens-loaded.vl:248:2");
assert(settle(feed3, reload2, [ 0, [ m(0, "new"), m(1, "a"), m(4, "d"), m(3, "c"), m(2, "b") ] ], [ 1 ]), "4b: the reload lands", "lens-loaded.vl:249:2");
assert(eq(shown(feed3), [ 0, 1, 4, 3, 2 ]) && eq6(at(doomed2), [ 0, 2 ]), "4b: and the reload orders it", "lens-loaded.vl:253:2");
const top = slot2(items(feed3), 0, [ 1 ]);
const delete4 = unwrap(propose_delete(overlay(feed3), top, [ 1 ]), "lens-loaded.vl:259:50");
const older2 = unwrap(request_older(feed3, [ 1 ]), "lens-loaded.vl:260:35");
assert(prepend_page(feed3, older2, [ 0, [ [ m(-(2), "y"), m(-(1), "z") ], false ] ], [ 1 ]), "4c: a page lands above", "lens-loaded.vl:261:2");
assert(reject(overlay(feed3), delete4, [ 1 ]), "4c: rejected", "lens-loaded.vl:265:2");
assert(eq(shown(feed3), [ -(2), -(1), 0, 1, 4, 3, 2 ]) && eq6(at(top), [ 0, 2 ]), "4c: the top row came back below the new page, not at position 0", "lens-loaded.vl:266:2");
const gone = slot2(items(feed3), 3, [ 1 ]);
const delete5 = unwrap(propose_delete(overlay(feed3), gone, [ 1 ]), "lens-loaded.vl:272:51");
assert(eq(held(feed3), [ -(2), -(1), 0, 1, 4, 3, 2 ]), "4d: hidden, still held", "lens-loaded.vl:273:2");
assert(confirm(overlay(feed3), delete5, [ 1 ]), "4d: confirmed", "lens-loaded.vl:274:2");
assert(eq(held(feed3), [ -(2), -(1), 0, 4, 3, 2 ]) && eq(shown(feed3), [ -(2), -(1), 0, 4, 3, 2 ]) && eq5(get4(gone), [ 1 ]), "4d: the confirmed delete is the server\'s word: the lens lets the row go", "lens-loaded.vl:275:2");
const feed4 = fresh([ 1 ], [ 1 ]);
resync(feed4, [ m(1, "a"), m(2, "b") ], [ 1 ]);
const row4 = slot2(items(feed4), 1, [ 1 ]);
const view4 = view(overlay(feed4), row4, [ 1 ], [ 1 ]);
const runs = __shared_new(0);
const other_runs = __shared_new(0);
on_change(__clone(view4), (_item, $hC) => {
	const $hD = [ runs, "v" ];
	return runs.v = $hD[0][$hD[1]] + 1;
}, "lens-loaded.vl:290:17");
on_change(view(overlay(feed4), slot2(items(feed4), 0, [ 1 ]), [ 1 ], [ 1 ]), (_item, $hE) => {
	const $hF = [ other_runs, "v" ];
	return other_runs.v = $hF[0][$hF[1]] + 1;
}, "lens-loaded.vl:291:54");
const first2 = unwrap(propose(overlay(feed4), row4, m(2, "A"), [ 1 ]), "lens-loaded.vl:292:53");
const second = unwrap(propose(overlay(feed4), row4, m(2, "B"), [ 1 ]), "lens-loaded.vl:293:54");
assert(eq5(get3(view4), [ 0, m(2, "B") ]) && runs.v === 2 && get3(pending(overlay(feed4))) === 2, "5: both edits showed, the newer on top", "lens-loaded.vl:294:2");
assert(eq5(get4(row4), [ 0, m(2, "b") ]), "5: the lens holds the server\'s word", "lens-loaded.vl:298:2");
assert(confirm(overlay(feed4), second, [ 1 ]), "5: the second is confirmed", "lens-loaded.vl:299:2");
assert(eq5(get3(view4), [ 0, m(2, "B") ]) && eq5(get4(row4), [ 0, m(2, "B") ]) && runs.v === 2, "5: confirming the shown value lands it in the lens and wakes no view", "lens-loaded.vl:300:2");
assert(get3(is_pending(overlay(feed4), row4)) && get3(pending(overlay(feed4))) === 1, "5: the first is still pending", "lens-loaded.vl:304:2");
assert(confirm(overlay(feed4), first2, [ 1 ]), "5: the first is confirmed late", "lens-loaded.vl:308:2");
assert(eq5(get3(view4), [ 0, m(2, "B") ]) && eq5(get4(row4), [ 0, m(2, "B") ]) && runs.v === 2, "5: the late, older ack does not walk the row back to A", "lens-loaded.vl:309:2");
assert(!(get3(is_pending(overlay(feed4), row4))) && get3(pending(overlay(feed4))) === 0 && other_runs.v === 0, "5: settled; the other row never woke", "lens-loaded.vl:313:2");
const first3 = unwrap(propose(overlay(feed4), row4, m(2, "C"), [ 1 ]), "lens-loaded.vl:323:53");
const second2 = unwrap(propose(overlay(feed4), row4, m(2, "D"), [ 1 ]), "lens-loaded.vl:324:54");
assert(reject(overlay(feed4), second2, [ 1 ]), "5b: the newer refused", "lens-loaded.vl:325:2");
assert(eq5(get3(view4), [ 0, m(2, "C") ]), "5b: the newer refused: the older still shows", "lens-loaded.vl:326:2");
assert(reject(overlay(feed4), first3, [ 1 ]), "5b: the older refused", "lens-loaded.vl:327:2");
assert(eq5(get3(view4), [ 0, m(2, "B") ]), "5b: both refused: the server\'s word", "lens-loaded.vl:328:2");
const first4 = unwrap(propose(overlay(feed4), row4, m(2, "E"), [ 1 ]), "lens-loaded.vl:329:53");
const second3 = unwrap(propose(overlay(feed4), row4, m(2, "F"), [ 1 ]), "lens-loaded.vl:330:54");
assert(confirm(overlay(feed4), second3, [ 1 ]) && reject(overlay(feed4), first4, [ 1 ]), "5c: answered", "lens-loaded.vl:331:2");
assert(eq5(get3(view4), [ 0, m(2, "F") ]) && eq5(get4(row4), [ 0, m(2, "F") ]) && get3(pending(overlay(feed4))) === 0, "5c: the older refused after the newer confirmed: F stands", "lens-loaded.vl:332:2");
assert(!(confirm(overlay(feed4), first4, [ 1 ])), "5c: an id already answered is not pending", "lens-loaded.vl:340:2");
const feed5 = fresh([ 1 ], [ 1 ]);
resync(feed5, [ m(10, "j"), m(11, "k") ], [ 1 ]);
const page2 = unwrap(request_older(feed5, [ 1 ]), "lens-loaded.vl:346:34");
assert(prepend_page(feed5, page2, [ 1, "timeout" ], [ 1 ]), "6: the failure is applied", "lens-loaded.vl:347:2");
assert(edge(get3(older(feed5))) === "Failed(timeout)" && eq(held(feed5), [ 10, 11 ]), "6: the end failed, the rows stay", "lens-loaded.vl:348:2");
const stuck = unwrap(request_older(feed5, [ 1 ]), "lens-loaded.vl:352:35");
const retry2 = unwrap(request_older(feed5, [ 1 ]), "lens-loaded.vl:353:35");
assert(!(prepend_page(feed5, stuck, [ 0, [ [ m(9, "STALE") ], true ] ], [ 1 ])), "6: the superseded ask is dropped", "lens-loaded.vl:354:2");
assert(prepend_page(feed5, retry2, [ 0, [ [ m(8, "h"), m(9, "i"), m(10, "j"), m(10, "j") ], false ] ], [ 1 ]), "6: the retry lands", "lens-loaded.vl:358:2");
assert(eq(held(feed5), [ 8, 9, 10, 11 ]), "6: no duplicate, nothing lost", "lens-loaded.vl:365:2");
assert(edge(get3(older(feed5))) === "End" && eq3(request_older(feed5, [ 1 ]), [ 1 ]), "6: the end is reached", "lens-loaded.vl:366:2");
const newer2 = unwrap(request_newer(feed5, [ 1 ]), "lens-loaded.vl:370:35");
assert(append_page(feed5, newer2, [ 0, [ [ m(11, "k"), m(12, "l") ], true ] ], [ 1 ]), "6: the newer end appends", "lens-loaded.vl:371:2");
assert(eq(held(feed5), [ 8, 9, 10, 11, 12 ]), "6: appended, the overlap once", "lens-loaded.vl:375:2");
const older_page = unwrap(request_newer(feed5, [ 1 ]), "lens-loaded.vl:377:40");
resync(feed5, [ m(20, "t") ], [ 1 ]);
assert(!(append_page(feed5, older_page, [ 0, [ [ m(13, "m") ], true ] ], [ 1 ])), "6b: a page asked before a reload is dropped", "lens-loaded.vl:379:2");
assert(eq(held(feed5), [ 20 ]) && edge(get3(newer(feed5))) === "Idle", "6b: the reload\'s rows only", "lens-loaded.vl:383:2");
const users = new13([ 1 ]);
const ada = view2(overlay2(users), "ada", [ 1 ], [ 1 ]);
const ada_runs = __shared_new(0);
on_change6(__clone(ada), (_name, $iI) => {
	const $iJ = [ ada_runs, "v" ];
	return ada_runs.v = $iJ[0][$iJ[1]] + 1;
}, "lens-loaded.vl:390:16");
const asked = load2(users, "ada", [ 1 ]);
assert(said(get3(phase2(users, "ada"))) === "Pending", "7: a key asked for is Pending", "lens-loaded.vl:392:2");
assert(settle2(users, "ada", asked, [ 0, [ 1 ] ], [ 1 ]), "7: no such user", "lens-loaded.vl:393:2");
assert(said(get3(phase2(users, "ada"))) === "Absent" && eq3(get3(ada), [ 1 ]) && ada_runs.v === 0, "7: Absent, the view reads None, nothing woke", "lens-loaded.vl:394:2");
const again = load2(users, "ada", [ 1 ]);
assert(said(get3(phase2(users, "ada"))) === "Pending", "7: asking again after Absent is Pending", "lens-loaded.vl:399:2");
assert(settle2(users, "ada", again, [ 0, [ 0, "Ada" ] ], [ 1 ]), "7: now there is", "lens-loaded.vl:400:2");
assert(said(get3(phase2(users, "ada"))) === "Ready(" + again + ")" && eq3(get3(ada), [ 0, "Ada" ]) && ada_runs.v === 1, "7: Ready, the same view woke once", "lens-loaded.vl:401:2");
const rename = propose2(overlay2(users), "ada", "Ada L.", [ 1 ]);
assert(eq3(get3(ada), [ 0, "Ada L." ]) && ada_runs.v === 2 && get3(is_pending2(overlay2(users), "ada")), "7: a local edit shows", "lens-loaded.vl:411:2");
push2(users, "ada", [ 0, "Ada (server)" ], [ 1 ]);
assert(eq3(get3(ada), [ 0, "Ada L." ]) && ada_runs.v === 2, "7: the server\'s push lands under the pending edit", "lens-loaded.vl:419:2");
assert(eq3(peek3(items2(users), (map2) => {
	return get7(map2, "ada");
}), [ 0, "Ada (server)" ]), "7: the lens holds the push", "lens-loaded.vl:423:2");
assert(reject2(overlay2(users), rename, [ 1 ]), "7: the rename is refused", "lens-loaded.vl:427:2");
assert(eq3(get3(ada), [ 0, "Ada (server)" ]) && ada_runs.v === 3, "7: the rejection shows the pushed value, not the old one", "lens-loaded.vl:428:2");
const bob = view2(overlay2(users), "bob", [ 1 ], [ 1 ]);
const bob_runs = __shared_new(0);
on_change6(__clone(bob), (_name, $jZ) => {
	const $ka = [ bob_runs, "v" ];
	return bob_runs.v = $ka[0][$ka[1]] + 1;
}, "lens-loaded.vl:435:16");
const create = propose2(overlay2(users), "bob", "Bob", [ 1 ]);
assert(eq3(get3(bob), [ 0, "Bob" ]) && bob_runs.v === 1, "7: the create shows", "lens-loaded.vl:437:2");
assert(eq3(peek3(items2(users), (map2) => {
	return get7(map2, "bob");
}), [ 1 ]), "7: and is not in the lens", "lens-loaded.vl:438:2");
push2(users, "bob", [ 0, "Bob, as stored" ], [ 1 ]);
assert(eq3(get3(bob), [ 0, "Bob" ]) && bob_runs.v === 1, "7: the arriving record waits under the create", "lens-loaded.vl:440:2");
assert(confirm2(overlay2(users), create, [ 1 ]), "7: the create is confirmed", "lens-loaded.vl:444:2");
assert(eq3(get3(bob), [ 0, "Bob" ]) && bob_runs.v === 1 && get3(pending2(overlay2(users))) === 0, "7: the confirm keeps the shown value and wakes nothing", "lens-loaded.vl:445:2");
const ada_slot = slot4(items2(users), "ada", [ 1 ]);
const ada_slot_runs = __shared_new(0);
on_change7(__clone(ada_slot), (_name, $kc) => {
	const $kd = [ ada_slot_runs, "v" ];
	return ada_slot_runs.v = $kd[0][$kd[1]] + 1;
}, "lens-loaded.vl:452:21");
const drop2 = propose_delete2(overlay2(users), "ada", [ 1 ]);
assert(eq3(get3(ada), [ 1 ]) && eq3(get8(ada_slot), [ 0, "Ada (server)" ]), "7: the delete hides the record", "lens-loaded.vl:454:2");
assert(reject2(overlay2(users), drop2, [ 1 ]), "7: the delete is refused", "lens-loaded.vl:458:2");
assert(eq3(get3(ada), [ 0, "Ada (server)" ]) && ada_slot_runs.v === 0, "7: rejected: the record shows again; the lens\'s slot never woke", "lens-loaded.vl:459:2");
const feed6 = fresh([ 1 ], [ 1 ]);
resync(feed6, [ m(1, "a"), m(2, "b") ], [ 1 ]);
const row5 = slot2(items(feed6), 1, [ 1 ]);
const view5 = view(overlay(feed6), row5, [ 1 ], [ 1 ]);
const mine = unwrap(propose(overlay(feed6), row5, m(2, "mine"), [ 1 ]), "lens-loaded.vl:468:55");
receive(feed6, m(2, "theirs"), [ 1 ]);
receive(feed6, m(3, "new"), [ 1 ]);
assert(eq5(get3(view5), [ 0, m(2, "mine") ]) && eq5(get4(row5), [ 0, m(2, "theirs") ]) && eq(shown(feed6), [ 1, 2, 3 ]), "7b: a push to a pending row lands under it; a new row arrives at the end", "lens-loaded.vl:471:2");
assert(reject(overlay(feed6), mine, [ 1 ]), "7b: refused", "lens-loaded.vl:479:2");
assert(eq5(get3(view5), [ 0, m(2, "theirs") ]), "7b: rejected: the row shows the pushed value", "lens-loaded.vl:480:2");
const feed7 = fresh([ 1 ], [ 1 ]);
resync(feed7, [ m(1, "a"), m(2, "b"), m(3, "c") ], [ 1 ]);
const before = get3(phase(feed7));
const one2 = view(overlay(feed7), slot2(items(feed7), 0, [ 1 ]), [ 1 ], [ 1 ]);
const two_slot = slot2(items(feed7), 1, [ 1 ]);
const two2 = view(overlay(feed7), two_slot, [ 1 ], [ 1 ]);
const one_runs2 = __shared_new(0);
const two_runs = __shared_new(0);
on_change(one2, (_item, $kf) => {
	const $kg = [ one_runs2, "v" ];
	return one_runs2.v = $kg[0][$kg[1]] + 1;
}, "lens-loaded.vl:492:16");
on_change(__clone(two2), (_item, $kh) => {
	const $ki = [ two_runs, "v" ];
	return two_runs.v = $ki[0][$ki[1]] + 1;
}, "lens-loaded.vl:493:16");
const local = unwrap(propose(overlay(feed7), two_slot, m(2, "local"), [ 1 ]), "lens-loaded.vl:494:62");
const in_flight = load(feed7, [ 1 ]);
disconnected(feed7, "offline", [ 1 ]);
assert(said(get3(phase(feed7))) === "Failed(offline, " + unwrap(latest(before), "lens-loaded.vl:498:66") + ")", "8: the phase says the rows are stale", "lens-loaded.vl:497:2");
assert(eq(held(feed7), [ 1, 2, 3 ]) && eq5(get3(two2), [ 0, m(2, "local") ]), "8: the rows stay, the edit still shows", "lens-loaded.vl:501:2");
assert(!(settle(feed7, in_flight, [ 0, [  ] ], [ 1 ])), "8: the load in flight when the connection dropped is dropped", "lens-loaded.vl:505:2");
resync(feed7, [ m(1, "a"), m(2, "server"), m(3, "c2"), m(4, "d") ], [ 1 ]);
assert(eq(held(feed7), [ 1, 2, 3, 4 ]) && said(get3(phase(feed7))) !== "Pending", "8: resynced", "lens-loaded.vl:510:2");
assert(eq5(get3(two2), [ 0, m(2, "local") ]) && get3(is_pending(overlay(feed7), two_slot)), "8: the pending edit is still on top", "lens-loaded.vl:511:2");
assert(one_runs2.v === 0 && two_runs.v === 1, "8: the resync woke neither unchanged view", "lens-loaded.vl:515:2");
assert(reject(overlay(feed7), local, [ 1 ]), "8: refused", "lens-loaded.vl:519:2");
assert(eq5(get3(two2), [ 0, m(2, "server") ]) && two_runs.v === 2, "8: rejected: the row shows the resynced value", "lens-loaded.vl:520:2");
console.log("loaded: all passed");
//# sourceMappingURL=lens-loaded.mjs.map
