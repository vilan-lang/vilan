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
function eq(self, b) {
	if (self.length !== b.length) {
		return false;
	}
	let index = 0;
	for (const item2 of self) {
		if (!(item2 === __at(b, index, "std/src/compare.vl:147:17"))) {
			return false;
		}
		index = index + 1;
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
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $cb = null;
	if (wrapped < 0) {
		$cb = wrapped + modulus;
	} else {
		$cb = wrapped;
	}
	return $cb;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $x = null;
	if (truncated > 0) {
		$x = truncated;
	} else {
		$x = 0;
	}
	return $x;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $cc = null;
	if (wrapped >= half) {
		$cc = wrapped - modulus;
	} else {
		$cc = wrapped;
	}
	return $cc;
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
	const $ei = self;
	let $el = null;
	if ($ei[0] === 0) {
		const $ej = b;
		let $ek = null;
		if ($ej[0] === 0) {
			$ek = $ei[1] === $ej[1];
		} else {
			$ek = false;
		}
		$el = $ek;
	} else {
		const $em = b;
		$el = $em[0] === 1;
	}
	return $el;
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
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
		const key = hash(subscriber[0]);
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
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:421:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:424:25");
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
	const $ak = turn;
	let $al = null;
	if ($ak[0] === 0) {
		const ambient = $ak[1];
		$al = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $an = last(draining_turns.v);
		let $ao = null;
		if ($an[0] === 0) {
			const draining = $an[1];
			$ao = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$ao = undefined;
		}
		$al = $ao;
	}
	return $al;
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
function dispose(self, $bJ) {
	const $bK = $bJ;
	let $bL = null;
	if ($bK[0] === 0) {
		const established = $bK[1];
		$bL = [ 0, established ];
	} else {
		$bL = last(draining_turns.v);
	}
	const ambient = $bL;
	release_under(self, ambient);
}
function detach(handle) {
	const $az = last2(releasing_turns.v);
	let $aA = null;
	if ($az[0] === 0) {
		const at_release = $az[1];
		$aA = at_release;
	} else {
		$aA = last(draining_turns.v);
	}
	const turn = $aA;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $aB = [ 0, handle[0] ];
	let $aC = null;
	if ($aB[0] === 0) {
		const subscribers = $aB[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aC = undefined;
	} else {
		$aC = undefined;
	}
	$aC;
	const $aD = ambient;
	let $aE = null;
	if ($aD[0] === 0) {
		const turn = $aD[1];
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
		$aE = undefined;
	} else {
		$aE = undefined;
	}
	$aE;
	const $aF = handle[3].v;
	let $aG = null;
	if ($aF[0] === 0) {
		const release6 = $aF[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release6, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aG = undefined;
	} else {
		$aG = undefined;
	}
	return $aG;
}
function new4() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $L = null;
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
		$L = undefined;
	}
	return $L;
}
function stage_nursery(self) {
	const $X = nursery(self);
	let $Y = null;
	if ($X[0] === 0) {
		const held = $X[1];
		$Y = held;
	} else {
		const made = detached_nursery();
		self[0].v[3] = [ 0, __clone(made) ];
		$Y = made;
	}
	return $Y;
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
	let $W = null;
	if (is_disposed(self)) {
		$W = [ 1 ];
	} else {
		$W = self[0].v[3];
	}
	return $W;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $aS = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $aJ = held[3];
		let $aK = null;
		if ($aJ[0] === 0) {
			const nursery2 = $aJ[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$aK = undefined;
		} else {
			$aK = undefined;
		}
		$aK;
		let $aR = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $aM = __guarded(cleanup);
				let $aN = null;
				if ($aM[0] === 0) {
					const message = $aM[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$aN = undefined;
				} else {
					$aN = undefined;
				}
				$aN;
			}
			const $aP = failure;
			let $aQ = null;
			if ($aP[0] === 0) {
				const message2 = $aP[1];
				$aQ = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$aQ = undefined;
			}
			$aR = $aQ;
		}
		$aS = $aR;
	}
	return $aS;
}
function register_with_owner(subscription, $bE, $bF) {
	const $bG = $bF;
	let $bH = null;
	if ($bG[0] === 0) {
		const owner = $bG[1];
		$bH = take(owner, subscription, $bE);
	} else {
		$bH = __clone(subscription);
	}
	return $bH;
}
function defer_to_owner(cleanup, $I) {
	const $J = $I;
	let $K = null;
	if ($J[0] === 0) {
		const owner = $J[1];
		$K = defer(owner, cleanup);
	} else {
		$K = undefined;
	}
	return $K;
}
function place_subscriber(id, site) {
	if (tracing.v) {
		trace_hooks.v[8](id, site);
	}
}
function also_releasing(handle, release6) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release6();
		const $bC = previous;
		let $bD = null;
		if ($bC[0] === 0) {
			const earlier = $bC[1];
			$bD = earlier();
		} else {
			$bD = undefined;
		}
		return $bD;
	} ];
}
function settle_together(body, $dR) {
	const $dS = $dR;
	let $dT = null;
	if ($dS[0] === 0) {
		const current = $dS[1];
		$dT = body(current);
	} else {
		const $dU = last(draining_turns.v);
		let $dV = null;
		if ($dU[0] === 0) {
			const draining = $dU[1];
			$dV = body(draining);
		} else {
			$dV = batch(body, $dR);
		}
		$dT = $dV;
	}
	return $dT;
}
function splice_start(held, at3) {
	let $as = null;
	if (at3 > held) {
		$as = held;
	} else {
		$as = at3;
	}
	return $as;
}
function splice_count(held, start, removed) {
	let $at = null;
	if (start + removed > held) {
		$at = held - start;
	} else {
		$at = removed;
	}
	return $at;
}
function kept_index(flags) {
	const index = [ __shared_new([  ]) ];
	rebuild_kept(index, flags);
	return index;
}
function rebuild_kept(index, flags) {
	let tree = [ 0 ];
	for (const flag of flags) {
		const $ca = tree;
		let $bZ = null;
		if (flag) {
			$bZ = 1;
		} else {
			$bZ = 0;
		}
		$ca.push($bZ);
	}
	const size3 = as_i32(flags.length);
	let at3 = 1;
	while (at3 <= size3) {
		const parent = at3 + (at3 & 0 - at3);
		if (parent <= size3) {
			__at_put(tree, as_usize(parent), __at(tree, as_usize(parent), "std/src/reactive/delta.vl:1810:30") + __at(tree, as_usize(at3), "std/src/reactive/delta.vl:1810:56"), "std/src/reactive/delta.vl:1810:4");
		}
		at3 = at3 + 1;
	}
	index[0].v = tree;
}
function kept_before(index, position) {
	let total = 0;
	let at3 = as_i32(position);
	while (at3 > 0) {
		total = total + __at(index[0].v, as_usize(at3), "std/src/reactive/delta.vl:1822:12");
		at3 = at3 - (at3 & 0 - at3);
	}
	return as_usize(total);
}
function kept_add(index, position, delta) {
	const size3 = as_i32(index[0].v.length) - 1;
	let at3 = as_i32(position) + 1;
	while (at3 <= size3) {
		__at_put(index[0].v, as_usize(at3), __at(index[0].v, as_usize(at3), "std/src/reactive/delta.vl:1833:39") + delta, "std/src/reactive/delta.vl:1833:3");
		at3 = at3 + (at3 & 0 - at3);
	}
}
function new5() {
	return [ [  ], [  ], [  ], [  ], [  ], [  ], 0 ];
}
function seat(self, at3) {
	let low = 0;
	let high = self[3].length;
	while (high - low > 1) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[3], middle, "std/src/reactive/lens.vl:157:7") <= at3) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at3 - __at(self[3], low, "std/src/reactive/lens.vl:163:14") ];
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
	let index = from;
	const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:183:14").length;
	while (index < held) {
		const slot2 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:185:15"), index, "std/src/reactive/lens.vl:185:15");
		if (slot2 !== -(1)) {
			__at_put(self[4], as_usize(slot2), block, "std/src/reactive/lens.vl:187:5");
			__at_put(self[5], as_usize(slot2), index, "std/src/reactive/lens.vl:188:5");
		}
		index = index + 1;
	}
}
function open(self, place) {
	const block = as_i32(self[0].length);
	self[0].push([  ]);
	self[2].push(0);
	__insert_at(self[1], place, block, "std/src/reactive/lens.vl:199:14");
	return block;
}
function widen(self, at3, count2) {
	if (count2 === 0 || at3 >= self[6]) {
		return;
	}
	const $dA = seat(self, at3);
	const place = $dA[0];
	const index = $dA[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:209:15");
	let $dB = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:213:5"), index, -(1), "std/src/reactive/lens.vl:213:35");
			made = made + 1;
		}
		$dB = undefined;
	} else {
		const before = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:217:17"));
		let grown = [  ];
		let copied = 0;
		while (copied < index) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:221:16"));
			copied = copied + 1;
		}
		let made2 = 0;
		while (made2 < count2) {
			grown.push(-(1));
			made2 = made2 + 1;
		}
		const held = before.length;
		while (copied < held) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:231:16"));
			copied = copied + 1;
		}
		__at_put(self[0], as_usize(block), grown, "std/src/reactive/lens.vl:234:4");
	}
	$dB;
	self[6] = self[6] + count2;
	renumber(self, block, index + count2);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:238:6").length > 2 * block_size) {
		rebalance(self, place);
	}
	restart(self);
}
function close(self, at3, count2, wakes) {
	let left = count2;
	while (left > 0 && at3 < self[6]) {
		const $dy = seat(self, at3);
		const place = $dy[0];
		const index = $dy[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:250:16");
		const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:251:15").length;
		let $dz = null;
		if (left < held - index) {
			$dz = left;
		} else {
			$dz = held - index;
		}
		const take2 = $dz;
		let taken = 0;
		while (taken < take2) {
			const slot2 = __remove_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:255:16"), index, "std/src/reactive/lens.vl:255:46");
			if (slot2 !== -(1)) {
				__at_put(self[4], as_usize(slot2), -(1), "std/src/reactive/lens.vl:257:6");
				wakes.push([ 2, slot2 ]);
			}
			taken = taken + 1;
		}
		self[6] = self[6] - take2;
		left = left - take2;
		if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:264:7").length === 0) {
			__remove_at(self[1], place, "std/src/reactive/lens.vl:265:16");
		} else {
			renumber(self, block, index);
		}
		restart(self);
	}
}
function rebalance(self, place) {
	const block = __at(self[1], place, "std/src/reactive/lens.vl:275:15");
	const all2 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:276:13"));
	let first = [  ];
	let index = 0;
	while (index < block_size) {
		first.push(__at(all2, index, "std/src/reactive/lens.vl:280:15"));
		index = index + 1;
	}
	__at_put(self[0], as_usize(block), first, "std/src/reactive/lens.vl:283:3");
	let next2 = place + 1;
	const held = all2.length;
	while (index < held) {
		const fresh = open(self, next2);
		let filled = 0;
		while (filled < block_size && index < held) {
			__at(self[0], as_usize(fresh), "std/src/reactive/lens.vl:290:5").push(__at(all2, index, "std/src/reactive/lens.vl:290:40"));
			index = index + 1;
			filled = filled + 1;
		}
		renumber(self, fresh, 0);
		next2 = next2 + 1;
	}
}
function follow(self, at3) {
	while (self[6] <= at3) {
		const places = self[1].length;
		if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:304:34")), "std/src/reactive/lens.vl:304:22").length >= block_size) {
			const fresh = open(self, places);
			__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:306:5");
			self[3].push(self[6]);
		}
		const last3 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:309:15");
		__at(self[0], as_usize(last3), "std/src/reactive/lens.vl:310:4").push(-(1));
		self[6] = self[6] + 1;
	}
	const $cO = seat(self, at3);
	const place = $cO[0];
	const index = $cO[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:314:15");
	const held = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:315:14"), index, "std/src/reactive/lens.vl:315:14");
	if (held !== -(1)) {
		return held;
	}
	const slot2 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:322:3"), index, slot2, "std/src/reactive/lens.vl:322:3");
	return slot2;
}
function apply(self, change2, wakes) {
	const $dw = change2;
	let $dx = null;
	if ($dw[0] === 0) {
		const at3 = $dw[1];
		const removed = $dw[2];
		const inserted = $dw[3];
		close(self, at3, removed, wakes);
		widen(self, at3, inserted);
		$dx = undefined;
	} else if ($dw[0] === 1) {
		const at4 = $dw[1];
		const count2 = $dw[2];
		$dx = widen(self, at4, count2);
	} else if ($dw[0] === 3) {
		const at5 = $dw[1];
		const count3 = $dw[2];
		$dx = close(self, at5, count3, wakes);
	} else if ($dw[0] === 2) {
		const at6 = $dw[1];
		let $dD = null;
		if (at6 < self[6]) {
			const $dC = seat(self, at6);
			const place = $dC[0];
			const index = $dC[1];
			const slot2 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:339:29")), "std/src/reactive/lens.vl:339:17"), index, "std/src/reactive/lens.vl:339:17");
			if (slot2 !== -(1)) {
				wakes.push([ 0, slot2 ]);
			}
			$dD = undefined;
		}
		$dx = $dD;
	} else if ($dw[0] === 4) {
		const at7 = $dw[1];
		const part = $dw[2];
		let $dF = null;
		if (at7 < self[6]) {
			const $dE = seat(self, at7);
			const place2 = $dE[0];
			const index2 = $dE[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:348:29")), "std/src/reactive/lens.vl:348:17"), index2, "std/src/reactive/lens.vl:348:17");
			if (slot3 !== -(1)) {
				wakes.push([ 1, slot3, part ]);
			}
			$dF = undefined;
		}
		$dx = $dF;
	} else if ($dw[0] === 5) {
		const from = $dw[1];
		const to = $dw[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $dG = null;
		if (from > to) {
			$dG = from;
		} else {
			$dG = to;
		}
		const far = $dG;
		while (self[6] <= far) {
			const places = self[1].length;
			if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:362:36")), "std/src/reactive/lens.vl:362:24").length >= block_size) {
				const fresh = open(self, places);
				__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:364:7");
				self[3].push(self[6]);
			}
			const last3 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:367:17");
			__at(self[0], as_usize(last3), "std/src/reactive/lens.vl:368:6").push(-(1));
			self[6] = self[6] + 1;
		}
		const $dH = seat(self, from);
		const left = $dH[0];
		const index3 = $dH[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:372:18");
		const slot4 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:373:16"), index3, "std/src/reactive/lens.vl:373:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:375:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:376:17");
		} else {
			renumber(self, source, index3);
		}
		restart(self);
		let $dI = null;
		if (to >= self[6]) {
			const last4 = self[1].length - 1;
			$dI = [ last4, __at(self[0], as_usize(__at(self[1], last4, "std/src/reactive/lens.vl:384:25")), "std/src/reactive/lens.vl:384:13").length ];
		} else {
			$dI = seat(self, to);
		}
		const $dJ = $dI;
		const place3 = $dJ[0];
		const landing = $dJ[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:388:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:389:5"), landing, slot4, "std/src/reactive/lens.vl:389:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing);
		restart(self);
		$dx = undefined;
	} else {
		let slot5 = 0;
		const slots = as_i32(self[4].length);
		while (slot5 < slots) {
			if (__at(self[4], as_usize(slot5), "std/src/reactive/lens.vl:398:9") !== -(1)) {
				__at_put(self[4], as_usize(slot5), -(1), "std/src/reactive/lens.vl:399:7");
				wakes.push([ 2, slot5 ]);
			}
			slot5 = slot5 + 1;
		}
		self[0] = [  ];
		self[1] = [  ];
		self[2] = [  ];
		self[3] = [  ];
		self[6] = 0;
		$dx = undefined;
	}
	return $dx;
}
function follows_items(self) {
	return true;
}
function release(self, slot2) {
	const block = __at(self[4], as_usize(slot2), "std/src/reactive/lens.vl:418:15");
	if (block === -(1)) {
		return;
	}
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:422:3"), __at(self[5], as_usize(slot2), "std/src/reactive/lens.vl:422:33"), -(1), "std/src/reactive/lens.vl:422:3");
	__at_put(self[4], as_usize(slot2), -(1), "std/src/reactive/lens.vl:423:3");
}
function locate(self, slot2) {
	const block = __at(self[4], as_usize(slot2), "std/src/reactive/lens.vl:427:15");
	if (block === -(1)) {
		return [ 1 ];
	}
	return [ 0, __at(self[3], __at(self[2], as_usize(block), "std/src/reactive/lens.vl:431:20"), "std/src/reactive/lens.vl:431:8") + __at(self[5], as_usize(slot2), "std/src/reactive/lens.vl:431:51") ];
}
function fresh_cells() {
	return [ new6(0, "std/src/reactive/lens.vl:888:33"), __shared_new([  ]), __shared_new(0) ];
}
function wake_all(cells, slot2, $dP) {
	const held = viewing2(cells, (list) => {
		return __clone(__at(list, as_usize(slot2), "std/src/reactive/lens.vl:928:35"));
	});
	notify(held[0], $dP);
	for (const part of __clone(held[1].v)) {
		notify(part[1], $dP);
	}
}
function wake_part(cells, slot2, name, $dQ) {
	const held = viewing2(cells, (list) => {
		return __clone(__at(list, as_usize(slot2), "std/src/reactive/lens.vl:936:35"));
	});
	notify(held[0], $dQ);
	for (const part of __clone(held[1].v)) {
		if (part[0] === name) {
			notify(part[1], $dQ);
		}
	}
}
function extend_run(runs, from, to) {
	const held = runs.v.length;
	let $t = null;
	if (held > 0) {
		const $s = __at(runs.v, held - 1, "std/src/reactive/lens.vl:1761:31");
		const start = $s[0];
		const count2 = $s[1];
		const first = $s[2];
		const leftward = first < start && from === start + count2 && to === first + count2;
		const rightward = first > start && from === start && to === first;
		if (leftward || rightward) {
			__at_put(runs.v, held - 1, [ start, count2 + 1, first ], "std/src/reactive/lens.vl:1765:4");
			return;
		}
		$t = undefined;
	}
	$t;
	runs.v.push([ from, 1, to ]);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function next(self, below) {
	self[0] = (self[0] * 1103 + 12345) % 2147483;
	if (self[0] < 0) {
		self[0] = -(self[0]);
	}
	return as_usize(self[0]) % below;
}
function check(list, followed, step, what) {
	for (const held of followed) {
		let $ep = null;
		if (held[1] === -(1)) {
			if (!(eq3(get4(held[0]), [ 1 ]))) {
				(() => {
					throw __panic("step " + step + " (" + what + "): a dead slot reads " + debug(get4(held[0])), "lens-fuzz.vl:40:5");
				})();
			}
			$ep = undefined;
		} else {
			let found = [ 1 ];
			let at3 = 0;
			for (const id of list) {
				if (id === held[1]) {
					found = [ 0, at3 ];
				}
				at3 = at3 + 1;
			}
			if (!(eq4(at2(held[0]), found)) || !(eq3(get4(held[0]), [ 0, held[1] ]))) {
				(() => {
					throw __panic("step " + step + " (" + what + "): the slot of " + held[1] + " is at " + debug(at2(held[0])) + " reading " + debug(get4(held[0])) + ", the model says " + debug(found), "lens-fuzz.vl:52:5");
				})();
			}
			$ep = undefined;
		}
		$ep;
	}
}
function run(seed3, steps, $a, $b) {
	let random = [ seed3 ];
	let model = [  ];
	let next_id = 0;
	while (next_id < 40) {
		model.push(next_id);
		next_id = next_id + 1;
	}
	const list = sequence(__clone(model), $a);
	const tripled = memo(map(items(list, $b), (id, $M, $N) => {
		return id * 3;
	}), $a, $b, "lens-fuzz.vl:68:46");
	const evens = memo2(filter(items(list, $b), (id, $bM, $bN) => {
		return id % 2 === 0;
	}), $a, $b, "lens-fuzz.vl:69:52");
	const rows2 = rows(list, $b);
	const cursor4 = cursor3(rows2);
	let mirror = get2(rows2);
	const by_id = index_by(list, (id) => {
		return id[0][id[1]];
	}, $b);
	let followed = [  ];
	let step = 0;
	while (step < steps) {
		const size3 = model.length;
		const choice = next(random, 100);
		let what = "";
		let $ee = null;
		if (choice < 14 && size3 > 0) {
			what = "follow";
			const at3 = next(random, size3);
			followed.push([ slot(list, at3, $b), __at(model, at3, "lens-fuzz.vl:86:56") ]);
		} else if (choice < 40) {
			what = "insert";
			const at4 = next(random, size3 + 1);
			insert_at(list, at4, next_id, $a);
			__insert_at(model, at4, next_id, "lens-fuzz.vl:91:10");
			next_id = next_id + 1;
		} else if (choice < 60 && size3 > 0) {
			what = "remove";
			const at5 = next(random, size3);
			const gone = __remove_at(model, at5, "lens-fuzz.vl:96:21");
			remove_at(list, at5, $a);
			mark_gone(followed, gone);
		} else if (choice < 66) {
			what = "bulk insert";
			const at6 = next(random, size3 + 1);
			const count2 = 1 + next(random, 400);
			let run2 = [  ];
			let made = 0;
			while (made < count2) {
				run2.push(next_id);
				__insert_at(model, at6 + made, next_id, "lens-fuzz.vl:108:11");
				next_id = next_id + 1;
				made = made + 1;
			}
			splice3(list, at6, 0, run2, $a);
		} else if (choice < 72 && size3 > 1) {
			what = "bulk remove";
			const at7 = next(random, size3);
			const count3 = 1 + next(random, size3 - at7);
			let taken = 0;
			while (taken < count3) {
				const gone2 = __remove_at(model, at7, "lens-fuzz.vl:120:22");
				mark_gone(followed, gone2);
				taken = taken + 1;
			}
			splice3(list, at7, count3, [  ], $a);
		} else if (choice < 80 && size3 > 0) {
			what = "replacing splice";
			const at8 = next(random, size3);
			const count4 = 1 + next(random, smaller(size3 - at8, 6));
			let taken2 = 0;
			while (taken2 < count4) {
				const gone3 = __remove_at(model, at8, "lens-fuzz.vl:132:22");
				mark_gone(followed, gone3);
				taken2 = taken2 + 1;
			}
			const arriving = next(random, 7);
			let run3 = [  ];
			let made2 = 0;
			while (made2 < arriving) {
				run3.push(next_id);
				__insert_at(model, at8 + made2, next_id, "lens-fuzz.vl:141:11");
				next_id = next_id + 1;
				made2 = made2 + 1;
			}
			splice3(list, at8, count4, run3, $a);
		} else if (choice < 90 && size3 > 1) {
			what = "move";
			const from = next(random, size3);
			const to = next(random, size3);
			if (from !== to) {
				const moved = __remove_at(model, from, "lens-fuzz.vl:151:23");
				__insert_at(model, to, moved, "lens-fuzz.vl:152:11");
				move_item(list, from, to, $a);
			}
			$ee = undefined;
		} else if (size3 > 0) {
			what = "replace";
			const at9 = next(random, size3);
			const old = __at(model, at9, "lens-fuzz.vl:159:14");
			__at_put(model, at9, next_id, "lens-fuzz.vl:160:4");
			replace(list, at9, next_id, $a);
			let index = 0;
			while (index < followed.length) {
				if (__at(followed, index, "lens-fuzz.vl:164:8")[1] === old) {
					__at(followed, index, "lens-fuzz.vl:165:6")[1] = next_id;
				}
				index = index + 1;
			}
			next_id = next_id + 1;
		}
		$ee;
		if (!(eq(all(list), model))) {
			(() => {
				throw __panic("step " + step + " (" + what + "): the list and the model differ", "lens-fuzz.vl:172:4");
			})();
		}
		check(model, followed, step, what);
		if (!(eq(get6(tripled), map2(model, (id) => {
			return id * 3;
		})))) {
			(() => {
				throw __panic("step " + step + " (" + what + "): map over the lens fell out of step", "lens-fuzz.vl:176:4");
			})();
		}
		if (!(eq(get6(evens), filter2(model, (id) => {
			return id % 2 === 0;
		})))) {
			(() => {
				throw __panic("step " + step + " (" + what + "): filter over the lens fell out of step", "lens-fuzz.vl:179:4");
			})();
		}
		for (const op of since3(rows2, cursor4)) {
			replay(mirror, op);
		}
		let $eH = null;
		if (step % 25 === 0) {
			check_rows(model, mirror, step, what);
			check_index(model, find(by_id, __at(model, next(random, model.length), "lens-fuzz.vl:186:34")), step, what);
			if (get2(rows2).length !== model.length) {
				(() => {
					throw __panic("step " + step + " (" + what + "): the row feed holds " + get2(rows2).length + " slots for " + model.length + " items", "lens-fuzz.vl:188:5");
				})();
			}
			$eH = undefined;
		}
		$eH;
		step = step + 1;
	}
	console.log("fuzz seed " + seed3 + ": " + steps + " writes, " + followed.length + " slots, " + model.length + " items held at the end");
}
function smaller(first, second) {
	let $ea = null;
	if (first < second) {
		$ea = first;
	} else {
		$ea = second;
	}
	return $ea;
}
function replay(mirror, op) {
	const $eB = op;
	let $eC = null;
	if ($eB[0] === 0) {
		const at3 = $eB[1];
		const removed = $eB[2];
		const inserted = $eB[3];
		let taken = 0;
		while (taken < removed.length) {
			__remove_at(mirror, at3, "lens-fuzz.vl:213:12");
			taken = taken + 1;
		}
		let offset = 0;
		for (const slot2 of inserted) {
			__insert_at(mirror, at3 + offset, __clone(slot2), "lens-fuzz.vl:218:12");
			offset = offset + 1;
		}
		$eC = undefined;
	} else if ($eB[0] === 1) {
		const at4 = $eB[1];
		const _old = $eB[2];
		const slot3 = $eB[3];
		$eC = __at_put(mirror, at4, __clone(slot3), "lens-fuzz.vl:222:47");
	} else if ($eB[0] === 2) {
		const all2 = $eB[1];
		$eC = __replace(mirror, __clone(all2));
	} else {
		const from = $eB[1];
		const count2 = $eB[2];
		const to = $eB[3];
		let run2 = [  ];
		let taken2 = 0;
		while (taken2 < count2) {
			run2.push(__remove_at(mirror, from, "lens-fuzz.vl:228:21"));
			taken2 = taken2 + 1;
		}
		let offset2 = 0;
		for (const slot4 of run2) {
			__insert_at(mirror, to + offset2, __clone(slot4), "lens-fuzz.vl:233:12");
			offset2 = offset2 + 1;
		}
		$eC = undefined;
	}
	return $eC;
}
function check_rows(model, mirror, step, what) {
	if (mirror.length !== model.length) {
		(() => {
			throw __panic("step " + step + " (" + what + "): the replayed rows hold " + mirror.length + " slots for " + model.length + " items", "lens-fuzz.vl:248:3");
		})();
	}
	let at3 = 0;
	for (const slot2 of mirror) {
		if (!(eq3(get4(slot2), [ 0, __at(model, at3, "lens-fuzz.vl:252:25") ])) || !(eq4(at2(slot2), [ 0, at3 ]))) {
			(() => {
				throw __panic("step " + step + " (" + what + "): replayed row " + at3 + " reads " + debug(get4(slot2)) + " at " + debug(at2(slot2)) + ", the model holds " + __at(model, at3, "lens-fuzz.vl:253:120"), "lens-fuzz.vl:253:4");
			})();
		}
		at3 = at3 + 1;
	}
}
function check_index(model, found, step, what) {
	const $eF = found;
	if ($eF[0] === 0) {

	} else {
		(() => {
			throw __panic("step " + step + " (" + what + "): the index lost an item", "lens-fuzz.vl:266:31");
		})();
	}
	const $eG = at2($eF[1]);
	if ($eG[0] === 0) {

	} else {
		(() => {
			throw __panic("step " + step + " (" + what + "): the index holds a dead slot", "lens-fuzz.vl:267:33");
		})();
	}
	if (!(eq3(get4($eF[1]), [ 0, __at(model, $eG[1], "lens-fuzz.vl:268:24") ]))) {
		(() => {
			throw __panic("step " + step + " (" + what + "): the index\'s slot reads " + debug(get4($eF[1])) + ", the model holds " + __at(model, $eG[1], "lens-fuzz.vl:269:95") + " there", "lens-fuzz.vl:269:3");
		})();
	}
}
function mark_gone(followed, gone) {
	let index = 0;
	while (index < followed.length) {
		if (__at(followed, index, "lens-fuzz.vl:276:6")[1] === gone) {
			__at(followed, index, "lens-fuzz.vl:277:4")[1] = -(1);
		}
		index = index + 1;
	}
}
function item(self, at3) {
	return __list_get(self, at3);
}
function lend(self, at3, read) {
	if (at3 < self.length) {
		read(__at_view(self, at3, "std/src/reactive/lens.vl:2008:10"));
	}
}
function put(self, at3, value) {
	__at_put(self, at3, __clone(value), "std/src/reactive/lens.vl:2003:3");
}
function change(self, at3, with2) {
	if (at3 < self.length) {
		with2(__at_view(self, at3, "std/src/reactive/lens.vl:2014:14"));
	}
}
function new6(value, caller) {
	let subscribers = [  ];
	const cell = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell[0]), caller);
	}
	return cell;
}
function over(value, handler, access) {
	return [ [ 0, __shared_new(value) ], __shared_new(handler), __clone(access), __shared_new([  ]), new6(0, "std/src/reactive/lens.vl:647:28"), new6(0, "std/src/reactive/lens.vl:648:24"), new6(0, "std/src/reactive/lens.vl:649:24"), __shared_new([  ]), __shared_new(0), __shared_new([ 1 ]) ];
}
function sequence(value, $c) {
	return over(value, new5(), [ (held, at3) => {
		return item(held, at3);
	}, (held, at3, read) => {
		return lend(held, at3, read);
	}, (held, at3, value2) => {
		return put(held, at3, value2, $c);
	}, (held, at3, with2) => {
		return change(held, at3, with2, $c);
	} ]);
}
function new7() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function lend2(value, read) {
	return read(value);
}
function viewing(cell, read) {
	return lend2(cell.v, read);
}
function peek_storage(storage, read) {
	const $i = storage;
	let $j = null;
	if ($i[0] === 0) {
		const cell = $i[1];
		$j = viewing(cell, read);
	} else {
		const port = $i[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $k = out;
		let $l = null;
		if ($k[0] === 0) {
			const value = __clone($k[1]);
			$l = value;
		} else {
			$l = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:632:13");
			})();
		}
		$j = $l;
	}
	return $j;
}
function arrived_at(storage, at3, count2) {
	return peek_storage(storage, (held) => {
		let arrived = [  ];
		let index = at3;
		while (index < at3 + count2) {
			const $h = item(held, index);
			if ($h[0] === 0) {
				arrived.push($h[1]);
			}
			index = index + 1;
		}
		return arrived;
	});
}
function viewing2(cell, read) {
	return lend2(cell.v, read);
}
function peek_storage2(storage, read) {
	const $m = storage;
	let $n = null;
	if ($m[0] === 0) {
		const cell = $m[1];
		$n = viewing2(cell, read);
	} else {
		const port = $m[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $o = out;
		let $p = null;
		if ($o[0] === 0) {
			const value = __clone($o[1]);
			$p = value;
		} else {
			$p = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:632:13");
			})();
		}
		$n = $p;
	}
	return $n;
}
function one(item2) {
	const $q = item2;
	let $r = null;
	if ($q[0] === 0) {
		const value = $q[1];
		$r = [ __clone(value) ];
	} else {
		$r = [  ];
	}
	return $r;
}
function count(self) {
	return self.length;
}
function all_of(storage) {
	return peek_storage(storage, (held) => {
		let out = [  ];
		let at3 = 0;
		const count2 = count(held);
		while (at3 < count2) {
			const $w = item(held, at3);
			if ($w[0] === 0) {
				out.push($w[1]);
			}
			at3 = at3 + 1;
		}
		return out;
	});
}
function trim(self) {
	let lowest = self[1].v;
	for (const cursor4 of self[3].v) {
		const at3 = cursor4[1].v;
		if (at3 < lowest) {
			lowest = at3;
		}
	}
	const base = self[2].v;
	if (lowest > base) {
		const dropped = lowest - base;
		let kept = [  ];
		let index = as_usize(dropped);
		const held = __clone(self[0].v);
		const length = held.length;
		while (index < length) {
			const $y = __list_get(held, index);
			let $z = null;
			if ($y[0] === 0) {
				const op = $y[1];
				$z = kept.push(op);
			} else {
				$z = undefined;
			}
			$z;
			index = index + 1;
		}
		self[0].v = kept;
		self[2].v = lowest;
	}
}
function record(self, op) {
	trim(self);
	const next2 = self[1].v + 1;
	self[1].v = next2;
	if (self[0].v.length >= self[4]) {
		self[0].v = [  ];
		self[2].v = next2;
	} else {
		self[0].v.push(__clone(op));
	}
}
function record_set(log, storage, leaving, at3, late) {
	if (late) {
		record(log, [ 2, all_of(storage) ]);
		return;
	}
	const before = __list_get(leaving.v, 0);
	const after = peek_storage2(storage, (held) => {
		return item(held, at3);
	});
	const $D = before;
	let $E = undefined;
	if ($D[0] === 0 && (($E = after), $E[0] === 0)) {
		record(log, [ 1, at3, __clone($D[1]), __clone($E[1]) ]);
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
function release2(self) {
	untap(self[0], self[2]);
}
function items_into(self, log, $e) {
	const leaving = __shared_new([  ]);
	const runs = __shared_new([  ]);
	const skipping = __shared_new(0);
	const storage = __clone(self[0]);
	const tap2 = tap(self, (change2) => {
		const $f = change2;
		let $g = null;
		if ($f[0] === 0) {
			const at3 = $f[1];
			const removed = $f[2];
			const _inserted = $f[3];
			leaving.v = arrived_at(storage, at3, removed);
			$g = undefined;
		} else if ($f[0] === 3) {
			const at4 = $f[1];
			const count2 = $f[2];
			leaving.v = arrived_at(storage, at4, count2);
			$g = undefined;
		} else if ($f[0] === 2) {
			const at5 = $f[1];
			leaving.v = one(peek_storage2(storage, (held) => {
				return item(held, at5);
			}));
			$g = undefined;
		} else if ($f[0] === 4) {
			const at6 = $f[1];
			const _part = $f[2];
			leaving.v = one(peek_storage2(storage, (held) => {
				return item(held, at6);
			}));
			$g = undefined;
		} else if ($f[0] === 5) {
			const from = $f[1];
			const to = $f[2];
			$g = extend_run(runs, from, to);
		} else {
			$g = undefined;
		}
		return $g;
	}, (change2, stage) => {
		const $u = stage;
		let $v = null;
		if ($u[0] === 2) {
			record(log, [ 2, all_of(storage) ]);
			return;
			$v = undefined;
		} else if ($u[0] === 3) {
			return;
		} else {
			$v = undefined;
		}
		$v;
		const $A = stage;
		const late = $A[0] === 1;
		const $B = change2;
		let $C = null;
		if ($B[0] === 0) {
			const at3 = $B[1];
			const removed = $B[2];
			const inserted = $B[3];
			if (late && removed > 0) {
				record(log, [ 2, all_of(storage) ]);
			} else if (late) {
				record(log, [ 0, at3, [  ], arrived_at(storage, at3, inserted) ]);
			} else {
				record(log, [ 0, at3, __clone(leaving.v), arrived_at(storage, at3, inserted) ]);
			}
			$C = undefined;
		} else if ($B[0] === 1) {
			const at4 = $B[1];
			const count2 = $B[2];
			record(log, [ 0, at4, [  ], arrived_at(storage, at4, count2) ]);
			$C = undefined;
		} else if ($B[0] === 3) {
			const at5 = $B[1];
			const _count = $B[2];
			if (late) {
				record(log, [ 2, all_of(storage) ]);
			} else {
				record(log, [ 0, at5, __clone(leaving.v), [  ] ]);
			}
			$C = undefined;
		} else if ($B[0] === 2) {
			const at6 = $B[1];
			$C = record_set(log, storage, leaving, at6, late);
		} else if ($B[0] === 4) {
			const at7 = $B[1];
			const _part = $B[2];
			$C = record_set(log, storage, leaving, at7, late);
		} else if ($B[0] === 5) {
			const from = $B[1];
			const to = $B[2];
			if (skipping.v > 0) {
				const $F = [ skipping, "v" ];
				skipping.v = $F[0][$F[1]] - 1;
				return;
			}
			if (runs.v.length === 0) {
				record(log, [ 3, from, 1, to ]);
				return;
			}
			const $G = __remove_at(runs.v, 0, "std/src/reactive/lens.vl:1447:47");
			const start = $G[0];
			const count3 = $G[1];
			const first = $G[2];
			skipping.v = count3 - 1;
			let $H = null;
			if (first > start) {
				$H = first + 1 - count3;
			} else {
				$H = first;
			}
			const landing = $H;
			record(log, [ 3, start, count3, landing ]);
			$C = undefined;
		} else {
			$C = record(log, [ 2, all_of(storage) ]);
		}
		return $C;
	});
	const feed = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release2(feed);
	}, $e);
	return feed;
}
function items(self, $d) {
	return items_into(self, new7(), $d);
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
function since(self, cursor4) {
	const at3 = cursor4[1].v;
	const version = self[1].v;
	let ops = [  ];
	if (at3 >= version) {
		return [ 0, ops ];
	}
	cursor4[1].v = version;
	const base = self[2].v;
	if (at3 < base) {
		return [ 1 ];
	}
	let index = as_usize(at3 - base);
	const held = __clone(self[0].v);
	const length = held.length;
	while (index < length) {
		const $Q = __list_get(held, index);
		let $R = null;
		if ($Q[0] === 0) {
			const op = $Q[1];
			$R = ops.push(op);
		} else {
			$R = undefined;
		}
		$R;
		index = index + 1;
	}
	return [ 0, ops ];
}
function all(self) {
	return all_of(self[0]);
}
function since2(self, cursor4) {
	const $S = since(self[1], cursor4);
	let $T = null;
	if ($S[0] === 0) {
		const recorded = $S[1];
		$T = recorded;
	} else {
		$T = [ [ 2, all(self[0]) ] ];
	}
	return $T;
}
function reader(self) {
	return (cursor4) => {
		return since2(self, cursor4);
	};
}
function get(self) {
	return all(self[0]);
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
	return on_settle(self[0][4], subscriber);
}
function drop_cursor(self, cursor4) {
	let kept = [  ];
	for (const held of self[3].v) {
		if (held[0] !== cursor4[0]) {
			kept.push(__clone(held));
		}
	}
	self[3].v = kept;
}
function drop_cursor2(self, cursor4) {
	drop_cursor(self[1], cursor4);
}
function open2(self) {
	const cursor4 = cursor2(self);
	const read = reader(self);
	return [ () => {
		return get(self);
	}, (subscriber) => {
		return on_settle2(self, subscriber);
	}, () => {
		return read(cursor4);
	}, () => {
		return drop_cursor2(self, cursor4);
	} ];
}
function run_element(stage, body) {
	const nursery2 = stage_nursery(stage);
	const value = (($Z) => {
		return (($aa) => {
			return body($Z, $aa);
		})(nursery2);
	})(stage);
	return [ value, split_registrations(stage) ];
}
function start_element_value(self) {
	return [ 0, self ];
}
function element_core(stage, run2) {
	const ids = [  ];
	const items2 = [  ];
	const values = [  ];
	const holds2 = new Map();
	const dirty = [  ];
	const consumer = [ 1 ];
	return [ __shared_new(ids), __shared_new(items2), __shared_new(values), __shared_new(holds2), __shared_new(dirty), __shared_new(0), __clone(stage), __shared_new(consumer), run2 ];
}
function keep_hold(core, id, hold) {
	element_holds_allocated_count.v = element_holds_allocated_count.v + 1;
	core[3].v.set(hash(id), __clone(hold));
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $aj = null;
	if (is_empty(self)) {
		$aj = [ 1 ];
	} else {
		$aj = __list_get(self, self.length - 1);
	}
	return $aj;
}
function admit(core, item2) {
	const id = core[5].v;
	core[5].v = id + 1;
	const $ac = core[8](item2);
	const answer = $ac[0];
	const owner = $ac[1];
	const $ad = answer;
	let $ae = null;
	if ($ad[0] === 0) {
		const value = $ad[1];
		const $af = owner;
		let $ag = null;
		if ($af[0] === 0) {
			const held = $af[1];
			$ag = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held) ] ]);
		} else {
			$ag = undefined;
		}
		$ag;
		$ae = [ id, __clone(value) ];
	} else {
		const instance = $ad[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $ah = consumer.v;
			let $ai = null;
			if ($ah[0] === 0) {
				const downstream = $ah[1];
				$ai = wake(downstream);
			} else {
				$ai = undefined;
			}
			return $ai;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner ]);
		$ae = [ id, pull() ];
	}
	return $ae;
}
function seed(core, items2) {
	for (const item2 of items2) {
		const $ap = admit(core, item2);
		const id = $ap[0];
		const value = $ap[1];
		core[0].v.push(id);
		core[1].v.push(__clone(item2));
		core[2].v.push(value);
	}
}
function fresh_list(list) {
	let copy = [  ];
	for (const item2 of list) {
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
function held_pairs(core, at3, count2) {
	let pairs = [  ];
	let index = at3;
	while (index < at3 + count2) {
		pairs.push([ __clone(__at(core[1].v, index, "std/src/reactive/delta.vl:1618:15")), __clone(__at(core[2].v, index, "std/src/reactive/delta.vl:1618:41")) ]);
		index = index + 1;
	}
	return pairs;
}
function last2(self) {
	let $ay = null;
	if (is_empty(self)) {
		$ay = [ 1 ];
	} else {
		$ay = __list_get(self, self.length - 1);
	}
	return $ay;
}
function is_none(self) {
	const $aL = self;
	return $aL[0] === 1;
}
function retire(core, id) {
	const $au = __map_get(core[3].v, hash(id));
	let $av = null;
	if ($au[0] === 0) {
		const hold = $au[1];
		core[3].v.delete(hash(id));
		const $aw = hold[2];
		let $ax = null;
		if ($aw[0] === 0) {
			const handle = $aw[1];
			$ax = detach(handle);
		} else {
			$ax = undefined;
		}
		$ax;
		hold[3]();
		const $aH = hold[4];
		let $aI = null;
		if ($aH[0] === 0) {
			const owner = $aH[1];
			$aI = dispose2(owner);
		} else {
			$aI = undefined;
		}
		$av = $aI;
	} else {
		$av = undefined;
	}
	return $av;
}
function all_pairs(core) {
	return held_pairs(core, 0, core[0].v.length);
}
function resolve(core, ops) {
	let events = [  ];
	for (const op of ops) {
		const $aq = op;
		let $ar = null;
		if ($aq[0] === 0) {
			const at3 = $aq[1];
			const removed = $aq[2];
			const inserted = $aq[3];
			const held = core[0].v.length;
			const start = splice_start(held, at3);
			const taking = splice_count(held, start, removed.length);
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
				const $aT = admit(core, item2);
				const id2 = $aT[0];
				const value = $aT[1];
				__insert_at(core[0].v, start + offset, id2, "std/src/reactive/delta.vl:1652:23");
				__insert_at(core[1].v, start + offset, __clone(item2), "std/src/reactive/delta.vl:1653:25");
				__insert_at(core[2].v, start + offset, __clone(value), "std/src/reactive/delta.vl:1654:26");
				arrived.push([ __clone(item2), value ]);
				offset = offset + 1;
			}
			events.push([ 0, start, left, arrived ]);
			$ar = undefined;
		} else if ($aq[0] === 1) {
			const at4 = $aq[1];
			const _was = $aq[2];
			const item3 = $aq[3];
			const $aU = __list_get(core[0].v, at4);
			let $aV = null;
			if ($aU[0] === 0) {
				const old_id = $aU[1];
				const before = [ __clone(__at(core[1].v, at4, "std/src/reactive/delta.vl:1663:21")), __clone(__at(core[2].v, at4, "std/src/reactive/delta.vl:1663:44")) ];
				retire(core, old_id);
				const $aW = admit(core, item3);
				const id3 = $aW[0];
				const value2 = $aW[1];
				__at_put(core[0].v, at4, id3, "std/src/reactive/delta.vl:1669:7");
				__at_put(core[1].v, at4, __clone(item3), "std/src/reactive/delta.vl:1670:7");
				__at_put(core[2].v, at4, __clone(value2), "std/src/reactive/delta.vl:1671:7");
				events.push([ 1, at4, before, [ __clone(item3), value2 ], true ]);
				$aV = undefined;
			} else {
				$aV = undefined;
			}
			$ar = $aV;
		} else if ($aq[0] === 2) {
			const items2 = $aq[1];
			const left2 = all_pairs(core);
			for (const id4 of __clone(core[0].v)) {
				retire(core, id4);
			}
			core[0].v = [  ];
			core[1].v = [  ];
			core[2].v = [  ];
			seed(core, items2);
			events.push([ 2, left2, all_pairs(core) ]);
			$ar = undefined;
		} else {
			const from = $aq[1];
			const count2 = $aq[2];
			const to = $aq[3];
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
			$ar = undefined;
		}
		$ar;
	}
	const woke = __clone(core[4].v);
	let $bb = null;
	if (woke.length > 0) {
		core[4].v = [  ];
		let positions = new Map();
		let index = 0;
		for (const id5 of core[0].v) {
			positions.set(hash(id5), index);
			index = index + 1;
		}
		let seen = new Map();
		for (const id6 of woke) {
			if (seen.has(hash(id6))) {
				continue;
			}
			seen.set(hash(id6), true);
			const $aX = __map_get(positions, hash(id6));
			let $aY = null;
			if ($aX[0] === 0) {
				const at5 = $aX[1];
				const $aZ = __map_get(core[3].v, hash(id6));
				let $ba = null;
				if ($aZ[0] === 0) {
					const hold = $aZ[1];
					if (hold[0]) {
						const item4 = __clone(__at(core[1].v, at5, "std/src/reactive/delta.vl:1731:19"));
						const before2 = __clone(__at(core[2].v, at5, "std/src/reactive/delta.vl:1732:21"));
						const now = hold[1]();
						__at_put(core[2].v, at5, __clone(now), "std/src/reactive/delta.vl:1734:8");
						events.push([ 1, at5, [ __clone(item4), before2 ], [ item4, now ], false ]);
					}
					$ba = undefined;
				} else {
					$ba = undefined;
				}
				$aY = $ba;
			} else {
				$aY = undefined;
			}
			$aY;
		}
		$bb = undefined;
	}
	$bb;
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
	let ops = [  ];
	for (const event of events) {
		const $bc = event;
		let $bd = null;
		if ($bc[0] === 0) {
			const at3 = $bc[1];
			const left = $bc[2];
			const arrived = $bc[3];
			ops.push([ 0, at3, map2(left, (pair) => {
				return __clone(pair[1]);
			}), map2(arrived, (pair) => {
				return __clone(pair[1]);
			}) ]);
			$bd = undefined;
		} else if ($bc[0] === 1) {
			const at4 = $bc[1];
			const before = $bc[2];
			const after = $bc[3];
			const _rerun = $bc[4];
			ops.push([ 1, at4, __clone(before[1]), __clone(after[1]) ]);
			$bd = undefined;
		} else if ($bc[0] === 2) {
			const _left = $bc[1];
			const now = $bc[2];
			$bd = ops.push([ 2, map2(now, (pair) => {
				return __clone(pair[1]);
			}) ]);
		} else {
			const from = $bc[1];
			const count2 = $bc[2];
			const to = $bc[3];
			$bd = ops.push([ 3, from, count2, to ]);
		}
		$bd;
	}
	return ops;
}
function release_core(core) {
	for (const id of __clone(core[0].v)) {
		retire(core, id);
	}
	dispose2(core[6]);
}
function core_release(core, upstream) {
	const release6 = upstream[3];
	return () => {
		release_core(core);
		release6();
		return;
	};
}
function open3(self) {
	const transform = self[1];
	const upstream = open2(__clone(self[0]));
	const stage = new4();
	const core = element_core(stage, (item2) => {
		const $ab = run_element(stage, ($U, $V) => {
			return transform(item2, $U, $V);
		});
		const answer = $ab[0];
		const owner = $ab[1];
		return [ start_element_value(answer), owner ];
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
	return [ __shared_new(elements), new6(0, "std/src/reactive/delta.vl:792:68"), __clone(log) ];
}
function of(elements) {
	return list_cell(elements, new7());
}
function size(self) {
	return self[0].v.length;
}
function splice_in_place(list, start, taking, inserted) {
	let left = [  ];
	let taken = 0;
	while (taken < taking) {
		left.push(__remove_at(list, start, "std/src/reactive/delta.vl:695:18"));
		taken = taken + 1;
	}
	let offset = 0;
	const arriving = inserted.length;
	while (offset < arriving) {
		const $bk = __list_get(inserted, offset);
		let $bl = null;
		if ($bk[0] === 0) {
			const value = $bk[1];
			$bl = __insert_at(list, start + offset, value, "std/src/reactive/delta.vl:702:28");
		} else {
			$bl = undefined;
		}
		$bl;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function at(self) {
	return self[1].v;
}
function notify(self, $bo) {
	const traced = tracing.v;
	let $bp = null;
	if (traced) {
		$bp = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$bp = "";
	}
	const cause = $bp;
	const $bq = $bo;
	let $br = null;
	if ($bq[0] === 0) {
		const turn = $bq[1];
		$br = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $bs = last(draining_turns.v);
		let $bt = null;
		if ($bs[0] === 0) {
			const draining = $bs[1];
			$bt = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $bu = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$bu = undefined;
				}
				$bu;
			}
			$bt = undefined;
		}
		$br = $bt;
	}
	return $br;
}
function set(self, value, $bn, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $bn);
}
function publish(cell, $bm) {
	set(cell[1], at(cell[2]), $bm, "std/src/reactive/delta.vl:799:17");
}
function splice(self, at3, removed, inserted, $bj) {
	const held = size(self);
	const start = splice_start(held, at3);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	record(self[2], splice_in_place(self[0].v, start, taking, inserted));
	publish(self, $bj);
}
function set_at(self, at3, value, $bv) {
	const $bw = __list_get(self[0].v, at3);
	let $bx = null;
	if ($bw[0] === 0) {
		const previous = $bw[1];
		__at_put(self[0].v, at3, __clone(value), "std/src/reactive/delta.vl:855:5");
		record(self[2], [ 1, at3, previous, __clone(value) ]);
		publish(self, $bv);
		$bx = undefined;
	} else {
		$bx = undefined;
	}
	return $bx;
}
function set2(self, value, $by, caller) {
	self[0].v = __clone(value);
	record(self[2], [ 2, __clone(value) ]);
	publish(self, $by);
}
function move_range(self, from, count2, to, $bz) {
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
		const $bA = __list_get(lifted, offset);
		let $bB = null;
		if ($bA[0] === 0) {
			const value = $bA[1];
			$bB = __insert_at(self[0].v, to + offset, value, "std/src/reactive/delta.vl:881:43");
		} else {
			$bB = undefined;
		}
		$bB;
		offset = offset + 1;
	}
	record(self[2], [ 3, from, count2, to ]);
	publish(self, $bz);
}
function apply_ops(cell, ops, $bg) {
	for (const op of ops) {
		const $bh = op;
		let $bi = null;
		if ($bh[0] === 0) {
			const at3 = $bh[1];
			const removed = $bh[2];
			const inserted = $bh[3];
			$bi = splice(cell, at3, removed.length, inserted, $bg);
		} else if ($bh[0] === 1) {
			const at4 = $bh[1];
			const _was = $bh[2];
			const value = $bh[3];
			$bi = set_at(cell, at4, value, $bg);
		} else if ($bh[0] === 2) {
			const items2 = $bh[1];
			$bi = set2(cell, items2, $bg, "std/src/reactive/delta.vl:1325:36");
		} else {
			const from = $bh[1];
			const count2 = $bh[2];
			const to = $bh[3];
			$bi = move_range(cell, from, count2, to, $bg);
		}
		$bi;
	}
}
function take(self, item2, $bI) {
	defer(self, () => {
		dispose(item2, $bI);
		return;
	});
	return __clone(item2);
}
function seal_collection(instance, owned, $be, $bf, caller) {
	const sealed = of(fresh_list(instance[0]()));
	const drain2 = instance[2];
	if (owned) {
		const refresh = subscriber_of(() => {
			return apply_ops(sealed, drain2(), $be);
		}, true);
		place_subscriber(refresh[0], caller);
		const refreshed = instance[1](refresh);
		also_releasing(refreshed, instance[3]);
		register_with_owner(refreshed, $be, $bf);
	} else {
		const cell_global_refresh = () => {
			return apply_ops(sealed, drain2(), $be);
		};
		const refresh2 = subscriber_of(cell_global_refresh, true);
		place_subscriber(refresh2[0], caller);
		instance[1](refresh2);
	}
	return sealed;
}
function memo(self, $O, $P, caller) {
	return [ seal_collection(open3(self), true, $O, $P, caller) ];
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
	const $bR = core[8](item2);
	const answer = $bR[0];
	const owner = $bR[1];
	const $bS = answer;
	let $bT = null;
	if ($bS[0] === 0) {
		const value = $bS[1];
		const $bU = owner;
		let $bV = null;
		if ($bU[0] === 0) {
			const held = $bU[1];
			$bV = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held) ] ]);
		} else {
			$bV = undefined;
		}
		$bV;
		$bT = [ id, __clone(value) ];
	} else {
		const instance = $bS[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $bW = consumer.v;
			let $bX = null;
			if ($bW[0] === 0) {
				const downstream = $bW[1];
				$bX = wake(downstream);
			} else {
				$bX = undefined;
			}
			return $bX;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner ]);
		$bT = [ id, pull() ];
	}
	return $bT;
}
function seed2(core, items2) {
	for (const item2 of items2) {
		const $bY = admit2(core, item2);
		const id = $bY[0];
		const value = $bY[1];
		core[0].v.push(id);
		core[1].v.push(__clone(item2));
		core[2].v.push(value);
	}
}
function retire2(core, id) {
	const $cf = __map_get(core[3].v, hash(id));
	let $cg = null;
	if ($cf[0] === 0) {
		const hold = $cf[1];
		core[3].v.delete(hash(id));
		const $ch = hold[2];
		let $ci = null;
		if ($ch[0] === 0) {
			const handle = $ch[1];
			$ci = detach(handle);
		} else {
			$ci = undefined;
		}
		$ci;
		hold[3]();
		const $cj = hold[4];
		let $ck = null;
		if ($cj[0] === 0) {
			const owner = $cj[1];
			$ck = dispose2(owner);
		} else {
			$ck = undefined;
		}
		$cg = $ck;
	} else {
		$cg = undefined;
	}
	return $cg;
}
function resolve2(core, ops) {
	let events = [  ];
	for (const op of ops) {
		const $cd = op;
		let $ce = null;
		if ($cd[0] === 0) {
			const at3 = $cd[1];
			const removed = $cd[2];
			const inserted = $cd[3];
			const held = core[0].v.length;
			const start = splice_start(held, at3);
			const taking = splice_count(held, start, removed.length);
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
				const $cl = admit2(core, item2);
				const id2 = $cl[0];
				const value = $cl[1];
				__insert_at(core[0].v, start + offset, id2, "std/src/reactive/delta.vl:1652:23");
				__insert_at(core[1].v, start + offset, __clone(item2), "std/src/reactive/delta.vl:1653:25");
				__insert_at(core[2].v, start + offset, __clone(value), "std/src/reactive/delta.vl:1654:26");
				arrived.push([ __clone(item2), value ]);
				offset = offset + 1;
			}
			events.push([ 0, start, left, arrived ]);
			$ce = undefined;
		} else if ($cd[0] === 1) {
			const at4 = $cd[1];
			const _was = $cd[2];
			const item3 = $cd[3];
			const $cm = __list_get(core[0].v, at4);
			let $cn = null;
			if ($cm[0] === 0) {
				const old_id = $cm[1];
				const before = [ __clone(__at(core[1].v, at4, "std/src/reactive/delta.vl:1663:21")), __clone(__at(core[2].v, at4, "std/src/reactive/delta.vl:1663:44")) ];
				retire2(core, old_id);
				const $co = admit2(core, item3);
				const id3 = $co[0];
				const value2 = $co[1];
				__at_put(core[0].v, at4, id3, "std/src/reactive/delta.vl:1669:7");
				__at_put(core[1].v, at4, __clone(item3), "std/src/reactive/delta.vl:1670:7");
				__at_put(core[2].v, at4, __clone(value2), "std/src/reactive/delta.vl:1671:7");
				events.push([ 1, at4, before, [ __clone(item3), value2 ], true ]);
				$cn = undefined;
			} else {
				$cn = undefined;
			}
			$ce = $cn;
		} else if ($cd[0] === 2) {
			const items2 = $cd[1];
			const left2 = all_pairs(core);
			for (const id4 of __clone(core[0].v)) {
				retire2(core, id4);
			}
			core[0].v = [  ];
			core[1].v = [  ];
			core[2].v = [  ];
			seed2(core, items2);
			events.push([ 2, left2, all_pairs(core) ]);
			$ce = undefined;
		} else {
			const from = $cd[1];
			const count2 = $cd[2];
			const to = $cd[3];
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
			$ce = undefined;
		}
		$ce;
	}
	const woke = __clone(core[4].v);
	let $ct = null;
	if (woke.length > 0) {
		core[4].v = [  ];
		let positions = new Map();
		let index = 0;
		for (const id5 of core[0].v) {
			positions.set(hash(id5), index);
			index = index + 1;
		}
		let seen = new Map();
		for (const id6 of woke) {
			if (seen.has(hash(id6))) {
				continue;
			}
			seen.set(hash(id6), true);
			const $cp = __map_get(positions, hash(id6));
			let $cq = null;
			if ($cp[0] === 0) {
				const at5 = $cp[1];
				const $cr = __map_get(core[3].v, hash(id6));
				let $cs = null;
				if ($cr[0] === 0) {
					const hold = $cr[1];
					if (hold[0]) {
						const item4 = __clone(__at(core[1].v, at5, "std/src/reactive/delta.vl:1731:19"));
						const before2 = __clone(__at(core[2].v, at5, "std/src/reactive/delta.vl:1732:21"));
						const now = hold[1]();
						__at_put(core[2].v, at5, __clone(now), "std/src/reactive/delta.vl:1734:8");
						events.push([ 1, at5, [ __clone(item4), before2 ], [ item4, now ], false ]);
					}
					$cs = undefined;
				} else {
					$cs = undefined;
				}
				$cq = $cs;
			} else {
				$cq = undefined;
			}
			$cq;
		}
		$ct = undefined;
	}
	$ct;
	return events;
}
function kept_ops(kept, index, events, out, payload_follows) {
	let ops = [  ];
	for (const event of events) {
		const $cv = event;
		let $cw = null;
		if ($cv[0] === 0) {
			const at3 = $cv[1];
			const left = $cv[2];
			const arrived = $cv[3];
			const position = kept_before(index, at3);
			let removed = [  ];
			for (const pair of left) {
				const $cx = out(pair);
				let $cy = null;
				if ($cx[0] === 0) {
					const value = $cx[1];
					$cy = removed.push(value);
				} else {
					$cy = undefined;
				}
				$cy;
			}
			let inserted = [  ];
			let flags = [  ];
			for (const pair2 of arrived) {
				const $cz = out(pair2);
				let $cA = null;
				if ($cz[0] === 0) {
					const value2 = $cz[1];
					inserted.push(value2);
					flags.push(true);
					$cA = undefined;
				} else {
					$cA = flags.push(false);
				}
				$cA;
			}
			let taken = 0;
			while (taken < left.length) {
				__remove_at(kept.v, at3, "std/src/reactive/delta.vl:2153:31");
				taken = taken + 1;
			}
			let offset = 0;
			for (const flag of flags) {
				__insert_at(kept.v, at3 + offset, flag, "std/src/reactive/delta.vl:2158:19");
				offset = offset + 1;
			}
			rebuild_kept(index, kept.v);
			if (removed.length > 0 || inserted.length > 0) {
				ops.push([ 0, position, removed, inserted ]);
			}
			$cw = undefined;
		} else if ($cv[0] === 1) {
			const at4 = $cv[1];
			const before = $cv[2];
			const after = $cv[3];
			const rerun = $cv[4];
			const position2 = kept_before(index, at4);
			const $cB = [ out(before), out(after) ];
			let $cC = null;
			if ($cB[0][0] === 0 && $cB[1][0] === 0) {
				const was = $cB[0][1];
				const now = $cB[1][1];
				if (rerun || payload_follows) {
					ops.push([ 1, position2, was, now ]);
				}
				$cC = undefined;
			} else if ($cB[0][0] === 0 && $cB[1][0] === 1) {
				const was2 = $cB[0][1];
				__at_put(kept.v, at4, false, "std/src/reactive/delta.vl:2179:7");
				kept_add(index, at4, -(1));
				ops.push([ 0, position2, [ was2 ], [  ] ]);
				$cC = undefined;
			} else if ($cB[0][0] === 1 && $cB[1][0] === 0) {
				const now2 = $cB[1][1];
				__at_put(kept.v, at4, true, "std/src/reactive/delta.vl:2184:7");
				kept_add(index, at4, 1);
				ops.push([ 0, position2, [  ], [ now2 ] ]);
				$cC = undefined;
			} else {
				$cC = undefined;
			}
			$cw = $cC;
		} else if ($cv[0] === 2) {
			const _left = $cv[1];
			const now3 = $cv[2];
			let flags2 = [  ];
			let values = [  ];
			for (const pair3 of now3) {
				const $cD = out(pair3);
				let $cE = null;
				if ($cD[0] === 0) {
					const value3 = $cD[1];
					values.push(value3);
					flags2.push(true);
					$cE = undefined;
				} else {
					$cE = flags2.push(false);
				}
				$cE;
			}
			kept.v = flags2;
			rebuild_kept(index, kept.v);
			ops.push([ 2, values ]);
			$cw = undefined;
		} else {
			const from = $cv[1];
			const count2 = $cv[2];
			const to = $cv[3];
			const out_from = kept_before(index, from);
			const moved = kept_before(index, from + count2) - out_from;
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
			rebuild_kept(index, kept.v);
			const out_to = kept_before(index, to);
			if (moved > 0 && out_from !== out_to) {
				ops.push([ 3, out_from, moved, out_to ]);
			}
			$cw = undefined;
		}
		$cw;
	}
	return ops;
}
function release_core2(core) {
	for (const id of __clone(core[0].v)) {
		retire2(core, id);
	}
	dispose2(core[6]);
}
function core_release2(core, upstream) {
	const release6 = upstream[3];
	return () => {
		release_core2(core);
		release6();
		return;
	};
}
function open4(self) {
	const keep = self[1];
	const upstream = open2(__clone(self[0]));
	const stage = new4();
	const core = element_core(stage, (item2) => {
		const $bQ = run_element(stage, ($bO, $bP) => {
			return keep(item2, $bO, $bP);
		});
		const answer = $bQ[0];
		const owner = $bQ[1];
		return [ start_element(answer), owner ];
	});
	seed2(core, upstream[0]());
	const kept = __shared_new(fresh_list(core[2].v));
	const index = kept_index(kept.v);
	const drain2 = upstream[2];
	return [ () => {
		let out = [  ];
		let at3 = 0;
		for (const item2 of core[1].v) {
			if (__at(core[2].v, at3, "std/src/reactive/delta.vl:2040:9")) {
				out.push(__clone(item2));
			}
			at3 = at3 + 1;
		}
		return out;
	}, core_attach(core, upstream), () => {
		return kept_ops(kept, index, resolve2(core, drain2()), (pair) => {
			let $cu = null;
			if (pair[1]) {
				$cu = [ 0, __clone(pair[0]) ];
			} else {
				$cu = [ 1 ];
			}
			return $cu;
		}, false);
	}, core_release2(core, upstream) ];
}
function memo2(self, $O, $P, caller) {
	return [ seal_collection(open4(self), true, $O, $P, caller) ];
}
function peek_storage3(storage, read) {
	const $cG = storage;
	let $cH = null;
	if ($cG[0] === 0) {
		const cell = $cG[1];
		$cH = viewing2(cell, read);
	} else {
		const port = $cG[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $cI = out;
		let $cJ = null;
		if ($cI[0] === 0) {
			const value = __clone($cI[1]);
			$cJ = value;
		} else {
			$cJ = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:632:13");
			})();
		}
		$cH = $cJ;
	}
	return $cH;
}
function holds(storage, view, at3) {
	let found = false;
	peek_storage3(storage, (held) => {
		return view(held, at3, (_item) => {
			return found = true;
		});
	});
	return found;
}
function follow_id(storage, handler, cells, view, at3) {
	if (viewing2(handler, (held) => {
		return follows_items(held);
	}) && !(holds(storage, view, at3))) {
		return -(1);
	}
	const id = follow(handler.v, at3);
	while (cells.v.length <= as_usize(id)) {
		cells.v.push(fresh_cells());
	}
	const slot_cells = viewing2(cells, (held) => {
		return __clone(__at(held, as_usize(id), "std/src/reactive/lens.vl:909:41"));
	});
	const $cP = [ slot_cells[2], "v" ];
	slot_cells[2].v = $cP[0][$cP[1]] + 1;
	return id;
}
function release_id(handler, cells, id) {
	if (id < 0) {
		return;
	}
	const slot_cells = viewing2(cells, (held) => {
		return __clone(__at(held, as_usize(id), "std/src/reactive/lens.vl:919:41"));
	});
	const left = slot_cells[2].v - 1;
	slot_cells[2].v = left;
	if (left <= 0) {
		release(handler.v, id);
	}
}
function release3(self) {
	untap(self[0], self[3]);
	for (const id of self[1].v) {
		release_id(self[0][1], self[0][3], id);
	}
	self[1].v = [  ];
}
function rows(self, $cF) {
	const log = new7();
	const held = __shared_new([  ]);
	const storage = __clone(self[0]);
	const handler = self[1];
	const cells = self[3];
	const view = self[2][1];
	const fill = () => {
		let ids = [  ];
		let at3 = 0;
		const count2 = peek_storage3(storage, (list) => {
			return count(list);
		});
		while (at3 < count2) {
			ids.push(follow_id(storage, handler, cells, view, at3));
			at3 = at3 + 1;
		}
		return ids;
	};
	const leave = (at3, count2) => {
		let gone = [  ];
		let taken = 0;
		while (taken < count2) {
			const id = __remove_at(held.v, at3, "std/src/reactive/lens.vl:1608:27");
			release_id(handler, cells, id);
			gone.push(id);
			taken = taken + 1;
		}
		return gone;
	};
	const arrive = (at3, count2) => {
		let arrived = [  ];
		let index = at3;
		while (index < at3 + count2) {
			const id = follow_id(storage, handler, cells, view, index);
			__insert_at(held.v, index, id, "std/src/reactive/lens.vl:1620:18");
			arrived.push(id);
			index = index + 1;
		}
		return arrived;
	};
	held.v = fill();
	const tap2 = tap(self, (_change) => {
		return;
	}, (change2, _stage) => {
		const $cQ = change2;
		let $cR = null;
		if ($cQ[0] === 0) {
			const at3 = $cQ[1];
			const removed = $cQ[2];
			const inserted = $cQ[3];
			const gone = leave(at3, removed);
			record(log, [ 0, at3, gone, arrive(at3, inserted) ]);
			$cR = undefined;
		} else if ($cQ[0] === 1) {
			const at4 = $cQ[1];
			const count2 = $cQ[2];
			$cR = record(log, [ 0, at4, [  ], arrive(at4, count2) ]);
		} else if ($cQ[0] === 3) {
			const at5 = $cQ[1];
			const count3 = $cQ[2];
			$cR = record(log, [ 0, at5, leave(at5, count3), [  ] ]);
		} else if ($cQ[0] === 5) {
			const from = $cQ[1];
			const to = $cQ[2];
			const id = __remove_at(held.v, from, "std/src/reactive/lens.vl:1636:28");
			__insert_at(held.v, to, id, "std/src/reactive/lens.vl:1637:19");
			record(log, [ 3, from, 1, to ]);
			$cR = undefined;
		} else if ($cQ[0] === 6) {
			for (const id2 of __clone(held.v)) {
				release_id(handler, cells, id2);
			}
			held.v = fill();
			record(log, [ 2, __clone(held.v) ]);
			$cR = undefined;
		} else {
			$cR = undefined;
		}
		return $cR;
	});
	const feed = [ __clone(self), held, __clone(log), tap2 ];
	defer_to_owner(() => {
		return release3(feed);
	}, $cF);
	return feed;
}
function cursor3(self) {
	return cursor(self[2]);
}
function slot_of(self, id) {
	if (id < 0) {
		return [ __clone(self), -(1), fresh_cells() ];
	}
	return [ __clone(self), id, viewing2(self[3], (held) => {
		return __clone(__at(held, as_usize(id), "std/src/reactive/lens.vl:827:66"));
	}) ];
}
function handles(self, ids) {
	let out = [  ];
	for (const id of ids) {
		out.push(slot_of(self[0], id));
	}
	return out;
}
function get2(self) {
	return handles(self, self[1].v);
}
function get3(self, key) {
	return __nmap_get(self[0], hash(key));
}
function insert(self, key, value) {
	__nmap_insert(self[0], hash(key), __clone(key), __clone(value));
}
function remove(self, key) {
	__nmap_remove(self[0], hash(key));
}
function entries(self) {
	let keys = __nmap_keys(self[0]);
	let values = __nmap_values(self[0]);
	let backwards = [  ];
	while (!(is_empty(keys))) {
		const $cV = __list_pop(keys);
		let $cW = null;
		if ($cV[0] === 0) {
			const key = $cV[1];
			const $cX = __list_pop(values);
			let $cY = null;
			if ($cX[0] === 0) {
				const value = $cX[1];
				$cY = backwards.push([ key, value ]);
			} else {
				$cY = undefined;
			}
			$cW = $cY;
		} else {
			$cW = undefined;
		}
		$cW;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $cZ = __list_pop(backwards);
		let $da = null;
		if ($cZ[0] === 0) {
			const pair = $cZ[1];
			$da = result.push(pair);
		} else {
			$da = undefined;
		}
		$da;
	}
	return result;
}
function new8() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function release4(self) {
	untap(self[0], self[2]);
	for (const _ of entries(self[1].v)) {
		const $dl = _;
		const _key = $dl[0];
		const id = $dl[1];
		release_id(self[0][1], self[0][3], id);
	}
	self[1].v = new8();
}
function index_by(self, key, $cS) {
	const found = __shared_new(new2());
	const leaving = __shared_new([  ]);
	const storage = __clone(self[0]);
	const handler = self[1];
	const cells = self[3];
	const view = self[2][1];
	const key_at = (at3) => {
		let out = [ 1 ];
		peek_storage3(storage, (held) => {
			return view(held, at3, (item2) => {
				return out = [ 0, key(item2) ];
			});
		});
		return out;
	};
	const map4 = (held, at3) => {
		const id = follow_id(storage, handler, cells, view, at3);
		const before = get3(found.v, held);
		insert(found.v, held, id);
		const $cT = before;
		if ($cT[0] === 0) {
			release_id(handler, cells, $cT[1]);
		}
		return;
	};
	const unmap = (held) => {
		const $cU = get3(found.v, held);
		if ($cU[0] === 0) {
			remove(found.v, held);
			release_id(handler, cells, $cU[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $db = _;
			const _held = $db[0];
			const old = $db[1];
			release_id(handler, cells, old);
		}
		found.v = new8();
		let at3 = 0;
		const count2 = peek_storage3(storage, (held) => {
			return count(held);
		});
		while (at3 < count2) {
			const $dc = key_at(at3);
			if ($dc[0] === 0) {
				map4($dc[1], at3);
			}
			at3 = at3 + 1;
		}
		return;
	};
	fill();
	const note = (at3, count2) => {
		let gone = [  ];
		let index2 = at3;
		while (index2 < at3 + count2) {
			const $dd = key_at(index2);
			if ($dd[0] === 0) {
				gone.push($dd[1]);
			}
			index2 = index2 + 1;
		}
		leaving.v = gone;
		return;
	};
	const arrive = (at3, count2) => {
		let index2 = at3;
		while (index2 < at3 + count2) {
			const $de = key_at(index2);
			if ($de[0] === 0) {
				map4($de[1], index2);
			}
			index2 = index2 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change2) => {
		const $df = change2;
		let $dg = null;
		if ($df[0] === 0) {
			const at3 = $df[1];
			const removed = $df[2];
			const _inserted = $df[3];
			$dg = note(at3, removed);
		} else if ($df[0] === 3) {
			const at4 = $df[1];
			const count2 = $df[2];
			$dg = note(at4, count2);
		} else if ($df[0] === 2) {
			const at5 = $df[1];
			$dg = note(at5, 1);
		} else if ($df[0] === 4) {
			const at6 = $df[1];
			const _part = $df[2];
			$dg = note(at6, 1);
		} else {
			$dg = undefined;
		}
		return $dg;
	}, (change2, stage) => {
		const $dh = stage;
		let $di = null;
		if ($dh[0] === 0) {
			$di = undefined;
		} else if ($dh[0] === 3) {
			return;
		} else {
			fill();
			return;
			$di = undefined;
		}
		$di;
		const $dj = change2;
		let $dk = null;
		if ($dj[0] === 0) {
			const at3 = $dj[1];
			const _removed = $dj[2];
			const inserted = $dj[3];
			for (const held of leaving.v) {
				unmap(held);
			}
			arrive(at3, inserted);
			$dk = undefined;
		} else if ($dj[0] === 1) {
			const at4 = $dj[1];
			const count2 = $dj[2];
			$dk = arrive(at4, count2);
		} else if ($dj[0] === 3) {
			const _at = $dj[1];
			const _count = $dj[2];
			for (const held2 of leaving.v) {
				unmap(held2);
			}
			$dk = undefined;
		} else if ($dj[0] === 2) {
			const at5 = $dj[1];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			arrive(at5, 1);
			$dk = undefined;
		} else if ($dj[0] === 4) {
			const at6 = $dj[1];
			const _part = $dj[2];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			arrive(at6, 1);
			$dk = undefined;
		} else if ($dj[0] === 5) {
			const _from = $dj[1];
			const _to = $dj[2];
			$dk = undefined;
		} else {
			$dk = fill();
		}
		return $dk;
	});
	const index = [ __clone(self), found, tap2 ];
	defer_to_owner(() => {
		return release4(index);
	}, $cS);
	return index;
}
function follow2(self, at3) {
	return slot_of(self, follow_id(self[0], self[1], self[3], self[2][1], at3));
}
function release5(self) {
	release_id(self[0][1], self[0][3], self[1]);
}
function slot(self, at3, $dm) {
	const handle = follow2(self, at3);
	defer_to_owner(() => {
		return release5(handle);
	}, $dm);
	return handle;
}
function peek(self, read) {
	return peek_storage3(self[0], read);
}
function size2(self) {
	return peek(self, (held) => {
		return count(held);
	});
}
function coming(self, changes) {
	if (self[7].v.length === 0) {
		return;
	}
	for (const tap2 of __clone(self[7].v)) {
		for (const change2 of changes) {
			tap2[1](change2);
		}
	}
}
function splice2(self, at3, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self, at3, "std/src/reactive/lens.vl:1992:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at3 + offset, __clone(value), "std/src/reactive/lens.vl:1997:9");
		offset = offset + 1;
	}
}
function mutate(self, change2) {
	const $dr = self[0];
	let $ds = null;
	if ($dr[0] === 0) {
		const cell = $dr[1];
		$ds = change2(cell.v);
	} else {
		const port = $dr[1];
		$ds = port[1](change2);
	}
	return $ds;
}
function batch(body, $dW) {
	const $dX = $dW;
	let $dY = null;
	if ($dX[0] === 0) {
		const current = $dX[1];
		$dY = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$dY = result;
	}
	return $dY;
}
function landed(self, changes, late, $dt) {
	if (changes.length === 0) {
		return;
	}
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	const tapped = self[7].v.length > 0;
	let index = 0;
	for (const change2 of changes) {
		const $du = change2;
		let $dv = null;
		if ($du[0] === 0) {
			const _at = $du[1];
			const removed = $du[2];
			const inserted = $du[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$dv = undefined;
		} else if ($du[0] === 1) {
			const _at2 = $du[1];
			const _count = $du[2];
			$dv = resized = true;
		} else if ($du[0] === 3) {
			const _at3 = $du[1];
			const _count2 = $du[2];
			$dv = resized = true;
		} else if ($du[0] === 6) {
			$dv = resized = true;
		} else if ($du[0] === 5) {
			const _from = $du[1];
			const _to = $du[2];
			$dv = shaped = true;
		} else {
			$dv = undefined;
		}
		$dv;
		apply(self[1].v, change2, wakes);
		let $dL = null;
		if (tapped) {
			let $dK = null;
			if (!(late)) {
				$dK = [ 0 ];
			} else if (!(several)) {
				$dK = [ 1 ];
			} else if (index === 0) {
				$dK = [ 2 ];
			} else {
				$dK = [ 3 ];
			}
			const stage = $dK;
			for (const tap2 of __clone(self[7].v)) {
				tap2[2](change2, stage);
			}
			$dL = undefined;
		}
		$dL;
		index = index + 1;
	}
	const cells = self[3];
	const published = __clone(self[4]);
	const shape = __clone(self[5]);
	const sized = __clone(self[6]);
	settle_together(($dM) => {
		for (const wake2 of wakes) {
			const $dN = wake2;
			let $dO = null;
			if ($dN[0] === 0) {
				const slot2 = $dN[1];
				$dO = wake_all(cells, slot2, [ 0, $dM ]);
			} else if ($dN[0] === 2) {
				const slot3 = $dN[1];
				$dO = wake_all(cells, slot3, [ 0, $dM ]);
			} else {
				const slot4 = $dN[1];
				const part = $dN[2];
				$dO = wake_part(cells, slot4, part, [ 0, $dM ]);
			}
			$dO;
		}
		if (resized) {
			notify(sized, [ 0, $dM ]);
		}
		if (shaped || resized) {
			notify(shape, [ 0, $dM ]);
		}
		notify(published, [ 0, $dM ]);
		return;
	}, $dt);
}
function splice3(self, at3, removed, inserted, $do) {
	const held = size2(self);
	let $dp = null;
	if (at3 > held) {
		$dp = held;
	} else {
		$dp = at3;
	}
	const start = $dp;
	let $dq = null;
	if (start + removed > held) {
		$dq = held - start;
	} else {
		$dq = removed;
	}
	const taking = $dq;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start, taking, arriving ];
	coming(self, [ __clone(change2) ]);
	mutate(self, (held2) => {
		return splice2(held2, start, taking, inserted);
	});
	landed(self, [ change2 ], false, $do);
}
function insert_at(self, at3, value, $dn) {
	return splice3(self, at3, 0, [ __clone(value) ], $dn);
}
function remove_at(self, at3, $dZ) {
	return splice3(self, at3, 1, [  ], $dZ);
}
function peek2(self, read) {
	return peek_storage2(self[0], read);
}
function move_item(self, from, to, $eb) {
	const held = size2(self);
	if (from >= held || from === to) {
		return;
	}
	let $ec = null;
	if (to >= held) {
		$ec = held - 1;
	} else {
		$ec = to;
	}
	const landing = $ec;
	const $ed = peek2(self, (list) => {
		return item(list, from);
	});
	if ($ed[0] === 0) {

	} else {
		return;
	}
	coming(self, [ [ 5, from, landing ] ]);
	mutate(self, (held2) => {
		splice2(held2, from, 1, [  ]);
		splice2(held2, landing, 0, [ __clone($ed[1]) ]);
		return;
	});
	landed(self, [ [ 5, from, landing ] ], false, $eb);
}
function has(self, at3) {
	return holds(self[0], self[2][1], at3);
}
function replace(self, at3, value, $ef) {
	let $eg = null;
	if (has(self, at3)) {
		$eg = [ 2, __clone(at3) ];
	} else if (viewing2(self[1], (handler) => {
		return follows_items(handler);
	})) {
		return false;
	} else {
		$eg = [ 1, __clone(at3), 1 ];
	}
	const change2 = $eg;
	coming(self, [ __clone(change2) ]);
	const write = self[2][2];
	mutate(self, (held) => {
		return write(held, at3, value);
	});
	landed(self, [ change2 ], false, $ef);
	return true;
}
function at2(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][1], (handler) => {
		return locate(handler, self[1]);
	});
}
function get4(self) {
	const $eh = at2(self);
	if ($eh[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][2][0];
	return peek2(self[0], (held) => {
		return read(held, $eh[1]);
	});
}
function debug(self) {
	const $en = self;
	let $eo = null;
	if ($en[0] === 0) {
		const value = $en[1];
		$eo = "Some(" + JSON.stringify(value) + ")";
	} else {
		$eo = "None";
	}
	return $eo;
}
function eq4(self, b) {
	const $eq = self;
	let $et = null;
	if ($eq[0] === 0) {
		const $er = b;
		let $es = null;
		if ($er[0] === 0) {
			$es = eq2($eq[1], $er[1]);
		} else {
			$es = false;
		}
		$et = $es;
	} else {
		const $eu = b;
		$et = $eu[0] === 1;
	}
	return $et;
}
function get5(self) {
	return __clone(self[0].v);
}
function get6(self) {
	return get5(self[0]);
}
function filter2(self, predicate) {
	let result = [  ];
	for (const item2 of self) {
		if (predicate(item2)) {
			result.push(__clone(item2));
		}
	}
	return result;
}
function lift(self, op) {
	const $ez = op;
	let $eA = null;
	if ($ez[0] === 0) {
		const at3 = $ez[1];
		const gone = $ez[2];
		const arrived = $ez[3];
		$eA = [ 0, at3, handles(self, gone), handles(self, arrived) ];
	} else if ($ez[0] === 1) {
		const at4 = $ez[1];
		const old = $ez[2];
		const new9 = $ez[3];
		$eA = [ 1, at4, slot_of(self[0], old), slot_of(self[0], new9) ];
	} else if ($ez[0] === 2) {
		const all2 = $ez[1];
		$eA = [ 2, handles(self, all2) ];
	} else {
		const from = $ez[1];
		const count2 = $ez[2];
		const to = $ez[3];
		$eA = [ 3, from, count2, to ];
	}
	return $eA;
}
function since3(self, cursor4) {
	const $ex = since(self[2], cursor4);
	let $ey = null;
	if ($ex[0] === 0) {
		const recorded = $ex[1];
		let out = [  ];
		for (const op of recorded) {
			out.push(lift(self, op));
		}
		$ey = out;
	} else {
		$ey = [ [ 2, get2(self) ] ];
	}
	return $ey;
}
function map3(self, fn) {
	const $eD = self;
	let $eE = null;
	if ($eD[0] === 0) {
		const x = $eD[1];
		$eE = [ 0, fn(x) ];
	} else {
		$eE = [ 1 ];
	}
	return $eE;
}
function find(self, key) {
	const lens = __clone(self[0]);
	return map3(viewing2(self[1], (held) => {
		return get3(held, key);
	}), (id) => {
		return slot_of(lens, id);
	});
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
const delta_log_limit = 1024;
const element_holds_allocated_count = __shared_new(0);
const block_size = 128;
run(1, 3000, [ 1 ], [ 1 ]);
run(7, 3000, [ 1 ], [ 1 ]);
run(42, 3000, [ 1 ], [ 1 ]);
console.log("fuzz: all passed");
//# sourceMappingURL=lens-fuzz.mjs.map
