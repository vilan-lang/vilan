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
	let $ax = null;
	if (!(condition)) {
		$ax = (() => {
			throw __panic(message, caller);
		})();
	}
	return $ax;
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
	const $hF = self;
	let $hI = null;
	if ($hF[0] === 0) {
		const $hG = b;
		let $hH = null;
		if ($hG[0] === 0) {
			$hH = $hF[1] === $hG[1];
		} else {
			$hH = false;
		}
		$hI = $hH;
	} else {
		const $hJ = b;
		$hI = $hJ[0] === 1;
	}
	return $hI;
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
		let $am = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$am = undefined;
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
		$am;
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
	const $dW = turn;
	let $dX = null;
	if ($dW[0] === 0) {
		const ambient = $dW[1];
		$dX = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $dY = last(draining_turns.v);
		let $dZ = null;
		if ($dY[0] === 0) {
			const draining = $dY[1];
			$dZ = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$dZ = undefined;
		}
		$dX = $dZ;
	}
	return $dX;
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
		const $eu = previous;
		let $ev = null;
		if ($eu[0] === 0) {
			const release8 = $eu[1];
			$ev = release8();
		} else {
			$ev = undefined;
		}
		return $ev;
	} ];
}
function detached(id, live, release8) {
	let subscribers = [  ];
	const empty = __shared_new(subscribers);
	return [ empty, id, live, __shared_new([ 0, release8 ]) ];
}
function dispose(self, $eJ) {
	const $eK = $eJ;
	let $eL = null;
	if ($eK[0] === 0) {
		const established = $eK[1];
		$eL = [ 0, established ];
	} else {
		$eL = last(draining_turns.v);
	}
	const ambient = $eL;
	release_under(self, ambient);
}
function detach(handle) {
	const $ed = last(releasing_turns.v);
	let $ee = null;
	if ($ed[0] === 0) {
		const at_release = $ed[1];
		$ee = at_release;
	} else {
		$ee = last(draining_turns.v);
	}
	const turn = $ee;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ef = [ 0, handle[0] ];
	let $eg = null;
	if ($ef[0] === 0) {
		const subscribers = $ef[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$eg = undefined;
	} else {
		$eg = undefined;
	}
	$eg;
	const $eh = ambient;
	let $ei = null;
	if ($eh[0] === 0) {
		const turn = $eh[1];
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
		$ei = undefined;
	} else {
		$ei = undefined;
	}
	$ei;
	const $ej = handle[3].v;
	let $ek = null;
	if ($ej[0] === 0) {
		const release8 = $ej[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release8, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ek = undefined;
	} else {
		$ek = undefined;
	}
	return $ek;
}
function new4() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $V = null;
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
		$V = undefined;
	}
	return $V;
}
function renew(self) {
	const $dw = self[0].v[3];
	let $dx = null;
	if ($dw[0] === 0) {
		const nursery2 = __clone($dw[1]);
		let $dy = null;
		if (has_spawned(nursery2)) {
			$dy = [ 1 ];
		} else {
			$dy = [ 0, nursery2 ];
		}
		$dx = $dy;
	} else {
		$dx = [ 1 ];
	}
	const carried = $dx;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $dJ = null;
	if (is_disposed(self)) {
		$dJ = [ 1 ];
	} else {
		$dJ = self[0].v[3];
	}
	return $dJ;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $dI = null;
	if (!(is_disposed(self))) {
		const held3 = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $dz = held3[3];
		let $dA = null;
		if ($dz[0] === 0) {
			const nursery2 = $dz[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$dA = undefined;
		} else {
			$dA = undefined;
		}
		$dA;
		let $dH = null;
		if (held3[2]) {
			let failure = [ 1 ];
			for (const cleanup of held3[1].v) {
				const $dC = __guarded(cleanup);
				let $dD = null;
				if ($dC[0] === 0) {
					const message = $dC[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$dD = undefined;
				} else {
					$dD = undefined;
				}
				$dD;
			}
			const $dF = failure;
			let $dG = null;
			if ($dF[0] === 0) {
				const message2 = $dF[1];
				$dG = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$dG = undefined;
			}
			$dH = $dG;
		}
		$dI = $dH;
	}
	return $dI;
}
function register_with_owner(subscription, $eE, $eF) {
	const $eG = $eF;
	let $eH = null;
	if ($eG[0] === 0) {
		const owner = $eG[1];
		$eH = take(owner, subscription, $eE);
	} else {
		$eH = __clone(subscription);
	}
	return $eH;
}
function defer_to_owner(cleanup, $S) {
	const $T = $S;
	let $U = null;
	if ($T[0] === 0) {
		const owner = $T[1];
		$U = defer(owner, cleanup);
	} else {
		$U = undefined;
	}
	return $U;
}
function release_runs(runs2) {
	dispose2([ runs2[0], runs2[0].v[0] ]);
}
function new_tracker(site) {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ], site ]) ];
}
function lists_of(tracker) {
	const $cW = tracker[0].v[6];
	let $cX = null;
	if ($cW[0] === 0) {
		const lists = $cW[1];
		$cX = lists;
	} else {
		trackers_allocated_count.v = trackers_allocated_count.v + 1;
		const made = __shared_new([ [  ], [  ], [  ] ]);
		tracker[0].v[6] = [ 0, made ];
		$cX = made;
	}
	return $cX;
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $dr = tracker[0].v[6];
	let $ds = null;
	if ($dr[0] === 0) {
		const lists = $dr[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$ds = undefined;
	} else {
		$ds = undefined;
	}
	$ds;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $dO = tracker[0].v[6];
	let $dP = null;
	if ($dO[0] === 0) {
		const lists = $dO[1];
		$dP = lists;
	} else {
		return;
		$dP = undefined;
	}
	const lists2 = $dP;
	const $dQ = tracker[0].v[5];
	let $dR = null;
	if ($dQ[0] === 0) {
		const target = __clone($dQ[1]);
		$dR = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$dR = undefined;
	}
	$dR;
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
		const $dU = reusable(held3, kept, dependency[0], position);
		let $dV = null;
		if ($dU[0] === 0) {
			const index2 = $dU[1];
			__at_put(kept, index2, true, "std/src/reactive.vl:1715:5");
			next.push(__clone(__at(held3, index2, "std/src/reactive.vl:1716:15")));
			$dV = undefined;
		} else {
			next.push([ __clone(dependency[0]), dependency[1](relay_for(tracker, target)) ]);
			$dV = undefined;
		}
		$dV;
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
	const $dS = identity5;
	let $dT = null;
	if ($dS[0] === 0) {
		const wanted = $dS[1];
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
		$dT = [ 1 ];
	} else {
		$dT = [ 1 ];
	}
	return $dT;
}
function same_identity(identity5, wanted) {
	const $cU = identity5;
	let $cV = null;
	if ($cU[0] === 0) {
		const held3 = $cU[1];
		$cV = held3 === wanted;
	} else {
		$cV = false;
	}
	return $cV;
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
	const $ea = tracker[0].v[7];
	let $eb = null;
	if ($ea[0] === 0) {
		const site = __clone($ea[1]);
		$eb = place_subscriber(relay[0], site);
	} else {
		$eb = undefined;
	}
	$eb;
	return relay;
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $eo = tracker[0].v[6];
	let $ep = null;
	if ($eo[0] === 0) {
		const lists = $eo[1];
		$ep = lists;
	} else {
		return;
		$ep = undefined;
	}
	const lists2 = $ep;
	let $eq = null;
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
		$eq = undefined;
	}
	return $eq;
}
function forget_reads(tracker) {
	const $er = tracker[0].v[6];
	let $es = null;
	if ($er[0] === 0) {
		const lists = $er[1];
		$es = lists;
	} else {
		return;
		$es = undefined;
	}
	const lists2 = $es;
	let $et = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge2 of edges) {
			detach(edge2[1]);
		}
		$et = undefined;
	}
	$et;
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
	const $cQ = tracker[0].v[6];
	let $cR = null;
	if ($cQ[0] === 0) {
		const lists = $cQ[1];
		$cR = lists;
	} else {
		return true;
		$cR = undefined;
	}
	const lists2 = $cR;
	const $cS = identity5;
	let $cT = null;
	if ($cS[0] === 0) {
		const wanted = $cS[1];
		const count2 = lists2.v[0].length;
		let index2 = 0;
		while (index2 < count2) {
			if (same_identity(__at(lists2.v[0], index2, "std/src/reactive.vl:1877:22")[0], wanted)) {
				return false;
			}
			index2 = index2 + 1;
		}
		$cT = true;
	} else {
		$cT = true;
	}
	return $cT;
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
		const $eC = previous;
		let $eD = null;
		if ($eC[0] === 0) {
			const earlier = $eC[1];
			$eD = earlier();
		} else {
			$eD = undefined;
		}
		return $eD;
	} ];
}
function placed_at(subscriber, site) {
	place_subscriber(subscriber[0], site);
	return subscriber;
}
function settle_together(body, $bU) {
	const $bV = $bU;
	let $bW = null;
	if ($bV[0] === 0) {
		const current = $bV[1];
		$bW = body(current);
	} else {
		const $bX = last(draining_turns.v);
		let $bY = null;
		if ($bX[0] === 0) {
			const draining = $bX[1];
			$bY = body(draining);
		} else {
			$bY = batch(body, $bU);
		}
		$bW = $bY;
	}
	return $bW;
}
function new5() {
	return [ [  ] ];
}
function follow(self, at3) {
	if (self[0].length === 0) {
		self[0].push(new6());
	}
	return follow2(__at(self[0], 0, "std/src/reactive/lens.vl:142:3"), at3);
}
function apply(self, change2, wakes) {
	if (self[0].length > 0) {
		apply2(__at(self[0], 0, "std/src/reactive/lens.vl:147:4"), change2, wakes);
	}
}
function locate(self, slot5) {
	if (self[0].length === 0) {
		return [ 1 ];
	}
	return locate2(__at(self[0], 0, "std/src/reactive/lens.vl:155:3"), slot5);
}
function follows_items(self) {
	return true;
}
function release(self, slot5) {
	if (self[0].length > 0) {
		release2(__at(self[0], 0, "std/src/reactive/lens.vl:164:4"), slot5);
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
		if (__at(self[3], middle, "std/src/reactive/lens.vl:210:7") <= at3) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at3 - __at(self[3], low, "std/src/reactive/lens.vl:216:14") ];
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
	let index2 = from;
	const held3 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:236:14").length;
	while (index2 < held3) {
		const slot5 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:238:15"), index2, "std/src/reactive/lens.vl:238:15");
		if (slot5 !== -(1)) {
			__at_put(self[4], as_usize(slot5), block, "std/src/reactive/lens.vl:240:5");
			__at_put(self[5], as_usize(slot5), index2, "std/src/reactive/lens.vl:241:5");
		}
		index2 = index2 + 1;
	}
}
function open(self, place) {
	const block = as_i32(self[0].length);
	self[0].push([  ]);
	self[2].push(0);
	__insert_at(self[1], place, block, "std/src/reactive/lens.vl:252:14");
	return block;
}
function widen(self, at3, count2) {
	if (count2 === 0 || at3 >= self[6]) {
		return;
	}
	const $bp = seat(self, at3);
	const place = $bp[0];
	const index2 = $bp[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:262:15");
	let $bq = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:266:5"), index2, -(1), "std/src/reactive/lens.vl:266:35");
			made = made + 1;
		}
		$bq = undefined;
	} else {
		const before2 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:270:17"));
		let grown = [  ];
		let copied = 0;
		while (copied < index2) {
			grown.push(__at(before2, copied, "std/src/reactive/lens.vl:274:16"));
			copied = copied + 1;
		}
		let made2 = 0;
		while (made2 < count2) {
			grown.push(-(1));
			made2 = made2 + 1;
		}
		const held3 = before2.length;
		while (copied < held3) {
			grown.push(__at(before2, copied, "std/src/reactive/lens.vl:284:16"));
			copied = copied + 1;
		}
		__at_put(self[0], as_usize(block), grown, "std/src/reactive/lens.vl:287:4");
	}
	$bq;
	self[6] = self[6] + count2;
	renumber(self, block, index2 + count2);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:291:6").length > 2 * block_size) {
		rebalance(self, place);
	}
	restart(self);
}
function close(self, at3, count2, wakes) {
	let left = count2;
	while (left > 0 && at3 < self[6]) {
		const $bn = seat(self, at3);
		const place = $bn[0];
		const index2 = $bn[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:303:16");
		const held3 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:304:15").length;
		let $bo = null;
		if (left < held3 - index2) {
			$bo = left;
		} else {
			$bo = held3 - index2;
		}
		const take2 = $bo;
		let taken = 0;
		while (taken < take2) {
			const slot5 = __remove_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:308:16"), index2, "std/src/reactive/lens.vl:308:46");
			if (slot5 !== -(1)) {
				__at_put(self[4], as_usize(slot5), -(1), "std/src/reactive/lens.vl:310:6");
				wakes.push([ 2, slot5 ]);
			}
			taken = taken + 1;
		}
		self[6] = self[6] - take2;
		left = left - take2;
		if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:317:7").length === 0) {
			__remove_at(self[1], place, "std/src/reactive/lens.vl:318:16");
		} else {
			renumber(self, block, index2);
		}
		restart(self);
	}
}
function rebalance(self, place) {
	const block = __at(self[1], place, "std/src/reactive/lens.vl:328:15");
	const all3 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:329:13"));
	let first5 = [  ];
	let index2 = 0;
	while (index2 < block_size) {
		first5.push(__at(all3, index2, "std/src/reactive/lens.vl:333:15"));
		index2 = index2 + 1;
	}
	__at_put(self[0], as_usize(block), first5, "std/src/reactive/lens.vl:336:3");
	let next = place + 1;
	const held3 = all3.length;
	while (index2 < held3) {
		const fresh2 = open(self, next);
		let filled = 0;
		while (filled < block_size && index2 < held3) {
			__at(self[0], as_usize(fresh2), "std/src/reactive/lens.vl:343:5").push(__at(all3, index2, "std/src/reactive/lens.vl:343:40"));
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
		if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:358:34")), "std/src/reactive/lens.vl:358:22").length >= block_size) {
			const fresh2 = open(self, places);
			__at_put(self[2], as_usize(fresh2), places, "std/src/reactive/lens.vl:360:5");
			self[3].push(self[6]);
		}
		const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:363:15");
		__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:364:4").push(-(1));
		self[6] = self[6] + 1;
	}
	const $u = seat(self, at3);
	const place = $u[0];
	const index2 = $u[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:368:15");
	const held3 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:369:14"), index2, "std/src/reactive/lens.vl:369:14");
	if (held3 !== -(1)) {
		return held3;
	}
	const slot5 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index2);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:376:3"), index2, slot5, "std/src/reactive/lens.vl:376:3");
	return slot5;
}
function apply2(self, change2, wakes) {
	const $bl = change2;
	let $bm = null;
	if ($bl[0] === 0) {
		const at3 = $bl[1];
		const removed = $bl[2];
		const inserted = $bl[3];
		close(self, at3, removed, wakes);
		widen(self, at3, inserted);
		$bm = undefined;
	} else if ($bl[0] === 1) {
		const at4 = $bl[1];
		const count2 = $bl[2];
		$bm = widen(self, at4, count2);
	} else if ($bl[0] === 3) {
		const at5 = $bl[1];
		const count3 = $bl[2];
		$bm = close(self, at5, count3, wakes);
	} else if ($bl[0] === 2) {
		const at6 = $bl[1];
		let $bs = null;
		if (at6 < self[6]) {
			const $br = seat(self, at6);
			const place = $br[0];
			const index2 = $br[1];
			const slot5 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:393:29")), "std/src/reactive/lens.vl:393:17"), index2, "std/src/reactive/lens.vl:393:17");
			if (slot5 !== -(1)) {
				wakes.push([ 0, slot5 ]);
			}
			$bs = undefined;
		}
		$bm = $bs;
	} else if ($bl[0] === 4) {
		const at7 = $bl[1];
		const part = $bl[2];
		let $bu = null;
		if (at7 < self[6]) {
			const $bt = seat(self, at7);
			const place2 = $bt[0];
			const index3 = $bt[1];
			const slot6 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:402:29")), "std/src/reactive/lens.vl:402:17"), index3, "std/src/reactive/lens.vl:402:17");
			if (slot6 !== -(1)) {
				wakes.push([ 1, slot6, part ]);
			}
			$bu = undefined;
		}
		$bm = $bu;
	} else if ($bl[0] === 5) {
		const from = $bl[1];
		const to = $bl[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $bv = null;
		if (from > to) {
			$bv = from;
		} else {
			$bv = to;
		}
		const far = $bv;
		while (self[6] <= far) {
			const places = self[1].length;
			if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:416:36")), "std/src/reactive/lens.vl:416:24").length >= block_size) {
				const fresh2 = open(self, places);
				__at_put(self[2], as_usize(fresh2), places, "std/src/reactive/lens.vl:418:7");
				self[3].push(self[6]);
			}
			const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:421:17");
			__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:422:6").push(-(1));
			self[6] = self[6] + 1;
		}
		const $bw = seat(self, from);
		const left = $bw[0];
		const index4 = $bw[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:426:18");
		const slot7 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:427:16"), index4, "std/src/reactive/lens.vl:427:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:429:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:430:17");
		} else {
			renumber(self, source, index4);
		}
		restart(self);
		let $bx = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$bx = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:438:25")), "std/src/reactive/lens.vl:438:13").length ];
		} else {
			$bx = seat(self, to);
		}
		const $by = $bx;
		const place3 = $by[0];
		const landing2 = $by[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:442:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:443:5"), landing2, slot7, "std/src/reactive/lens.vl:443:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing2);
		restart(self);
		$bm = undefined;
	} else {
		let slot8 = 0;
		const slots = as_i32(self[4].length);
		while (slot8 < slots) {
			if (__at(self[4], as_usize(slot8), "std/src/reactive/lens.vl:452:9") !== -(1)) {
				__at_put(self[4], as_usize(slot8), -(1), "std/src/reactive/lens.vl:453:7");
				wakes.push([ 2, slot8 ]);
			}
			slot8 = slot8 + 1;
		}
		self[0] = [  ];
		self[1] = [  ];
		self[2] = [  ];
		self[3] = [  ];
		self[6] = 0;
		$bm = undefined;
	}
	return $bm;
}
function release2(self, slot5) {
	const block = __at(self[4], as_usize(slot5), "std/src/reactive/lens.vl:468:15");
	if (block === -(1)) {
		return;
	}
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:472:3"), __at(self[5], as_usize(slot5), "std/src/reactive/lens.vl:472:33"), -(1), "std/src/reactive/lens.vl:472:3");
	__at_put(self[4], as_usize(slot5), -(1), "std/src/reactive/lens.vl:473:3");
}
function locate2(self, slot5) {
	const block = __at(self[4], as_usize(slot5), "std/src/reactive/lens.vl:477:15");
	if (block === -(1)) {
		return [ 1 ];
	}
	return [ 0, __at(self[3], __at(self[2], as_usize(block), "std/src/reactive/lens.vl:481:20"), "std/src/reactive/lens.vl:481:8") + __at(self[5], as_usize(slot5), "std/src/reactive/lens.vl:481:51") ];
}
function new7() {
	return [ new10(), [  ] ];
}
function cancel(task) {
	const $as = task;
	if ($as[0] === 0) {
		$as[1].cancel();
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
	const $at = state;
	let $au = null;
	if ($at[0] === 0) {
		$au = "Pending";
	} else if ($at[0] === 1) {
		const token = $at[1];
		$au = "Ready(" + token + ")";
	} else if ($at[0] === 2) {
		const token2 = $at[1];
		$au = "Refreshing(" + token2 + ")";
	} else if ($at[0] === 3) {
		const error = $at[1];
		const stale = $at[2];
		const $av = stale;
		let $aw = null;
		if ($av[0] === 0) {
			const token3 = $av[1];
			$aw = "Failed(" + error + ", " + token3 + ")";
		} else {
			$aw = "Failed(" + error + ")";
		}
		$au = $aw;
	} else {
		$au = "Absent";
	}
	return $au;
}
function edge(state) {
	const $fz = state;
	let $fA = null;
	if ($fz[0] === 0) {
		$fA = "Idle";
	} else if ($fz[0] === 1) {
		$fA = "Loading";
	} else if ($fz[0] === 2) {
		$fA = "End";
	} else {
		const error = $fz[1];
		$fA = "Failed(" + error + ")";
	}
	return $fA;
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
		read(__at(self, at3, "std/src/reactive/lens.vl:2193:10"));
	}
}
function put(self, at3, value) {
	__at_put(self, at3, __clone(value), "std/src/reactive/lens.vl:2188:3");
}
function change(self, at3, with2) {
	if (at3 < self.length) {
		with2(__at(self, at3, "std/src/reactive/lens.vl:2199:14"));
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
	return [ __shared_new([ __clone(storage), __clone(access), new8(0, "std/src/reactive/lens.vl:726:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
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
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:710:13");
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
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:710:13");
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
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1058:2")[2] = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1058:2")[2] + 1;
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
		const $x = __list_pop(keys2);
		let $y = null;
		if ($x[0] === 0) {
			const key = $x[1];
			const $z = __list_pop(values);
			let $A = null;
			if ($z[0] === 0) {
				const value = $z[1];
				$A = backwards.push([ key, value ]);
			} else {
				$A = undefined;
			}
			$y = $A;
		} else {
			$y = undefined;
		}
		$y;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $B = __list_pop(backwards);
		let $C = null;
		if ($B[0] === 0) {
			const pair = $B[1];
			$C = result.push(pair);
		} else {
			$C = undefined;
		}
		$C;
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
		const $R = _;
		const _key = $R[0];
		const id = $R[1];
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
		const $v = before2;
		if ($v[0] === 0) {
			release_id(watch, $v[1]);
		}
		return;
	};
	const unmap = (held3) => {
		const $w = get(found.v, held3);
		if ($w[0] === 0) {
			remove(found.v, held3);
			release_id(watch, $w[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $D = _;
			const _held = $D[0];
			const old = $D[1];
			release_id(watch, old);
		}
		found.v = new10();
		let at3 = 0;
		const count2 = peek_storage2(storage, (held3) => {
			return count(held3);
		});
		while (at3 < count2) {
			const $I = key_at(at3);
			if ($I[0] === 0) {
				map2($I[1], at3);
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
			const $J = key_at(index3);
			if ($J[0] === 0) {
				gone2.push($J[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone2;
		return;
	};
	const arrive = (at3, count2) => {
		let index3 = at3;
		while (index3 < at3 + count2) {
			const $K = key_at(index3);
			if ($K[0] === 0) {
				map2($K[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change2) => {
		const $L = change2;
		let $M = null;
		if ($L[0] === 0) {
			const at3 = $L[1];
			const removed = $L[2];
			const _inserted = $L[3];
			$M = note(at3, removed);
		} else if ($L[0] === 3) {
			const at4 = $L[1];
			const count2 = $L[2];
			$M = note(at4, count2);
		} else if ($L[0] === 2) {
			const at5 = $L[1];
			$M = note(at5, 1);
		} else if ($L[0] === 4) {
			const at6 = $L[1];
			const _part = $L[2];
			$M = note(at6, 1);
		} else {
			$M = undefined;
		}
		return $M;
	}, (change2, stage) => {
		const $N = stage;
		let $O = null;
		if ($N[0] === 0) {
			$O = undefined;
		} else if ($N[0] === 3) {
			return;
		} else {
			fill();
			return;
			$O = undefined;
		}
		$O;
		const $P = change2;
		let $Q = null;
		if ($P[0] === 0) {
			const at3 = $P[1];
			const _removed = $P[2];
			const inserted = $P[3];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			arrive(at3, inserted);
			$Q = undefined;
		} else if ($P[0] === 1) {
			const at4 = $P[1];
			const count2 = $P[2];
			$Q = arrive(at4, count2);
		} else if ($P[0] === 3) {
			const _at = $P[1];
			const _count = $P[2];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			$Q = undefined;
		} else if ($P[0] === 2) {
			const at5 = $P[1];
			for (const held5 of leaving.v) {
				unmap(held5);
			}
			arrive(at5, 1);
			$Q = undefined;
		} else if ($P[0] === 4) {
			const at6 = $P[1];
			const _part = $P[2];
			for (const held6 of leaving.v) {
				unmap(held6);
			}
			arrive(at6, 1);
			$Q = undefined;
		} else if ($P[0] === 5) {
			const _from = $P[1];
			const _to = $P[2];
			$Q = undefined;
		} else {
			$Q = fill();
		}
		return $Q;
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
			const $aa = __list_get(held3, index2);
			let $ab = null;
			if ($aa[0] === 0) {
				const op = $aa[1];
				$ab = kept.push(op);
			} else {
				$ab = undefined;
			}
			$ab;
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
function rows(self, $X) {
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
			const id = __remove_at(held3.v, at3, "std/src/reactive/lens.vl:1793:27");
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
			__insert_at(held3.v, index2, id, "std/src/reactive/lens.vl:1805:18");
			arrived.push(id);
			index2 = index2 + 1;
		}
		return arrived;
	};
	held3.v = fill();
	const tap2 = tap(self, (_change) => {
		return;
	}, (change2, _stage) => {
		const $Y = change2;
		let $Z = null;
		if ($Y[0] === 0) {
			const at3 = $Y[1];
			const removed = $Y[2];
			const inserted = $Y[3];
			const gone2 = leave(at3, removed);
			record(log, [ 0, at3, gone2, arrive(at3, inserted) ]);
			$Z = undefined;
		} else if ($Y[0] === 1) {
			const at4 = $Y[1];
			const count2 = $Y[2];
			$Z = record(log, [ 0, at4, [  ], arrive(at4, count2) ]);
		} else if ($Y[0] === 3) {
			const at5 = $Y[1];
			const count3 = $Y[2];
			$Z = record(log, [ 0, at5, leave(at5, count3), [  ] ]);
		} else if ($Y[0] === 5) {
			const from = $Y[1];
			const to = $Y[2];
			const id = __remove_at(held3.v, from, "std/src/reactive/lens.vl:1821:28");
			__insert_at(held3.v, to, id, "std/src/reactive/lens.vl:1822:19");
			record(log, [ 3, from, 1, to ]);
			$Z = undefined;
		} else if ($Y[0] === 6) {
			for (const id2 of held3.v) {
				release_id(watch, id2);
			}
			held3.v = fill();
			record(log, [ 2, __clone(held3.v) ]);
			$Z = undefined;
		} else {
			$Z = undefined;
		}
		return $Z;
	});
	const feed8 = [ __clone(self), held3, __clone(log), tap2 ];
	defer_to_owner(() => {
		return release4(feed8);
	}, $X);
	return feed8;
}
function rows2(self, $W) {
	return rows(self[0], $W);
}
function lens(self) {
	return __clone(self[0]);
}
function slot_of(self, id) {
	const $ad = __clone(self);
	let $ac = null;
	if (id < 0) {
		$ac = -(1);
	} else {
		$ac = id;
	}
	return [ $ad, $ac ];
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
	let $an = null;
	if (is_empty(self)) {
		$an = [ 1 ];
	} else {
		$an = __list_get(self, self.length - 1);
	}
	return $an;
}
function notify(self, $ai) {
	const traced = tracing.v;
	let $aj = null;
	if (traced) {
		$aj = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$aj = "";
	}
	const cause = $aj;
	const $ak = $ai;
	let $al = null;
	if ($ak[0] === 0) {
		const turn = $ak[1];
		$al = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $ao = last(draining_turns.v);
		let $ap = null;
		if ($ao[0] === 0) {
			const draining = $ao[1];
			$ap = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $aq = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$aq = undefined;
				}
				$aq;
			}
			$ap = undefined;
		}
		$al = $ap;
	}
	return $al;
}
function set(self, value, $ah, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $ah);
}
function drop_pages(self, $af) {
	self[6].v[2] = 0;
	self[6].v[3] = 0;
	const $ag = get2(self[4]);
	if ($ag[0] === 1) {
		set(self[4], [ 0 ], $af, "std/src/reactive/lens/loaded.vl:873:20");
	}
	const $ar = get2(self[5]);
	if ($ar[0] === 1) {
		set(self[5], [ 0 ], $af, "std/src/reactive/lens/loaded.vl:876:20");
	}
}
function retire(self, $ae) {
	self[6].v[1] = 0;
	drop_pages(self, $ae);
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
	const $az = self;
	let $aA = null;
	if ($az[0] === 1) {
		const value = $az[1];
		$aA = [ 0, __clone(value) ];
	} else if ($az[0] === 2) {
		const value2 = $az[1];
		$aA = [ 0, __clone(value2) ];
	} else if ($az[0] === 3) {
		const _error = $az[1];
		const stale = __clone($az[2]);
		$aA = stale;
	} else {
		$aA = [ 1 ];
	}
	return $aA;
}
function refreshed(self) {
	const $aB = latest(self);
	let $aC = null;
	if ($aB[0] === 0) {
		const value = $aB[1];
		$aC = [ 2, value ];
	} else {
		$aC = [ 0 ];
	}
	return $aC;
}
function same_phase(first5, second4) {
	const $aE = [ first5, second4 ];
	let $aF = null;
	if ($aE[0][0] === 0 && $aE[1][0] === 0) {
		$aF = true;
	} else if ($aE[0][0] === 4 && $aE[1][0] === 4) {
		$aF = true;
	} else if ($aE[0][0] === 1 && $aE[1][0] === 1) {
		const mine2 = $aE[0][1];
		const theirs = $aE[1][1];
		$aF = mine2 === theirs;
	} else if ($aE[0][0] === 2 && $aE[1][0] === 2) {
		const mine3 = $aE[0][1];
		const theirs2 = $aE[1][1];
		$aF = mine3 === theirs2;
	} else {
		$aF = false;
	}
	return $aF;
}
function notify2(self, $ai) {
	const traced = tracing.v;
	let $aG = null;
	if (traced) {
		$aG = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$aG = "";
	}
	const cause = $aG;
	const $aH = $ai;
	let $aI = null;
	if ($aH[0] === 0) {
		const turn = $aH[1];
		$aI = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $aJ = last(draining_turns.v);
		let $aK = null;
		if ($aJ[0] === 0) {
			const draining = $aJ[1];
			$aK = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $aL = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$aL = undefined;
				}
				$aL;
			}
			$aK = undefined;
		}
		$aI = $aK;
	}
	return $aI;
}
function set2(self, value, $ah, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $ah);
}
function set_phase(cell4, next, $aD) {
	if (!(same_phase(get2(cell4), next))) {
		set2(cell4, next, $aD, "std/src/reactive/lens/loaded.vl:716:8");
	}
}
function load(self, $ay) {
	const token = claim(self);
	self[6].v[1] = token;
	set_phase(self[3], refreshed(get2(self[3])), $ay);
	return token;
}
function all_of(storage) {
	return peek_storage2(storage, (held3) => {
		let out = [  ];
		let at3 = 0;
		const count2 = count(held3);
		while (at3 < count2) {
			const $aS = item(held3, at3);
			if ($aS[0] === 0) {
				out.push($aS[1]);
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
		__remove_at(self, at3, "std/src/reactive/lens.vl:2177:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at3 + offset, __clone(value), "std/src/reactive/lens.vl:2182:9");
		offset = offset + 1;
	}
}
function mutate(self, change2) {
	const $bf = self[0].v[0];
	let $bg = null;
	if ($bf[0] === 0) {
		const cell4 = $bf[1];
		$bg = change2(cell4.v);
	} else {
		const port = __clone($bf[1]);
		$bg = port[1](change2);
	}
	return $bg;
}
function wake_all(watch, slot5, $bO) {
	const held3 = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot5), "std/src/reactive/lens.vl:1117:36"));
	});
	const $bP = held3[0];
	if ($bP[0] === 0) {
		notify2($bP[1], $bO);
	}
	for (const part of held3[1]) {
		notify2(part[1], $bO);
	}
}
function wake_part(watch, slot5, name, $bQ) {
	const held3 = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot5), "std/src/reactive/lens.vl:1127:36"));
	});
	const $bR = held3[0];
	if ($bR[0] === 0) {
		notify2($bR[1], $bQ);
	}
	for (const part of held3[1]) {
		if (part[0] === name) {
			notify2(part[1], $bQ);
		}
	}
}
function batch(body, $bZ) {
	const $ca = $bZ;
	let $cb = null;
	if ($ca[0] === 0) {
		const current = $ca[1];
		$cb = body(current);
	} else {
		const fresh2 = new3();
		const result = body(fresh2);
		drain(fresh2);
		fresh2[5].v = true;
		$cb = result;
	}
	return $cb;
}
function landed(self, changes, late, $bi) {
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
		const $bj = change2;
		let $bk = null;
		if ($bj[0] === 0) {
			const _at = $bj[1];
			const removed = $bj[2];
			const inserted = $bj[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$bk = undefined;
		} else if ($bj[0] === 1) {
			const _at2 = $bj[1];
			const _count = $bj[2];
			$bk = resized = true;
		} else if ($bj[0] === 3) {
			const _at3 = $bj[1];
			const _count2 = $bj[2];
			$bk = resized = true;
		} else if ($bj[0] === 6) {
			$bk = resized = true;
		} else if ($bj[0] === 5) {
			const _from = $bj[1];
			const _to = $bj[2];
			$bk = shaped = true;
		} else {
			$bk = undefined;
		}
		$bk;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change2, wakes);
		}
		let $bA = null;
		if (self[0].v[4].length > 0) {
			let $bz = null;
			if (!(late)) {
				$bz = [ 0 ];
			} else if (!(several)) {
				$bz = [ 1 ];
			} else if (index2 === 0) {
				$bz = [ 2 ];
			} else {
				$bz = [ 3 ];
			}
			const stage = $bz;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$bA = undefined;
		}
		$bA;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $bB = null;
	if (shaped || resized) {
		$bB = watch.v[2];
	} else {
		$bB = [ 1 ];
	}
	const shape = __clone($bB);
	let $bC = null;
	if (resized) {
		$bC = watch.v[3];
	} else {
		$bC = [ 1 ];
	}
	const sized = __clone($bC);
	let $bD = undefined;
	let $bE = undefined;
	if (wakes.length === 0 && (($bD = shape), $bD[0] === 1) && (($bE = sized), $bE[0] === 1)) {
		notify2(published, $bi);
		return;
	}
	settle_together(($bL) => {
		for (const wake2 of wakes) {
			const $bM = wake2;
			let $bN = null;
			if ($bM[0] === 0) {
				const slot5 = $bM[1];
				$bN = wake_all(watch, slot5, [ 0, $bL ]);
			} else if ($bM[0] === 2) {
				const slot6 = $bM[1];
				$bN = wake_all(watch, slot6, [ 0, $bL ]);
			} else {
				const slot7 = $bM[1];
				const part = $bM[2];
				$bN = wake_part(watch, slot7, part, [ 0, $bL ]);
			}
			$bN;
		}
		const $bS = sized;
		if ($bS[0] === 0) {
			notify2($bS[1], [ 0, $bL ]);
		}
		const $bT = shape;
		if ($bT[0] === 0) {
			notify2($bT[1], [ 0, $bL ]);
		}
		notify2(published, [ 0, $bL ]);
		return;
	}, $bi);
}
function landed_one(self, change2, $bh) {
	landed(self, [ __clone(change2) ], false, $bh);
}
function splice2(self, at3, removed, inserted, $aY) {
	const held3 = size3(self);
	let $bd = null;
	if (at3 > held3) {
		$bd = held3;
	} else {
		$bd = at3;
	}
	const start7 = $bd;
	let $be = null;
	if (start7 + removed > held3) {
		$be = held3 - start7;
	} else {
		$be = removed;
	}
	const taking = $be;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start7, taking, arriving ];
	coming_one(self, change2);
	mutate(self, (held4) => {
		return splice(held4, start7, taking, inserted);
	});
	landed_one(self, change2, $aY);
}
function remove_at(self, at3, $aX) {
	return splice2(self, at3, 1, [  ], $aX);
}
function eq5(self, b) {
	const $cg = self;
	let $cj = null;
	if ($cg[0] === 0) {
		const $ch = b;
		let $ci = null;
		if ($ch[0] === 0) {
			$ci = eq4($cg[1], $ch[1]);
		} else {
			$ci = false;
		}
		$cj = $ci;
	} else {
		const $ck = b;
		$cj = $ck[0] === 1;
	}
	return $cj;
}
function has(self, at3) {
	const holds2 = self[0].v[1][0];
	return peek(self, (held3) => {
		return holds2(held3, at3);
	});
}
function landed2(self, changes, late, $bi) {
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
		const $cp = change2;
		let $cq = null;
		if ($cp[0] === 0) {
			const _at = $cp[1];
			const removed = $cp[2];
			const inserted = $cp[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$cq = undefined;
		} else if ($cp[0] === 1) {
			const _at2 = $cp[1];
			const _count = $cp[2];
			$cq = resized = true;
		} else if ($cp[0] === 3) {
			const _at3 = $cp[1];
			const _count2 = $cp[2];
			$cq = resized = true;
		} else if ($cp[0] === 6) {
			$cq = resized = true;
		} else if ($cp[0] === 5) {
			const _from = $cp[1];
			const _to = $cp[2];
			$cq = shaped = true;
		} else {
			$cq = undefined;
		}
		$cq;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change2, wakes);
		}
		let $cs = null;
		if (self[0].v[4].length > 0) {
			let $cr = null;
			if (!(late)) {
				$cr = [ 0 ];
			} else if (!(several)) {
				$cr = [ 1 ];
			} else if (index2 === 0) {
				$cr = [ 2 ];
			} else {
				$cr = [ 3 ];
			}
			const stage = $cr;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$cs = undefined;
		}
		$cs;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $ct = null;
	if (shaped || resized) {
		$ct = watch.v[2];
	} else {
		$ct = [ 1 ];
	}
	const shape = __clone($ct);
	let $cu = null;
	if (resized) {
		$cu = watch.v[3];
	} else {
		$cu = [ 1 ];
	}
	const sized = __clone($cu);
	let $cv = undefined;
	let $cw = undefined;
	if (wakes.length === 0 && (($cv = shape), $cv[0] === 1) && (($cw = sized), $cw[0] === 1)) {
		notify2(published, $bi);
		return;
	}
	settle_together(($bL) => {
		for (const wake2 of wakes) {
			const $cx = wake2;
			let $cy = null;
			if ($cx[0] === 0) {
				const slot5 = $cx[1];
				$cy = wake_all(watch, slot5, [ 0, $bL ]);
			} else if ($cx[0] === 2) {
				const slot6 = $cx[1];
				$cy = wake_all(watch, slot6, [ 0, $bL ]);
			} else {
				const slot7 = $cx[1];
				const part = $cx[2];
				$cy = wake_part(watch, slot7, part, [ 0, $bL ]);
			}
			$cy;
		}
		const $cz = sized;
		if ($cz[0] === 0) {
			notify2($cz[1], [ 0, $bL ]);
		}
		const $cA = shape;
		if ($cA[0] === 0) {
			notify2($cA[1], [ 0, $bL ]);
		}
		notify2(published, [ 0, $bL ]);
		return;
	}, $bi);
}
function landed_one2(self, change2, $bh) {
	landed2(self, [ __clone(change2) ], false, $bh);
}
function replace(self, at3, value, $cl) {
	const watch = self[0].v[3];
	let $cm = null;
	if (has(self, at3)) {
		$cm = [ 2, __clone(at3) ];
	} else if (viewing2(watch, (held3) => {
		return follows_items(held3[0]);
	})) {
		return false;
	} else {
		$cm = [ 1, __clone(at3), 1 ];
	}
	const change2 = $cm;
	coming_one(self, change2);
	const write = self[0].v[1][3];
	mutate(self, (held3) => {
		return write(held3, at3, value);
	});
	landed_one2(self, change2, $cl);
	return true;
}
function shift(self, from, to) {
	const item2 = __remove_at(self, from, "std/src/reactive/lens.vl:2204:19");
	__insert_at(self, to, item2, "std/src/reactive/lens.vl:2205:8");
}
function move_item(self, from, to, $cE) {
	const held3 = size(self);
	if (from >= held3 || from === to) {
		return;
	}
	let $cF = null;
	if (to >= held3) {
		$cF = held3 - 1;
	} else {
		$cF = to;
	}
	const landing2 = $cF;
	const change2 = [ 5, from, landing2 ];
	coming_one(self, change2);
	mutate(self, (held4) => {
		return shift(held4, from, landing2, $cE);
	});
	landed_one2(self, change2, $cE);
}
function insert_at(self, at3, value, $cG) {
	return splice2(self, at3, 0, [ __clone(value) ], $cG);
}
function reconcile_by_key(self, next, key, $aQ) {
	const lens2 = __clone(self);
	batch(($aR) => {
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
			if (!(contains_key(wanted, __at(keys2, at3, "std/src/reactive/lens.vl:1864:29")))) {
				remove_at(lens2, at3, [ 0, $aR ]);
				__remove_at(keys2, at3, "std/src/reactive/lens.vl:1866:11");
			}
		}
		let index2 = 0;
		for (const item4 of next) {
			const want = key(item4);
			let $cB = null;
			if (index2 < keys2.length && __at(keys2, index2, "std/src/reactive/lens.vl:1872:30") === want) {
				if (!(eq5(peek(lens2, (list) => {
					return item(list, index2);
				}), [ 0, __clone(item4) ]))) {
					replace(lens2, index2, item4, [ 0, $aR ]);
				}
				$cB = undefined;
			} else {
				let found = [ 1 ];
				let search = index2 + 1;
				while (search < keys2.length) {
					if (__at(keys2, search, "std/src/reactive/lens.vl:1880:10") === want) {
						found = [ 0, search ];
						search = keys2.length;
					}
					search = search + 1;
				}
				const $cC = found;
				let $cD = null;
				if ($cC[0] === 0) {
					const from = $cC[1];
					move_item(lens2, from, index2, [ 0, $aR ]);
					const moved = __remove_at(keys2, from, "std/src/reactive/lens.vl:1889:25");
					__insert_at(keys2, index2, moved, "std/src/reactive/lens.vl:1890:13");
					if (!(eq5(peek(lens2, (list) => {
						return item(list, index2);
					}), [ 0, __clone(item4) ]))) {
						replace(lens2, index2, item4, [ 0, $aR ]);
					}
					$cD = undefined;
				} else {
					insert_at(lens2, index2, item4, [ 0, $aR ]);
					__insert_at(keys2, index2, want, "std/src/reactive/lens.vl:1897:13");
					$cD = undefined;
				}
				$cB = $cD;
			}
			$cB;
			index2 = index2 + 1;
		}
		return;
	}, $aQ);
}
function failed(self, error) {
	return [ 3, __clone(error), latest(self) ];
}
function settle(self, token, reply, $aM) {
	if (token === 0 || self[6].v[1] !== token) {
		return false;
	}
	self[6].v[1] = 0;
	const feed8 = __clone(self);
	const $aN = reply;
	let $aO = null;
	if ($aN[0] === 0) {
		const list = $aN[1];
		$aO = batch(($aP) => {
			reconcile_by_key(lens(feed8[0]), list, feed8[2], [ 0, $aP ]);
			drop_pages(feed8, [ 0, $aP ]);
			set_phase(feed8[3], [ 1, token ], [ 0, $aP ]);
			return;
		}, $aM);
	} else {
		const error = $aN[1];
		$aO = set_phase(self[3], failed(get2(self[3]), error), $aM);
	}
	$aO;
	return true;
}
function follow3(self, at3) {
	const core = self[0];
	return slot_of(self, follow_id(core.v[0], core.v[3], core.v[1][0], at3));
}
function release5(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot(self, at3, $cI) {
	const handle = follow3(self, at3);
	defer_to_owner(() => {
		return release5(handle);
	}, $cI);
	return handle;
}
function slot2(self, at3, $cH) {
	return slot(self[0], at3, $cH);
}
function overlay(self) {
	return __clone(self[1]);
}
function slot_cell(watch, slot5) {
	if (slot5 < 0) {
		return new8(0, "std/src/reactive/lens.vl:1079:19");
	}
	const entry = as_usize(slot5);
	const $cP = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1082:5")[0];
	const cell4 = __clone($cP[1]);
	if ($cP[0] === 0) {
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
	const $cY = at(self);
	if ($cY[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held3) => {
		return read(held3, $cY[1]);
	});
}
function track(self, $cO) {
	const scope = $cO;
	const identity5 = identity2(self);
	if (admits(scope, identity5)) {
		lists_of(scope[0]).v[0].push([ identity5, (subscriber) => {
			return on_settle2(self, subscriber);
		} ]);
	}
	return get4(self);
}
function row(self, key) {
	const $db = get(self[3].v, key);
	if ($db[0] === 0) {
		return $db[1];
	}
	const made = [ new8(0, "std/src/reactive/lens/loaded.vl:141:44"), new8(false, "std/src/reactive/lens/loaded.vl:141:74") ];
	insert(self[3].v, key, made);
	return made;
}
function track2(self, $cO) {
	const scope = $cO;
	const identity5 = identity(self);
	if (admits(scope, identity5)) {
		lists_of(scope[0]).v[0].push([ identity5, (subscriber) => {
			return on_settle(self, subscriber);
		} ]);
	}
	return get2(self);
}
function showing(self, key) {
	const $dc = get(self[0].v, key);
	if ($dc[0] === 0) {

	} else {
		return [ 1 ];
	}
	let out = [ 1 ];
	for (const proposal of $dc[1][0]) {
		if (proposal[0] > $dc[1][1]) {
			out = [ 0, __clone(proposal) ];
		}
	}
	return out;
}
function provisional(self, key) {
	const $di = get(self[0].v, key);
	if ($di[0] === 0) {

	} else {
		return false;
	}
	let out = false;
	for (const proposal of $di[1][0]) {
		if (proposal[2]) {
			out = true;
		}
	}
	return out;
}
function over3(self, key, held3) {
	const $dd = showing(self, key);
	let $de = null;
	if ($dd[0] === 1) {
		$de = __clone(held3);
	} else {
		const proposal = $dd[1];
		const $df = proposal[1];
		let $dg = null;
		if ($df[0] === 1) {
			$dg = [ 1 ];
		} else {
			const value = $df[1];
			const $dh = held3;
			let $dj = null;
			if ($dh[0] === 0 || provisional(self, key)) {
				$dj = [ 0, __clone(value) ];
			} else {
				$dj = [ 1 ];
			}
			$dg = $dj;
		}
		$de = $dg;
	}
	return $de;
}
function derive(body, caller) {
	return [ body, caller ];
}
function distinct(self, caller) {
	return [ self, caller ];
}
function is_none(self) {
	const $dB = self;
	return $dB[0] === 1;
}
function run_body(runs2, body) {
	const run = renew(runs2);
	const $dK = nursery(run);
	let $dL = null;
	if ($dK[0] === 0) {
		const nursery2 = $dK[1];
		$dL = (($dM) => {
			return (($dN) => {
				return body($dM, $dN);
			})(nursery2);
		})(run);
	} else {
		$dL = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $dL;
}
function run_once(runs2, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body(runs2, ($dt, $du) => {
		return (($dv) => {
			return body($dt, $dv, $du);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs2, tracker, body) {
	let value = run_once(runs2, tracker, ($do, $dp, $dq) => {
		return body($do, $dp, $dq);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs2, tracker, ($el, $em, $en) => {
			return body($el, $em, $en);
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
function set3(self, value, $ah, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $ah);
}
function take(self, item2, $eI) {
	defer(self, () => {
		dispose(item2, $eI);
		return;
	});
	return __clone(item2);
}
function cell(self, $dm, $dn, caller) {
	const instance = start2(self);
	const pull = instance[0];
	const cached = new8(pull(), caller);
	const refresh = subscriber_of(() => {
		set3(cached, pull(), $dm, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed2 = instance[1](refresh);
	also_releasing(refreshed2, instance[2]);
	register_with_owner(refreshed2, $dm, $dn);
	return cached;
}
function memo(self, $dk, $dl, caller) {
	return [ cell(self, $dk, $dl, caller) ];
}
function view(self, slot5, $cJ, $cK) {
	const ledger = __clone(self[2]);
	const key = self[1];
	return memo(distinct(derive(($cL, $cM, $cN) => {
		const $cZ = track(slot5, $cM);
		let $da = null;
		if ($cZ[0] === 0) {
			const item2 = $cZ[1];
			const id = key(item2);
			track2(row(ledger, id)[0], $cM);
			$da = over3(ledger, id, [ 0, item2 ]);
		} else {
			$da = [ 1 ];
		}
		return $da;
	}, "std/src/reactive/lens/loaded.vl:507:3"), "std/src/reactive/lens/loaded.vl:516:6"), $cJ, $cK, "std/src/reactive/lens/loaded.vl:516:17");
}
function observe(signal, observer) {
	const cell4 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $eO = [ 0, cell4 ];
		let $eP = null;
		if ($eO[0] === 0) {
			const live = $eO[1];
			$eP = observer(live.v);
		} else {
			$eP = undefined;
		}
		return $eP;
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
	return size(self[0]);
}
function sized_cell(self) {
	const watch = self[0].v[3];
	const $eU = watch.v[3];
	const cell4 = __clone($eU[1]);
	if ($eU[0] === 0) {
		return cell4;
	}
	const cell5 = new8(0, "std/src/reactive/lens.vl:1016:26");
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
	const $eV = self;
	let $eW = null;
	if ($eV[0] === 0) {
		const x = $eV[1];
		$eW = [ 0, fn(x) ];
	} else {
		$eW = [ 1 ];
	}
	return $eW;
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
	const $eX = key_of(self, slot5);
	let $eY = null;
	if ($eX[0] === 0) {
		const key = $eX[1];
		$eY = is_pending_key(self, key);
	} else {
		$eY = [ new8(false, "std/src/reactive/lens/loaded.vl:609:42") ];
	}
	return $eY;
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
function open2(self, key, value, insert3, above, below, $ff) {
	const id = self[6].v + 1;
	self[6].v = id;
	const ledger = __clone(self);
	batch(($fg) => {
		const cells = row(ledger, key);
		const found = get(ledger[0].v, key);
		const $fh = found;
		let $fi = null;
		if ($fh[0] === 0) {
			const held3 = $fh[1];
			$fi = held3;
		} else {
			ledger[1].v.push(__clone(key));
			set3(cells[1], true, [ 0, $fg ], "std/src/reactive/lens/loaded.vl:176:20");
			$fi = [ [  ], 0, __clone(above), __clone(below) ];
		}
		let entry = $fi;
		if (insert3) {
			entry[2] = __clone(above);
			entry[3] = __clone(below);
		}
		entry[0].push([ id, __clone(value), insert3 ]);
		insert(ledger[0].v, key, entry);
		insert(ledger[2].v, id, key);
		set3(ledger[4], get2(ledger[4]) + 1, [ 0, $fg ], "std/src/reactive/lens/loaded.vl:187:17");
		notify2(cells[0], [ 0, $fg ]);
		let $fp = undefined;
		if (insert3 || (($fp = value), $fp[0] === 1)) {
			notify2(ledger[5], [ 0, $fg ]);
		}
		return;
	}, $ff);
	return id;
}
function propose(self, slot5, value, $fd) {
	const $fe = key_of(self, slot5);
	if ($fe[0] === 0) {

	} else {
		return [ 1 ];
	}
	return [ 0, open2(self[2], $fe[1], [ 0, __clone(value) ], false, [ 1 ], [ 1 ], $fd) ];
}
function unwrap(self, caller) {
	const $fq = self;
	let $fr = null;
	if ($fq[0] === 0) {
		const x = $fq[1];
		$fr = x;
	} else {
		$fr = (() => {
			throw __panic("expected Some but got None", caller);
		})();
	}
	return $fr;
}
function pending(self) {
	return [ __clone(self[2][4]) ];
}
function set_edge(cell4, next, $fw) {
	const $fx = [ get2(cell4), __clone(next) ];
	let $fy = null;
	if ($fx[0][0] === 0 && $fx[1][0] === 0) {
		$fy = true;
	} else if ($fx[0][0] === 1 && $fx[1][0] === 1) {
		$fy = true;
	} else if ($fx[0][0] === 2 && $fx[1][0] === 2) {
		$fy = true;
	} else {
		$fy = false;
	}
	const same = $fy;
	if (!(same)) {
		set(cell4, next, $fw, "std/src/reactive/lens/loaded.vl:729:8");
	}
}
function request(self, older3, $ft) {
	let $fu = null;
	if (older3) {
		$fu = self[4];
	} else {
		$fu = self[5];
	}
	const edge2 = __clone($fu);
	const $fv = get2(edge2);
	if ($fv[0] === 2) {
		return [ 1 ];
	}
	const token = claim(self);
	if (older3) {
		self[6].v[2] = token;
	} else {
		self[6].v[3] = token;
	}
	set_edge(edge2, [ 1 ], $ft);
	return [ 0, token ];
}
function request_older(self, $fs) {
	return request(self, true, $fs);
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
	const $fM = self;
	let $fN = null;
	if ($fM[0] === 0) {
		const x = $fM[1];
		$fN = fn(x);
	} else {
		$fN = [ 1 ];
	}
	return $fN;
}
function peek2(self, read) {
	return peek(self[0], read);
}
function replace2(self, at3, value, $fQ) {
	return replace(self[0], at3, value, $fQ);
}
function splice3(self, at3, removed, inserted, $aY) {
	const held3 = size(self);
	let $fT = null;
	if (at3 > held3) {
		$fT = held3;
	} else {
		$fT = at3;
	}
	const start7 = $fT;
	let $fU = null;
	if (start7 + removed > held3) {
		$fU = held3 - start7;
	} else {
		$fU = removed;
	}
	const taking = $fU;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start7, taking, arriving ];
	coming_one(self, change2);
	mutate(self, (held4) => {
		return splice(held4, start7, taking, inserted);
	});
	landed_one2(self, change2, $aY);
}
function splice4(self, at3, removed, inserted, $fS) {
	splice3(self[0], at3, removed, inserted, $fS);
}
function push(self, value, $fR) {
	return splice4(self, size2(self), 0, [ __clone(value) ], $fR);
}
function receive(self, item2, $fL) {
	const id = self[2](item2);
	const $fO = and_then(held2(self[1], id), (slot5) => {
		return at(slot5);
	});
	let $fP = null;
	if ($fO[0] === 0) {
		const at3 = $fO[1];
		if (!(eq5(peek2(self[0], (list) => {
			return __list_get(list, at3);
		}), [ 0, __clone(item2) ]))) {
			replace2(self[0], at3, item2, $fL);
		}
		$fP = undefined;
	} else {
		$fP = push(self[0], item2, $fL);
	}
	return $fP;
}
function settle_page(self, older3, token, reply, $fC) {
	let $fD = null;
	if (older3) {
		$fD = self[6].v[2];
	} else {
		$fD = self[6].v[3];
	}
	const mine2 = $fD;
	if (token === 0 || mine2 !== token) {
		return false;
	}
	if (older3) {
		self[6].v[2] = 0;
	} else {
		self[6].v[3] = 0;
	}
	let $fE = null;
	if (older3) {
		$fE = self[4];
	} else {
		$fE = self[5];
	}
	const edge2 = __clone($fE);
	const feed8 = __clone(self);
	const $fF = reply;
	let $fG = null;
	if ($fF[0] === 0) {
		const page3 = $fF[1];
		$fG = batch(($fH) => {
			const key = feed8[2];
			let taken = new10();
			let fresh2 = [  ];
			for (const item2 of page3[0]) {
				const id = key(item2);
				const $fK = held2(feed8[1], id);
				if ($fK[0] === 0) {
					receive(feed8, item2, [ 0, $fH ]);
				} else if (!(contains_key(taken, id))) {
					insert(taken, id, true);
					fresh2.push(__clone(item2));
				}
			}
			let $fV = null;
			if (older3) {
				$fV = 0;
			} else {
				$fV = size2(feed8[0]);
			}
			const at3 = $fV;
			splice4(feed8[0], at3, 0, fresh2, [ 0, $fH ]);
			let $fW = null;
			if (page3[1]) {
				$fW = [ 0 ];
			} else {
				$fW = [ 2 ];
			}
			set_edge(edge2, $fW, [ 0, $fH ]);
			return;
		}, $fC);
	} else {
		const error = $fF[1];
		$fG = set(edge2, [ 3, __clone(error) ], $fC, "std/src/reactive/lens/loaded.vl:949:27");
	}
	$fG;
	return true;
}
function prepend_page(self, token, reply, $fB) {
	return settle_page(self, true, token, reply, $fB);
}
function all2(self) {
	return all(self[0]);
}
function eq6(self, b) {
	const $fX = self;
	let $ga = null;
	if ($fX[0] === 0) {
		const $fY = b;
		let $fZ = null;
		if ($fY[0] === 0) {
			$fZ = eq2($fX[1], $fY[1]);
		} else {
			$fZ = false;
		}
		$ga = $fZ;
	} else {
		const $gb = b;
		$ga = $gb[0] === 1;
	}
	return $ga;
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
	const $gk = get(self[0].v, key);
	let $gl = null;
	if ($gk[0] === 0) {
		const entry = $gk[1];
		$gl = [ __clone(entry[2]), __clone(entry[3]) ];
	} else {
		$gl = [ [ 1 ], [ 1 ] ];
	}
	return $gl;
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
	const $gn = above;
	let $go = undefined;
	if ($gn[0] === 0 && (($go = position_of(rows5, $gn[1])), $go[0] === 0)) {
		return $go[1] + 1;
	}
	const $gp = below;
	let $gq = undefined;
	if ($gp[0] === 0 && (($gq = position_of(rows5, $gp[1])), $gq[0] === 0)) {
		return $gq[1];
	}
	const $gr = above;
	if ($gr[0] === 0) {
		return rows5.length;
	}
	return 0;
}
function hides(self, key) {
	const $gs = showing(self, key);
	if ($gs[0] === 0) {
		const $gt = $gs[1][1];
		return $gt[0] === 1;
	}
	return false;
}
function compose(self, with_hidden) {
	const key = self[1];
	let all3 = [  ];
	for (const slot5 of get6(self[4])) {
		const $gh = get4(slot5);
		if ($gh[0] === 0) {
			all3.push([ key($gh[1]), [ 0, __clone(slot5) ] ]);
		}
	}
	for (const row_key of keys(self[2])) {
		let $gi = undefined;
		let $gj = undefined;
		if (provisional(self[2], row_key) && (($gi = find(self[3], row_key)), $gi[0] === 1) && (($gj = over3(self[2], row_key, [ 1 ])), $gj[0] === 0)) {
			const $gm = anchors(self[2], row_key);
			const above = $gm[0];
			const below = $gm[1];
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
		const $gu = row6[1];
		if ($gu[0] === 0) {
			at3 = at3 + 1;
		}
	}
	return at3;
}
function close2(self, id, accepted, $gv) {
	const $gw = get(self[2].v, id);
	let $gx = null;
	if ($gw[0] === 0) {
		const held3 = $gw[1];
		$gx = held3;
	} else {
		return [ 1 ];
	}
	const key = $gx;
	const found = get(self[0].v, key);
	const $gy = found;
	if ($gy[0] === 0) {

	} else {
		return [ 1 ];
	}
	let entry = __clone($gy[1]);
	let ended = [ 1 ];
	let kept = [  ];
	for (const proposal of entry[0]) {
		if (proposal[0] === id) {
			ended = [ 0, __clone(proposal) ];
		} else {
			kept.push(__clone(proposal));
		}
	}
	const $gz = ended;
	if ($gz[0] === 0) {

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
	batch(($gA) => {
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
			set3(cells[1], false, [ 0, $gA ], "std/src/reactive/lens/loaded.vl:235:19");
		} else {
			insert(ledger[0].v, key, entry);
		}
		set3(ledger[4], get2(ledger[4]) - 1, [ 0, $gA ], "std/src/reactive/lens/loaded.vl:239:17");
		notify2(cells[0], [ 0, $gA ]);
		let $gB = undefined;
		if ($gz[1][2] || (($gB = $gz[1][1]), $gB[0] === 1)) {
			notify2(ledger[5], [ 0, $gA ]);
		}
		return;
	}, $gv);
	return [ 0, [ __clone(key), __clone($gz[1][1]), $gz[1][2], lands ] ];
}
function remove_at2(self, at3, $aX) {
	return splice4(self, at3, 1, [  ], $aX);
}
function insert_at2(self, at3, value, $cG) {
	return splice4(self, at3, 0, [ __clone(value) ], $cG);
}
function answer(self, id, accepted, reply, $gd) {
	const overlay3 = __clone(self);
	let answered = false;
	batch(($ge) => {
		const $gf = get(overlay3[2][2].v, id);
		let $gg = null;
		if ($gf[0] === 0) {
			const key = $gf[1];
			$gg = landing(overlay3, key);
		} else {
			$gg = 0;
		}
		const landing2 = $gg;
		const $gC = close2(overlay3[2], id, accepted, [ 0, $ge ]);
		if ($gC[0] === 0) {

		} else {
			return;
		}
		answered = true;
		if (!($gC[1][3])) {
			return;
		}
		const $gD = reply;
		let $gE = null;
		if ($gD[0] === 0) {
			const item2 = $gD[1];
			$gE = [ 0, __clone(item2) ];
		} else {
			$gE = $gC[1][1];
		}
		const landed4 = __clone($gE);
		const lens2 = __clone(overlay3[0]);
		const $gF = landed4;
		let $gG = null;
		if ($gF[0] === 1) {
			const $gH = held2(overlay3, $gC[1][0]);
			let $gI = undefined;
			if ($gH[0] === 0 && (($gI = at($gH[1])), $gI[0] === 0)) {
				remove_at2(lens2, $gI[1], [ 0, $ge ]);
			}
			$gG = undefined;
		} else {
			const item3 = $gF[1];
			const server_key = overlay3[1](item3);
			const $gJ = held2(overlay3, $gC[1][0]);
			let $gK = null;
			if ($gJ[0] === 0) {
				$gK = at($gJ[1]);
			} else {
				$gK = and_then(held2(overlay3, server_key), (slot5) => {
					return at(slot5);
				});
			}
			const target = $gK;
			const $gL = target;
			let $gM = null;
			if ($gL[0] === 0) {
				const at3 = $gL[1];
				if (!(eq5(peek2(lens2, (list) => {
					return __list_get(list, at3);
				}), [ 0, __clone(item3) ]))) {
					replace2(lens2, at3, item3, [ 0, $ge ]);
				}
				$gM = undefined;
			} else {
				if ($gC[1][2]) {
					insert_at2(lens2, landing2, item3, [ 0, $ge ]);
				}
				$gM = undefined;
			}
			$gG = $gM;
		}
		return $gG;
	}, $gd);
	return answered;
}
function confirm(self, id, $gc) {
	return answer(self, id, true, [ 1 ], $gc);
}
function resync(self, list, $gN) {
	const token = load(self, $gN);
	settle(self, token, [ 0, __clone(list) ], $gN);
}
function identity3(self) {
	return [ 1 ];
}
function shape_cell(self) {
	const watch = self[0].v[3];
	const $hd = watch.v[2];
	const cell4 = __clone($hd[1]);
	if ($hd[0] === 0) {
		return cell4;
	}
	const cell5 = new8(0, "std/src/reactive/lens.vl:1028:26");
	watch.v[2] = [ 0, __clone(cell5) ];
	return cell5;
}
function on_settle4(self, subscriber) {
	return on_settle(shape_cell(self[0]), subscriber);
}
function track3(self, $cO) {
	const scope = $cO;
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
	const $he = nursery(run);
	let $hf = null;
	if ($he[0] === 0) {
		const nursery2 = $he[1];
		$hf = (($dM) => {
			return (($dN) => {
				return body($dM, $dN);
			})(nursery2);
		})(run);
	} else {
		$hf = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $hf;
}
function run_once2(runs2, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body2(runs2, ($dt, $du) => {
		return (($dv) => {
			return body($dt, $dv, $du);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked2(runs2, tracker, body) {
	let value = run_once2(runs2, tracker, ($do, $dp, $dq) => {
		return body($do, $dp, $dq);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once2(runs2, tracker, ($el, $em, $en) => {
			return body($el, $em, $en);
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
	const $hg = self;
	let $hj = null;
	if ($hg[0] === 0) {
		const $hh = b;
		let $hi = null;
		if ($hh[0] === 0) {
			$hi = eq7($hg[1], $hh[1]);
		} else {
			$hi = false;
		}
		$hj = $hi;
	} else {
		const $hk = b;
		$hj = $hk[0] === 1;
	}
	return $hj;
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
function cell2(self, $dm, $dn, caller) {
	const instance = start4(self);
	const pull = instance[0];
	const cached = new8(pull(), caller);
	const refresh = subscriber_of(() => {
		set3(cached, pull(), $dm, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed2 = instance[1](refresh);
	also_releasing(refreshed2, instance[2]);
	register_with_owner(refreshed2, $dm, $dn);
	return cached;
}
function memo2(self, $dk, $dl, caller) {
	return [ cell2(self, $dk, $dl, caller) ];
}
function rows3(self, $gY, $gZ) {
	const overlay3 = __clone(self);
	const feed8 = __clone(self[4]);
	const layout = __clone(self[2][5]);
	return memo2(distinct(derive(($ha, $hb, $hc) => {
		track3(feed8, $hb);
		track2(layout, $hb);
		return compose(overlay3, false);
	}, "std/src/reactive/lens/loaded.vl:544:3"), "std/src/reactive/lens/loaded.vl:548:6"), $gY, $gZ, "std/src/reactive/lens/loaded.vl:548:17");
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
function propose_delete(self, slot5, $hv) {
	const $hw = key_of(self, slot5);
	if ($hw[0] === 0) {

	} else {
		return [ 1 ];
	}
	return [ 0, open2(self[2], $hw[1], [ 1 ], false, [ 1 ], [ 1 ], $hv) ];
}
function shown2(self) {
	let out = [  ];
	for (const row6 of compose(self, false)) {
		const $hx = row6[1];
		let $hy = null;
		if ($hx[0] === 0) {
			const slot5 = $hx[1];
			$hy = get4(slot5);
		} else {
			$hy = [ 1 ];
		}
		const held3 = $hy;
		const $hz = over3(self[2], row6[0], held3);
		if ($hz[0] === 0) {
			out.push($hz[1]);
		}
	}
	return out;
}
function reject(self, id, $hA) {
	return answer(self, id, false, [ 1 ], $hA);
}
function shown_rows(self) {
	return compose(self, false);
}
function request_newer(self, $hK) {
	return request(self, false, $hK);
}
function append_page(self, token, reply, $hL) {
	return settle_page(self, false, token, reply, $hL);
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
	return [ __shared_new([ __clone(storage), __clone(access), new8(0, "std/src/reactive/lens.vl:726:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over4(value, handler, access) {
	return lens_over2([ 0, __shared_new(value) ], handler, access);
}
function keyed(value) {
	return over4(value, new7(), [ (held3, key) => {
		const $hN = find2(held3, key);
		return $hN[0] === 0;
	}, (held3, key) => {
		return find2(held3, key);
	}, (held3, key, read) => {
		const $hO = find2(held3, key);
		const item2 = [ $hO[1] ];
		if ($hO[0] === 0) {
			read([ item2, 0 ]);
		}
		return;
	}, (held3, key, value2) => {
		return store(held3, key, value2);
	}, (held3, key, with2) => {
		const $hP = find2(held3, key);
		if ($hP[0] === 0) {

		} else {
			return;
		}
		let item2 = [ $hP[1] ];
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
function new13($hM) {
	const empty = new10();
	const lens2 = over5(empty);
	const records = [ __clone(lens2), beside2(lens2), __shared_new(new2()), __shared_new(new2()), __shared_new(0) ];
	defer_to_owner(() => {
		return retire2(records);
	}, $hM);
	return records;
}
function overlay2(self) {
	return __clone(self[1]);
}
function follows_items2(self) {
	return false;
}
function follow4(self, at3) {
	const $hX = get7(self[0], at3);
	if ($hX[0] === 0) {
		return $hX[1];
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
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1058:2")[2] = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1058:2")[2] + 1;
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
	const $ia = __list_get(self[1], as_usize(slot5));
	if ($ia[0] === 0) {
		remove2(self[0], $ia[1]);
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
function slot3(self, at3, $cI) {
	const handle = follow5(self, at3);
	defer_to_owner(() => {
		return release7(handle);
	}, $cI);
	return handle;
}
function slot4(self, key, $hS) {
	return slot3(self[0], key, $hS);
}
function row2(self, key) {
	const $ie = get7(self[3].v, key);
	if ($ie[0] === 0) {
		return $ie[1];
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
	const $ig = at2(self);
	if ($ig[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held3) => {
		return read(held3, $ig[1]);
	});
}
function track4(self, $cO) {
	const scope = $cO;
	const identity5 = identity4(self);
	if (admits(scope, identity5)) {
		lists_of(scope[0]).v[0].push([ identity5, (subscriber) => {
			return on_settle5(self, subscriber);
		} ]);
	}
	return get8(self);
}
function showing2(self, key) {
	const $il = get7(self[0].v, key);
	if ($il[0] === 0) {

	} else {
		return [ 1 ];
	}
	let out = [ 1 ];
	for (const proposal of $il[1][0]) {
		if (proposal[0] > $il[1][1]) {
			out = [ 0, __clone(proposal) ];
		}
	}
	return out;
}
function provisional2(self, key) {
	const $ir = get7(self[0].v, key);
	if ($ir[0] === 0) {

	} else {
		return false;
	}
	let out = false;
	for (const proposal of $ir[1][0]) {
		if (proposal[2]) {
			out = true;
		}
	}
	return out;
}
function over6(self, key, held3) {
	const $im = showing2(self, key);
	let $in = null;
	if ($im[0] === 1) {
		$in = __clone(held3);
	} else {
		const proposal = $im[1];
		const $io = proposal[1];
		let $ip = null;
		if ($io[0] === 1) {
			$ip = [ 1 ];
		} else {
			const value = $io[1];
			const $iq = held3;
			let $is = null;
			if ($iq[0] === 0 || provisional2(self, key)) {
				$is = [ 0, __clone(value) ];
			} else {
				$is = [ 1 ];
			}
			$ip = $is;
		}
		$in = $ip;
	}
	return $in;
}
function run_once3(runs2, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body2(runs2, ($dt, $du) => {
		return (($dv) => {
			return body($dt, $dv, $du);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked3(runs2, tracker, body) {
	let value = run_once3(runs2, tracker, ($do, $dp, $dq) => {
		return body($do, $dp, $dq);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once3(runs2, tracker, ($el, $em, $en) => {
			return body($el, $em, $en);
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
	const $iv = self;
	let $iy = null;
	if ($iv[0] === 0) {
		const $iw = b;
		let $ix = null;
		if ($iw[0] === 0) {
			$ix = $iv[1] === $iw[1];
		} else {
			$ix = false;
		}
		$iy = $ix;
	} else {
		const $iz = b;
		$iy = $iz[0] === 1;
	}
	return $iy;
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
function cell3(self, $dm, $dn, caller) {
	const instance = start6(self);
	const pull = instance[0];
	const cached = new8(pull(), caller);
	const refresh = subscriber_of(() => {
		set3(cached, pull(), $dm, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed2 = instance[1](refresh);
	also_releasing(refreshed2, instance[2]);
	register_with_owner(refreshed2, $dm, $dn);
	return cached;
}
function memo3(self, $dk, $dl, caller) {
	return [ cell3(self, $dk, $dl, caller) ];
}
function view2(self, key, $hQ, $hR) {
	const ledger = __clone(self[1]);
	const slot5 = slot4(self[0], key, $hR);
	return memo3(distinct(derive(($ib, $ic, $id) => {
		track2(row2(ledger, key)[0], $ic);
		return over6(ledger, key, track4(slot5, $ic));
	}, "std/src/reactive/lens/loaded.vl:1150:3"), "std/src/reactive/lens/loaded.vl:1153:6"), $hQ, $hR, "std/src/reactive/lens/loaded.vl:1153:17");
}
function on_change6(self, observer, caller) {
	return place_subscription(attach_observer7(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function phase_cell(self, key) {
	const $iL = get7(self[2].v, key);
	if ($iL[0] === 0) {
		return $iL[1];
	}
	const made = new8([ 0 ], "std/src/reactive/lens/loaded.vl:1214:62");
	insert2(self[2].v, key, made);
	return made;
}
function load2(self, key, $iK) {
	const token = self[4].v + 1;
	self[4].v = token;
	insert2(self[3].v, key, token);
	const phase3 = phase_cell(self, key);
	set_phase(phase3, refreshed(get2(phase3)), $iK);
	return token;
}
function phase2(self, key) {
	return [ phase_cell(self, key) ];
}
function peek3(self, read) {
	return peek(self[0], read);
}
function apply3(self, change2, wakes) {
	const $jb = change2;
	let $jc = null;
	if ($jb[0] === 0) {
		const key = $jb[1];
		const _removed = $jb[2];
		const _inserted = $jb[3];
		const $jd = get7(self[0], key);
		if ($jd[0] === 0) {
			wakes.push([ 0, $jd[1] ]);
		}
		$jc = undefined;
	} else if ($jb[0] === 1) {
		const key2 = $jb[1];
		const _count = $jb[2];
		const $je = get7(self[0], key2);
		if ($je[0] === 0) {
			wakes.push([ 0, $je[1] ]);
		}
		$jc = undefined;
	} else if ($jb[0] === 2) {
		const key3 = $jb[1];
		const $jf = get7(self[0], key3);
		if ($jf[0] === 0) {
			wakes.push([ 0, $jf[1] ]);
		}
		$jc = undefined;
	} else if ($jb[0] === 3) {
		const key4 = $jb[1];
		const _count2 = $jb[2];
		const $jg = get7(self[0], key4);
		if ($jg[0] === 0) {
			wakes.push([ 0, $jg[1] ]);
		}
		$jc = undefined;
	} else if ($jb[0] === 4) {
		const key5 = $jb[1];
		const part = $jb[2];
		const $jh = get7(self[0], key5);
		if ($jh[0] === 0) {
			wakes.push([ 1, $jh[1], part ]);
		}
		$jc = undefined;
	} else if ($jb[0] === 5) {
		const _from = $jb[1];
		const _to = $jb[2];
		$jc = undefined;
	} else {
		let slot5 = 0;
		const held3 = as_i32(self[1].length);
		while (slot5 < held3) {
			wakes.push([ 0, slot5 ]);
			slot5 = slot5 + 1;
		}
		$jc = undefined;
	}
	return $jc;
}
function landed3(self, changes, late, $bi) {
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
		const $iZ = change2;
		let $ja = null;
		if ($iZ[0] === 0) {
			const _at = $iZ[1];
			const removed = $iZ[2];
			const inserted = $iZ[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$ja = undefined;
		} else if ($iZ[0] === 1) {
			const _at2 = $iZ[1];
			const _count = $iZ[2];
			$ja = resized = true;
		} else if ($iZ[0] === 3) {
			const _at3 = $iZ[1];
			const _count2 = $iZ[2];
			$ja = resized = true;
		} else if ($iZ[0] === 6) {
			$ja = resized = true;
		} else if ($iZ[0] === 5) {
			const _from = $iZ[1];
			const _to = $iZ[2];
			$ja = shaped = true;
		} else {
			$ja = undefined;
		}
		$ja;
		if (watch.v[1].length > 0) {
			apply3(watch.v[0], change2, wakes);
		}
		let $jj = null;
		if (self[0].v[4].length > 0) {
			let $ji = null;
			if (!(late)) {
				$ji = [ 0 ];
			} else if (!(several)) {
				$ji = [ 1 ];
			} else if (index2 === 0) {
				$ji = [ 2 ];
			} else {
				$ji = [ 3 ];
			}
			const stage = $ji;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$jj = undefined;
		}
		$jj;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $jk = null;
	if (shaped || resized) {
		$jk = watch.v[2];
	} else {
		$jk = [ 1 ];
	}
	const shape = __clone($jk);
	let $jl = null;
	if (resized) {
		$jl = watch.v[3];
	} else {
		$jl = [ 1 ];
	}
	const sized = __clone($jl);
	let $jm = undefined;
	let $jn = undefined;
	if (wakes.length === 0 && (($jm = shape), $jm[0] === 1) && (($jn = sized), $jn[0] === 1)) {
		notify2(published, $bi);
		return;
	}
	settle_together(($bL) => {
		for (const wake2 of wakes) {
			const $jo = wake2;
			let $jp = null;
			if ($jo[0] === 0) {
				const slot5 = $jo[1];
				$jp = wake_all(watch, slot5, [ 0, $bL ]);
			} else if ($jo[0] === 2) {
				const slot6 = $jo[1];
				$jp = wake_all(watch, slot6, [ 0, $bL ]);
			} else {
				const slot7 = $jo[1];
				const part = $jo[2];
				$jp = wake_part(watch, slot7, part, [ 0, $bL ]);
			}
			$jp;
		}
		const $js = sized;
		if ($js[0] === 0) {
			notify2($js[1], [ 0, $bL ]);
		}
		const $jt = shape;
		if ($jt[0] === 0) {
			notify2($jt[1], [ 0, $bL ]);
		}
		notify2(published, [ 0, $bL ]);
		return;
	}, $bi);
}
function landed_one3(self, change2, $bh) {
	landed3(self, [ __clone(change2) ], false, $bh);
}
function put2(self, key, value, $iU) {
	const had = peek(self, (held3) => {
		const $iV = find2(held3, key);
		return $iV[0] === 0;
	});
	let $iW = null;
	if (had) {
		$iW = [ 2, __clone(key) ];
	} else {
		$iW = [ 1, __clone(key), 1 ];
	}
	const change2 = $iW;
	coming_one(self, change2);
	mutate(self, (held3) => {
		return store(held3, key, value);
	});
	landed_one3(self, change2, $iU);
}
function put3(self, key, value, $iT) {
	return put2(self[0], key, value, $iT);
}
function drop(self, key) {
	remove2(self, key);
}
function remove3(self, key, $jv) {
	const had = peek(self, (held3) => {
		const $jw = find2(held3, key);
		return $jw[0] === 0;
	});
	if (!(had)) {
		return;
	}
	coming_one(self, [ 3, __clone(key), 1 ]);
	mutate(self, (held3) => {
		return drop(held3, key);
	});
	landed_one3(self, [ 3, __clone(key), 1 ], $jv);
}
function remove4(self, key, $ju) {
	return remove3(self[0], key, $ju);
}
function receive2(self, key, value, $iQ) {
	const held3 = peek3(self[0], (map2) => {
		return get7(map2, key);
	});
	if (eq3(held3, value)) {
		return;
	}
	const $iR = value;
	let $iS = null;
	if ($iR[0] === 0) {
		const record2 = $iR[1];
		$iS = put3(self[0], key, record2, $iQ);
	} else {
		$iS = remove4(self[0], key, $iQ);
	}
	return $iS;
}
function settle2(self, key, token, reply, $iM) {
	if (!(eq3(get7(self[3].v, key), [ 0, token ]))) {
		return false;
	}
	remove2(self[3].v, key);
	const phase3 = phase_cell(self, key);
	const records = __clone(self);
	const $iN = reply;
	let $iO = null;
	if ($iN[0] === 0) {
		const found = $iN[1];
		$iO = batch(($iP) => {
			receive2(records, key, found, [ 0, $iP ]);
			const $jx = found;
			let $jy = null;
			if ($jx[0] === 0) {
				$jy = [ 1, token ];
			} else {
				$jy = [ 4 ];
			}
			set_phase(phase3, $jy, [ 0, $iP ]);
			return;
		}, $iM);
	} else {
		const error = $iN[1];
		$iO = set_phase(phase3, failed(get2(phase3), error), $iM);
	}
	$iO;
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
function open3(self, key, value, insert3, above, below, $ff) {
	const id = self[6].v + 1;
	self[6].v = id;
	const ledger = __clone(self);
	batch(($fg) => {
		const cells = row2(ledger, key);
		const found = get7(ledger[0].v, key);
		const $jA = found;
		let $jB = null;
		if ($jA[0] === 0) {
			const held3 = $jA[1];
			$jB = held3;
		} else {
			ledger[1].v.push(__clone(key));
			set3(cells[1], true, [ 0, $fg ], "std/src/reactive/lens/loaded.vl:176:20");
			$jB = [ [  ], 0, __clone(above), __clone(below) ];
		}
		let entry = $jB;
		if (insert3) {
			entry[2] = __clone(above);
			entry[3] = __clone(below);
		}
		entry[0].push([ id, __clone(value), insert3 ]);
		insert2(ledger[0].v, key, entry);
		insert(ledger[2].v, id, key);
		set3(ledger[4], get2(ledger[4]) + 1, [ 0, $fg ], "std/src/reactive/lens/loaded.vl:187:17");
		notify2(cells[0], [ 0, $fg ]);
		let $jC = undefined;
		if (insert3 || (($jC = value), $jC[0] === 1)) {
			notify2(ledger[5], [ 0, $fg ]);
		}
		return;
	}, $ff);
	return id;
}
function propose2(self, key, value, $jz) {
	return open3(self[1], key, [ 0, __clone(value) ], !(holds(self, key)), [ 1 ], [ 1 ], $jz);
}
function is_pending2(self, key) {
	return [ __clone(row2(self[1], key)[1]) ];
}
function push2(self, key, value, $jD) {
	const phase3 = phase_cell(self, key);
	const records = __clone(self);
	batch(($jE) => {
		receive2(records, key, value, [ 0, $jE ]);
		const $jF = value;
		let $jG = null;
		if ($jF[0] === 0) {
			$jG = [ 1, 0 ];
		} else {
			$jG = [ 4 ];
		}
		set_phase(phase3, $jG, [ 0, $jE ]);
		return;
	}, $jD);
}
function items2(self) {
	return __clone(self[0]);
}
function close3(self, id, accepted, $gv) {
	const $jK = get(self[2].v, id);
	let $jL = null;
	if ($jK[0] === 0) {
		const held3 = $jK[1];
		$jL = held3;
	} else {
		return [ 1 ];
	}
	const key = $jL;
	const found = get7(self[0].v, key);
	const $jM = found;
	if ($jM[0] === 0) {

	} else {
		return [ 1 ];
	}
	let entry = __clone($jM[1]);
	let ended = [ 1 ];
	let kept = [  ];
	for (const proposal of entry[0]) {
		if (proposal[0] === id) {
			ended = [ 0, __clone(proposal) ];
		} else {
			kept.push(__clone(proposal));
		}
	}
	const $jN = ended;
	if ($jN[0] === 0) {

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
	batch(($gA) => {
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
			set3(cells[1], false, [ 0, $gA ], "std/src/reactive/lens/loaded.vl:235:19");
		} else {
			insert2(ledger[0].v, key, entry);
		}
		set3(ledger[4], get2(ledger[4]) - 1, [ 0, $gA ], "std/src/reactive/lens/loaded.vl:239:17");
		notify2(cells[0], [ 0, $gA ]);
		let $jO = undefined;
		if ($jN[1][2] || (($jO = $jN[1][1]), $jO[0] === 1)) {
			notify2(ledger[5], [ 0, $gA ]);
		}
		return;
	}, $gv);
	return [ 0, [ __clone(key), __clone($jN[1][1]), $jN[1][2], lands ] ];
}
function answer2(self, id, accepted, reply, $jI) {
	const overlay3 = __clone(self);
	let answered = false;
	batch(($jJ) => {
		const $jP = close3(overlay3[1], id, accepted, [ 0, $jJ ]);
		if ($jP[0] === 0) {

		} else {
			return;
		}
		answered = true;
		if (!($jP[1][3])) {
			return;
		}
		const $jQ = reply;
		let $jR = null;
		if ($jQ[0] === 0) {
			const value = $jQ[1];
			$jR = [ 0, __clone(value) ];
		} else {
			$jR = $jP[1][1];
		}
		const landed4 = __clone($jR);
		const held3 = peek3(overlay3[0], (map2) => {
			return get7(map2, $jP[1][0]);
		});
		const $jS = landed4;
		let $jT = null;
		if ($jS[0] === 1) {
			const $jU = held3;
			if ($jU[0] === 0) {
				remove4(overlay3[0], $jP[1][0], [ 0, $jJ ]);
			}
			$jT = undefined;
		} else {
			const value2 = $jS[1];
			const $jV = held3;
			let $jW = null;
			if ($jV[0] === 0 || $jP[1][2]) {
				if (!(eq3(held3, [ 0, __clone(value2) ]))) {
					put3(overlay3[0], $jP[1][0], value2, [ 0, $jJ ]);
				}
				$jW = undefined;
			}
			$jT = $jW;
		}
		return $jT;
	}, $jI);
	return answered;
}
function reject2(self, id, $jH) {
	return answer2(self, id, false, [ 1 ], $jH);
}
function confirm2(self, id, $jZ) {
	return answer2(self, id, true, [ 1 ], $jZ);
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
function propose_delete2(self, key, $kc) {
	return open3(self[1], key, [ 1 ], false, [ 1 ], [ 1 ], $kc);
}
function disconnected(self, error, $kh) {
	retire(self, $kh);
	set_phase(self[3], failed(get2(self[3]), error), $kh);
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
on_change(__clone(view3), (_item, $eM) => {
	const $eN = [ view_runs, "v" ];
	return view_runs.v = $eN[0][$eN[1]] + 1;
}, "lens-loaded.vl:83:17");
on_change2(__clone(row3), (_item, $eQ) => {
	const $eR = [ slot_runs, "v" ];
	return slot_runs.v = $eR[0][$eR[1]] + 1;
}, "lens-loaded.vl:84:16");
on_change3(length2(items(feed)), (_count, $eS) => {
	const $eT = [ length_runs, "v" ];
	return length_runs.v = $eT[0][$eT[1]] + 1;
}, "lens-loaded.vl:85:34");
const flag = is_pending(overlay(feed), row3);
on_change4(__clone(flag), (_on, $eZ) => {
	const $fa = [ flag_runs, "v" ];
	return flag_runs.v = $fa[0][$fa[1]] + 1;
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
on_change2(__clone(one), (_item, $gO) => {
	const $gP = [ one_runs, "v" ];
	return one_runs.v = $gP[0][$gP[1]] + 1;
}, "lens-loaded.vl:141:16");
on_change2(__clone(three), (_item, $gQ) => {
	const $gR = [ three_runs, "v" ];
	return three_runs.v = $gR[0][$gR[1]] + 1;
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
on_change2(__clone(doomed), (_item, $gW) => {
	const $gX = [ doomed_runs, "v" ];
	return doomed_runs.v = $gX[0][$gX[1]] + 1;
}, "lens-loaded.vl:192:19");
const rows4 = rows3(overlay(feed3), [ 1 ], [ 1 ]);
const rows_runs = __shared_new(0);
on_change5(rows4, (_rows, $hr) => {
	const $hs = [ rows_runs, "v" ];
	return rows_runs.v = $hs[0][$hs[1]] + 1;
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
on_change(__clone(view4), (_item, $hB) => {
	const $hC = [ runs, "v" ];
	return runs.v = $hC[0][$hC[1]] + 1;
}, "lens-loaded.vl:290:17");
on_change(view(overlay(feed4), slot2(items(feed4), 0, [ 1 ]), [ 1 ], [ 1 ]), (_item, $hD) => {
	const $hE = [ other_runs, "v" ];
	return other_runs.v = $hE[0][$hE[1]] + 1;
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
on_change6(__clone(ada), (_name, $iG) => {
	const $iH = [ ada_runs, "v" ];
	return ada_runs.v = $iH[0][$iH[1]] + 1;
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
on_change6(__clone(bob), (_name, $jX) => {
	const $jY = [ bob_runs, "v" ];
	return bob_runs.v = $jY[0][$jY[1]] + 1;
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
on_change7(__clone(ada_slot), (_name, $ka) => {
	const $kb = [ ada_slot_runs, "v" ];
	return ada_slot_runs.v = $kb[0][$kb[1]] + 1;
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
on_change(one2, (_item, $kd) => {
	const $ke = [ one_runs2, "v" ];
	return one_runs2.v = $ke[0][$ke[1]] + 1;
}, "lens-loaded.vl:492:16");
on_change(__clone(two2), (_item, $kf) => {
	const $kg = [ two_runs, "v" ];
	return two_runs.v = $kg[0][$kg[1]] + 1;
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
