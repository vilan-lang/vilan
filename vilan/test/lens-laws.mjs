function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_put(list, index, value, location) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_view(list, index, location) {
	if (index >= 0 && index < list.length) return [ list, index ];
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
function __map_get(map, key) {
	return map.has(key) ? [ 0, __clone(map.get(key)) ] : [ 1 ];
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
function __replace(target, value) {
	if (Array.isArray(target) && Array.isArray(value)) target.length = value.length;
	return Object.assign(target, value);
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
function hash(self) {
	return __hash(self);
}
function hash2(self) {
	return __hash(self);
}
function new2() {
	const table2 = [ new Map(), undefined ];
	return [ table2 ];
}
function assert(condition, message, caller) {
	let $az = null;
	if (!(condition)) {
		$az = (() => {
			throw __panic(message, caller);
		})();
	}
	return $az;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped2 = truncated % modulus;
	let $h = null;
	if (wrapped2 < 0) {
		$h = wrapped2 + modulus;
	} else {
		$h = wrapped2;
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
	const wrapped2 = fold_unsigned(value, modulus);
	let $i = null;
	if (wrapped2 >= half) {
		$i = wrapped2 - modulus;
	} else {
		$i = wrapped2;
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
function eq2(self, b2) {
	return self === b2;
}
function eq3(self, b2) {
	const $au = self;
	let $ax = null;
	if ($au[0] === 0) {
		const $av = b2;
		let $aw = null;
		if ($av[0] === 0) {
			$aw = $au[1] === $av[1];
		} else {
			$aw = false;
		}
		$ax = $aw;
	} else {
		const $ay = b2;
		$ax = $ay[0] === 1;
	}
	return $ax;
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
		let $ag = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$ag = undefined;
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
		$ag;
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
	const $dx = turn;
	let $dy = null;
	if ($dx[0] === 0) {
		const ambient = $dx[1];
		$dy = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $dz = last(draining_turns.v);
		let $dA = null;
		if ($dz[0] === 0) {
			const draining = $dz[1];
			$dA = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$dA = undefined;
		}
		$dy = $dA;
	}
	return $dy;
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
function dispose(self, $eO) {
	const $eP = $eO;
	let $eQ = null;
	if ($eP[0] === 0) {
		const established = $eP[1];
		$eQ = [ 0, established ];
	} else {
		$eQ = last(draining_turns.v);
	}
	const ambient = $eQ;
	release_under(self, ambient);
}
function detach(handle) {
	const $dL = last(releasing_turns.v);
	let $dM = null;
	if ($dL[0] === 0) {
		const at_release = $dL[1];
		$dM = at_release;
	} else {
		$dM = last(draining_turns.v);
	}
	const turn = $dM;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $dN = [ 0, handle[0] ];
	let $dO = null;
	if ($dN[0] === 0) {
		const subscribers = $dN[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$dO = undefined;
	} else {
		$dO = undefined;
	}
	$dO;
	const $dP = ambient;
	let $dQ = null;
	if ($dP[0] === 0) {
		const turn = $dP[1];
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
		$dQ = undefined;
	} else {
		$dQ = undefined;
	}
	$dQ;
	const $dR = handle[3].v;
	let $dS = null;
	if ($dR[0] === 0) {
		const release10 = $dR[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release10, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$dS = undefined;
	} else {
		$dS = undefined;
	}
	return $dS;
}
function new4() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $o = null;
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
		$o = undefined;
	}
	return $o;
}
function stage_nursery(self) {
	const $dl = nursery(self);
	let $dm = null;
	if ($dl[0] === 0) {
		const held2 = $dl[1];
		$dm = held2;
	} else {
		const made = detached_nursery();
		self[0].v[3] = [ 0, __clone(made) ];
		$dm = made;
	}
	return $dm;
}
function split_registrations(self) {
	if (!(self[0].v[2]) || is_disposed(self)) {
		return [ 1 ];
	}
	const cleanups = self[0].v[1];
	self[0].v[1] = no_cleanups;
	self[0].v[2] = false;
	return [ 0, [ __shared_new([ 0, cleanups, true, [ 1 ] ]), 0 ] ];
}
function nursery(self) {
	let $dk = null;
	if (is_disposed(self)) {
		$dk = [ 1 ];
	} else {
		$dk = self[0].v[3];
	}
	return $dk;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $ee = null;
	if (!(is_disposed(self))) {
		const held2 = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $dV = held2[3];
		let $dW = null;
		if ($dV[0] === 0) {
			const nursery2 = $dV[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$dW = undefined;
		} else {
			$dW = undefined;
		}
		$dW;
		let $ed = null;
		if (held2[2]) {
			let failure = [ 1 ];
			for (const cleanup of held2[1].v) {
				const $dY = __guarded(cleanup);
				let $dZ = null;
				if ($dY[0] === 0) {
					const message = $dY[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$dZ = undefined;
				} else {
					$dZ = undefined;
				}
				$dZ;
			}
			const $eb = failure;
			let $ec = null;
			if ($eb[0] === 0) {
				const message2 = $eb[1];
				$ec = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$ec = undefined;
			}
			$ed = $ec;
		}
		$ee = $ed;
	}
	return $ee;
}
function register_with_owner(subscription, $eJ, $eK) {
	const $eL = $eK;
	let $eM = null;
	if ($eL[0] === 0) {
		const owner2 = $eL[1];
		$eM = take(owner2, subscription, $eJ);
	} else {
		$eM = __clone(subscription);
	}
	return $eM;
}
function defer_to_owner(cleanup, $l) {
	const $m = $l;
	let $n = null;
	if ($m[0] === 0) {
		const owner2 = $m[1];
		$n = defer(owner2, cleanup);
	} else {
		$n = undefined;
	}
	return $n;
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
function also_releasing(handle, release10) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release10();
		const $eH = previous;
		let $eI = null;
		if ($eH[0] === 0) {
			const earlier = $eH[1];
			$eI = earlier();
		} else {
			$eI = undefined;
		}
		return $eI;
	} ];
}
function settle_together(body, $am) {
	const $an = $am;
	let $ao = null;
	if ($an[0] === 0) {
		const current = $an[1];
		$ao = body(current);
	} else {
		const $ap = last(draining_turns.v);
		let $aq = null;
		if ($ap[0] === 0) {
			const draining = $ap[1];
			$aq = body(draining);
		} else {
			$aq = batch(body, $am);
		}
		$ao = $aq;
	}
	return $ao;
}
function splice_start(held2, at4) {
	let $dE = null;
	if (at4 > held2) {
		$dE = held2;
	} else {
		$dE = at4;
	}
	return $dE;
}
function splice_count(held2, start, removed) {
	let $dF = null;
	if (start + removed > held2) {
		$dF = held2 - start;
	} else {
		$dF = removed;
	}
	return $dF;
}
function kept_index(flags) {
	const index2 = [ __shared_new([  ]) ];
	rebuild_kept(index2, flags);
	return index2;
}
function rebuild_kept(index2, flags) {
	let tree = [ 0 ];
	for (const flag of flags) {
		const $ff = tree;
		let $fe = null;
		if (flag) {
			$fe = 1;
		} else {
			$fe = 0;
		}
		$ff.push($fe);
	}
	const size5 = as_i32(flags.length);
	let at4 = 1;
	while (at4 <= size5) {
		const parent = at4 + (at4 & 0 - at4);
		if (parent <= size5) {
			__at_put(tree, as_usize(parent), __at(tree, as_usize(parent), "std/src/reactive/delta.vl:1810:30") + __at(tree, as_usize(at4), "std/src/reactive/delta.vl:1810:56"), "std/src/reactive/delta.vl:1810:4");
		}
		at4 = at4 + 1;
	}
	index2[0].v = tree;
}
function kept_before(index2, position) {
	let total = 0;
	let at4 = as_i32(position);
	while (at4 > 0) {
		total = total + __at(index2[0].v, as_usize(at4), "std/src/reactive/delta.vl:1822:12");
		at4 = at4 - (at4 & 0 - at4);
	}
	return as_usize(total);
}
function kept_add(index2, position, delta) {
	const size5 = as_i32(index2[0].v.length) - 1;
	let at4 = as_i32(position) + 1;
	while (at4 <= size5) {
		__at_put(index2[0].v, as_usize(at4), __at(index2[0].v, as_usize(at4), "std/src/reactive/delta.vl:1833:39") + delta, "std/src/reactive/delta.vl:1833:3");
		at4 = at4 + (at4 & 0 - at4);
	}
}
function new5() {
	return [ [  ], [  ], [  ], [  ], [  ], [  ], 0 ];
}
function seat(self, at4) {
	let low = 0;
	let high = self[3].length;
	while (high - low > 1) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[3], middle, "std/src/reactive/lens.vl:157:7") <= at4) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at4 - __at(self[3], low, "std/src/reactive/lens.vl:163:14") ];
}
function restart(self) {
	let starts = [  ];
	let running = 0;
	let place = 0;
	for (const block of self[1]) {
		starts.push(running);
		__at_put(self[2], as_usize(block), place, "std/src/reactive/lens.vl:173:4");
		running = running + __at(self[0], as_usize(block), "std/src/reactive/lens.vl:174:15").length;
		place = place + 1;
	}
	self[3] = starts;
}
function renumber(self, block, from) {
	let index2 = from;
	const held2 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:183:14").length;
	while (index2 < held2) {
		const slot6 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:185:15"), index2, "std/src/reactive/lens.vl:185:15");
		if (slot6 !== -(1)) {
			__at_put(self[4], as_usize(slot6), block, "std/src/reactive/lens.vl:187:5");
			__at_put(self[5], as_usize(slot6), index2, "std/src/reactive/lens.vl:188:5");
		}
		index2 = index2 + 1;
	}
}
function open(self, place) {
	const block = as_i32(self[0].length);
	self[0].push([  ]);
	self[2].push(0);
	__insert_at(self[1], place, block, "std/src/reactive/lens.vl:199:14");
	return block;
}
function widen(self, at4, count2) {
	if (count2 === 0 || at4 >= self[6]) {
		return;
	}
	const $M = seat(self, at4);
	const place = $M[0];
	const index2 = $M[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:209:15");
	let $N = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:213:5"), index2, -(1), "std/src/reactive/lens.vl:213:35");
			made = made + 1;
		}
		$N = undefined;
	} else {
		const before = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:217:17"));
		let grown = [  ];
		let copied = 0;
		while (copied < index2) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:221:16"));
			copied = copied + 1;
		}
		let made2 = 0;
		while (made2 < count2) {
			grown.push(-(1));
			made2 = made2 + 1;
		}
		const held2 = before.length;
		while (copied < held2) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:231:16"));
			copied = copied + 1;
		}
		__at_put(self[0], as_usize(block), grown, "std/src/reactive/lens.vl:234:4");
	}
	$N;
	self[6] = self[6] + count2;
	renumber(self, block, index2 + count2);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:238:6").length > 2 * block_size) {
		rebalance(self, place);
	}
	restart(self);
}
function close(self, at4, count2, wakes) {
	let left = count2;
	while (left > 0 && at4 < self[6]) {
		const $K = seat(self, at4);
		const place = $K[0];
		const index2 = $K[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:250:16");
		const held2 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:251:15").length;
		let $L = null;
		if (left < held2 - index2) {
			$L = left;
		} else {
			$L = held2 - index2;
		}
		const take2 = $L;
		let taken = 0;
		while (taken < take2) {
			const slot6 = __remove_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:255:16"), index2, "std/src/reactive/lens.vl:255:46");
			if (slot6 !== -(1)) {
				__at_put(self[4], as_usize(slot6), -(1), "std/src/reactive/lens.vl:257:6");
				wakes.push([ 2, slot6 ]);
			}
			taken = taken + 1;
		}
		self[6] = self[6] - take2;
		left = left - take2;
		if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:264:7").length === 0) {
			__remove_at(self[1], place, "std/src/reactive/lens.vl:265:16");
		} else {
			renumber(self, block, index2);
		}
		restart(self);
	}
}
function rebalance(self, place) {
	const block = __at(self[1], place, "std/src/reactive/lens.vl:275:15");
	const all4 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:276:13"));
	let first2 = [  ];
	let index2 = 0;
	while (index2 < block_size) {
		first2.push(__at(all4, index2, "std/src/reactive/lens.vl:280:15"));
		index2 = index2 + 1;
	}
	__at_put(self[0], as_usize(block), first2, "std/src/reactive/lens.vl:283:3");
	let next = place + 1;
	const held2 = all4.length;
	while (index2 < held2) {
		const fresh = open(self, next);
		let filled = 0;
		while (filled < block_size && index2 < held2) {
			__at(self[0], as_usize(fresh), "std/src/reactive/lens.vl:290:5").push(__at(all4, index2, "std/src/reactive/lens.vl:290:40"));
			index2 = index2 + 1;
			filled = filled + 1;
		}
		renumber(self, fresh, 0);
		next = next + 1;
	}
}
function follow(self, at4) {
	while (self[6] <= at4) {
		const places2 = self[1].length;
		if (places2 === 0 || __at(self[0], as_usize(__at(self[1], places2 - 1, "std/src/reactive/lens.vl:304:34")), "std/src/reactive/lens.vl:304:22").length >= block_size) {
			const fresh = open(self, places2);
			__at_put(self[2], as_usize(fresh), places2, "std/src/reactive/lens.vl:306:5");
			self[3].push(self[6]);
		}
		const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:309:15");
		__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:310:4").push(-(1));
		self[6] = self[6] + 1;
	}
	const $j = seat(self, at4);
	const place = $j[0];
	const index2 = $j[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:314:15");
	const held2 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:315:14"), index2, "std/src/reactive/lens.vl:315:14");
	if (held2 !== -(1)) {
		return held2;
	}
	const slot6 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index2);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:322:3"), index2, slot6, "std/src/reactive/lens.vl:322:3");
	return slot6;
}
function apply(self, change3, wakes) {
	const $I = change3;
	let $J = null;
	if ($I[0] === 0) {
		const at4 = $I[1];
		const removed = $I[2];
		const inserted = $I[3];
		close(self, at4, removed, wakes);
		widen(self, at4, inserted);
		$J = undefined;
	} else if ($I[0] === 1) {
		const at5 = $I[1];
		const count2 = $I[2];
		$J = widen(self, at5, count2);
	} else if ($I[0] === 3) {
		const at6 = $I[1];
		const count3 = $I[2];
		$J = close(self, at6, count3, wakes);
	} else if ($I[0] === 2) {
		const at7 = $I[1];
		let $P = null;
		if (at7 < self[6]) {
			const $O = seat(self, at7);
			const place = $O[0];
			const index2 = $O[1];
			const slot6 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:339:29")), "std/src/reactive/lens.vl:339:17"), index2, "std/src/reactive/lens.vl:339:17");
			if (slot6 !== -(1)) {
				wakes.push([ 0, slot6 ]);
			}
			$P = undefined;
		}
		$J = $P;
	} else if ($I[0] === 4) {
		const at8 = $I[1];
		const part2 = $I[2];
		let $R = null;
		if (at8 < self[6]) {
			const $Q = seat(self, at8);
			const place2 = $Q[0];
			const index3 = $Q[1];
			const slot7 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:348:29")), "std/src/reactive/lens.vl:348:17"), index3, "std/src/reactive/lens.vl:348:17");
			if (slot7 !== -(1)) {
				wakes.push([ 1, slot7, part2 ]);
			}
			$R = undefined;
		}
		$J = $R;
	} else if ($I[0] === 5) {
		const from = $I[1];
		const to = $I[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $S = null;
		if (from > to) {
			$S = from;
		} else {
			$S = to;
		}
		const far = $S;
		while (self[6] <= far) {
			const places2 = self[1].length;
			if (places2 === 0 || __at(self[0], as_usize(__at(self[1], places2 - 1, "std/src/reactive/lens.vl:362:36")), "std/src/reactive/lens.vl:362:24").length >= block_size) {
				const fresh = open(self, places2);
				__at_put(self[2], as_usize(fresh), places2, "std/src/reactive/lens.vl:364:7");
				self[3].push(self[6]);
			}
			const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:367:17");
			__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:368:6").push(-(1));
			self[6] = self[6] + 1;
		}
		const $T = seat(self, from);
		const left = $T[0];
		const index4 = $T[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:372:18");
		const slot8 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:373:16"), index4, "std/src/reactive/lens.vl:373:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:375:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:376:17");
		} else {
			renumber(self, source, index4);
		}
		restart(self);
		let $U = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$U = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:384:25")), "std/src/reactive/lens.vl:384:13").length ];
		} else {
			$U = seat(self, to);
		}
		const $V = $U;
		const place3 = $V[0];
		const landing = $V[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:388:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:389:5"), landing, slot8, "std/src/reactive/lens.vl:389:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing);
		restart(self);
		$J = undefined;
	} else {
		let slot9 = 0;
		const slots = as_i32(self[4].length);
		while (slot9 < slots) {
			if (__at(self[4], as_usize(slot9), "std/src/reactive/lens.vl:398:9") !== -(1)) {
				__at_put(self[4], as_usize(slot9), -(1), "std/src/reactive/lens.vl:399:7");
				wakes.push([ 2, slot9 ]);
			}
			slot9 = slot9 + 1;
		}
		self[0] = [  ];
		self[1] = [  ];
		self[2] = [  ];
		self[3] = [  ];
		self[6] = 0;
		$J = undefined;
	}
	return $J;
}
function follows_items(self) {
	return true;
}
function release(self, slot6) {
	const block = __at(self[4], as_usize(slot6), "std/src/reactive/lens.vl:418:15");
	if (block === -(1)) {
		return;
	}
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:422:3"), __at(self[5], as_usize(slot6), "std/src/reactive/lens.vl:422:33"), -(1), "std/src/reactive/lens.vl:422:3");
	__at_put(self[4], as_usize(slot6), -(1), "std/src/reactive/lens.vl:423:3");
}
function locate(self, slot6) {
	const block = __at(self[4], as_usize(slot6), "std/src/reactive/lens.vl:427:15");
	if (block === -(1)) {
		return [ 1 ];
	}
	return [ 0, __at(self[3], __at(self[2], as_usize(block), "std/src/reactive/lens.vl:431:20"), "std/src/reactive/lens.vl:431:8") + __at(self[5], as_usize(slot6), "std/src/reactive/lens.vl:431:51") ];
}
function new6() {
	return [ new8(), [  ] ];
}
function fresh_cells() {
	return [ new7(0, "std/src/reactive/lens.vl:888:33"), __shared_new([  ]), __shared_new(0) ];
}
function wake_all(cells, slot6, $ab) {
	const held2 = viewing2(cells, (list2) => {
		return __clone(__at(list2, as_usize(slot6), "std/src/reactive/lens.vl:928:35"));
	});
	notify(held2[0], $ab);
	for (const part2 of __clone(held2[1].v)) {
		notify(part2[1], $ab);
	}
}
function wake_part(cells, slot6, name, $al) {
	const held2 = viewing2(cells, (list2) => {
		return __clone(__at(list2, as_usize(slot6), "std/src/reactive/lens.vl:936:35"));
	});
	notify(held2[0], $al);
	for (const part2 of __clone(held2[1].v)) {
		if (part2[0] === name) {
			notify(part2[1], $al);
		}
	}
}
function extend_run(runs2, from, to) {
	const held2 = runs2.v.length;
	let $cM = null;
	if (held2 > 0) {
		const $cL = __at(runs2.v, held2 - 1, "std/src/reactive/lens.vl:1761:31");
		const start = $cL[0];
		const count2 = $cL[1];
		const first2 = $cL[2];
		const leftward = first2 < start && from === start + count2 && to === first2 + count2;
		const rightward = first2 > start && from === start && to === first2;
		if (leftward || rightward) {
			__at_put(runs2.v, held2 - 1, [ start, count2 + 1, first2 ], "std/src/reactive/lens.vl:1765:4");
			return;
		}
		$cM = undefined;
	}
	$cM;
	runs2.v.push([ from, 1, to ]);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function tasks() {
	return [ [ "a", "open" ], [ "b", "open" ], [ "c", "open" ] ];
}
function ages_of() {
	let ages2 = new8();
	insert(ages2, "bo", 4);
	return ages2;
}
function eq4(self, other) {
	return self[0] === other[0] && self[1] === other[1];
}
function item(self, at4) {
	return __list_get(self, at4);
}
function lend(self, at4, read) {
	if (at4 < self.length) {
		read(__at_view(self, at4, "std/src/reactive/lens.vl:2008:10"));
	}
}
function put(self, at4, value) {
	__at_put(self, at4, __clone(value), "std/src/reactive/lens.vl:2003:3");
}
function change(self, at4, with2) {
	if (at4 < self.length) {
		with2(__at_view(self, at4, "std/src/reactive/lens.vl:2014:14"));
	}
}
function new7(value, caller) {
	let subscribers = [  ];
	const cell = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell[0]), caller);
	}
	return cell;
}
function over(value, handler, access) {
	return [ [ 0, __shared_new(value) ], __shared_new(handler), __clone(access), __shared_new([  ]), new7(0, "std/src/reactive/lens.vl:647:28"), new7(0, "std/src/reactive/lens.vl:648:24"), new7(0, "std/src/reactive/lens.vl:649:24"), __shared_new([  ]), __shared_new(0), __shared_new([ 1 ]) ];
}
function sequence(value, $a) {
	return over(value, new5(), [ (held2, at4) => {
		return item(held2, at4);
	}, (held2, at4, read) => {
		return lend(held2, at4, read);
	}, (held2, at4, value2) => {
		return put(held2, at4, value2, $a);
	}, (held2, at4, with2) => {
		return change(held2, at4, with2, $a);
	} ]);
}
function lend2(value, read) {
	return read(value);
}
function viewing(cell, read) {
	return lend2(cell.v, read);
}
function viewing2(cell, read) {
	return lend2(cell.v, read);
}
function peek_storage(storage, read) {
	const $c = storage;
	let $d = null;
	if ($c[0] === 0) {
		const cell = $c[1];
		$d = viewing2(cell, read);
	} else {
		const port = $c[1];
		let out = [ 1 ];
		port[0]((held2) => {
			return out = [ 0, read(held2) ];
		});
		const $e = out;
		let $f = null;
		if ($e[0] === 0) {
			const value = __clone($e[1]);
			$f = value;
		} else {
			$f = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:632:13");
			})();
		}
		$d = $f;
	}
	return $d;
}
function holds(storage, view, at4) {
	let found = false;
	peek_storage(storage, (held2) => {
		return view(held2, at4, (_item) => {
			return found = true;
		});
	});
	return found;
}
function follow_id(storage, handler, cells, view, at4) {
	if (viewing(handler, (held2) => {
		return follows_items(held2);
	}) && !(holds(storage, view, at4))) {
		return -(1);
	}
	const id = follow(handler.v, at4);
	while (cells.v.length <= as_usize(id)) {
		cells.v.push(fresh_cells());
	}
	const slot_cells = viewing2(cells, (held2) => {
		return __clone(__at(held2, as_usize(id), "std/src/reactive/lens.vl:909:41"));
	});
	const $k = [ slot_cells[2], "v" ];
	slot_cells[2].v = $k[0][$k[1]] + 1;
	return id;
}
function slot_of(self, id) {
	if (id < 0) {
		return [ __clone(self), -(1), fresh_cells() ];
	}
	return [ __clone(self), id, viewing2(self[3], (held2) => {
		return __clone(__at(held2, as_usize(id), "std/src/reactive/lens.vl:827:66"));
	}) ];
}
function follow2(self, at4) {
	return slot_of(self, follow_id(self[0], self[1], self[3], self[2][1], at4));
}
function release_id(handler, cells, id) {
	if (id < 0) {
		return;
	}
	const slot_cells = viewing2(cells, (held2) => {
		return __clone(__at(held2, as_usize(id), "std/src/reactive/lens.vl:919:41"));
	});
	const left = slot_cells[2].v - 1;
	slot_cells[2].v = left;
	if (left <= 0) {
		release(handler.v, id);
	}
}
function release2(self) {
	release_id(self[0][1], self[0][3], self[1]);
}
function slot(self, at4, $b) {
	const handle = follow2(self, at4);
	defer_to_owner(() => {
		return release2(handle);
	}, $b);
	return handle;
}
function at(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][1], (handler) => {
		return locate(handler, self[1]);
	});
}
function peek_storage2(storage, read) {
	const $r = storage;
	let $s = null;
	if ($r[0] === 0) {
		const cell = $r[1];
		$s = viewing2(cell, read);
	} else {
		const port = $r[1];
		let out = [ 1 ];
		port[0]((held2) => {
			return out = [ 0, read(held2) ];
		});
		const $t = out;
		let $u = null;
		if ($t[0] === 0) {
			const value = __clone($t[1]);
			$u = value;
		} else {
			$u = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:632:13");
			})();
		}
		$s = $u;
	}
	return $s;
}
function peek(self, read) {
	return peek_storage2(self[0], read);
}
function get(self) {
	const $q = at(self);
	if ($q[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][2][0];
	return peek(self[0], (held2) => {
		return read(held2, $q[1]);
	});
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
	return on_settle(self[2][0], subscriber);
}
function attach_observer(self, observer, immediately) {
	const subscription = on_settle2(self, mint_subscriber(() => {
		return observer(get(self));
	}));
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function on_change(self, observer, caller) {
	return place_subscription(attach_observer(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function count(self) {
	return self.length;
}
function peek2(self, read) {
	return peek_storage2(self[0], read);
}
function size(self) {
	return peek2(self, (held2) => {
		return count(held2);
	});
}
function coming(self, changes) {
	if (self[7].v.length === 0) {
		return;
	}
	for (const tap2 of __clone(self[7].v)) {
		for (const change3 of changes) {
			tap2[1](change3);
		}
	}
}
function splice(self, at4, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self, at4, "std/src/reactive/lens.vl:1992:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at4 + offset, __clone(value), "std/src/reactive/lens.vl:1997:9");
		offset = offset + 1;
	}
}
function mutate(self, change3) {
	const $D = self[0];
	let $E = null;
	if ($D[0] === 0) {
		const cell = $D[1];
		$E = change3(cell.v);
	} else {
		const port = $D[1];
		$E = port[1](change3);
	}
	return $E;
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $ah = null;
	if (is_empty(self)) {
		$ah = [ 1 ];
	} else {
		$ah = __list_get(self, self.length - 1);
	}
	return $ah;
}
function notify(self, $ac) {
	const traced = tracing.v;
	let $ad = null;
	if (traced) {
		$ad = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$ad = "";
	}
	const cause = $ad;
	const $ae = $ac;
	let $af = null;
	if ($ae[0] === 0) {
		const turn = $ae[1];
		$af = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $ai = last(draining_turns.v);
		let $aj = null;
		if ($ai[0] === 0) {
			const draining = $ai[1];
			$aj = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $ak = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$ak = undefined;
				}
				$ak;
			}
			$aj = undefined;
		}
		$af = $aj;
	}
	return $af;
}
function batch(body, $ar) {
	const $as = $ar;
	let $at = null;
	if ($as[0] === 0) {
		const current = $as[1];
		$at = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$at = result;
	}
	return $at;
}
function landed(self, changes, late, $F) {
	if (changes.length === 0) {
		return;
	}
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	const tapped = self[7].v.length > 0;
	let index2 = 0;
	for (const change3 of changes) {
		const $G = change3;
		let $H = null;
		if ($G[0] === 0) {
			const _at = $G[1];
			const removed = $G[2];
			const inserted = $G[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$H = undefined;
		} else if ($G[0] === 1) {
			const _at2 = $G[1];
			const _count = $G[2];
			$H = resized = true;
		} else if ($G[0] === 3) {
			const _at3 = $G[1];
			const _count2 = $G[2];
			$H = resized = true;
		} else if ($G[0] === 6) {
			$H = resized = true;
		} else if ($G[0] === 5) {
			const _from = $G[1];
			const _to = $G[2];
			$H = shaped = true;
		} else {
			$H = undefined;
		}
		$H;
		apply(self[1].v, change3, wakes);
		let $X = null;
		if (tapped) {
			let $W = null;
			if (!(late)) {
				$W = [ 0 ];
			} else if (!(several)) {
				$W = [ 1 ];
			} else if (index2 === 0) {
				$W = [ 2 ];
			} else {
				$W = [ 3 ];
			}
			const stage = $W;
			for (const tap2 of __clone(self[7].v)) {
				tap2[2](change3, stage);
			}
			$X = undefined;
		}
		$X;
		index2 = index2 + 1;
	}
	const cells = self[3];
	const published = __clone(self[4]);
	const shape = __clone(self[5]);
	const sized = __clone(self[6]);
	settle_together(($Y) => {
		for (const wake2 of wakes) {
			const $Z = wake2;
			let $aa = null;
			if ($Z[0] === 0) {
				const slot6 = $Z[1];
				$aa = wake_all(cells, slot6, [ 0, $Y ]);
			} else if ($Z[0] === 2) {
				const slot7 = $Z[1];
				$aa = wake_all(cells, slot7, [ 0, $Y ]);
			} else {
				const slot8 = $Z[1];
				const part2 = $Z[2];
				$aa = wake_part(cells, slot8, part2, [ 0, $Y ]);
			}
			$aa;
		}
		if (resized) {
			notify(sized, [ 0, $Y ]);
		}
		if (shaped || resized) {
			notify(shape, [ 0, $Y ]);
		}
		notify(published, [ 0, $Y ]);
		return;
	}, $F);
}
function splice2(self, at4, removed, inserted, $w) {
	const held2 = size(self);
	let $B = null;
	if (at4 > held2) {
		$B = held2;
	} else {
		$B = at4;
	}
	const start = $B;
	let $C = null;
	if (start + removed > held2) {
		$C = held2 - start;
	} else {
		$C = removed;
	}
	const taking = $C;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change3 = [ 0, start, taking, arriving ];
	coming(self, [ __clone(change3) ]);
	mutate(self, (held3) => {
		return splice(held3, start, taking, inserted);
	});
	landed(self, [ change3 ], false, $w);
}
function insert_at(self, at4, value, $v) {
	return splice2(self, at4, 0, [ __clone(value) ], $v);
}
function eq5(self, b2) {
	const $aA = self;
	let $aD = null;
	if ($aA[0] === 0) {
		const $aB = b2;
		let $aC = null;
		if ($aB[0] === 0) {
			$aC = eq2($aA[1], $aB[1]);
		} else {
			$aC = false;
		}
		$aD = $aC;
	} else {
		const $aE = b2;
		$aD = $aE[0] === 1;
	}
	return $aD;
}
function has(self, at4) {
	return holds(self[0], self[2][1], at4);
}
function replace(self, at4, value, $aO) {
	let $aP = null;
	if (has(self, at4)) {
		$aP = [ 2, __clone(at4) ];
	} else if (viewing(self[1], (handler) => {
		return follows_items(handler);
	})) {
		return false;
	} else {
		$aP = [ 1, __clone(at4), 1 ];
	}
	const change3 = $aP;
	coming(self, [ __clone(change3) ]);
	const write = self[2][2];
	mutate(self, (held2) => {
		return write(held2, at4, value);
	});
	landed(self, [ change3 ], false, $aO);
	return true;
}
function set(self, value, $aM) {
	const $aN = at(self);
	if ($aN[0] === 0) {

	} else {
		return false;
	}
	return replace(self[0], $aN[1], value, $aM);
}
function all_of(storage) {
	return peek_storage2(storage, (held2) => {
		let out = [  ];
		let at4 = 0;
		const count2 = count(held2);
		while (at4 < count2) {
			const $aQ = item(held2, at4);
			if ($aQ[0] === 0) {
				out.push($aQ[1]);
			}
			at4 = at4 + 1;
		}
		return out;
	});
}
function all(self) {
	return all_of(self[0]);
}
function remove_at(self, at4, $aV) {
	return splice2(self, at4, 1, [  ], $aV);
}
function push(self, value, $aW) {
	return splice2(self, size(self), 0, [ __clone(value) ], $aW);
}
function lend3(self, at4, read) {
	if (at4 < self.length) {
		read(__at(self, at4, "std/src/reactive/lens.vl:2008:10"));
	}
}
function change2(self, at4, with2) {
	if (at4 < self.length) {
		with2(__at(self, at4, "std/src/reactive/lens.vl:2014:14"));
	}
}
function over2(value, handler, access) {
	return [ [ 0, __shared_new(value) ], __shared_new(handler), __clone(access), __shared_new([  ]), new7(0, "std/src/reactive/lens.vl:647:28"), new7(0, "std/src/reactive/lens.vl:648:24"), new7(0, "std/src/reactive/lens.vl:649:24"), __shared_new([  ]), __shared_new(0), __shared_new([ 1 ]) ];
}
function sequence2(value, $a) {
	return over2(value, new5(), [ (held2, at4) => {
		return item(held2, at4);
	}, (held2, at4, read) => {
		return lend3(held2, at4, read);
	}, (held2, at4, value2) => {
		return put(held2, at4, value2, $a);
	}, (held2, at4, with2) => {
		return change2(held2, at4, with2, $a);
	} ]);
}
function holds2(storage, view, at4) {
	let found = false;
	peek_storage2(storage, (held2) => {
		return view(held2, at4, (_item) => {
			return found = true;
		});
	});
	return found;
}
function follow_id2(storage, handler, cells, view, at4) {
	if (viewing(handler, (held2) => {
		return follows_items(held2);
	}) && !(holds2(storage, view, at4))) {
		return -(1);
	}
	const id = follow(handler.v, at4);
	while (cells.v.length <= as_usize(id)) {
		cells.v.push(fresh_cells());
	}
	const slot_cells = viewing2(cells, (held2) => {
		return __clone(__at(held2, as_usize(id), "std/src/reactive/lens.vl:909:41"));
	});
	const $bb = [ slot_cells[2], "v" ];
	slot_cells[2].v = $bb[0][$bb[1]] + 1;
	return id;
}
function follow3(self, at4) {
	return slot_of(self, follow_id2(self[0], self[1], self[3], self[2][1], at4));
}
function release3(self) {
	release_id(self[0][1], self[0][3], self[1]);
}
function slot2(self, at4, $b) {
	const handle = follow3(self, at4);
	defer_to_owner(() => {
		return release3(handle);
	}, $b);
	return handle;
}
function part(self, name, read) {
	for (const held2 of self[2][1].v) {
		if (held2[0] === name) {
			return [ __clone(self), __clone(held2[1]), read ];
		}
	}
	const woke = new7(0, "std/src/reactive/lens.vl:1081:26");
	self[2][1].v.push([ name, __clone(woke) ]);
	return [ __clone(self), woke, read ];
}
function get2(self) {
	const $be = at(self[0]);
	if ($be[0] === 0) {

	} else {
		return [ 1 ];
	}
	let out = [ 1 ];
	const view = self[0][0][2][1];
	const read = self[2];
	peek2(self[0][0], (held2) => {
		return view(held2, $be[1], (item2) => {
			return out = [ 0, read(item2) ];
		});
	});
	return out;
}
function on_settle3(self, subscriber) {
	return on_settle(self[1], subscriber);
}
function attach_observer2(self, observer, immediately) {
	const subscription = on_settle3(self, mint_subscriber(() => {
		return observer(get2(self));
	}));
	if (immediately) {
		observer(get2(self));
	}
	return subscription;
}
function on_change2(self, observer, caller) {
	return place_subscription(attach_observer2(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function has2(self, at4) {
	return holds2(self[0], self[2][1], at4);
}
function landed2(self, changes, late, $F) {
	if (changes.length === 0) {
		return;
	}
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	const tapped = self[7].v.length > 0;
	let index2 = 0;
	for (const change3 of changes) {
		const $bo = change3;
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
		apply(self[1].v, change3, wakes);
		let $br = null;
		if (tapped) {
			let $bq = null;
			if (!(late)) {
				$bq = [ 0 ];
			} else if (!(several)) {
				$bq = [ 1 ];
			} else if (index2 === 0) {
				$bq = [ 2 ];
			} else {
				$bq = [ 3 ];
			}
			const stage = $bq;
			for (const tap2 of __clone(self[7].v)) {
				tap2[2](change3, stage);
			}
			$br = undefined;
		}
		$br;
		index2 = index2 + 1;
	}
	const cells = self[3];
	const published = __clone(self[4]);
	const shape = __clone(self[5]);
	const sized = __clone(self[6]);
	settle_together(($Y) => {
		for (const wake2 of wakes) {
			const $bs = wake2;
			let $bt = null;
			if ($bs[0] === 0) {
				const slot6 = $bs[1];
				$bt = wake_all(cells, slot6, [ 0, $Y ]);
			} else if ($bs[0] === 2) {
				const slot7 = $bs[1];
				$bt = wake_all(cells, slot7, [ 0, $Y ]);
			} else {
				const slot8 = $bs[1];
				const part2 = $bs[2];
				$bt = wake_part(cells, slot8, part2, [ 0, $Y ]);
			}
			$bt;
		}
		if (resized) {
			notify(sized, [ 0, $Y ]);
		}
		if (shaped || resized) {
			notify(shape, [ 0, $Y ]);
		}
		notify(published, [ 0, $Y ]);
		return;
	}, $F);
}
function edit(self, at4, part2, change3, $bl) {
	if (!(has2(self, at4))) {
		return false;
	}
	coming(self, [ [ 4, __clone(at4), part2 ] ]);
	const modify = self[2][3];
	mutate(self, (held2) => {
		return modify(held2, at4, change3);
	});
	landed2(self, [ [ 4, __clone(at4), part2 ] ], false, $bl);
	return true;
}
function edit2(self, part2, change3, $bj) {
	const $bk = at(self);
	if ($bk[0] === 0) {

	} else {
		return false;
	}
	return edit(self[0], $bk[1], part2, change3, $bj);
}
function size2(self) {
	return peek2(self, (held2) => {
		return count(held2);
	});
}
function splice3(self, at4, removed, inserted, $w) {
	const held2 = size2(self);
	let $by = null;
	if (at4 > held2) {
		$by = held2;
	} else {
		$by = at4;
	}
	const start = $by;
	let $bz = null;
	if (start + removed > held2) {
		$bz = held2 - start;
	} else {
		$bz = removed;
	}
	const taking = $bz;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change3 = [ 0, start, taking, arriving ];
	coming(self, [ __clone(change3) ]);
	mutate(self, (held3) => {
		return splice(held3, start, taking, inserted);
	});
	landed2(self, [ change3 ], false, $w);
}
function insert_at2(self, at4, value, $v) {
	return splice3(self, at4, 0, [ __clone(value) ], $v);
}
function audit(self, before, changes) {
	const $bN = self[9].v;
	if ($bN[0] === 0) {

	} else {
		return;
	}
	const $bO = before;
	if ($bO[0] === 0) {

	} else {
		return;
	}
	const problem = peek2(self, (held2) => {
		return $bN[1]($bO[1], held2, changes);
	});
	const $bT = problem;
	if ($bT[0] === 0) {
		(() => {
			throw __panic("lens: the changes do not match the write: " + $bT[1], "std/src/reactive/lens.vl:713:4");
		})();
	}
}
function update(self, change3, $bE) {
	const $bF = self[9].v;
	let $bG = null;
	if ($bF[0] === 0) {
		const _check = $bF[1];
		$bG = [ 0, peek2(self, (held2) => {
			return __clone(held2);
		}) ];
	} else {
		$bG = [ 1 ];
	}
	const before = $bG;
	let out = [ 1 ];
	mutate(self, (held2) => {
		return out = [ 0, change3(held2) ];
	});
	const $bL = out;
	let $bM = null;
	if ($bL[0] === 0) {
		const result = __clone($bL[1][0]);
		const changes = __clone($bL[1][1]);
		audit(self, before, changes);
		landed2(self, changes, true, $bE);
		$bM = result;
	} else {
		$bM = (() => {
			throw __panic("lens: an update\'s write never ran", "std/src/reactive/lens.vl:684:12");
		})();
	}
	return $bM;
}
function get3(self) {
	const $bU = at(self);
	if ($bU[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][2][0];
	return peek2(self[0], (held2) => {
		return read(held2, $bU[1]);
	});
}
function eq6(self, b2) {
	const $bZ = self;
	let $cc = null;
	if ($bZ[0] === 0) {
		const $ca = b2;
		let $cb = null;
		if ($ca[0] === 0) {
			$cb = eq4($bZ[1], $ca[1]);
		} else {
			$cb = false;
		}
		$cc = $cb;
	} else {
		const $cd = b2;
		$cc = $cd[0] === 1;
	}
	return $cc;
}
function get4(self) {
	return peek2(self, (held2) => {
		return __clone(held2);
	});
}
function on_settle4(self, subscriber) {
	return on_settle(self[4], subscriber);
}
function attach_observer3(self, observer, immediately) {
	const subscription = on_settle4(self, mint_subscriber(() => {
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
function length(self) {
	return [ __clone(self) ];
}
function get5(self) {
	return size2(self[0]);
}
function on_settle5(self, subscriber) {
	return on_settle(self[0][6], subscriber);
}
function attach_observer4(self, observer, immediately) {
	const subscription = on_settle5(self, mint_subscriber(() => {
		return observer(get5(self));
	}));
	if (immediately) {
		observer(get5(self));
	}
	return subscription;
}
function on_change4(self, observer, caller) {
	return place_subscription(attach_observer4(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function replace2(self, at4, value, $aO) {
	let $ci = null;
	if (has2(self, at4)) {
		$ci = [ 2, __clone(at4) ];
	} else if (viewing(self[1], (handler) => {
		return follows_items(handler);
	})) {
		return false;
	} else {
		$ci = [ 1, __clone(at4), 1 ];
	}
	const change3 = $ci;
	coming(self, [ __clone(change3) ]);
	const write = self[2][2];
	mutate(self, (held2) => {
		return write(held2, at4, value);
	});
	landed2(self, [ change3 ], false, $aO);
	return true;
}
function push2(self, value, $aW) {
	return splice3(self, size2(self), 0, [ __clone(value) ], $aW);
}
function move_item(self, from, to, $cl) {
	const held2 = size(self);
	if (from >= held2 || from === to) {
		return;
	}
	let $cm = null;
	if (to >= held2) {
		$cm = held2 - 1;
	} else {
		$cm = to;
	}
	const landing = $cm;
	const $cn = peek(self, (list2) => {
		return item(list2, from);
	});
	if ($cn[0] === 0) {

	} else {
		return;
	}
	coming(self, [ [ 5, from, landing ] ]);
	mutate(self, (held3) => {
		splice(held3, from, 1, [  ]);
		splice(held3, landing, 0, [ __clone($cn[1]) ]);
		return;
	});
	landed(self, [ [ 5, from, landing ] ], false, $cl);
}
function new8() {
	const table2 = [ new Map(), undefined ];
	return [ table2 ];
}
function insert(self, key, value) {
	__nmap_insert(self[0], hash(key), __clone(key), __clone(value));
}
function contains_key(self, key) {
	return __nmap_has(self[0], hash(key));
}
function reconcile_by_key(self, next, key, $co) {
	const lens2 = __clone(self);
	batch(($cp) => {
		let wanted = new8();
		for (const $cq of next) {
			const item2 = [ $cq ];
			insert(wanted, key([ item2, 0 ]), true);
		}
		let keys2 = [  ];
		for (const $cr of all(lens2)) {
			const item3 = [ $cr ];
			keys2.push(key([ item3, 0 ]));
		}
		let at4 = keys2.length;
		while (at4 > 0) {
			at4 = at4 - 1;
			if (!(contains_key(wanted, __at(keys2, at4, "std/src/reactive/lens.vl:1679:29")))) {
				remove_at(lens2, at4, [ 0, $cp ]);
				__remove_at(keys2, at4, "std/src/reactive/lens.vl:1681:11");
			}
		}
		let index2 = 0;
		for (const $cs of next) {
			const item4 = [ $cs ];
			const want = key([ item4, 0 ]);
			let $ct = null;
			if (index2 < keys2.length && __at(keys2, index2, "std/src/reactive/lens.vl:1687:30") === want) {
				if (!(eq3(peek(lens2, (list2) => {
					return item(list2, index2);
				}), [ 0, __clone(item4[0]) ]))) {
					replace(lens2, index2, item4[0], [ 0, $cp ]);
				}
				$ct = undefined;
			} else {
				let found = [ 1 ];
				let search = index2 + 1;
				while (search < keys2.length) {
					if (__at(keys2, search, "std/src/reactive/lens.vl:1695:10") === want) {
						found = [ 0, search ];
						search = keys2.length;
					}
					search = search + 1;
				}
				const $cu = found;
				let $cv = null;
				if ($cu[0] === 0) {
					const from = $cu[1];
					move_item(lens2, from, index2, [ 0, $cp ]);
					const moved = __remove_at(keys2, from, "std/src/reactive/lens.vl:1704:25");
					__insert_at(keys2, index2, moved, "std/src/reactive/lens.vl:1705:13");
					if (!(eq3(peek(lens2, (list2) => {
						return item(list2, index2);
					}), [ 0, __clone(item4[0]) ]))) {
						replace(lens2, index2, item4[0], [ 0, $cp ]);
					}
					$cv = undefined;
				} else {
					insert_at(lens2, index2, item4[0], [ 0, $cp ]);
					__insert_at(keys2, index2, want, "std/src/reactive/lens.vl:1712:13");
					$cv = undefined;
				}
				$ct = $cv;
			}
			$ct;
			index2 = index2 + 1;
		}
		return;
	}, $co);
}
function sequence3(value, $a) {
	return over2(value, new5(), [ (held2, at4) => {
		return item(held2, at4);
	}, (held2, at4, read) => {
		return lend(held2, at4, read);
	}, (held2, at4, value2) => {
		return put(held2, at4, value2, $a);
	}, (held2, at4, with2) => {
		return change(held2, at4, with2, $a);
	} ]);
}
function new9() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function arrived_at(storage, at4, count2) {
	return peek_storage2(storage, (held2) => {
		let arrived = [  ];
		let index2 = at4;
		while (index2 < at4 + count2) {
			const $cA = item(held2, index2);
			if ($cA[0] === 0) {
				arrived.push($cA[1]);
			}
			index2 = index2 + 1;
		}
		return arrived;
	});
}
function one(item2) {
	const $cJ = item2;
	let $cK = null;
	if ($cJ[0] === 0) {
		const value = $cJ[1];
		$cK = [ __clone(value) ];
	} else {
		$cK = [  ];
	}
	return $cK;
}
function trim(self) {
	let lowest = self[1].v;
	for (const cursor6 of self[3].v) {
		const at4 = cursor6[1].v;
		if (at4 < lowest) {
			lowest = at4;
		}
	}
	const base = self[2].v;
	if (lowest > base) {
		const dropped = lowest - base;
		let kept = [  ];
		let index2 = as_usize(dropped);
		const held2 = __clone(self[0].v);
		const length4 = held2.length;
		while (index2 < length4) {
			const $cQ = __list_get(held2, index2);
			let $cR = null;
			if ($cQ[0] === 0) {
				const op = $cQ[1];
				$cR = kept.push(op);
			} else {
				$cR = undefined;
			}
			$cR;
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
function record_set(log, storage, leaving, at4, late) {
	if (late) {
		record(log, [ 2, all_of(storage) ]);
		return;
	}
	const before = __list_get(leaving.v, 0);
	const after = peek_storage2(storage, (held2) => {
		return item(held2, at4);
	});
	const $cV = before;
	let $cW = undefined;
	if ($cV[0] === 0 && (($cW = after), $cW[0] === 0)) {
		record(log, [ 1, at4, __clone($cV[1]), __clone($cW[1]) ]);
	}
}
function tap(self, before, after) {
	const id = self[8].v;
	self[8].v = id + 1;
	self[7].v.push([ id, before, after ]);
	return id;
}
function untap(self, id) {
	let kept = [  ];
	for (const tap2 of self[7].v) {
		if (tap2[0] !== id) {
			kept.push(__clone(tap2));
		}
	}
	self[7].v = kept;
}
function release4(self) {
	untap(self[0], self[2]);
}
function items_into(self, log, $cx) {
	const leaving = __shared_new([  ]);
	const runs2 = __shared_new([  ]);
	const skipping = __shared_new(0);
	const storage = __clone(self[0]);
	const tap2 = tap(self, (change3) => {
		const $cy = change3;
		let $cz = null;
		if ($cy[0] === 0) {
			const at4 = $cy[1];
			const removed = $cy[2];
			const _inserted = $cy[3];
			leaving.v = arrived_at(storage, at4, removed);
			$cz = undefined;
		} else if ($cy[0] === 3) {
			const at5 = $cy[1];
			const count2 = $cy[2];
			leaving.v = arrived_at(storage, at5, count2);
			$cz = undefined;
		} else if ($cy[0] === 2) {
			const at6 = $cy[1];
			leaving.v = one(peek_storage2(storage, (held2) => {
				return item(held2, at6);
			}));
			$cz = undefined;
		} else if ($cy[0] === 4) {
			const at7 = $cy[1];
			const _part = $cy[2];
			leaving.v = one(peek_storage2(storage, (held2) => {
				return item(held2, at7);
			}));
			$cz = undefined;
		} else if ($cy[0] === 5) {
			const from = $cy[1];
			const to = $cy[2];
			$cz = extend_run(runs2, from, to);
		} else {
			$cz = undefined;
		}
		return $cz;
	}, (change3, stage) => {
		const $cN = stage;
		let $cO = null;
		if ($cN[0] === 2) {
			record(log, [ 2, all_of(storage) ]);
			return;
			$cO = undefined;
		} else if ($cN[0] === 3) {
			return;
		} else {
			$cO = undefined;
		}
		$cO;
		const $cS = stage;
		const late = $cS[0] === 1;
		const $cT = change3;
		let $cU = null;
		if ($cT[0] === 0) {
			const at4 = $cT[1];
			const removed = $cT[2];
			const inserted = $cT[3];
			if (late && removed > 0) {
				record(log, [ 2, all_of(storage) ]);
			} else if (late) {
				record(log, [ 0, at4, [  ], arrived_at(storage, at4, inserted) ]);
			} else {
				record(log, [ 0, at4, __clone(leaving.v), arrived_at(storage, at4, inserted) ]);
			}
			$cU = undefined;
		} else if ($cT[0] === 1) {
			const at5 = $cT[1];
			const count2 = $cT[2];
			record(log, [ 0, at5, [  ], arrived_at(storage, at5, count2) ]);
			$cU = undefined;
		} else if ($cT[0] === 3) {
			const at6 = $cT[1];
			const _count = $cT[2];
			if (late) {
				record(log, [ 2, all_of(storage) ]);
			} else {
				record(log, [ 0, at6, __clone(leaving.v), [  ] ]);
			}
			$cU = undefined;
		} else if ($cT[0] === 2) {
			const at7 = $cT[1];
			$cU = record_set(log, storage, leaving, at7, late);
		} else if ($cT[0] === 4) {
			const at8 = $cT[1];
			const _part = $cT[2];
			$cU = record_set(log, storage, leaving, at8, late);
		} else if ($cT[0] === 5) {
			const from = $cT[1];
			const to = $cT[2];
			if (skipping.v > 0) {
				const $cX = [ skipping, "v" ];
				skipping.v = $cX[0][$cX[1]] - 1;
				return;
			}
			if (runs2.v.length === 0) {
				record(log, [ 3, from, 1, to ]);
				return;
			}
			const $cY = __remove_at(runs2.v, 0, "std/src/reactive/lens.vl:1447:47");
			const start = $cY[0];
			const count3 = $cY[1];
			const first2 = $cY[2];
			skipping.v = count3 - 1;
			let $cZ = null;
			if (first2 > start) {
				$cZ = first2 + 1 - count3;
			} else {
				$cZ = first2;
			}
			const landing = $cZ;
			record(log, [ 3, start, count3, landing ]);
			$cU = undefined;
		} else {
			$cU = record(log, [ 2, all_of(storage) ]);
		}
		return $cU;
	});
	const feed4 = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release4(feed4);
	}, $cx);
	return feed4;
}
function items(self, $cw) {
	return items_into(self, new9(), $cw);
}
function map(self, transform) {
	return [ self, transform ];
}
function cursor(self) {
	const minted = [ fresh_id(), __shared_new(self[1].v) ];
	self[3].v.push(__clone(minted));
	return minted;
}
function cursor2(self) {
	return cursor(self[1]);
}
function since(self, cursor6) {
	const at4 = cursor6[1].v;
	const version = self[1].v;
	let ops2 = [  ];
	if (at4 >= version) {
		return [ 0, ops2 ];
	}
	cursor6[1].v = version;
	const base = self[2].v;
	if (at4 < base) {
		return [ 1 ];
	}
	let index2 = as_usize(at4 - base);
	const held2 = __clone(self[0].v);
	const length4 = held2.length;
	while (index2 < length4) {
		const $de = __list_get(held2, index2);
		let $df = null;
		if ($de[0] === 0) {
			const op = $de[1];
			$df = ops2.push(op);
		} else {
			$df = undefined;
		}
		$df;
		index2 = index2 + 1;
	}
	return [ 0, ops2 ];
}
function all2(self) {
	return all_of(self[0]);
}
function since2(self, cursor6) {
	const $dg = since(self[1], cursor6);
	let $dh = null;
	if ($dg[0] === 0) {
		const recorded = $dg[1];
		$dh = recorded;
	} else {
		$dh = [ [ 2, all2(self[0]) ] ];
	}
	return $dh;
}
function reader(self) {
	return (cursor6) => {
		return since2(self, cursor6);
	};
}
function get6(self) {
	return all2(self[0]);
}
function on_settle6(self, subscriber) {
	return on_settle(self[0][4], subscriber);
}
function drop_cursor(self, cursor6) {
	let kept = [  ];
	for (const held2 of self[3].v) {
		if (held2[0] !== cursor6[0]) {
			kept.push(__clone(held2));
		}
	}
	self[3].v = kept;
}
function drop_cursor2(self, cursor6) {
	drop_cursor(self[1], cursor6);
}
function open2(self) {
	const cursor6 = cursor2(self);
	const read = reader(self);
	return [ () => {
		return get6(self);
	}, (subscriber) => {
		return on_settle6(self, subscriber);
	}, () => {
		return read(cursor6);
	}, () => {
		return drop_cursor2(self, cursor6);
	} ];
}
function run_element(stage, body) {
	const nursery2 = stage_nursery(stage);
	const value = (($dn) => {
		return (($do) => {
			return body($dn, $do);
		})(nursery2);
	})(stage);
	return [ value, split_registrations(stage) ];
}
function start_element_value(self) {
	return [ 0, self ];
}
function element_core(stage, run) {
	const ids = [  ];
	const items2 = [  ];
	const values = [  ];
	const holds3 = new Map();
	const dirty = [  ];
	const consumer = [ 1 ];
	return [ __shared_new(ids), __shared_new(items2), __shared_new(values), __shared_new(holds3), __shared_new(dirty), __shared_new(0), __clone(stage), __shared_new(consumer), run ];
}
function keep_hold(core, id, hold) {
	element_holds_allocated_count.v = element_holds_allocated_count.v + 1;
	core[3].v.set(hash2(id), __clone(hold));
}
function admit(core, item2) {
	const id = core[5].v;
	core[5].v = id + 1;
	const $dq = core[8](item2);
	const answer = $dq[0];
	const owner2 = $dq[1];
	const $dr = answer;
	let $ds = null;
	if ($dr[0] === 0) {
		const value = $dr[1];
		const $dt = owner2;
		let $du = null;
		if ($dt[0] === 0) {
			const held2 = $dt[1];
			$du = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held2) ] ]);
		} else {
			$du = undefined;
		}
		$du;
		$ds = [ id, __clone(value) ];
	} else {
		const instance = $dr[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $dv = consumer.v;
			let $dw = null;
			if ($dv[0] === 0) {
				const downstream = $dv[1];
				$dw = wake(downstream);
			} else {
				$dw = undefined;
			}
			return $dw;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner2 ]);
		$ds = [ id, pull() ];
	}
	return $ds;
}
function seed(core, items2) {
	for (const item2 of items2) {
		const $dB = admit(core, item2);
		const id = $dB[0];
		const value = $dB[1];
		core[0].v.push(id);
		core[1].v.push(__clone(item2));
		core[2].v.push(value);
	}
}
function fresh_list(list2) {
	let copy = [  ];
	for (const item2 of list2) {
		copy.push(__clone(item2));
	}
	return copy;
}
function core_attach(core, upstream) {
	const consumer = core[7];
	const attach2 = upstream[1];
	return (subscriber) => {
		consumer.v = [ 0, __clone(subscriber) ];
		return attach2(subscriber);
	};
}
function held_pairs(core, at4, count2) {
	let pairs = [  ];
	let index2 = at4;
	while (index2 < at4 + count2) {
		pairs.push([ __clone(__at(core[1].v, index2, "std/src/reactive/delta.vl:1618:15")), __clone(__at(core[2].v, index2, "std/src/reactive/delta.vl:1618:41")) ]);
		index2 = index2 + 1;
	}
	return pairs;
}
function is_none(self) {
	const $dX = self;
	return $dX[0] === 1;
}
function retire(core, id) {
	const $dG = __map_get(core[3].v, hash2(id));
	let $dH = null;
	if ($dG[0] === 0) {
		const hold = $dG[1];
		core[3].v.delete(hash2(id));
		const $dI = hold[2];
		let $dJ = null;
		if ($dI[0] === 0) {
			const handle = $dI[1];
			$dJ = detach(handle);
		} else {
			$dJ = undefined;
		}
		$dJ;
		hold[3]();
		const $dT = hold[4];
		let $dU = null;
		if ($dT[0] === 0) {
			const owner2 = $dT[1];
			$dU = dispose2(owner2);
		} else {
			$dU = undefined;
		}
		$dH = $dU;
	} else {
		$dH = undefined;
	}
	return $dH;
}
function all_pairs(core) {
	return held_pairs(core, 0, core[0].v.length);
}
function resolve(core, ops2) {
	let events = [  ];
	for (const op of ops2) {
		const $dC = op;
		let $dD = null;
		if ($dC[0] === 0) {
			const at4 = $dC[1];
			const removed = $dC[2];
			const inserted = $dC[3];
			const held2 = core[0].v.length;
			const start = splice_start(held2, at4);
			const taking = splice_count(held2, start, removed.length);
			const left = held_pairs(core, start, taking);
			let taken = 0;
			while (taken < taking) {
				const id = __remove_at(core[0].v, start, "std/src/reactive/delta.vl:1642:32");
				retire(core, id);
				__remove_at(core[1].v, start, "std/src/reactive/delta.vl:1644:37");
				__remove_at(core[2].v, start, "std/src/reactive/delta.vl:1645:39");
				taken = taken + 1;
			}
			let arrived = [  ];
			let offset = 0;
			for (const item2 of inserted) {
				const $ef = admit(core, item2);
				const id2 = $ef[0];
				const value = $ef[1];
				__insert_at(core[0].v, start + offset, id2, "std/src/reactive/delta.vl:1652:23");
				__insert_at(core[1].v, start + offset, __clone(item2), "std/src/reactive/delta.vl:1653:25");
				__insert_at(core[2].v, start + offset, __clone(value), "std/src/reactive/delta.vl:1654:26");
				arrived.push([ __clone(item2), value ]);
				offset = offset + 1;
			}
			events.push([ 0, start, left, arrived ]);
			$dD = undefined;
		} else if ($dC[0] === 1) {
			const at5 = $dC[1];
			const _was = $dC[2];
			const item3 = $dC[3];
			const $eg = __list_get(core[0].v, at5);
			let $eh = null;
			if ($eg[0] === 0) {
				const old_id = $eg[1];
				const before = [ __clone(__at(core[1].v, at5, "std/src/reactive/delta.vl:1663:21")), __clone(__at(core[2].v, at5, "std/src/reactive/delta.vl:1663:44")) ];
				retire(core, old_id);
				const $ei = admit(core, item3);
				const id3 = $ei[0];
				const value2 = $ei[1];
				__at_put(core[0].v, at5, id3, "std/src/reactive/delta.vl:1669:7");
				__at_put(core[1].v, at5, __clone(item3), "std/src/reactive/delta.vl:1670:7");
				__at_put(core[2].v, at5, __clone(value2), "std/src/reactive/delta.vl:1671:7");
				events.push([ 1, at5, before, [ __clone(item3), value2 ], true ]);
				$eh = undefined;
			} else {
				$eh = undefined;
			}
			$dD = $eh;
		} else if ($dC[0] === 2) {
			const items2 = $dC[1];
			const left2 = all_pairs(core);
			for (const id4 of __clone(core[0].v)) {
				retire(core, id4);
			}
			core[0].v = [  ];
			core[1].v = [  ];
			core[2].v = [  ];
			seed(core, items2);
			events.push([ 2, left2, all_pairs(core) ]);
			$dD = undefined;
		} else {
			const from = $dC[1];
			const count2 = $dC[2];
			const to = $dC[3];
			let lifted_ids = [  ];
			let lifted_items = [  ];
			let lifted_values = [  ];
			let taken2 = 0;
			while (taken2 < count2) {
				lifted_ids.push(__remove_at(core[0].v, from, "std/src/reactive/delta.vl:1694:39"));
				lifted_items.push(__remove_at(core[1].v, from, "std/src/reactive/delta.vl:1695:43"));
				lifted_values.push(__remove_at(core[2].v, from, "std/src/reactive/delta.vl:1696:45"));
				taken2 = taken2 + 1;
			}
			let offset2 = 0;
			while (offset2 < count2) {
				__insert_at(core[0].v, to + offset2, __at(lifted_ids, offset2, "std/src/reactive/delta.vl:1701:43"), "std/src/reactive/delta.vl:1701:23");
				__insert_at(core[1].v, to + offset2, __clone(__at(lifted_items, offset2, "std/src/reactive/delta.vl:1702:45")), "std/src/reactive/delta.vl:1702:25");
				__insert_at(core[2].v, to + offset2, __clone(__at(lifted_values, offset2, "std/src/reactive/delta.vl:1703:46")), "std/src/reactive/delta.vl:1703:26");
				offset2 = offset2 + 1;
			}
			events.push([ 3, from, count2, to ]);
			$dD = undefined;
		}
		$dD;
	}
	const woke = __clone(core[4].v);
	let $en = null;
	if (woke.length > 0) {
		core[4].v = [  ];
		let positions = new Map();
		let index2 = 0;
		for (const id5 of core[0].v) {
			positions.set(hash2(id5), index2);
			index2 = index2 + 1;
		}
		let seen = new Map();
		for (const id6 of woke) {
			if (seen.has(hash2(id6))) {
				continue;
			}
			seen.set(hash2(id6), true);
			const $ej = __map_get(positions, hash2(id6));
			let $ek = null;
			if ($ej[0] === 0) {
				const at6 = $ej[1];
				const $el = __map_get(core[3].v, hash2(id6));
				let $em = null;
				if ($el[0] === 0) {
					const hold = $el[1];
					if (hold[0]) {
						const item4 = __clone(__at(core[1].v, at6, "std/src/reactive/delta.vl:1731:19"));
						const before2 = __clone(__at(core[2].v, at6, "std/src/reactive/delta.vl:1732:21"));
						const now = hold[1]();
						__at_put(core[2].v, at6, __clone(now), "std/src/reactive/delta.vl:1734:8");
						events.push([ 1, at6, [ __clone(item4), before2 ], [ item4, now ], false ]);
					}
					$em = undefined;
				} else {
					$em = undefined;
				}
				$ek = $em;
			} else {
				$ek = undefined;
			}
			$ek;
		}
		$en = undefined;
	}
	$en;
	return events;
}
function map2(self, fn) {
	let result = [  ];
	for (const item2 of self) {
		result.push(fn(item2));
	}
	return result;
}
function mapped_ops(events) {
	let ops2 = [  ];
	for (const event of events) {
		const $eo = event;
		let $ep = null;
		if ($eo[0] === 0) {
			const at4 = $eo[1];
			const left = $eo[2];
			const arrived = $eo[3];
			ops2.push([ 0, at4, map2(left, (pair) => {
				return __clone(pair[1]);
			}), map2(arrived, (pair) => {
				return __clone(pair[1]);
			}) ]);
			$ep = undefined;
		} else if ($eo[0] === 1) {
			const at5 = $eo[1];
			const before = $eo[2];
			const after = $eo[3];
			const _rerun = $eo[4];
			ops2.push([ 1, at5, __clone(before[1]), __clone(after[1]) ]);
			$ep = undefined;
		} else if ($eo[0] === 2) {
			const _left = $eo[1];
			const now = $eo[2];
			$ep = ops2.push([ 2, map2(now, (pair) => {
				return __clone(pair[1]);
			}) ]);
		} else {
			const from = $eo[1];
			const count2 = $eo[2];
			const to = $eo[3];
			$ep = ops2.push([ 3, from, count2, to ]);
		}
		$ep;
	}
	return ops2;
}
function release_core(core) {
	for (const id of __clone(core[0].v)) {
		retire(core, id);
	}
	dispose2(core[6]);
}
function core_release(core, upstream) {
	const release10 = upstream[3];
	return () => {
		release_core(core);
		release10();
		return;
	};
}
function open3(self) {
	const transform = self[1];
	const upstream = open2(__clone(self[0]));
	const stage = new4();
	const core = element_core(stage, (item2) => {
		const $dp = run_element(stage, ($di, $dj) => {
			return transform(item2, $di, $dj);
		});
		const answer = $dp[0];
		const owner2 = $dp[1];
		return [ start_element_value(answer), owner2 ];
	});
	seed(core, upstream[0]());
	const drain2 = upstream[2];
	return [ () => {
		return fresh_list(core[2].v);
	}, core_attach(core, upstream), () => {
		return mapped_ops(resolve(core, drain2()));
	}, core_release(core, upstream) ];
}
function list_cell(elements, log) {
	return [ __shared_new(elements), new7(0, "std/src/reactive/delta.vl:792:68"), __clone(log) ];
}
function of(elements) {
	return list_cell(elements, new9());
}
function size3(self) {
	return self[0].v.length;
}
function splice_in_place(list2, start, taking, inserted) {
	let left = [  ];
	let taken = 0;
	while (taken < taking) {
		left.push(__remove_at(list2, start, "std/src/reactive/delta.vl:695:18"));
		taken = taken + 1;
	}
	let offset = 0;
	const arriving = inserted.length;
	while (offset < arriving) {
		const $ew = __list_get(inserted, offset);
		let $ex = null;
		if ($ew[0] === 0) {
			const value = $ew[1];
			$ex = __insert_at(list2, start + offset, value, "std/src/reactive/delta.vl:702:28");
		} else {
			$ex = undefined;
		}
		$ex;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function at2(self) {
	return self[1].v;
}
function set2(self, value, $ez, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $ez);
}
function publish(cell, $ey) {
	set2(cell[1], at2(cell[2]), $ey, "std/src/reactive/delta.vl:799:17");
}
function splice4(self, at4, removed, inserted, $ev) {
	const held2 = size3(self);
	const start = splice_start(held2, at4);
	const taking = splice_count(held2, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	record(self[2], splice_in_place(self[0].v, start, taking, inserted));
	publish(self, $ev);
}
function set_at(self, at4, value, $eA) {
	const $eB = __list_get(self[0].v, at4);
	let $eC = null;
	if ($eB[0] === 0) {
		const previous = $eB[1];
		__at_put(self[0].v, at4, __clone(value), "std/src/reactive/delta.vl:855:5");
		record(self[2], [ 1, at4, previous, __clone(value) ]);
		publish(self, $eA);
		$eC = undefined;
	} else {
		$eC = undefined;
	}
	return $eC;
}
function set3(self, value, $eD, caller) {
	self[0].v = __clone(value);
	record(self[2], [ 2, __clone(value) ]);
	publish(self, $eD);
}
function move_range(self, from, count2, to, $eE) {
	if (count2 === 0 || from === to) {
		return;
	}
	let lifted = [  ];
	let taken = 0;
	while (taken < count2) {
		lifted.push(__remove_at(self[0].v, from, "std/src/reactive/delta.vl:875:35"));
		taken = taken + 1;
	}
	let offset = 0;
	while (offset < lifted.length) {
		const $eF = __list_get(lifted, offset);
		let $eG = null;
		if ($eF[0] === 0) {
			const value = $eF[1];
			$eG = __insert_at(self[0].v, to + offset, value, "std/src/reactive/delta.vl:881:43");
		} else {
			$eG = undefined;
		}
		$eG;
		offset = offset + 1;
	}
	record(self[2], [ 3, from, count2, to ]);
	publish(self, $eE);
}
function apply_ops(cell, ops2, $es) {
	for (const op of ops2) {
		const $et = op;
		let $eu = null;
		if ($et[0] === 0) {
			const at4 = $et[1];
			const removed = $et[2];
			const inserted = $et[3];
			$eu = splice4(cell, at4, removed.length, inserted, $es);
		} else if ($et[0] === 1) {
			const at5 = $et[1];
			const _was = $et[2];
			const value = $et[3];
			$eu = set_at(cell, at5, value, $es);
		} else if ($et[0] === 2) {
			const items2 = $et[1];
			$eu = set3(cell, items2, $es, "std/src/reactive/delta.vl:1325:36");
		} else {
			const from = $et[1];
			const count2 = $et[2];
			const to = $et[3];
			$eu = move_range(cell, from, count2, to, $es);
		}
		$eu;
	}
}
function take(self, item2, $eN) {
	defer(self, () => {
		dispose(item2, $eN);
		return;
	});
	return __clone(item2);
}
function seal_collection(instance, owned, $eq, $er, caller) {
	const sealed = of(fresh_list(instance[0]()));
	const drain2 = instance[2];
	if (owned) {
		const refresh = subscriber_of(() => {
			return apply_ops(sealed, drain2(), $eq);
		}, true);
		place_subscriber(refresh[0], caller);
		const refreshed = instance[1](refresh);
		also_releasing(refreshed, instance[3]);
		register_with_owner(refreshed, $eq, $er);
	} else {
		const cell_global_refresh = () => {
			return apply_ops(sealed, drain2(), $eq);
		};
		const refresh2 = subscriber_of(cell_global_refresh, true);
		place_subscriber(refresh2[0], caller);
		instance[1](refresh2);
	}
	return sealed;
}
function memo(self, $dc, $dd, caller) {
	return [ seal_collection(open3(self), true, $dc, $dd, caller) ];
}
function filter(self, keep) {
	return [ self, keep ];
}
function start_element(self) {
	return [ 0, self ];
}
function admit2(core, item2) {
	const id = core[5].v;
	core[5].v = id + 1;
	const $eW = core[8](item2);
	const answer = $eW[0];
	const owner2 = $eW[1];
	const $eX = answer;
	let $eY = null;
	if ($eX[0] === 0) {
		const value = $eX[1];
		const $eZ = owner2;
		let $fa = null;
		if ($eZ[0] === 0) {
			const held2 = $eZ[1];
			$fa = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held2) ] ]);
		} else {
			$fa = undefined;
		}
		$fa;
		$eY = [ id, __clone(value) ];
	} else {
		const instance = $eX[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $fb = consumer.v;
			let $fc = null;
			if ($fb[0] === 0) {
				const downstream = $fb[1];
				$fc = wake(downstream);
			} else {
				$fc = undefined;
			}
			return $fc;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner2 ]);
		$eY = [ id, pull() ];
	}
	return $eY;
}
function seed2(core, items2) {
	for (const item2 of items2) {
		const $fd = admit2(core, item2);
		const id = $fd[0];
		const value = $fd[1];
		core[0].v.push(id);
		core[1].v.push(__clone(item2));
		core[2].v.push(value);
	}
}
function retire2(core, id) {
	const $fi = __map_get(core[3].v, hash2(id));
	let $fj = null;
	if ($fi[0] === 0) {
		const hold = $fi[1];
		core[3].v.delete(hash2(id));
		const $fk = hold[2];
		let $fl = null;
		if ($fk[0] === 0) {
			const handle = $fk[1];
			$fl = detach(handle);
		} else {
			$fl = undefined;
		}
		$fl;
		hold[3]();
		const $fm = hold[4];
		let $fn = null;
		if ($fm[0] === 0) {
			const owner2 = $fm[1];
			$fn = dispose2(owner2);
		} else {
			$fn = undefined;
		}
		$fj = $fn;
	} else {
		$fj = undefined;
	}
	return $fj;
}
function resolve2(core, ops2) {
	let events = [  ];
	for (const op of ops2) {
		const $fg = op;
		let $fh = null;
		if ($fg[0] === 0) {
			const at4 = $fg[1];
			const removed = $fg[2];
			const inserted = $fg[3];
			const held2 = core[0].v.length;
			const start = splice_start(held2, at4);
			const taking = splice_count(held2, start, removed.length);
			const left = held_pairs(core, start, taking);
			let taken = 0;
			while (taken < taking) {
				const id = __remove_at(core[0].v, start, "std/src/reactive/delta.vl:1642:32");
				retire2(core, id);
				__remove_at(core[1].v, start, "std/src/reactive/delta.vl:1644:37");
				__remove_at(core[2].v, start, "std/src/reactive/delta.vl:1645:39");
				taken = taken + 1;
			}
			let arrived = [  ];
			let offset = 0;
			for (const item2 of inserted) {
				const $fo = admit2(core, item2);
				const id2 = $fo[0];
				const value = $fo[1];
				__insert_at(core[0].v, start + offset, id2, "std/src/reactive/delta.vl:1652:23");
				__insert_at(core[1].v, start + offset, __clone(item2), "std/src/reactive/delta.vl:1653:25");
				__insert_at(core[2].v, start + offset, __clone(value), "std/src/reactive/delta.vl:1654:26");
				arrived.push([ __clone(item2), value ]);
				offset = offset + 1;
			}
			events.push([ 0, start, left, arrived ]);
			$fh = undefined;
		} else if ($fg[0] === 1) {
			const at5 = $fg[1];
			const _was = $fg[2];
			const item3 = $fg[3];
			const $fp = __list_get(core[0].v, at5);
			let $fq = null;
			if ($fp[0] === 0) {
				const old_id = $fp[1];
				const before = [ __clone(__at(core[1].v, at5, "std/src/reactive/delta.vl:1663:21")), __clone(__at(core[2].v, at5, "std/src/reactive/delta.vl:1663:44")) ];
				retire2(core, old_id);
				const $fr = admit2(core, item3);
				const id3 = $fr[0];
				const value2 = $fr[1];
				__at_put(core[0].v, at5, id3, "std/src/reactive/delta.vl:1669:7");
				__at_put(core[1].v, at5, __clone(item3), "std/src/reactive/delta.vl:1670:7");
				__at_put(core[2].v, at5, __clone(value2), "std/src/reactive/delta.vl:1671:7");
				events.push([ 1, at5, before, [ __clone(item3), value2 ], true ]);
				$fq = undefined;
			} else {
				$fq = undefined;
			}
			$fh = $fq;
		} else if ($fg[0] === 2) {
			const items2 = $fg[1];
			const left2 = all_pairs(core);
			for (const id4 of __clone(core[0].v)) {
				retire2(core, id4);
			}
			core[0].v = [  ];
			core[1].v = [  ];
			core[2].v = [  ];
			seed2(core, items2);
			events.push([ 2, left2, all_pairs(core) ]);
			$fh = undefined;
		} else {
			const from = $fg[1];
			const count2 = $fg[2];
			const to = $fg[3];
			let lifted_ids = [  ];
			let lifted_items = [  ];
			let lifted_values = [  ];
			let taken2 = 0;
			while (taken2 < count2) {
				lifted_ids.push(__remove_at(core[0].v, from, "std/src/reactive/delta.vl:1694:39"));
				lifted_items.push(__remove_at(core[1].v, from, "std/src/reactive/delta.vl:1695:43"));
				lifted_values.push(__remove_at(core[2].v, from, "std/src/reactive/delta.vl:1696:45"));
				taken2 = taken2 + 1;
			}
			let offset2 = 0;
			while (offset2 < count2) {
				__insert_at(core[0].v, to + offset2, __at(lifted_ids, offset2, "std/src/reactive/delta.vl:1701:43"), "std/src/reactive/delta.vl:1701:23");
				__insert_at(core[1].v, to + offset2, __clone(__at(lifted_items, offset2, "std/src/reactive/delta.vl:1702:45")), "std/src/reactive/delta.vl:1702:25");
				__insert_at(core[2].v, to + offset2, __clone(__at(lifted_values, offset2, "std/src/reactive/delta.vl:1703:46")), "std/src/reactive/delta.vl:1703:26");
				offset2 = offset2 + 1;
			}
			events.push([ 3, from, count2, to ]);
			$fh = undefined;
		}
		$fh;
	}
	const woke = __clone(core[4].v);
	let $fw = null;
	if (woke.length > 0) {
		core[4].v = [  ];
		let positions = new Map();
		let index2 = 0;
		for (const id5 of core[0].v) {
			positions.set(hash2(id5), index2);
			index2 = index2 + 1;
		}
		let seen = new Map();
		for (const id6 of woke) {
			if (seen.has(hash2(id6))) {
				continue;
			}
			seen.set(hash2(id6), true);
			const $fs = __map_get(positions, hash2(id6));
			let $ft = null;
			if ($fs[0] === 0) {
				const at6 = $fs[1];
				const $fu = __map_get(core[3].v, hash2(id6));
				let $fv = null;
				if ($fu[0] === 0) {
					const hold = $fu[1];
					if (hold[0]) {
						const item4 = __clone(__at(core[1].v, at6, "std/src/reactive/delta.vl:1731:19"));
						const before2 = __clone(__at(core[2].v, at6, "std/src/reactive/delta.vl:1732:21"));
						const now = hold[1]();
						__at_put(core[2].v, at6, __clone(now), "std/src/reactive/delta.vl:1734:8");
						events.push([ 1, at6, [ __clone(item4), before2 ], [ item4, now ], false ]);
					}
					$fv = undefined;
				} else {
					$fv = undefined;
				}
				$ft = $fv;
			} else {
				$ft = undefined;
			}
			$ft;
		}
		$fw = undefined;
	}
	$fw;
	return events;
}
function kept_ops(kept, index2, events, out, payload_follows) {
	let ops2 = [  ];
	for (const event of events) {
		const $fy = event;
		let $fz = null;
		if ($fy[0] === 0) {
			const at4 = $fy[1];
			const left = $fy[2];
			const arrived = $fy[3];
			const position = kept_before(index2, at4);
			let removed = [  ];
			for (const pair of left) {
				const $fA = out(pair);
				let $fB = null;
				if ($fA[0] === 0) {
					const value = $fA[1];
					$fB = removed.push(value);
				} else {
					$fB = undefined;
				}
				$fB;
			}
			let inserted = [  ];
			let flags = [  ];
			for (const pair2 of arrived) {
				const $fC = out(pair2);
				let $fD = null;
				if ($fC[0] === 0) {
					const value2 = $fC[1];
					inserted.push(value2);
					flags.push(true);
					$fD = undefined;
				} else {
					$fD = flags.push(false);
				}
				$fD;
			}
			let taken = 0;
			while (taken < left.length) {
				__remove_at(kept.v, at4, "std/src/reactive/delta.vl:2153:31");
				taken = taken + 1;
			}
			let offset = 0;
			for (const flag of flags) {
				__insert_at(kept.v, at4 + offset, flag, "std/src/reactive/delta.vl:2158:19");
				offset = offset + 1;
			}
			rebuild_kept(index2, kept.v);
			if (removed.length > 0 || inserted.length > 0) {
				ops2.push([ 0, position, removed, inserted ]);
			}
			$fz = undefined;
		} else if ($fy[0] === 1) {
			const at5 = $fy[1];
			const before = $fy[2];
			const after = $fy[3];
			const rerun = $fy[4];
			const position2 = kept_before(index2, at5);
			const $fE = [ out(before), out(after) ];
			let $fF = null;
			if ($fE[0][0] === 0 && $fE[1][0] === 0) {
				const was = $fE[0][1];
				const now = $fE[1][1];
				if (rerun || payload_follows) {
					ops2.push([ 1, position2, was, now ]);
				}
				$fF = undefined;
			} else if ($fE[0][0] === 0 && $fE[1][0] === 1) {
				const was2 = $fE[0][1];
				__at_put(kept.v, at5, false, "std/src/reactive/delta.vl:2179:7");
				kept_add(index2, at5, -(1));
				ops2.push([ 0, position2, [ was2 ], [  ] ]);
				$fF = undefined;
			} else if ($fE[0][0] === 1 && $fE[1][0] === 0) {
				const now2 = $fE[1][1];
				__at_put(kept.v, at5, true, "std/src/reactive/delta.vl:2184:7");
				kept_add(index2, at5, 1);
				ops2.push([ 0, position2, [  ], [ now2 ] ]);
				$fF = undefined;
			} else {
				$fF = undefined;
			}
			$fz = $fF;
		} else if ($fy[0] === 2) {
			const _left = $fy[1];
			const now3 = $fy[2];
			let flags2 = [  ];
			let values = [  ];
			for (const pair3 of now3) {
				const $fG = out(pair3);
				let $fH = null;
				if ($fG[0] === 0) {
					const value3 = $fG[1];
					values.push(value3);
					flags2.push(true);
					$fH = undefined;
				} else {
					$fH = flags2.push(false);
				}
				$fH;
			}
			kept.v = flags2;
			rebuild_kept(index2, kept.v);
			ops2.push([ 2, values ]);
			$fz = undefined;
		} else {
			const from = $fy[1];
			const count2 = $fy[2];
			const to = $fy[3];
			const out_from = kept_before(index2, from);
			const moved = kept_before(index2, from + count2) - out_from;
			let lifted = [  ];
			let taken2 = 0;
			while (taken2 < count2) {
				lifted.push(__remove_at(kept.v, from, "std/src/reactive/delta.vl:2213:31"));
				taken2 = taken2 + 1;
			}
			let offset2 = 0;
			for (const flag2 of lifted) {
				__insert_at(kept.v, to + offset2, flag2, "std/src/reactive/delta.vl:2218:19");
				offset2 = offset2 + 1;
			}
			rebuild_kept(index2, kept.v);
			const out_to = kept_before(index2, to);
			if (moved > 0 && out_from !== out_to) {
				ops2.push([ 3, out_from, moved, out_to ]);
			}
			$fz = undefined;
		}
		$fz;
	}
	return ops2;
}
function release_core2(core) {
	for (const id of __clone(core[0].v)) {
		retire2(core, id);
	}
	dispose2(core[6]);
}
function core_release2(core, upstream) {
	const release10 = upstream[3];
	return () => {
		release_core2(core);
		release10();
		return;
	};
}
function open4(self) {
	const keep = self[1];
	const upstream = open2(__clone(self[0]));
	const stage = new4();
	const core = element_core(stage, (item2) => {
		const $eV = run_element(stage, ($eT, $eU) => {
			return keep(item2, $eT, $eU);
		});
		const answer = $eV[0];
		const owner2 = $eV[1];
		return [ start_element(answer), owner2 ];
	});
	seed2(core, upstream[0]());
	const kept = __shared_new(fresh_list(core[2].v));
	const index2 = kept_index(kept.v);
	const drain2 = upstream[2];
	return [ () => {
		let out = [  ];
		let at4 = 0;
		for (const item2 of core[1].v) {
			if (__at(core[2].v, at4, "std/src/reactive/delta.vl:2040:9")) {
				out.push(__clone(item2));
			}
			at4 = at4 + 1;
		}
		return out;
	}, core_attach(core, upstream), () => {
		return kept_ops(kept, index2, resolve2(core, drain2()), (pair) => {
			let $fx = null;
			if (pair[1]) {
				$fx = [ 0, __clone(pair[0]) ];
			} else {
				$fx = [ 1 ];
			}
			return $fx;
		}, false);
	}, core_release2(core, upstream) ];
}
function memo2(self, $dc, $dd, caller) {
	return [ seal_collection(open4(self), true, $dc, $dd, caller) ];
}
function get7(self) {
	return __clone(self[0].v);
}
function get8(self) {
	return get7(self[0]);
}
function splice5(self, at4, removed, inserted, $w) {
	const held2 = size2(self);
	let $fM = null;
	if (at4 > held2) {
		$fM = held2;
	} else {
		$fM = at4;
	}
	const start = $fM;
	let $fN = null;
	if (start + removed > held2) {
		$fN = held2 - start;
	} else {
		$fN = removed;
	}
	const taking = $fN;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change3 = [ 0, start, taking, arriving ];
	coming(self, [ __clone(change3) ]);
	mutate(self, (held3) => {
		return splice(held3, start, taking, inserted);
	});
	landed2(self, [ change3 ], false, $w);
}
function insert_at3(self, at4, value, $v) {
	return splice5(self, at4, 0, [ __clone(value) ], $v);
}
function remove_at2(self, at4, $aV) {
	return splice5(self, at4, 1, [  ], $aV);
}
function get9(self, key) {
	return __nmap_get(self[0], hash(key));
}
function remove(self, key) {
	__nmap_remove(self[0], hash(key));
}
function entries(self) {
	let keys2 = __nmap_keys(self[0]);
	let values = __nmap_values(self[0]);
	let backwards = [  ];
	while (!(is_empty(keys2))) {
		const $ge = __list_pop(keys2);
		let $gf = null;
		if ($ge[0] === 0) {
			const key = $ge[1];
			const $gg = __list_pop(values);
			let $gh = null;
			if ($gg[0] === 0) {
				const value = $gg[1];
				$gh = backwards.push([ key, value ]);
			} else {
				$gh = undefined;
			}
			$gf = $gh;
		} else {
			$gf = undefined;
		}
		$gf;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $gi = __list_pop(backwards);
		let $gj = null;
		if ($gi[0] === 0) {
			const pair = $gi[1];
			$gj = result.push(pair);
		} else {
			$gj = undefined;
		}
		$gj;
	}
	return result;
}
function release5(self) {
	untap(self[0], self[2]);
	for (const _ of entries(self[1].v)) {
		const $gu = _;
		const _key = $gu[0];
		const id = $gu[1];
		release_id(self[0][1], self[0][3], id);
	}
	self[1].v = new8();
}
function index_by(self, key, $gb) {
	const found = __shared_new(new2());
	const leaving = __shared_new([  ]);
	const storage = __clone(self[0]);
	const handler = self[1];
	const cells = self[3];
	const view = self[2][1];
	const key_at = (at4) => {
		let out = [ 1 ];
		peek_storage2(storage, (held2) => {
			return view(held2, at4, (item2) => {
				return out = [ 0, key(item2) ];
			});
		});
		return out;
	};
	const map4 = (held2, at4) => {
		const id = follow_id2(storage, handler, cells, view, at4);
		const before = get9(found.v, held2);
		insert(found.v, held2, id);
		const $gc = before;
		if ($gc[0] === 0) {
			release_id(handler, cells, $gc[1]);
		}
		return;
	};
	const unmap = (held2) => {
		const $gd = get9(found.v, held2);
		if ($gd[0] === 0) {
			remove(found.v, held2);
			release_id(handler, cells, $gd[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $gk = _;
			const _held = $gk[0];
			const old = $gk[1];
			release_id(handler, cells, old);
		}
		found.v = new8();
		let at4 = 0;
		const count2 = peek_storage2(storage, (held2) => {
			return count(held2);
		});
		while (at4 < count2) {
			const $gl = key_at(at4);
			if ($gl[0] === 0) {
				map4($gl[1], at4);
			}
			at4 = at4 + 1;
		}
		return;
	};
	fill();
	const note = (at4, count2) => {
		let gone = [  ];
		let index3 = at4;
		while (index3 < at4 + count2) {
			const $gm = key_at(index3);
			if ($gm[0] === 0) {
				gone.push($gm[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone;
		return;
	};
	const arrive = (at4, count2) => {
		let index3 = at4;
		while (index3 < at4 + count2) {
			const $gn = key_at(index3);
			if ($gn[0] === 0) {
				map4($gn[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change3) => {
		const $go = change3;
		let $gp = null;
		if ($go[0] === 0) {
			const at4 = $go[1];
			const removed = $go[2];
			const _inserted = $go[3];
			$gp = note(at4, removed);
		} else if ($go[0] === 3) {
			const at5 = $go[1];
			const count2 = $go[2];
			$gp = note(at5, count2);
		} else if ($go[0] === 2) {
			const at6 = $go[1];
			$gp = note(at6, 1);
		} else if ($go[0] === 4) {
			const at7 = $go[1];
			const _part = $go[2];
			$gp = note(at7, 1);
		} else {
			$gp = undefined;
		}
		return $gp;
	}, (change3, stage) => {
		const $gq = stage;
		let $gr = null;
		if ($gq[0] === 0) {
			$gr = undefined;
		} else if ($gq[0] === 3) {
			return;
		} else {
			fill();
			return;
			$gr = undefined;
		}
		$gr;
		const $gs = change3;
		let $gt = null;
		if ($gs[0] === 0) {
			const at4 = $gs[1];
			const _removed = $gs[2];
			const inserted = $gs[3];
			for (const held2 of leaving.v) {
				unmap(held2);
			}
			arrive(at4, inserted);
			$gt = undefined;
		} else if ($gs[0] === 1) {
			const at5 = $gs[1];
			const count2 = $gs[2];
			$gt = arrive(at5, count2);
		} else if ($gs[0] === 3) {
			const _at = $gs[1];
			const _count = $gs[2];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			$gt = undefined;
		} else if ($gs[0] === 2) {
			const at6 = $gs[1];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			arrive(at6, 1);
			$gt = undefined;
		} else if ($gs[0] === 4) {
			const at7 = $gs[1];
			const _part = $gs[2];
			for (const held5 of leaving.v) {
				unmap(held5);
			}
			arrive(at7, 1);
			$gt = undefined;
		} else if ($gs[0] === 5) {
			const _from = $gs[1];
			const _to = $gs[2];
			$gt = undefined;
		} else {
			$gt = fill();
		}
		return $gt;
	});
	const index2 = [ __clone(self), found, tap2 ];
	defer_to_owner(() => {
		return release5(index2);
	}, $gb);
	return index2;
}
function map3(self, fn) {
	const $gv = self;
	let $gw = null;
	if ($gv[0] === 0) {
		const x = $gv[1];
		$gw = [ 0, fn(x) ];
	} else {
		$gw = [ 1 ];
	}
	return $gw;
}
function find(self, key) {
	const lens2 = __clone(self[0]);
	return map3(viewing2(self[1], (held2) => {
		return get9(held2, key);
	}), (id) => {
		return slot_of(lens2, id);
	});
}
function eq7(self, b2) {
	const $gz = self;
	let $gC = null;
	if ($gz[0] === 0) {
		const $gA = b2;
		let $gB = null;
		if ($gA[0] === 0) {
			$gB = eq5($gz[1], $gA[1]);
		} else {
			$gB = false;
		}
		$gC = $gB;
	} else {
		const $gD = b2;
		$gC = $gD[0] === 1;
	}
	return $gC;
}
function remove_at3(self, at4, $aV) {
	return splice3(self, at4, 1, [  ], $aV);
}
function index(self) {
	return [ __clone(self) ];
}
function get10(self) {
	return at(self[0]);
}
function on_settle7(self, subscriber) {
	return on_settle(self[0][0][5], subscriber);
}
function attach_observer5(self, observer, immediately) {
	const subscription = on_settle7(self, mint_subscriber(() => {
		return observer(get10(self));
	}));
	if (immediately) {
		observer(get10(self));
	}
	return subscription;
}
function on_change5(self, observer, caller) {
	return place_subscription(attach_observer5(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function release6(self) {
	untap(self[0], self[3]);
	for (const id of self[1].v) {
		release_id(self[0][1], self[0][3], id);
	}
	self[1].v = [  ];
}
function rows(self, $gF) {
	const log = new9();
	const held2 = __shared_new([  ]);
	const storage = __clone(self[0]);
	const handler = self[1];
	const cells = self[3];
	const view = self[2][1];
	const fill = () => {
		let ids = [  ];
		let at4 = 0;
		const count2 = peek_storage2(storage, (list2) => {
			return count(list2);
		});
		while (at4 < count2) {
			ids.push(follow_id(storage, handler, cells, view, at4));
			at4 = at4 + 1;
		}
		return ids;
	};
	const leave = (at4, count2) => {
		let gone = [  ];
		let taken = 0;
		while (taken < count2) {
			const id = __remove_at(held2.v, at4, "std/src/reactive/lens.vl:1608:27");
			release_id(handler, cells, id);
			gone.push(id);
			taken = taken + 1;
		}
		return gone;
	};
	const arrive = (at4, count2) => {
		let arrived = [  ];
		let index2 = at4;
		while (index2 < at4 + count2) {
			const id = follow_id(storage, handler, cells, view, index2);
			__insert_at(held2.v, index2, id, "std/src/reactive/lens.vl:1620:18");
			arrived.push(id);
			index2 = index2 + 1;
		}
		return arrived;
	};
	held2.v = fill();
	const tap2 = tap(self, (_change) => {
		return;
	}, (change3, _stage) => {
		const $gG = change3;
		let $gH = null;
		if ($gG[0] === 0) {
			const at4 = $gG[1];
			const removed = $gG[2];
			const inserted = $gG[3];
			const gone = leave(at4, removed);
			record(log, [ 0, at4, gone, arrive(at4, inserted) ]);
			$gH = undefined;
		} else if ($gG[0] === 1) {
			const at5 = $gG[1];
			const count2 = $gG[2];
			$gH = record(log, [ 0, at5, [  ], arrive(at5, count2) ]);
		} else if ($gG[0] === 3) {
			const at6 = $gG[1];
			const count3 = $gG[2];
			$gH = record(log, [ 0, at6, leave(at6, count3), [  ] ]);
		} else if ($gG[0] === 5) {
			const from = $gG[1];
			const to = $gG[2];
			const id = __remove_at(held2.v, from, "std/src/reactive/lens.vl:1636:28");
			__insert_at(held2.v, to, id, "std/src/reactive/lens.vl:1637:19");
			record(log, [ 3, from, 1, to ]);
			$gH = undefined;
		} else if ($gG[0] === 6) {
			for (const id2 of __clone(held2.v)) {
				release_id(handler, cells, id2);
			}
			held2.v = fill();
			record(log, [ 2, __clone(held2.v) ]);
			$gH = undefined;
		} else {
			$gH = undefined;
		}
		return $gH;
	});
	const feed4 = [ __clone(self), held2, __clone(log), tap2 ];
	defer_to_owner(() => {
		return release6(feed4);
	}, $gF);
	return feed4;
}
function handles(self, ids) {
	let out = [  ];
	for (const id of ids) {
		out.push(slot_of(self[0], id));
	}
	return out;
}
function get11(self) {
	return handles(self, self[1].v);
}
function comp(body) {
	const scope = new4();
	const result = body(scope);
	return [ ...result, scope ];
}
function find2(self, key) {
	return get9(self, key);
}
function store(self, key, value) {
	insert(self, key, value);
}
function keyed(value) {
	return over2(value, new6(), [ (held2, key) => {
		return find2(held2, key);
	}, (held2, key, read) => {
		const $gM = find2(held2, key);
		const item2 = [ $gM[1] ];
		if ($gM[0] === 0) {
			read([ item2, 0 ]);
		}
		return;
	}, (held2, key, value2) => {
		return store(held2, key, value2);
	}, (held2, key, with2) => {
		const $gN = find2(held2, key);
		if ($gN[0] === 0) {

		} else {
			return;
		}
		let item2 = [ $gN[1] ];
		with2([ item2, 0 ]);
		store(held2, key, item2[0]);
		return;
	} ]);
}
function follows_items2(self) {
	return false;
}
function follow4(self, at4) {
	const $gS = get9(self[0], at4);
	if ($gS[0] === 0) {
		return $gS[1];
	}
	const slot6 = as_i32(self[1].length);
	self[1].push(__clone(at4));
	insert(self[0], at4, slot6);
	return slot6;
}
function follow_id3(storage, handler, cells, view, at4) {
	if (viewing2(handler, (held2) => {
		return follows_items2(held2);
	}) && !(holds2(storage, view, at4))) {
		return -(1);
	}
	const id = follow4(handler.v, at4);
	while (cells.v.length <= as_usize(id)) {
		cells.v.push(fresh_cells());
	}
	const slot_cells = viewing2(cells, (held2) => {
		return __clone(__at(held2, as_usize(id), "std/src/reactive/lens.vl:909:41"));
	});
	const $gT = [ slot_cells[2], "v" ];
	slot_cells[2].v = $gT[0][$gT[1]] + 1;
	return id;
}
function follow5(self, at4) {
	return slot_of(self, follow_id3(self[0], self[1], self[3], self[2][1], at4));
}
function release7(self, slot6) {
	const $gU = __list_get(self[1], as_usize(slot6));
	if ($gU[0] === 0) {
		remove(self[0], $gU[1]);
	}
}
function release_id2(handler, cells, id) {
	if (id < 0) {
		return;
	}
	const slot_cells = viewing2(cells, (held2) => {
		return __clone(__at(held2, as_usize(id), "std/src/reactive/lens.vl:919:41"));
	});
	const left = slot_cells[2].v - 1;
	slot_cells[2].v = left;
	if (left <= 0) {
		release7(handler.v, id);
	}
}
function release8(self) {
	release_id2(self[0][1], self[0][3], self[1]);
}
function slot3(self, at4, $b) {
	const handle = follow5(self, at4);
	defer_to_owner(() => {
		return release8(handle);
	}, $b);
	return handle;
}
function locate2(self, slot6) {
	return __list_get(self[1], as_usize(slot6));
}
function at3(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][1], (handler) => {
		return locate2(handler, self[1]);
	});
}
function get12(self) {
	const $gX = at3(self);
	if ($gX[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][2][0];
	return peek2(self[0], (held2) => {
		return read(held2, $gX[1]);
	});
}
function on_settle8(self, subscriber) {
	return on_settle(self[2][0], subscriber);
}
function attach_observer6(self, observer, immediately) {
	const subscription = on_settle8(self, mint_subscriber(() => {
		return observer(get12(self));
	}));
	if (immediately) {
		observer(get12(self));
	}
	return subscription;
}
function on_change6(self, observer, caller) {
	return place_subscription(attach_observer6(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function apply2(self, change3, wakes) {
	const $hp = change3;
	let $hq = null;
	if ($hp[0] === 0) {
		const key = $hp[1];
		const _removed = $hp[2];
		const _inserted = $hp[3];
		const $hr = get9(self[0], key);
		if ($hr[0] === 0) {
			wakes.push([ 0, $hr[1] ]);
		}
		$hq = undefined;
	} else if ($hp[0] === 1) {
		const key2 = $hp[1];
		const _count = $hp[2];
		const $hs = get9(self[0], key2);
		if ($hs[0] === 0) {
			wakes.push([ 0, $hs[1] ]);
		}
		$hq = undefined;
	} else if ($hp[0] === 2) {
		const key3 = $hp[1];
		const $ht = get9(self[0], key3);
		if ($ht[0] === 0) {
			wakes.push([ 0, $ht[1] ]);
		}
		$hq = undefined;
	} else if ($hp[0] === 3) {
		const key4 = $hp[1];
		const _count2 = $hp[2];
		const $hu = get9(self[0], key4);
		if ($hu[0] === 0) {
			wakes.push([ 0, $hu[1] ]);
		}
		$hq = undefined;
	} else if ($hp[0] === 4) {
		const key5 = $hp[1];
		const part2 = $hp[2];
		const $hv = get9(self[0], key5);
		if ($hv[0] === 0) {
			wakes.push([ 1, $hv[1], part2 ]);
		}
		$hq = undefined;
	} else if ($hp[0] === 5) {
		const _from = $hp[1];
		const _to = $hp[2];
		$hq = undefined;
	} else {
		let slot6 = 0;
		const held2 = as_i32(self[1].length);
		while (slot6 < held2) {
			wakes.push([ 0, slot6 ]);
			slot6 = slot6 + 1;
		}
		$hq = undefined;
	}
	return $hq;
}
function landed3(self, changes, late, $F) {
	if (changes.length === 0) {
		return;
	}
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	const tapped = self[7].v.length > 0;
	let index2 = 0;
	for (const change3 of changes) {
		const $hn = change3;
		let $ho = null;
		if ($hn[0] === 0) {
			const _at = $hn[1];
			const removed = $hn[2];
			const inserted = $hn[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$ho = undefined;
		} else if ($hn[0] === 1) {
			const _at2 = $hn[1];
			const _count = $hn[2];
			$ho = resized = true;
		} else if ($hn[0] === 3) {
			const _at3 = $hn[1];
			const _count2 = $hn[2];
			$ho = resized = true;
		} else if ($hn[0] === 6) {
			$ho = resized = true;
		} else if ($hn[0] === 5) {
			const _from = $hn[1];
			const _to = $hn[2];
			$ho = shaped = true;
		} else {
			$ho = undefined;
		}
		$ho;
		apply2(self[1].v, change3, wakes);
		let $hx = null;
		if (tapped) {
			let $hw = null;
			if (!(late)) {
				$hw = [ 0 ];
			} else if (!(several)) {
				$hw = [ 1 ];
			} else if (index2 === 0) {
				$hw = [ 2 ];
			} else {
				$hw = [ 3 ];
			}
			const stage = $hw;
			for (const tap2 of __clone(self[7].v)) {
				tap2[2](change3, stage);
			}
			$hx = undefined;
		}
		$hx;
		index2 = index2 + 1;
	}
	const cells = self[3];
	const published = __clone(self[4]);
	const shape = __clone(self[5]);
	const sized = __clone(self[6]);
	settle_together(($Y) => {
		for (const wake2 of wakes) {
			const $hy = wake2;
			let $hz = null;
			if ($hy[0] === 0) {
				const slot6 = $hy[1];
				$hz = wake_all(cells, slot6, [ 0, $Y ]);
			} else if ($hy[0] === 2) {
				const slot7 = $hy[1];
				$hz = wake_all(cells, slot7, [ 0, $Y ]);
			} else {
				const slot8 = $hy[1];
				const part2 = $hy[2];
				$hz = wake_part(cells, slot8, part2, [ 0, $Y ]);
			}
			$hz;
		}
		if (resized) {
			notify(sized, [ 0, $Y ]);
		}
		if (shaped || resized) {
			notify(shape, [ 0, $Y ]);
		}
		notify(published, [ 0, $Y ]);
		return;
	}, $F);
}
function put2(self, key, value, $he) {
	const had = peek2(self, (held2) => {
		const $hf = find2(held2, key);
		return $hf[0] === 0;
	});
	let $hk = null;
	if (had) {
		$hk = [ 2, __clone(key) ];
	} else {
		$hk = [ 1, __clone(key), 1 ];
	}
	const change3 = $hk;
	coming(self, [ __clone(change3) ]);
	mutate(self, (held2) => {
		return store(held2, key, value);
	});
	landed3(self, [ change3 ], false, $he);
}
function drop(self, key) {
	remove(self, key);
}
function remove2(self, key, $hA) {
	const had = peek2(self, (held2) => {
		const $hB = find2(held2, key);
		return $hB[0] === 0;
	});
	if (!(had)) {
		return;
	}
	coming(self, [ [ 3, __clone(key), 1 ] ]);
	mutate(self, (held2) => {
		return drop(held2, key);
	});
	landed3(self, [ [ 3, __clone(key), 1 ] ], false, $hA);
}
function push3(self, value, $aW) {
	return splice5(self, size2(self), 0, [ __clone(value) ], $aW);
}
function eq8(self, other) {
	const $hE = [ self, other ];
	let $hF = null;
	if ($hE[0][0] === 0 && $hE[1][0] === 0) {
		const s0 = $hE[0][1];
		const s1 = $hE[0][2];
		const s2 = $hE[0][3];
		const o0 = $hE[1][1];
		const o1 = $hE[1][2];
		const o2 = $hE[1][3];
		$hF = eq2(s0, o0) && s1 === o1 && s2 === o2;
	} else if ($hE[0][0] === 1 && $hE[1][0] === 1) {
		const s02 = $hE[0][1];
		const s12 = $hE[0][2];
		const o02 = $hE[1][1];
		const o12 = $hE[1][2];
		$hF = eq2(s02, o02) && s12 === o12;
	} else if ($hE[0][0] === 2 && $hE[1][0] === 2) {
		const s03 = $hE[0][1];
		const o03 = $hE[1][1];
		$hF = eq2(s03, o03);
	} else if ($hE[0][0] === 3 && $hE[1][0] === 3) {
		const s04 = $hE[0][1];
		const s13 = $hE[0][2];
		const o04 = $hE[1][1];
		const o13 = $hE[1][2];
		$hF = eq2(s04, o04) && s13 === o13;
	} else if ($hE[0][0] === 4 && $hE[1][0] === 4) {
		const s05 = $hE[0][1];
		const s14 = $hE[0][2];
		const o05 = $hE[1][1];
		const o14 = $hE[1][2];
		$hF = eq2(s05, o05) && s14 === o14;
	} else if ($hE[0][0] === 5 && $hE[1][0] === 5) {
		const s06 = $hE[0][1];
		const s15 = $hE[0][2];
		const o06 = $hE[1][1];
		const o15 = $hE[1][2];
		$hF = eq2(s06, o06) && eq2(s15, o15);
	} else if ($hE[0][0] === 6 && $hE[1][0] === 6) {
		$hF = true;
	} else {
		$hF = false;
	}
	return $hF;
}
function eq9(self, b2) {
	if (self.length !== b2.length) {
		return false;
	}
	let index2 = 0;
	for (const item2 of self) {
		if (!(eq8(item2, __at(b2, index2, "std/src/compare.vl:147:17")))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function get13(self) {
	return size(self[0]);
}
function attach_observer7(self, observer, immediately) {
	const subscription = on_settle5(self, mint_subscriber(() => {
		return observer(get13(self));
	}));
	if (immediately) {
		observer(get13(self));
	}
	return subscription;
}
function on_change7(self, observer, caller) {
	return place_subscription(attach_observer7(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function attach_observer8(self, observer, immediately) {
	const subscription = on_settle7(self, mint_subscriber(() => {
		return observer(get10(self));
	}));
	if (immediately) {
		observer(get10(self));
	}
	return subscription;
}
function on_change8(self, observer, caller) {
	return place_subscription(attach_observer8(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function cursor3(self) {
	return cursor(self[2]);
}
function lift(self, op) {
	const $hM = op;
	let $hN = null;
	if ($hM[0] === 0) {
		const at4 = $hM[1];
		const gone = $hM[2];
		const arrived = $hM[3];
		$hN = [ 0, at4, handles(self, gone), handles(self, arrived) ];
	} else if ($hM[0] === 1) {
		const at5 = $hM[1];
		const old = $hM[2];
		const new10 = $hM[3];
		$hN = [ 1, at5, slot_of(self[0], old), slot_of(self[0], new10) ];
	} else if ($hM[0] === 2) {
		const all4 = $hM[1];
		$hN = [ 2, handles(self, all4) ];
	} else {
		const from = $hM[1];
		const count2 = $hM[2];
		const to = $hM[3];
		$hN = [ 3, from, count2, to ];
	}
	return $hN;
}
function since3(self, cursor6) {
	const $hK = since(self[2], cursor6);
	let $hL = null;
	if ($hK[0] === 0) {
		const recorded = $hK[1];
		let out = [  ];
		for (const op of recorded) {
			out.push(lift(self, op));
		}
		$hL = out;
	} else {
		$hL = [ [ 2, get11(self) ] ];
	}
	return $hL;
}
function eq10(self, other) {
	return self[1] === other[1];
}
function eq11(self, b2) {
	if (self.length !== b2.length) {
		return false;
	}
	let index2 = 0;
	for (const item2 of self) {
		if (!(eq10(item2, __at(b2, index2, "std/src/compare.vl:147:17")))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function row_arrived_reads(op) {
	const $hP = op;
	let $hQ = null;
	if ($hP[0] === 0) {
		const _at = $hP[1];
		const _gone = $hP[2];
		const arrived = $hP[3];
		$hQ = map2(arrived, (slot6) => {
			return get(slot6);
		});
	} else {
		$hQ = [  ];
	}
	return $hQ;
}
function eq12(self, b2) {
	const $hR = self;
	let $hU = null;
	if ($hR[0] === 0) {
		const $hS = b2;
		let $hT = null;
		if ($hS[0] === 0) {
			$hT = $hR[1] === $hS[1];
		} else {
			$hT = false;
		}
		$hU = $hT;
	} else {
		const $hV = b2;
		$hU = $hV[0] === 1;
	}
	return $hU;
}
function eq13(self, b2) {
	if (self.length !== b2.length) {
		return false;
	}
	let index2 = 0;
	for (const item2 of self) {
		if (!(eq12(item2, __at(b2, index2, "std/src/compare.vl:147:17")))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function index_by2(self, key, $gb) {
	const found = __shared_new(new2());
	const leaving = __shared_new([  ]);
	const storage = __clone(self[0]);
	const handler = self[1];
	const cells = self[3];
	const view = self[2][1];
	const key_at = (at4) => {
		let out = [ 1 ];
		peek_storage(storage, (held2) => {
			return view(held2, at4, (item2) => {
				return out = [ 0, key(item2) ];
			});
		});
		return out;
	};
	const map4 = (held2, at4) => {
		const id = follow_id(storage, handler, cells, view, at4);
		const before = get9(found.v, held2);
		insert(found.v, held2, id);
		const $hW = before;
		if ($hW[0] === 0) {
			release_id(handler, cells, $hW[1]);
		}
		return;
	};
	const unmap = (held2) => {
		const $hX = get9(found.v, held2);
		if ($hX[0] === 0) {
			remove(found.v, held2);
			release_id(handler, cells, $hX[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $hY = _;
			const _held = $hY[0];
			const old = $hY[1];
			release_id(handler, cells, old);
		}
		found.v = new8();
		let at4 = 0;
		const count2 = peek_storage2(storage, (held2) => {
			return count(held2);
		});
		while (at4 < count2) {
			const $hZ = key_at(at4);
			if ($hZ[0] === 0) {
				map4($hZ[1], at4);
			}
			at4 = at4 + 1;
		}
		return;
	};
	fill();
	const note = (at4, count2) => {
		let gone = [  ];
		let index3 = at4;
		while (index3 < at4 + count2) {
			const $ia = key_at(index3);
			if ($ia[0] === 0) {
				gone.push($ia[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone;
		return;
	};
	const arrive = (at4, count2) => {
		let index3 = at4;
		while (index3 < at4 + count2) {
			const $ib = key_at(index3);
			if ($ib[0] === 0) {
				map4($ib[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change3) => {
		const $ic = change3;
		let $id = null;
		if ($ic[0] === 0) {
			const at4 = $ic[1];
			const removed = $ic[2];
			const _inserted = $ic[3];
			$id = note(at4, removed);
		} else if ($ic[0] === 3) {
			const at5 = $ic[1];
			const count2 = $ic[2];
			$id = note(at5, count2);
		} else if ($ic[0] === 2) {
			const at6 = $ic[1];
			$id = note(at6, 1);
		} else if ($ic[0] === 4) {
			const at7 = $ic[1];
			const _part = $ic[2];
			$id = note(at7, 1);
		} else {
			$id = undefined;
		}
		return $id;
	}, (change3, stage) => {
		const $ie = stage;
		let $if = null;
		if ($ie[0] === 0) {
			$if = undefined;
		} else if ($ie[0] === 3) {
			return;
		} else {
			fill();
			return;
			$if = undefined;
		}
		$if;
		const $ig = change3;
		let $ih = null;
		if ($ig[0] === 0) {
			const at4 = $ig[1];
			const _removed = $ig[2];
			const inserted = $ig[3];
			for (const held2 of leaving.v) {
				unmap(held2);
			}
			arrive(at4, inserted);
			$ih = undefined;
		} else if ($ig[0] === 1) {
			const at5 = $ig[1];
			const count2 = $ig[2];
			$ih = arrive(at5, count2);
		} else if ($ig[0] === 3) {
			const _at = $ig[1];
			const _count = $ig[2];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			$ih = undefined;
		} else if ($ig[0] === 2) {
			const at6 = $ig[1];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			arrive(at6, 1);
			$ih = undefined;
		} else if ($ig[0] === 4) {
			const at7 = $ig[1];
			const _part = $ig[2];
			for (const held5 of leaving.v) {
				unmap(held5);
			}
			arrive(at7, 1);
			$ih = undefined;
		} else if ($ig[0] === 5) {
			const _from = $ig[1];
			const _to = $ig[2];
			$ih = undefined;
		} else {
			$ih = fill();
		}
		return $ih;
	});
	const index2 = [ __clone(self), found, tap2 ];
	defer_to_owner(() => {
		return release5(index2);
	}, $gb);
	return index2;
}
function find3(self, key) {
	const lens2 = __clone(self[0]);
	return map3(viewing2(self[1], (held2) => {
		return get9(held2, key);
	}), (id) => {
		return slot_of(lens2, id);
	});
}
function eq14(self, b2) {
	const $in = self;
	let $iq = null;
	if ($in[0] === 0) {
		const $io = b2;
		let $ip = null;
		if ($io[0] === 0) {
			$ip = eq12($in[1], $io[1]);
		} else {
			$ip = false;
		}
		$iq = $ip;
	} else {
		const $ir = b2;
		$iq = $ir[0] === 1;
	}
	return $iq;
}
function attach_observer9(self, observer, immediately) {
	const subscription = on_settle4(self, mint_subscriber(() => {
		return observer(get4(self));
	}));
	if (immediately) {
		observer(get4(self));
	}
	return subscription;
}
function on_change9(self, observer, caller) {
	return place_subscription(attach_observer9(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function observe(signal, observer) {
	const cell = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $iv = [ 0, cell ];
		let $iw = null;
		if ($iv[0] === 0) {
			const live = $iv[1];
			$iw = observer(live.v);
		} else {
			$iw = undefined;
		}
		return $iw;
	}));
}
function get14(self) {
	return __clone(self[0].v);
}
function attach_observer10(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get14(self));
	}
	return subscription;
}
function on_change10(self, observer, caller) {
	return place_subscription(attach_observer10(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function record2(self, op) {
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
function record_put(log, key, now, leaving, late) {
	const $iJ = now;
	if ($iJ[0] === 0) {
		const $iL = __clone(key);
		let $iK = null;
		if (late) {
			$iK = [ 1 ];
		} else {
			$iK = leaving.v;
		}
		record2(log, [ 0, $iL, __clone($iK), __clone($iJ[1]) ]);
	}
}
function keys(self) {
	return __nmap_keys(self[0]);
}
function held_keys(self) {
	return keys(self);
}
function entries_of(storage) {
	return peek_storage2(storage, (held2) => {
		let out = new8();
		for (const key of held_keys(held2)) {
			const $iM = find2(held2, key);
			if ($iM[0] === 0) {
				insert(out, key, $iM[1]);
			}
		}
		return out;
	});
}
function release9(self) {
	untap(self[0], self[2]);
}
function entries2(self, $iA) {
	const log = new9();
	const leaving = __shared_new([ 1 ]);
	const storage = __clone(self[0]);
	const find4 = (key) => {
		return peek_storage2(storage, (held2) => {
			return find2(held2, key);
		});
	};
	const tap2 = tap(self, (change3) => {
		const $iB = change3;
		let $iC = null;
		if ($iB[0] === 0) {
			const key = $iB[1];
			const _removed = $iB[2];
			const _inserted = $iB[3];
			$iC = leaving.v = find4(key);
		} else if ($iB[0] === 3) {
			const key2 = $iB[1];
			const _count = $iB[2];
			$iC = leaving.v = find4(key2);
		} else if ($iB[0] === 2) {
			const key3 = $iB[1];
			$iC = leaving.v = find4(key3);
		} else if ($iB[0] === 4) {
			const key4 = $iB[1];
			const _part = $iB[2];
			$iC = leaving.v = find4(key4);
		} else {
			$iC = undefined;
		}
		return $iC;
	}, (change3, stage) => {
		const $iD = stage;
		const late = !($iD[0] === 0);
		const $iE = change3;
		let $iF = null;
		if ($iE[0] === 1) {
			const key = $iE[1];
			const _count = $iE[2];
			const $iG = find4(key);
			if ($iG[0] === 0) {
				record2(log, [ 0, __clone(key), [ 1 ], $iG[1] ]);
			}
			$iF = undefined;
		} else if ($iE[0] === 0) {
			const key2 = $iE[1];
			const _removed = $iE[2];
			const _inserted = $iE[3];
			$iF = record_put(log, key2, find4(key2), leaving, late);
		} else if ($iE[0] === 2) {
			const key3 = $iE[1];
			$iF = record_put(log, key3, find4(key3), leaving, late);
		} else if ($iE[0] === 4) {
			const key4 = $iE[1];
			const _part = $iE[2];
			$iF = record_put(log, key4, find4(key4), leaving, late);
		} else if ($iE[0] === 3) {
			const key5 = $iE[1];
			const _count2 = $iE[2];
			if (late) {
				record2(log, [ 2, entries_of(storage) ]);
			} else {
				const $iR = leaving.v;
				if ($iR[0] === 0) {
					record2(log, [ 1, __clone(key5), $iR[1] ]);
				}
			}
			$iF = undefined;
		} else if ($iE[0] === 5) {
			const _from = $iE[1];
			const _to = $iE[2];
			$iF = undefined;
		} else {
			$iF = record2(log, [ 2, entries_of(storage) ]);
		}
		return $iF;
	});
	const feed4 = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release9(feed4);
	}, $iA);
	return feed4;
}
function cursor4(self) {
	return cursor(self[1]);
}
function update2(self, change3, $bE) {
	const $iS = self[9].v;
	let $iT = null;
	if ($iS[0] === 0) {
		const _check = $iS[1];
		$iT = [ 0, peek2(self, (held2) => {
			return __clone(held2);
		}) ];
	} else {
		$iT = [ 1 ];
	}
	const before = $iT;
	let out = [ 1 ];
	mutate(self, (held2) => {
		return out = [ 0, change3(held2) ];
	});
	const $iU = out;
	let $iV = null;
	if ($iU[0] === 0) {
		const result = __clone($iU[1][0]);
		const changes = __clone($iU[1][1]);
		audit(self, before, changes);
		landed3(self, changes, true, $bE);
		$iV = result;
	} else {
		$iV = (() => {
			throw __panic("lens: an update\'s write never ran", "std/src/reactive/lens.vl:684:12");
		})();
	}
	return $iV;
}
function all3(self) {
	return entries_of(self[0]);
}
function since4(self, cursor6) {
	const $jf = since(self[1], cursor6);
	let $jg = null;
	if ($jf[0] === 0) {
		const recorded = $jf[1];
		$jg = recorded;
	} else {
		$jg = [ [ 2, all3(self[0]) ] ];
	}
	return $jg;
}
function over3(value, $jh) {
	return [ sequence(value, $jh) ];
}
function over4(value) {
	return [ keyed(value) ];
}
function slot4(self, at4, $ji) {
	return slot(self[0], at4, $ji);
}
function splice6(self, at4, removed, inserted, $jj) {
	splice2(self[0], at4, removed, inserted, $jj);
}
function insert_at4(self, at4, value, $v) {
	return splice6(self, at4, 0, [ __clone(value) ], $v);
}
function size4(self) {
	return size(self[0]);
}
function put3(self, key, value, $jk) {
	return put2(self[0], key, value, $jk);
}
function slot5(self, key, $jl) {
	return slot3(self[0], key, $jl);
}
function length2(self) {
	return [ __clone(self) ];
}
function length3(self) {
	return length2(self[0]);
}
function get15(self) {
	return peek2(self[0], (held2) => {
		return held_keys(held2).length;
	});
}
function lens(self) {
	return __clone(self[0]);
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
const delta_log_limit = 1024;
const element_holds_allocated_count = __shared_new(0);
const block_size = 128;
const list = sequence([ "foo", "bar", "baz" ], [ 1 ]);
const bar = slot(list, 1, [ 1 ]);
const heard = __shared_new([  ]);
on_change(__clone(bar), (value, $p) => {
	return heard.v.push(__clone(value));
}, "lens-laws.vl:59:19");
insert_at(list, 0, "qux", [ 1 ]);
assert(eq3(get(bar), [ 0, "bar" ]), "L1: the slot still reads its item", "lens-laws.vl:61:2");
assert(eq5(at(bar), [ 0, 2 ]), "L1: and knows where it went", "lens-laws.vl:62:2");
assert(peek2(list, (held2) => {
	return __at(held2, 1, "lens-laws.vl:63:26");
}) === "foo", "position 1 is now foo", "lens-laws.vl:63:2");
assert(heard.v.length === 0, "L4: an insert above a slot does not wake it", "lens-laws.vl:64:2");
const rows2 = sequence([ "a", "b", "c" ], [ 1 ]);
const c = slot(rows2, 2, [ 1 ]);
const runs = __shared_new(0);
on_change(__clone(c), (_value, $aJ) => {
	const $aK = [ runs, "v" ];
	return runs.v = $aK[0][$aK[1]] + 1;
}, "lens-laws.vl:71:17");
batch(($aL) => {
	insert_at(rows2, 0, "z", [ 0, $aL ]);
	set(c, "C!", [ 0, $aL ]);
	set(c, "C!!", [ 0, $aL ]);
	assert(runs.v === 0, "L3: nothing runs inside the turn", "lens-laws.vl:76:3");
	return;
}, [ 1 ]);
assert(eq(all(rows2), [ "z", "a", "b", "C!!" ]), "L2: the writes landed on c, not on b", "lens-laws.vl:78:2");
assert(runs.v === 1, "L3: one run for the turn", "lens-laws.vl:79:2");
remove_at(rows2, 3, [ 1 ]);
assert(eq3(get(c), [ 1 ]), "L6: the slot reads None", "lens-laws.vl:83:2");
assert(runs.v === 2, "L6: and woke once to say so", "lens-laws.vl:84:2");
push(rows2, "d", [ 1 ]);
assert(eq3(get(c), [ 1 ]), "L1: a new item at the old position is not the slot\'s item", "lens-laws.vl:86:2");
const board = sequence2(tasks(), [ 1 ]);
const first = slot2(board, 0, [ 1 ]);
const second = slot2(board, 1, [ 1 ]);
const titles = __shared_new(0);
const statuses = __shared_new(0);
const others = __shared_new(0);
const title = part(second, "title", (task) => {
	return task[0];
});
const status = part(second, "status", (task) => {
	return task[1];
});
on_change2(__clone(title), (_text, $bc) => {
	const $bd = [ titles, "v" ];
	return titles.v = $bd[0][$bd[1]] + 1;
}, "lens-laws.vl:98:17");
on_change2(status, (_text, $bf) => {
	const $bg = [ statuses, "v" ];
	return statuses.v = $bg[0][$bg[1]] + 1;
}, "lens-laws.vl:99:18");
on_change2(part(first, "title", (task) => {
	return task[0];
}), (_text, $bh) => {
	const $bi = [ others, "v" ];
	return others.v = $bi[0][$bi[1]] + 1;
}, "lens-laws.vl:100:50");
edit2(second, "title", (task) => {
	return task[0] = "B";
}, [ 1 ]);
assert(eq3(get2(title), [ 0, "B" ]), "the part reads the edit", "lens-laws.vl:102:2");
assert(titles.v === 1 && statuses.v === 0 && others.v === 0, "L4: only the edited part woke", "lens-laws.vl:103:2");
insert_at2(board, 0, [ "new", "open" ], [ 1 ]);
edit2(second, "status", (task) => {
	return task[1] = "done";
}, [ 1 ]);
assert(peek2(board, (held2) => {
	return __at(held2, 2, "lens-laws.vl:109:27")[1];
}) === "done", "L2: the edit followed the row", "lens-laws.vl:109:2");
assert(titles.v === 1 && statuses.v === 1, "L4: only the status woke", "lens-laws.vl:110:2");
update(board, (held2) => {
	__replace(held2, tasks());
	return [ undefined, [ [ 6 ] ] ];
}, [ 1 ]);
assert(eq6(get3(second), [ 1 ]), "L7: a positional slot dies at a reset", "lens-laws.vl:117:2");
const whole = __shared_new(0);
const lengths = __shared_new(0);
on_change3(__clone(board), (_all, $ce) => {
	const $cf = [ whole, "v" ];
	return whole.v = $cf[0][$cf[1]] + 1;
}, "lens-laws.vl:122:17");
on_change4(length(board), (_n, $cg) => {
	const $ch = [ lengths, "v" ];
	return lengths.v = $ch[0][$ch[1]] + 1;
}, "lens-laws.vl:123:26");
replace2(board, 0, [ "x", "open" ], [ 1 ]);
assert(whole.v === 1 && lengths.v === 0, "a replace wakes the whole value, not the length", "lens-laws.vl:125:2");
push2(board, [ "y", "open" ], [ 1 ]);
assert(whole.v === 2 && lengths.v === 1, "a push wakes both", "lens-laws.vl:130:2");
const order = sequence([ "a", "b", "c", "d" ], [ 1 ]);
const d = slot(order, 3, [ 1 ]);
const d_runs = __shared_new(0);
on_change(__clone(d), (_value, $cj) => {
	const $ck = [ d_runs, "v" ];
	return d_runs.v = $ck[0][$ck[1]] + 1;
}, "lens-laws.vl:136:13");
move_item(order, 3, 0, [ 1 ]);
assert(eq(all(order), [ "d", "a", "b", "c" ]), "the move happened", "lens-laws.vl:138:2");
assert(eq5(at(d), [ 0, 0 ]) && eq3(get(d), [ 0, "d" ]) && d_runs.v === 0, "the slot went with its item, silently", "lens-laws.vl:139:2");
move_item(order, 0, 9, [ 1 ]);
assert(eq(all(order), [ "a", "b", "c", "d" ]) && eq5(at(d), [ 0, 3 ]), "a move past the end lands at the end", "lens-laws.vl:144:2");
const people2 = sequence([ "ann", "bob", "cy", "dee" ], [ 1 ]);
const bob2 = slot(people2, 1, [ 1 ]);
const dee2 = slot(people2, 3, [ 1 ]);
const cy2 = slot(people2, 2, [ 1 ]);
reconcile_by_key(people2, [ "dee", "zed", "bob", "ann" ], (name) => {
	return name[0][name[1]];
}, [ 1 ]);
assert(eq(all(people2), [ "dee", "zed", "bob", "ann" ]), "the list became the new list", "lens-laws.vl:155:2");
assert(eq5(at(bob2), [ 0, 2 ]) && eq5(at(dee2), [ 0, 0 ]), "kept items keep their slots", "lens-laws.vl:156:2");
assert(eq3(get(cy2), [ 1 ]), "an item that left ends its slot", "lens-laws.vl:157:2");
const numbers = sequence3([ 1, 2, 3, 4 ], [ 1 ]);
const doubled = memo(map(items(numbers, [ 1 ]), (n, $da, $db) => {
	return n * 2;
}), [ 1 ], [ 1 ], "lens-laws.vl:161:47");
const evens = memo2(filter(items(numbers, [ 1 ]), (n, $eR, $eS) => {
	return n % 2 === 0;
}), [ 1 ], [ 1 ], "lens-laws.vl:162:53");
assert(eq(get8(doubled), [ 2, 4, 6, 8 ]) && eq(get8(evens), [ 2, 4 ]), "the operators seed from the lens", "lens-laws.vl:163:2");
insert_at3(numbers, 1, 10, [ 1 ]);
remove_at2(numbers, 0, [ 1 ]);
replace2(numbers, 3, 5, [ 1 ]);
assert(eq(all2(numbers), [ 10, 2, 3, 5 ]), "the writes landed", "lens-laws.vl:170:2");
assert(eq(get8(doubled), [ 20, 4, 6, 10 ]), "map follows the lens", "lens-laws.vl:171:2");
assert(eq(get8(evens), [ 10, 2 ]), "filter follows the lens", "lens-laws.vl:172:2");
const tabs = sequence2(tasks(), [ 1 ]);
const by_title = index_by(tabs, (task) => {
	return task[0];
}, [ 1 ]);
const b = find(by_title, "b");
assert(eq7(map3(b, (slot6) => {
	return at(slot6);
}), [ 0, [ 0, 1 ] ]), "the index finds the slot of the item under a key", "lens-laws.vl:178:2");
insert_at2(tabs, 0, [ "first", "open" ], [ 1 ]);
remove_at3(tabs, 3, [ 1 ]);
assert(eq7(map3(find(by_title, "first"), (slot6) => {
	return at(slot6);
}), [ 0, [ 0, 0 ] ]), "an inserted item is indexed", "lens-laws.vl:184:2");
assert(eq7(map3(find(by_title, "c"), (slot6) => {
	return at(slot6);
}), [ 1 ]), "a deleted item leaves the index", "lens-laws.vl:188:2");
assert(eq7(map3(find(by_title, "b"), (slot6) => {
	return at(slot6);
}), [ 0, [ 0, 2 ] ]), "and the others are found where they are now", "lens-laws.vl:189:2");
edit(tabs, 2, "title", (task) => {
	return task[0] = "B";
}, [ 1 ]);
assert(eq7(map3(find(by_title, "b"), (slot6) => {
	return at(slot6);
}), [ 1 ]) && eq7(map3(find(by_title, "B"), (slot6) => {
	return at(slot6);
}), [ 0, [ 0, 2 ] ]), "an edit that changes the key re-indexes the item", "lens-laws.vl:194:2");
const places = __shared_new([  ]);
const first_tab = slot2(tabs, 0, [ 1 ]);
on_change5(index(first_tab), (place, $gE) => {
	return places.v.push(__clone(place));
}, "lens-laws.vl:206:29");
insert_at2(tabs, 0, [ "zero", "open" ], [ 1 ]);
assert(places.v.length === 1 && eq5(at(first_tab), [ 0, 1 ]), "a slot\'s position is a source that follows the shifts", "lens-laws.vl:208:2");
const held = sequence([ "p", "q", "r" ], [ 1 ]);
const one2 = slot(held, 1, [ 1 ]);
const two = slot(held, 1, [ 1 ]);
assert(one2[1] === two[1], "two asks for one item are one slot", "lens-laws.vl:217:2");
release2(one2);
assert(eq3(get(two), [ 0, "q" ]) && eq5(at(two), [ 0, 1 ]), "the slot lives while a handle holds it", "lens-laws.vl:219:2");
release2(two);
assert(eq3(get(two), [ 1 ]) && eq5(at(two), [ 1 ]), "the last release ends it", "lens-laws.vl:221:2");
insert_at(held, 0, "o", [ 1 ]);
assert(eq3(get(slot(held, 2, [ 1 ])), [ 0, "q" ]) && slot(held, 2, [ 1 ])[1] !== one2[1], "a new ask makes a new slot; the old entry is free", "lens-laws.vl:223:2");
const feed = rows(held, [ 1 ]);
assert(get11(feed).length === 4, "rows() holds a slot per item", "lens-laws.vl:228:2");
release6(feed);
assert(held[7].v.length === 0, "releasing the feed lets its tap and its slots go", "lens-laws.vl:230:2");
const scoped = sequence([ "m", "n" ], [ 1 ]);
const $gJ = comp(($gI) => {
	return [ slot(scoped, 1, [ 0, $gI ]), rows(scoped, [ 0, $gI ]) ];
});
const inner = $gJ[0];
const feed2 = $gJ[1];
const owner = $gJ[2];
assert(eq3(get(inner), [ 0, "n" ]) && scoped[7].v.length === 1, "alive while the owner is", "lens-laws.vl:235:2");
dispose2(owner);
assert(eq3(get(inner), [ 1 ]) && scoped[7].v.length === 0, "released with the owner", "lens-laws.vl:237:2");
assert(get11(feed2).length === 0, "the feed let its slots go", "lens-laws.vl:238:2");
const one_write = sequence3([ 1, 2, 3 ], [ 1 ]);
const feed3 = items(one_write, [ 1 ]);
const cursor5 = cursor2(feed3);
splice5(one_write, 0, 1, [ 9 ], [ 1 ]);
const ops = since2(feed3, cursor5);
assert(ops.length === 1, "a replacing splice is one op", "lens-laws.vl:248:2");
const $gK = __at(ops, 0, "lens-laws.vl:250:3");
assert($gK[0] === 0 && $gK[1] === 0 && eq($gK[2], [ 1 ]) && eq($gK[3], [ 9 ]), "and it carries what left and what arrived", "lens-laws.vl:249:2");
splice5(one_write, 1, 2, [ 7, 8, 6 ], [ 1 ]);
const again = since2(feed3, cursor5);
let $gL = undefined;
assert(again.length === 1 && (($gL = __at(again, 0, "lens-laws.vl:260:7")), $gL[0] === 0) && $gL[1] === 1 && eq($gL[2], [ 2, 3 ]) && eq($gL[3], [ 7, 8, 6 ]), "so is a splice that takes and inserts different counts", "lens-laws.vl:258:2");
const ages = new8();
const people = keyed(ages);
const ada = slot3(people, "ada", [ 1 ]);
const bob = slot3(people, "bob", [ 1 ]);
const ada_runs = __shared_new(0);
const bob_runs = __shared_new(0);
on_change6(__clone(ada), (_age, $gV) => {
	const $gW = [ ada_runs, "v" ];
	return ada_runs.v = $gW[0][$gW[1]] + 1;
}, "lens-laws.vl:274:15");
on_change6(bob, (_age, $hc) => {
	const $hd = [ bob_runs, "v" ];
	return bob_runs.v = $hd[0][$hd[1]] + 1;
}, "lens-laws.vl:275:15");
assert(eq3(get12(ada), [ 1 ]), "a key\'s slot exists before the key", "lens-laws.vl:276:2");
put2(people, "ada", 36, [ 1 ]);
assert(eq3(get12(ada), [ 0, 36 ]) && ada_runs.v === 1 && bob_runs.v === 0, "only ada\'s slot woke", "lens-laws.vl:278:2");
remove2(people, "ada", [ 1 ]);
assert(eq3(get12(ada), [ 1 ]) && ada_runs.v === 2, "the slot reads the removal", "lens-laws.vl:283:2");
put2(people, "ada", 37, [ 1 ]);
assert(eq3(get12(ada), [ 0, 37 ]) && ada_runs.v === 3, "and follows the key when it returns", "lens-laws.vl:285:2");
const spliced = sequence3([ 1, 2, 3, 4 ], [ 1 ]);
const heard_changes = __shared_new([  ]);
const before_changes = __shared_new(0);
tap(spliced, (_change) => {
	const $hC = [ before_changes, "v" ];
	return before_changes.v = $hC[0][$hC[1]] + 1;
}, (change3, stage) => {
	const $hD = stage;
	assert($hD[0] === 0, "a shape write is exact", "lens-laws.vl:292:3");
	heard_changes.v.push(__clone(change3));
	return;
});
splice5(spliced, 1, 2, [ 7, 8, 9 ], [ 1 ]);
push3(spliced, 5, [ 1 ]);
remove_at2(spliced, 0, [ 1 ]);
const one_each = [ [ 0, 1, 2, 3 ], [ 0, 5, 0, 1 ], [ 0, 0, 1, 0 ] ];
assert(eq9(heard_changes.v, one_each), "R16: each splice is one Splice change", "lens-laws.vl:305:2");
assert(before_changes.v === 3, "and `before` heard each once, ahead of the write", "lens-laws.vl:306:2");
const table = sequence([ "a", "b", "c", "d", "e" ], [ 1 ]);
const b_slot = slot(table, 1, [ 1 ]);
const e_slot = slot(table, 4, [ 1 ]);
splice2(table, 1, 2, [ "x", "y", "z" ], [ 1 ]);
assert(eq3(get(b_slot), [ 1 ]) && eq3(get(e_slot), [ 0, "e" ]) && eq5(at(e_slot), [ 0, 5 ]), "the splice moved the slots in one step", "lens-laws.vl:314:2");
assert(eq3(get(slot(table, 1, [ 1 ])), [ 0, "x" ]), "an arrival gets a slot of its own", "lens-laws.vl:318:2");
const sizes = __shared_new(0);
const shapes = __shared_new(0);
on_change7(length(table), (_n, $hG) => {
	const $hH = [ sizes, "v" ];
	return sizes.v = $hH[0][$hH[1]] + 1;
}, "lens-laws.vl:323:29");
on_change8(index(slot(table, 0, [ 1 ])), (_at, $hI) => {
	const $hJ = [ shapes, "v" ];
	return shapes.v = $hJ[0][$hJ[1]] + 1;
}, "lens-laws.vl:324:37");
splice2(table, 0, 1, [ "A" ], [ 1 ]);
assert(sizes.v === 0 && shapes.v === 1, "an equal-count splice leaves the length asleep and moves the shape", "lens-laws.vl:326:2");
splice2(table, 0, 1, [ "A", "A2" ], [ 1 ]);
assert(sizes.v === 1 && shapes.v === 2, "a growing splice wakes both", "lens-laws.vl:331:2");
const rowed = sequence([ "p", "q", "r" ], [ 1 ]);
const rows3 = rows(rowed, [ 1 ]);
const row_cursor = cursor3(rows3);
const q_row = __clone(__at(get11(rows3), 1, "lens-laws.vl:338:14"));
splice2(rowed, 1, 1, [ "Q", "Q2" ], [ 1 ]);
const row_ops = since3(rows3, row_cursor);
assert(row_ops.length === 1, "a replacing splice is one row op", "lens-laws.vl:341:2");
const $hO = __at(row_ops, 0, "lens-laws.vl:343:3");
assert($hO[0] === 0 && $hO[1] === 1 && eq11($hO[2], [ q_row ]) && $hO[3].length === 2, "with the slot that left and the two that arrived", "lens-laws.vl:342:2");
assert(eq13(row_arrived_reads(__at(row_ops, 0, "lens-laws.vl:350:21")), [ [ 0, "Q" ], [ 0, "Q2" ] ]), "the arrived slots read the arrivals", "lens-laws.vl:349:2");
release6(rows3);
const indexed = sequence([ "k", "l" ], [ 1 ]);
const by_name = index_by2(indexed, (name) => {
	return name[0][name[1]];
}, [ 1 ]);
const l_found = find3(by_name, "l");
assert(eq14(map3(__clone(l_found), (slot6) => {
	return get(slot6);
}), [ 0, [ 0, "l" ] ]), "the index holds l\'s slot", "lens-laws.vl:359:2");
release5(by_name);
assert(eq14(map3(l_found, (slot6) => {
	return get(slot6);
}), [ 0, [ 1 ] ]) && indexed[7].v.length === 0, "released, the index let its tap and its slots go", "lens-laws.vl:361:2");
const trigger = new7(0, "lens-laws.vl:368:28");
const echo = sequence3([ 0 ], [ 1 ]);
const echo_runs = __shared_new(0);
on_change9(__clone(echo), (_all, $is) => {
	const $it = [ echo_runs, "v" ];
	return echo_runs.v = $it[0][$it[1]] + 1;
}, "lens-laws.vl:371:19");
on_change10(__clone(trigger), (value, $iu) => {
	replace2(echo, 0, value, [ 1 ]);
	push3(echo, value, [ 1 ]);
	return;
}, "lens-laws.vl:372:23");
batch(($ix) => {
	return set2(trigger, 1, [ 0, $ix ], "lens-laws.vl:376:19");
}, [ 1 ]);
assert(eq(all2(echo), [ 1, 1 ]), "the observer\'s writes landed", "lens-laws.vl:377:2");
assert(echo_runs.v === 1, "and settled with the drain they ran in: one run for two writes", "lens-laws.vl:378:2");
const scores = new8();
const board2 = keyed(scores);
put2(board2, "ada", 1, [ 1 ]);
const ada_score = slot3(board2, "ada", [ 1 ]);
const ada_wakes = __shared_new(0);
on_change6(__clone(ada_score), (_score, $iy) => {
	const $iz = [ ada_wakes, "v" ];
	return ada_wakes.v = $iz[0][$iz[1]] + 1;
}, "lens-laws.vl:386:23");
const score_feed = entries2(board2, [ 1 ]);
const score_cursor = cursor4(score_feed);
update2(board2, (held2) => {
	insert(held2, "ada", 2);
	return [ undefined, [ [ 0, "ada", 1, 1 ] ] ];
}, [ 1 ]);
assert(eq3(get12(ada_score), [ 0, 2 ]) && ada_wakes.v === 1, "a keyed splice wakes the key\'s slot", "lens-laws.vl:393:2");
assert(since4(score_feed, score_cursor).length === 1, "and is one map op", "lens-laws.vl:397:2");
const wrapped = [ over3([ "x", "y" ], [ 1 ]), over4(ages_of()) ];
const y_slot = slot4(wrapped[0], 1, [ 1 ]);
insert_at4(wrapped[0], 0, "w", [ 1 ]);
assert(eq3(get(y_slot), [ 0, "y" ]) && eq5(at(y_slot), [ 0, 2 ]) && size4(wrapped[0]) === 3, "a SequenceLens is the sequence lens", "lens-laws.vl:406:2");
put3(wrapped[1], "cy", 9, [ 1 ]);
assert(eq3(get12(slot5(wrapped[1], "cy", [ 1 ])), [ 0, 9 ]) && get15(length3(wrapped[1])) === 2, "a KeyedLens is the keyed lens", "lens-laws.vl:411:2");
assert(eq(all(lens(wrapped[0])), [ "w", "x", "y" ]), "lens() hands out the full lens", "lens-laws.vl:415:2");
console.log("laws: all passed");
//# sourceMappingURL=lens-laws.mjs.map
