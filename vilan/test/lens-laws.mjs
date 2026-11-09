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
	for (const item3 of self) {
		if (!(item3 === __at(b2, index2, "std/src/compare.vl:147:17"))) {
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
	let $aK = null;
	if (!(condition)) {
		$aK = (() => {
			throw __panic(message, caller);
		})();
	}
	return $aK;
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
	const $aF = self;
	let $aI = null;
	if ($aF[0] === 0) {
		const $aG = b2;
		let $aH = null;
		if ($aG[0] === 0) {
			$aH = $aF[1] === $aG[1];
		} else {
			$aH = false;
		}
		$aI = $aH;
	} else {
		const $aJ = b2;
		$aI = $aJ[0] === 1;
	}
	return $aI;
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
		const key2 = hash2(subscriber[0]);
		let $aj = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key2))) {
				turn[3].v.set(key2, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$aj = undefined;
		} else if (!(turn[2].v.has(key2))) {
			turn[2].v.set(key2, true);
			if (tracing.v) {
				trace_hooks.v[3](turn, subscriber[0]);
			}
			let index2 = turn[0].v.length;
			while (index2 > 0 && __at(turn[0].v, index2 - 1, "std/src/reactive.vl:421:21")[0] > subscriber[0]) {
				index2 = index2 - 1;
			}
			__insert_at(turn[0].v, index2, __clone(subscriber), "std/src/reactive.vl:424:25");
		}
		$aj;
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
	const $dU = turn;
	let $dV = null;
	if ($dU[0] === 0) {
		const ambient = $dU[1];
		$dV = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $dW = last(draining_turns.v);
		let $dX = null;
		if ($dW[0] === 0) {
			const draining = $dW[1];
			$dX = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$dX = undefined;
		}
		$dV = $dX;
	}
	return $dV;
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
function dispose(self, $fl) {
	const $fm = $fl;
	let $fn = null;
	if ($fm[0] === 0) {
		const established = $fm[1];
		$fn = [ 0, established ];
	} else {
		$fn = last(draining_turns.v);
	}
	const ambient = $fn;
	release_under(self, ambient);
}
function detach(handle) {
	const $ei = last(releasing_turns.v);
	let $ej = null;
	if ($ei[0] === 0) {
		const at_release = $ei[1];
		$ej = at_release;
	} else {
		$ej = last(draining_turns.v);
	}
	const turn = $ej;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ek = [ 0, handle[0] ];
	let $el = null;
	if ($ek[0] === 0) {
		const subscribers = $ek[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$el = undefined;
	} else {
		$el = undefined;
	}
	$el;
	const $em = ambient;
	let $en = null;
	if ($em[0] === 0) {
		const turn = $em[1];
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
		$en = undefined;
	} else {
		$en = undefined;
	}
	$en;
	const $eo = handle[3].v;
	let $ep = null;
	if ($eo[0] === 0) {
		const release11 = $eo[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release11, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ep = undefined;
	} else {
		$ep = undefined;
	}
	return $ep;
}
function new4() {
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
function stage_nursery(self) {
	const $dI = nursery(self);
	let $dJ = null;
	if ($dI[0] === 0) {
		const held2 = $dI[1];
		$dJ = held2;
	} else {
		const made = detached_nursery();
		self[0].v[3] = [ 0, __clone(made) ];
		$dJ = made;
	}
	return $dJ;
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
	let $dH = null;
	if (is_disposed(self)) {
		$dH = [ 1 ];
	} else {
		$dH = self[0].v[3];
	}
	return $dH;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $eB = null;
	if (!(is_disposed(self))) {
		const held2 = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $es = held2[3];
		let $et = null;
		if ($es[0] === 0) {
			const nursery2 = $es[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$et = undefined;
		} else {
			$et = undefined;
		}
		$et;
		let $eA = null;
		if (held2[2]) {
			let failure = [ 1 ];
			for (const cleanup of held2[1].v) {
				const $ev = __guarded(cleanup);
				let $ew = null;
				if ($ev[0] === 0) {
					const message = $ev[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$ew = undefined;
				} else {
					$ew = undefined;
				}
				$ew;
			}
			const $ey = failure;
			let $ez = null;
			if ($ey[0] === 0) {
				const message2 = $ey[1];
				$ez = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$ez = undefined;
			}
			$eA = $ez;
		}
		$eB = $eA;
	}
	return $eB;
}
function register_with_owner(subscription, $fg, $fh) {
	const $fi = $fh;
	let $fj = null;
	if ($fi[0] === 0) {
		const owner2 = $fi[1];
		$fj = take(owner2, subscription, $fg);
	} else {
		$fj = __clone(subscription);
	}
	return $fj;
}
function defer_to_owner(cleanup, $m) {
	const $n = $m;
	let $o = null;
	if ($n[0] === 0) {
		const owner2 = $n[1];
		$o = defer(owner2, cleanup);
	} else {
		$o = undefined;
	}
	return $o;
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
function also_releasing(handle, release11) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release11();
		const $fe = previous;
		let $ff = null;
		if ($fe[0] === 0) {
			const earlier = $fe[1];
			$ff = earlier();
		} else {
			$ff = undefined;
		}
		return $ff;
	} ];
}
function settle_together(body, $ax) {
	const $ay = $ax;
	let $az = null;
	if ($ay[0] === 0) {
		const current = $ay[1];
		$az = body(current);
	} else {
		const $aA = last(draining_turns.v);
		let $aB = null;
		if ($aA[0] === 0) {
			const draining = $aA[1];
			$aB = body(draining);
		} else {
			$aB = batch(body, $ax);
		}
		$az = $aB;
	}
	return $az;
}
function splice_start(held2, at4) {
	let $eb = null;
	if (at4 > held2) {
		$eb = held2;
	} else {
		$eb = at4;
	}
	return $eb;
}
function splice_count(held2, start, removed) {
	let $ec = null;
	if (start + removed > held2) {
		$ec = held2 - start;
	} else {
		$ec = removed;
	}
	return $ec;
}
function kept_index(flags) {
	const index2 = [ __shared_new([  ]) ];
	rebuild_kept(index2, flags);
	return index2;
}
function rebuild_kept(index2, flags) {
	let tree = [ 0 ];
	for (const flag of flags) {
		const $fC = tree;
		let $fB = null;
		if (flag) {
			$fB = 1;
		} else {
			$fB = 0;
		}
		$fC.push($fB);
	}
	const size6 = as_i32(flags.length);
	let at4 = 1;
	while (at4 <= size6) {
		const parent = at4 + (at4 & 0 - at4);
		if (parent <= size6) {
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
	const size6 = as_i32(index2[0].v.length) - 1;
	let at4 = as_i32(position) + 1;
	while (at4 <= size6) {
		__at_put(index2[0].v, as_usize(at4), __at(index2[0].v, as_usize(at4), "std/src/reactive/delta.vl:1833:39") + delta, "std/src/reactive/delta.vl:1833:3");
		at4 = at4 + (at4 & 0 - at4);
	}
}
function new5() {
	return [ [  ] ];
}
function follow(self, at4) {
	if (self[0].length === 0) {
		self[0].push(new6());
	}
	return follow2(__at(self[0], 0, "std/src/reactive/lens.vl:145:3"), at4);
}
function apply(self, change4, wakes) {
	if (self[0].length > 0) {
		apply2(__at(self[0], 0, "std/src/reactive/lens.vl:150:4"), change4, wakes);
	}
}
function locate(self, slot7) {
	if (self[0].length === 0) {
		return [ 1 ];
	}
	return locate2(__at(self[0], 0, "std/src/reactive/lens.vl:158:3"), slot7);
}
function follows_items(self) {
	return true;
}
function release(self, slot7) {
	if (self[0].length > 0) {
		release2(__at(self[0], 0, "std/src/reactive/lens.vl:167:4"), slot7);
	}
}
function new6() {
	return [ [  ], [  ], [  ], [  ], [  ], [  ], 0 ];
}
function seat(self, at4) {
	let low = 0;
	let high = self[3].length;
	while (high - low > 1) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[3], middle, "std/src/reactive/lens.vl:213:7") <= at4) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at4 - __at(self[3], low, "std/src/reactive/lens.vl:219:14") ];
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
		const slot7 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:241:15"), index2, "std/src/reactive/lens.vl:241:15");
		if (slot7 !== -(1)) {
			__at_put(self[4], as_usize(slot7), block, "std/src/reactive/lens.vl:243:5");
			__at_put(self[5], as_usize(slot7), index2, "std/src/reactive/lens.vl:244:5");
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
function widen(self, at4, count3) {
	if (count3 === 0 || at4 >= self[6]) {
		return;
	}
	const $P = seat(self, at4);
	const place = $P[0];
	const index2 = $P[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:265:15");
	let $Q = null;
	if (count3 <= 8) {
		let made = 0;
		while (made < count3) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:269:5"), index2, -(1), "std/src/reactive/lens.vl:269:35");
			made = made + 1;
		}
		$Q = undefined;
	} else {
		const before = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:273:17"));
		let grown2 = [  ];
		let copied2 = 0;
		while (copied2 < index2) {
			grown2.push(__at(before, copied2, "std/src/reactive/lens.vl:277:16"));
			copied2 = copied2 + 1;
		}
		let made2 = 0;
		while (made2 < count3) {
			grown2.push(-(1));
			made2 = made2 + 1;
		}
		const held2 = before.length;
		while (copied2 < held2) {
			grown2.push(__at(before, copied2, "std/src/reactive/lens.vl:287:16"));
			copied2 = copied2 + 1;
		}
		__at_put(self[0], as_usize(block), grown2, "std/src/reactive/lens.vl:290:4");
	}
	$Q;
	self[6] = self[6] + count3;
	renumber(self, block, index2 + count3);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:294:6").length > 2 * block_size) {
		rebalance(self, place);
	}
	restart(self);
}
function close(self, at4, count3, wakes) {
	let left = count3;
	while (left > 0 && at4 < self[6]) {
		const $N = seat(self, at4);
		const place = $N[0];
		const index2 = $N[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:306:16");
		const held2 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:307:15").length;
		let $O = null;
		if (left < held2 - index2) {
			$O = left;
		} else {
			$O = held2 - index2;
		}
		const take2 = $O;
		let taken = 0;
		while (taken < take2) {
			const slot7 = __remove_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:311:16"), index2, "std/src/reactive/lens.vl:311:46");
			if (slot7 !== -(1)) {
				__at_put(self[4], as_usize(slot7), -(1), "std/src/reactive/lens.vl:313:6");
				wakes.push([ 2, slot7 ]);
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
	const all5 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:332:13"));
	let first2 = [  ];
	let index2 = 0;
	while (index2 < block_size) {
		first2.push(__at(all5, index2, "std/src/reactive/lens.vl:336:15"));
		index2 = index2 + 1;
	}
	__at_put(self[0], as_usize(block), first2, "std/src/reactive/lens.vl:339:3");
	let next = place + 1;
	const held2 = all5.length;
	while (index2 < held2) {
		const fresh = open(self, next);
		let filled = 0;
		while (filled < block_size && index2 < held2) {
			__at(self[0], as_usize(fresh), "std/src/reactive/lens.vl:346:5").push(__at(all5, index2, "std/src/reactive/lens.vl:346:40"));
			index2 = index2 + 1;
			filled = filled + 1;
		}
		renumber(self, fresh, 0);
		next = next + 1;
	}
}
function follow2(self, at4) {
	while (self[6] <= at4) {
		const places2 = self[1].length;
		if (places2 === 0 || __at(self[0], as_usize(__at(self[1], places2 - 1, "std/src/reactive/lens.vl:361:34")), "std/src/reactive/lens.vl:361:22").length >= block_size) {
			const fresh = open(self, places2);
			__at_put(self[2], as_usize(fresh), places2, "std/src/reactive/lens.vl:363:5");
			self[3].push(self[6]);
		}
		const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:366:15");
		__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:367:4").push(-(1));
		self[6] = self[6] + 1;
	}
	const $j = seat(self, at4);
	const place = $j[0];
	const index2 = $j[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:371:15");
	const held2 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:372:14"), index2, "std/src/reactive/lens.vl:372:14");
	if (held2 !== -(1)) {
		return held2;
	}
	const slot7 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index2);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:379:3"), index2, slot7, "std/src/reactive/lens.vl:379:3");
	return slot7;
}
function apply2(self, change4, wakes) {
	const $L = change4;
	let $M = null;
	if ($L[0] === 0) {
		const at4 = $L[1];
		const removed = $L[2];
		const inserted = $L[3];
		close(self, at4, removed, wakes);
		widen(self, at4, inserted);
		$M = undefined;
	} else if ($L[0] === 1) {
		const at5 = $L[1];
		const count3 = $L[2];
		$M = widen(self, at5, count3);
	} else if ($L[0] === 3) {
		const at6 = $L[1];
		const count4 = $L[2];
		$M = close(self, at6, count4, wakes);
	} else if ($L[0] === 2) {
		const at7 = $L[1];
		let $S = null;
		if (at7 < self[6]) {
			const $R = seat(self, at7);
			const place = $R[0];
			const index2 = $R[1];
			const slot7 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:396:29")), "std/src/reactive/lens.vl:396:17"), index2, "std/src/reactive/lens.vl:396:17");
			if (slot7 !== -(1)) {
				wakes.push([ 0, slot7 ]);
			}
			$S = undefined;
		}
		$M = $S;
	} else if ($L[0] === 4) {
		const at8 = $L[1];
		const part3 = $L[2];
		let $U = null;
		if (at8 < self[6]) {
			const $T = seat(self, at8);
			const place2 = $T[0];
			const index3 = $T[1];
			const slot8 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:405:29")), "std/src/reactive/lens.vl:405:17"), index3, "std/src/reactive/lens.vl:405:17");
			if (slot8 !== -(1)) {
				wakes.push([ 1, slot8, part3 ]);
			}
			$U = undefined;
		}
		$M = $U;
	} else if ($L[0] === 5) {
		const from = $L[1];
		const to = $L[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $V = null;
		if (from > to) {
			$V = from;
		} else {
			$V = to;
		}
		const far = $V;
		while (self[6] <= far) {
			const places2 = self[1].length;
			if (places2 === 0 || __at(self[0], as_usize(__at(self[1], places2 - 1, "std/src/reactive/lens.vl:419:36")), "std/src/reactive/lens.vl:419:24").length >= block_size) {
				const fresh = open(self, places2);
				__at_put(self[2], as_usize(fresh), places2, "std/src/reactive/lens.vl:421:7");
				self[3].push(self[6]);
			}
			const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:424:17");
			__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:425:6").push(-(1));
			self[6] = self[6] + 1;
		}
		const $W = seat(self, from);
		const left = $W[0];
		const index4 = $W[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:429:18");
		const slot9 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:430:16"), index4, "std/src/reactive/lens.vl:430:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:432:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:433:17");
		} else {
			renumber(self, source, index4);
		}
		restart(self);
		let $X = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$X = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:441:25")), "std/src/reactive/lens.vl:441:13").length ];
		} else {
			$X = seat(self, to);
		}
		const $Y = $X;
		const place3 = $Y[0];
		const landing = $Y[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:445:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:446:5"), landing, slot9, "std/src/reactive/lens.vl:446:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing);
		restart(self);
		$M = undefined;
	} else {
		let slot10 = 0;
		const slots = as_i32(self[4].length);
		while (slot10 < slots) {
			if (__at(self[4], as_usize(slot10), "std/src/reactive/lens.vl:455:9") !== -(1)) {
				__at_put(self[4], as_usize(slot10), -(1), "std/src/reactive/lens.vl:456:7");
				wakes.push([ 2, slot10 ]);
			}
			slot10 = slot10 + 1;
		}
		self[0] = [  ];
		self[1] = [  ];
		self[2] = [  ];
		self[3] = [  ];
		self[6] = 0;
		$M = undefined;
	}
	return $M;
}
function release2(self, slot7) {
	const block = __at(self[4], as_usize(slot7), "std/src/reactive/lens.vl:471:15");
	if (block === -(1)) {
		return;
	}
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:475:3"), __at(self[5], as_usize(slot7), "std/src/reactive/lens.vl:475:33"), -(1), "std/src/reactive/lens.vl:475:3");
	__at_put(self[4], as_usize(slot7), -(1), "std/src/reactive/lens.vl:476:3");
}
function locate2(self, slot7) {
	const block = __at(self[4], as_usize(slot7), "std/src/reactive/lens.vl:480:15");
	if (block === -(1)) {
		return [ 1 ];
	}
	return [ 0, __at(self[3], __at(self[2], as_usize(block), "std/src/reactive/lens.vl:484:20"), "std/src/reactive/lens.vl:484:8") + __at(self[5], as_usize(slot7), "std/src/reactive/lens.vl:484:51") ];
}
function new7() {
	return [ new9(), [  ] ];
}
function extend_run(runs2, from, to) {
	const held2 = runs2.v.length;
	let $dj = null;
	if (held2 > 0) {
		const $di = __at(runs2.v, held2 - 1, "std/src/reactive/lens.vl:1992:31");
		const start = $di[0];
		const count3 = $di[1];
		const first2 = $di[2];
		const leftward = first2 < start && from === start + count3 && to === first2 + count3;
		const rightward = first2 > start && from === start && to === first2;
		if (leftward || rightward) {
			__at_put(runs2.v, held2 - 1, [ start, count3 + 1, first2 ], "std/src/reactive/lens.vl:1996:4");
			return;
		}
		$dj = undefined;
	}
	$dj;
	runs2.v.push([ from, 1, to ]);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function tasks() {
	return [ [ "a", "open" ], [ "b", "open" ], [ "c", "open" ] ];
}
function ages_of() {
	let ages2 = new9();
	insert(ages2, "bo", 4);
	return ages2;
}
function count(self) {
	return self[0].length;
}
function item(self, at4) {
	return __list_get(self[0], at4);
}
function splice(self, at4, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self[0], at4, "lens-laws.vl:65:14");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self[0], at4 + offset, value, "lens-laws.vl:70:14");
		offset = offset + 1;
	}
}
function eq4(self, other) {
	return self[0] === other[0] && self[1] === other[1];
}
function count2(self) {
	return self.length;
}
function item2(self, at4) {
	return __list_get(self, at4);
}
function lend(self, at4, read) {
	if (at4 < self.length) {
		read(__at_view(self, at4, "std/src/reactive/lens.vl:2222:10"));
	}
}
function put(self, at4, value) {
	__at_put(self, at4, __clone(value), "std/src/reactive/lens.vl:2242:3");
}
function change(self, at4, with2) {
	if (at4 < self.length) {
		with2(__at_view(self, at4, "std/src/reactive/lens.vl:2247:14"));
	}
}
function new8(value, caller) {
	let subscribers = [  ];
	const cell = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell[0]), caller);
	}
	return cell;
}
function lens_over(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new8(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over(value, handler, access) {
	return lens_over([ 0, __shared_new(value) ], handler, access);
}
function sequence(value, $a) {
	return over(value, new5(), [ (held2, at4) => {
		return at4 < count2(held2);
	}, (held2, at4) => {
		return item2(held2, at4);
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
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$d = $f;
	}
	return $d;
}
function follow_id(storage, watch, holds, at4) {
	if (viewing(watch, (held2) => {
		return follows_items(held2[0]);
	}) && !(peek_storage(storage, (held2) => {
		return holds(held2, at4);
	}))) {
		return -(1);
	}
	const id = follow(watch.v[0], at4);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] + 1;
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
function follow3(self, at4) {
	const core = self[0];
	return slot_of(self, follow_id(core.v[0], core.v[3], core.v[1][0], at4));
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
function release3(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot(self, at4, $b) {
	const handle = follow3(self, at4);
	defer_to_owner(() => {
		return release3(handle);
	}, $b);
	return handle;
}
function at(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][0].v[3], (held2) => {
		return locate(held2[0], self[1]);
	});
}
function peek_storage2(storage, read) {
	const $s = storage;
	let $t = null;
	if ($s[0] === 0) {
		const cell = $s[1];
		$t = viewing2(cell, read);
	} else {
		const port = $s[1];
		let out = [ 1 ];
		port[0]((held2) => {
			return out = [ 0, read(held2) ];
		});
		const $u = out;
		let $v = null;
		if ($u[0] === 0) {
			const value = __clone($u[1]);
			$v = value;
		} else {
			$v = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$t = $v;
	}
	return $t;
}
function peek(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function get(self) {
	const $r = at(self);
	if ($r[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held2) => {
		return read(held2, $r[1]);
	});
}
function slot_cell(watch, slot7) {
	if (slot7 < 0) {
		return new8(0, "std/src/reactive/lens.vl:1082:19");
	}
	const entry = as_usize(slot7);
	const $w = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1085:5")[0];
	const cell = __clone($w[1]);
	if ($w[0] === 0) {
		return cell;
	}
	const cell2 = new8(0, "std/src/reactive/lens.vl:1088:25");
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1089:2")[0] = [ 0, __clone(cell2) ];
	return cell2;
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
function peek2(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function size(self) {
	return peek2(self, (held2) => {
		return count2(held2);
	});
}
function coming_one(self, change4) {
	if (self[0].v[4].length === 0) {
		return;
	}
	const taps = __clone(self[0].v[4]);
	for (const tap2 of taps) {
		tap2[1](change4);
	}
}
function splice2(self, at4, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self, at4, "std/src/reactive/lens.vl:2231:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at4 + offset, __clone(value), "std/src/reactive/lens.vl:2236:9");
		offset = offset + 1;
	}
}
function mutate(self, change4) {
	const $F = self[0].v[0];
	let $G = null;
	if ($F[0] === 0) {
		const cell = $F[1];
		$G = change4(cell.v);
	} else {
		const port = __clone($F[1]);
		$G = port[1](change4);
	}
	return $G;
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $ak = null;
	if (is_empty(self)) {
		$ak = [ 1 ];
	} else {
		$ak = __list_get(self, self.length - 1);
	}
	return $ak;
}
function notify(self, $af) {
	const traced = tracing.v;
	let $ag = null;
	if (traced) {
		$ag = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$ag = "";
	}
	const cause = $ag;
	const $ah = $af;
	let $ai = null;
	if ($ah[0] === 0) {
		const turn = $ah[1];
		$ai = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $al = last(draining_turns.v);
		let $am = null;
		if ($al[0] === 0) {
			const draining = $al[1];
			$am = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $an = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$an = undefined;
				}
				$an;
			}
			$am = undefined;
		}
		$ai = $am;
	}
	return $ai;
}
function wake_all(watch, slot7, $ar) {
	const held2 = viewing2(watch, (table2) => {
		return __clone(__at(table2[1], as_usize(slot7), "std/src/reactive/lens.vl:1120:36"));
	});
	const $as = held2[0];
	if ($as[0] === 0) {
		notify($as[1], $ar);
	}
	for (const part3 of held2[1]) {
		notify(part3[1], $ar);
	}
}
function wake_part(watch, slot7, name, $at) {
	const held2 = viewing2(watch, (table2) => {
		return __clone(__at(table2[1], as_usize(slot7), "std/src/reactive/lens.vl:1130:36"));
	});
	const $au = held2[0];
	if ($au[0] === 0) {
		notify($au[1], $at);
	}
	for (const part3 of held2[1]) {
		if (part3[0] === name) {
			notify(part3[1], $at);
		}
	}
}
function batch(body, $aC) {
	const $aD = $aC;
	let $aE = null;
	if ($aD[0] === 0) {
		const current = $aD[1];
		$aE = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$aE = result;
	}
	return $aE;
}
function landed(self, changes, late, $I) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index2 = 0;
	for (const change4 of changes) {
		const $J = change4;
		let $K = null;
		if ($J[0] === 0) {
			const _at = $J[1];
			const removed = $J[2];
			const inserted = $J[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$K = undefined;
		} else if ($J[0] === 1) {
			const _at2 = $J[1];
			const _count = $J[2];
			$K = resized = true;
		} else if ($J[0] === 3) {
			const _at3 = $J[1];
			const _count2 = $J[2];
			$K = resized = true;
		} else if ($J[0] === 6) {
			$K = resized = true;
		} else if ($J[0] === 5) {
			const _from = $J[1];
			const _to = $J[2];
			$K = shaped = true;
		} else {
			$K = undefined;
		}
		$K;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change4, wakes);
		}
		let $aa = null;
		if (self[0].v[4].length > 0) {
			let $Z = null;
			if (!(late)) {
				$Z = [ 0 ];
			} else if (!(several)) {
				$Z = [ 1 ];
			} else if (index2 === 0) {
				$Z = [ 2 ];
			} else {
				$Z = [ 3 ];
			}
			const stage = $Z;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change4, stage);
			}
			$aa = undefined;
		}
		$aa;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $ab = null;
	if (shaped || resized) {
		$ab = watch.v[2];
	} else {
		$ab = [ 1 ];
	}
	const shape = __clone($ab);
	let $ac = null;
	if (resized) {
		$ac = watch.v[3];
	} else {
		$ac = [ 1 ];
	}
	const sized = __clone($ac);
	let $ad = undefined;
	let $ae = undefined;
	if (wakes.length === 0 && (($ad = shape), $ad[0] === 1) && (($ae = sized), $ae[0] === 1)) {
		notify(published, $I);
		return;
	}
	settle_together(($ao) => {
		for (const wake2 of wakes) {
			const $ap = wake2;
			let $aq = null;
			if ($ap[0] === 0) {
				const slot7 = $ap[1];
				$aq = wake_all(watch, slot7, [ 0, $ao ]);
			} else if ($ap[0] === 2) {
				const slot8 = $ap[1];
				$aq = wake_all(watch, slot8, [ 0, $ao ]);
			} else {
				const slot9 = $ap[1];
				const part3 = $ap[2];
				$aq = wake_part(watch, slot9, part3, [ 0, $ao ]);
			}
			$aq;
		}
		const $av = sized;
		if ($av[0] === 0) {
			notify($av[1], [ 0, $ao ]);
		}
		const $aw = shape;
		if ($aw[0] === 0) {
			notify($aw[1], [ 0, $ao ]);
		}
		notify(published, [ 0, $ao ]);
		return;
	}, $I);
}
function landed_one(self, change4, $H) {
	landed(self, [ __clone(change4) ], false, $H);
}
function splice3(self, at4, removed, inserted, $y) {
	const held2 = size(self);
	let $D = null;
	if (at4 > held2) {
		$D = held2;
	} else {
		$D = at4;
	}
	const start = $D;
	let $E = null;
	if (start + removed > held2) {
		$E = held2 - start;
	} else {
		$E = removed;
	}
	const taking = $E;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change4 = [ 0, start, taking, arriving ];
	coming_one(self, change4);
	mutate(self, (held3) => {
		return splice2(held3, start, taking, inserted);
	});
	landed_one(self, change4, $y);
}
function insert_at(self, at4, value, $x) {
	return splice3(self, at4, 0, [ __clone(value) ], $x);
}
function eq5(self, b2) {
	const $aL = self;
	let $aO = null;
	if ($aL[0] === 0) {
		const $aM = b2;
		let $aN = null;
		if ($aM[0] === 0) {
			$aN = eq2($aL[1], $aM[1]);
		} else {
			$aN = false;
		}
		$aO = $aN;
	} else {
		const $aP = b2;
		$aO = $aP[0] === 1;
	}
	return $aO;
}
function peek3(self, read) {
	return peek_storage(self[0].v[0], read);
}
function has(self, at4) {
	const holds = self[0].v[1][0];
	return peek3(self, (held2) => {
		return holds(held2, at4);
	});
}
function replace(self, at4, value, $aZ) {
	const watch = self[0].v[3];
	let $ba = null;
	if (has(self, at4)) {
		$ba = [ 2, __clone(at4) ];
	} else if (viewing(watch, (held2) => {
		return follows_items(held2[0]);
	})) {
		return false;
	} else {
		$ba = [ 1, __clone(at4), 1 ];
	}
	const change4 = $ba;
	coming_one(self, change4);
	const write = self[0].v[1][3];
	mutate(self, (held2) => {
		return write(held2, at4, value);
	});
	landed_one(self, change4, $aZ);
	return true;
}
function set(self, value, $aX) {
	const $aY = at(self);
	if ($aY[0] === 0) {

	} else {
		return false;
	}
	return replace(self[0], $aY[1], value, $aX);
}
function all_of(storage) {
	return peek_storage2(storage, (held2) => {
		let out = [  ];
		let at4 = 0;
		const count3 = count2(held2);
		while (at4 < count3) {
			const $bb = item2(held2, at4);
			if ($bb[0] === 0) {
				out.push($bb[1]);
			}
			at4 = at4 + 1;
		}
		return out;
	});
}
function all(self) {
	return all_of(self[0].v[0]);
}
function remove_at(self, at4, $bg) {
	return splice3(self, at4, 1, [  ], $bg);
}
function push(self, value, $bh) {
	return splice3(self, size(self), 0, [ __clone(value) ], $bh);
}
function lend3(self, at4, read) {
	if (at4 < self.length) {
		read(__at(self, at4, "std/src/reactive/lens.vl:2222:10"));
	}
}
function change2(self, at4, with2) {
	if (at4 < self.length) {
		with2(__at(self, at4, "std/src/reactive/lens.vl:2247:14"));
	}
}
function lens_over2(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new8(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over2(value, handler, access) {
	return lens_over2([ 0, __shared_new(value) ], handler, access);
}
function sequence2(value, $a) {
	return over2(value, new5(), [ (held2, at4) => {
		return at4 < count2(held2);
	}, (held2, at4) => {
		return item2(held2, at4);
	}, (held2, at4, read) => {
		return lend3(held2, at4, read);
	}, (held2, at4, value2) => {
		return put(held2, at4, value2, $a);
	}, (held2, at4, with2) => {
		return change2(held2, at4, with2, $a);
	} ]);
}
function follow_id2(storage, watch, holds, at4) {
	if (viewing(watch, (held2) => {
		return follows_items(held2[0]);
	}) && !(peek_storage2(storage, (held2) => {
		return holds(held2, at4);
	}))) {
		return -(1);
	}
	const id = follow(watch.v[0], at4);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] + 1;
	return id;
}
function follow4(self, at4) {
	const core = self[0];
	return slot_of(self, follow_id2(core.v[0], core.v[3], core.v[1][0], at4));
}
function release4(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot2(self, at4, $b) {
	const handle = follow4(self, at4);
	defer_to_owner(() => {
		return release4(handle);
	}, $b);
	return handle;
}
function part_cell(watch, slot7, name) {
	if (slot7 < 0) {
		return new8(0, "std/src/reactive/lens.vl:1097:19");
	}
	const found = viewing2(watch, (held2) => {
		let out = [ 1 ];
		for (const part3 of __at(held2[1], as_usize(slot7), "std/src/reactive/lens.vl:1101:15")[1]) {
			if (part3[0] === name) {
				out = [ 0, __clone(part3[1]) ];
			}
		}
		return out;
	});
	const $bo = found;
	if ($bo[0] === 0) {
		return $bo[1];
	}
	const woke = new8(0, "std/src/reactive/lens.vl:1111:25");
	const entry = as_usize(slot7);
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1113:2")[1].push([ name, __clone(woke) ]);
	return woke;
}
function part(self, name, read) {
	return [ __clone(self), part_cell(self[0][0].v[3], self[1], name), read ];
}
function get2(self) {
	const $br = at(self[0]);
	if ($br[0] === 0) {

	} else {
		return [ 1 ];
	}
	const view = self[0][0][0].v[1][2];
	const read = self[2];
	let out = [ 1 ];
	peek2(self[0][0], (held2) => {
		return view(held2, $br[1], (item3) => {
			return out = [ 0, read(item3) ];
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
	const holds = self[0].v[1][0];
	return peek2(self, (held2) => {
		return holds(held2, at4);
	});
}
function landed2(self, changes, late, $I) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index2 = 0;
	for (const change4 of changes) {
		const $bF = change4;
		let $bG = null;
		if ($bF[0] === 0) {
			const _at = $bF[1];
			const removed = $bF[2];
			const inserted = $bF[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$bG = undefined;
		} else if ($bF[0] === 1) {
			const _at2 = $bF[1];
			const _count = $bF[2];
			$bG = resized = true;
		} else if ($bF[0] === 3) {
			const _at3 = $bF[1];
			const _count2 = $bF[2];
			$bG = resized = true;
		} else if ($bF[0] === 6) {
			$bG = resized = true;
		} else if ($bF[0] === 5) {
			const _from = $bF[1];
			const _to = $bF[2];
			$bG = shaped = true;
		} else {
			$bG = undefined;
		}
		$bG;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change4, wakes);
		}
		let $bI = null;
		if (self[0].v[4].length > 0) {
			let $bH = null;
			if (!(late)) {
				$bH = [ 0 ];
			} else if (!(several)) {
				$bH = [ 1 ];
			} else if (index2 === 0) {
				$bH = [ 2 ];
			} else {
				$bH = [ 3 ];
			}
			const stage = $bH;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change4, stage);
			}
			$bI = undefined;
		}
		$bI;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $bJ = null;
	if (shaped || resized) {
		$bJ = watch.v[2];
	} else {
		$bJ = [ 1 ];
	}
	const shape = __clone($bJ);
	let $bK = null;
	if (resized) {
		$bK = watch.v[3];
	} else {
		$bK = [ 1 ];
	}
	const sized = __clone($bK);
	let $bL = undefined;
	let $bM = undefined;
	if (wakes.length === 0 && (($bL = shape), $bL[0] === 1) && (($bM = sized), $bM[0] === 1)) {
		notify(published, $I);
		return;
	}
	settle_together(($ao) => {
		for (const wake2 of wakes) {
			const $bN = wake2;
			let $bO = null;
			if ($bN[0] === 0) {
				const slot7 = $bN[1];
				$bO = wake_all(watch, slot7, [ 0, $ao ]);
			} else if ($bN[0] === 2) {
				const slot8 = $bN[1];
				$bO = wake_all(watch, slot8, [ 0, $ao ]);
			} else {
				const slot9 = $bN[1];
				const part3 = $bN[2];
				$bO = wake_part(watch, slot9, part3, [ 0, $ao ]);
			}
			$bO;
		}
		const $bP = sized;
		if ($bP[0] === 0) {
			notify($bP[1], [ 0, $ao ]);
		}
		const $bQ = shape;
		if ($bQ[0] === 0) {
			notify($bQ[1], [ 0, $ao ]);
		}
		notify(published, [ 0, $ao ]);
		return;
	}, $I);
}
function landed_one2(self, change4, $H) {
	landed2(self, [ __clone(change4) ], false, $H);
}
function edit(self, at4, part3, change4, $bC) {
	if (!(has2(self, at4))) {
		return false;
	}
	coming_one(self, [ 4, __clone(at4), part3 ]);
	const modify = self[0].v[1][4];
	mutate(self, (held2) => {
		return modify(held2, at4, change4);
	});
	landed_one2(self, [ 4, __clone(at4), part3 ], $bC);
	return true;
}
function edit2(self, part3, change4, $bA) {
	const $bB = at(self);
	if ($bB[0] === 0) {

	} else {
		return false;
	}
	return edit(self[0], $bB[1], part3, change4, $bA);
}
function size2(self) {
	return peek2(self, (held2) => {
		return count2(held2);
	});
}
function splice4(self, at4, removed, inserted, $y) {
	const held2 = size2(self);
	let $bV = null;
	if (at4 > held2) {
		$bV = held2;
	} else {
		$bV = at4;
	}
	const start = $bV;
	let $bW = null;
	if (start + removed > held2) {
		$bW = held2 - start;
	} else {
		$bW = removed;
	}
	const taking = $bW;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change4 = [ 0, start, taking, arriving ];
	coming_one(self, change4);
	mutate(self, (held3) => {
		return splice2(held3, start, taking, inserted);
	});
	landed_one2(self, change4, $y);
}
function insert_at2(self, at4, value, $x) {
	return splice4(self, at4, 0, [ __clone(value) ], $x);
}
function audit(self, before, changes) {
	const $ck = self[0].v[6];
	if ($ck[0] === 0) {

	} else {
		return;
	}
	const $cl = before;
	if ($cl[0] === 0) {

	} else {
		return;
	}
	const problem = peek2(self, (held2) => {
		return $ck[1]($cl[1], held2, changes);
	});
	const $cq = problem;
	if ($cq[0] === 0) {
		(() => {
			throw __panic("lens: the changes do not match the write: " + $cq[1], "std/src/reactive/lens.vl:806:4");
		})();
	}
}
function update(self, change4, $cb) {
	const $cc = self[0].v[6];
	let $ch = null;
	if ($cc[0] === 0) {
		$ch = [ 0, peek2(self, (held2) => {
			return __clone(held2);
		}) ];
	} else {
		$ch = [ 1 ];
	}
	const before = $ch;
	let out = [ 1 ];
	mutate(self, (held2) => {
		return out = [ 0, change4(held2) ];
	});
	const $ci = out;
	let $cj = null;
	if ($ci[0] === 0) {
		const result = __clone($ci[1][0]);
		const changes = __clone($ci[1][1]);
		audit(self, before, changes);
		landed2(self, changes, true, $cb);
		$cj = result;
	} else {
		$cj = (() => {
			throw __panic("lens: an update\'s write never ran", "std/src/reactive/lens.vl:776:12");
		})();
	}
	return $cj;
}
function get3(self) {
	const $cr = at(self);
	if ($cr[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek2(self[0], (held2) => {
		return read(held2, $cr[1]);
	});
}
function eq6(self, b2) {
	const $cw = self;
	let $cz = null;
	if ($cw[0] === 0) {
		const $cx = b2;
		let $cy = null;
		if ($cx[0] === 0) {
			$cy = eq4($cw[1], $cx[1]);
		} else {
			$cy = false;
		}
		$cz = $cy;
	} else {
		const $cA = b2;
		$cz = $cA[0] === 1;
	}
	return $cz;
}
function get4(self) {
	return peek2(self, (held2) => {
		return __clone(held2);
	});
}
function on_settle4(self, subscriber) {
	return on_settle(self[0].v[2], subscriber);
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
	return peek2(self[0], (held2) => {
		return count2(held2);
	});
}
function sized_cell(self) {
	const watch = self[0].v[3];
	const $cF = watch.v[3];
	const cell = __clone($cF[1]);
	if ($cF[0] === 0) {
		return cell;
	}
	const cell2 = new8(0, "std/src/reactive/lens.vl:1019:26");
	watch.v[3] = [ 0, __clone(cell2) ];
	return cell2;
}
function on_settle5(self, subscriber) {
	return on_settle(sized_cell(self[0]), subscriber);
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
function replace2(self, at4, value, $aZ) {
	const watch = self[0].v[3];
	let $cG = null;
	if (has2(self, at4)) {
		$cG = [ 2, __clone(at4) ];
	} else if (viewing(watch, (held2) => {
		return follows_items(held2[0]);
	})) {
		return false;
	} else {
		$cG = [ 1, __clone(at4), 1 ];
	}
	const change4 = $cG;
	coming_one(self, change4);
	const write = self[0].v[1][3];
	mutate(self, (held2) => {
		return write(held2, at4, value);
	});
	landed_one2(self, change4, $aZ);
	return true;
}
function push2(self, value, $bh) {
	return splice4(self, size2(self), 0, [ __clone(value) ], $bh);
}
function shift(self, from, to) {
	const item3 = __remove_at(self, from, "std/src/reactive/lens.vl:2252:19");
	__insert_at(self, to, item3, "std/src/reactive/lens.vl:2253:8");
}
function move_item(self, from, to, $cJ) {
	const held2 = size(self);
	if (from >= held2 || from === to) {
		return;
	}
	let $cK = null;
	if (to >= held2) {
		$cK = held2 - 1;
	} else {
		$cK = to;
	}
	const landing = $cK;
	const change4 = [ 5, from, landing ];
	coming_one(self, change4);
	mutate(self, (held3) => {
		return shift(held3, from, landing, $cJ);
	});
	landed_one(self, change4, $cJ);
}
function new9() {
	const table2 = [ new Map(), undefined ];
	return [ table2 ];
}
function insert(self, key2, value) {
	__nmap_insert(self[0], hash(key2), __clone(key2), __clone(value));
}
function contains_key(self, key2) {
	return __nmap_has(self[0], hash(key2));
}
function reconcile_by_key(self, next, key2, $cL) {
	const lens2 = __clone(self);
	batch(($cM) => {
		let wanted = new9();
		for (const $cN of next) {
			const item3 = [ $cN ];
			insert(wanted, key2([ item3, 0 ]), true);
		}
		let keys2 = [  ];
		for (const $cO of all(lens2)) {
			const item4 = [ $cO ];
			keys2.push(key2([ item4, 0 ]));
		}
		let at4 = keys2.length;
		while (at4 > 0) {
			at4 = at4 - 1;
			if (!(contains_key(wanted, __at(keys2, at4, "std/src/reactive/lens.vl:1910:29")))) {
				remove_at(lens2, at4, [ 0, $cM ]);
				__remove_at(keys2, at4, "std/src/reactive/lens.vl:1912:11");
			}
		}
		let index2 = 0;
		for (const $cP of next) {
			const item5 = [ $cP ];
			const want = key2([ item5, 0 ]);
			let $cQ = null;
			if (index2 < keys2.length && __at(keys2, index2, "std/src/reactive/lens.vl:1918:30") === want) {
				if (!(eq3(peek(lens2, (list2) => {
					return item2(list2, index2);
				}), [ 0, __clone(item5[0]) ]))) {
					replace(lens2, index2, item5[0], [ 0, $cM ]);
				}
				$cQ = undefined;
			} else {
				let found = [ 1 ];
				let search = index2 + 1;
				while (search < keys2.length) {
					if (__at(keys2, search, "std/src/reactive/lens.vl:1926:10") === want) {
						found = [ 0, search ];
						search = keys2.length;
					}
					search = search + 1;
				}
				const $cR = found;
				let $cS = null;
				if ($cR[0] === 0) {
					const from = $cR[1];
					move_item(lens2, from, index2, [ 0, $cM ]);
					const moved = __remove_at(keys2, from, "std/src/reactive/lens.vl:1935:25");
					__insert_at(keys2, index2, moved, "std/src/reactive/lens.vl:1936:13");
					if (!(eq3(peek(lens2, (list2) => {
						return item2(list2, index2);
					}), [ 0, __clone(item5[0]) ]))) {
						replace(lens2, index2, item5[0], [ 0, $cM ]);
					}
					$cS = undefined;
				} else {
					insert_at(lens2, index2, item5[0], [ 0, $cM ]);
					__insert_at(keys2, index2, want, "std/src/reactive/lens.vl:1943:13");
					$cS = undefined;
				}
				$cQ = $cS;
			}
			$cQ;
			index2 = index2 + 1;
		}
		return;
	}, $cL);
}
function over3(value, handler, access) {
	return lens_over2([ 0, __shared_new(value) ], handler, access);
}
function sequence3(value, $a) {
	return over3(value, new5(), [ (held2, at4) => {
		return at4 < count2(held2);
	}, (held2, at4) => {
		return item2(held2, at4);
	}, (held2, at4, read) => {
		return lend(held2, at4, read);
	}, (held2, at4, value2) => {
		return put(held2, at4, value2, $a);
	}, (held2, at4, with2) => {
		return change(held2, at4, with2, $a);
	} ]);
}
function new10() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function arrived_at(storage, at4, count3) {
	return peek_storage2(storage, (held2) => {
		let arrived = [  ];
		let index2 = at4;
		while (index2 < at4 + count3) {
			const $cX = item2(held2, index2);
			if ($cX[0] === 0) {
				arrived.push($cX[1]);
			}
			index2 = index2 + 1;
		}
		return arrived;
	});
}
function one(item3) {
	const $dg = item3;
	let $dh = null;
	if ($dg[0] === 0) {
		const value = $dg[1];
		$dh = [ __clone(value) ];
	} else {
		$dh = [  ];
	}
	return $dh;
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
		const length5 = held2.length;
		while (index2 < length5) {
			const $dn = __list_get(held2, index2);
			let $do = null;
			if ($dn[0] === 0) {
				const op = $dn[1];
				$do = kept.push(op);
			} else {
				$do = undefined;
			}
			$do;
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
		return item2(held2, at4);
	});
	const $ds = before;
	let $dt = undefined;
	if ($ds[0] === 0 && (($dt = after), $dt[0] === 0)) {
		record(log, [ 1, at4, __clone($ds[1]), __clone($dt[1]) ]);
	}
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
function release5(self) {
	untap(self[0], self[2]);
}
function items_into(self, log, $cU) {
	const leaving = __shared_new([  ]);
	const runs2 = __shared_new([  ]);
	const skipping = __shared_new(0);
	const storage = __clone(self[0].v[0]);
	const tap2 = tap(self, (change4) => {
		const $cV = change4;
		let $cW = null;
		if ($cV[0] === 0) {
			const at4 = $cV[1];
			const removed = $cV[2];
			const _inserted = $cV[3];
			leaving.v = arrived_at(storage, at4, removed);
			$cW = undefined;
		} else if ($cV[0] === 3) {
			const at5 = $cV[1];
			const count3 = $cV[2];
			leaving.v = arrived_at(storage, at5, count3);
			$cW = undefined;
		} else if ($cV[0] === 2) {
			const at6 = $cV[1];
			leaving.v = one(peek_storage2(storage, (held2) => {
				return item2(held2, at6);
			}));
			$cW = undefined;
		} else if ($cV[0] === 4) {
			const at7 = $cV[1];
			const _part = $cV[2];
			leaving.v = one(peek_storage2(storage, (held2) => {
				return item2(held2, at7);
			}));
			$cW = undefined;
		} else if ($cV[0] === 5) {
			const from = $cV[1];
			const to = $cV[2];
			$cW = extend_run(runs2, from, to);
		} else {
			$cW = undefined;
		}
		return $cW;
	}, (change4, stage) => {
		const $dk = stage;
		let $dl = null;
		if ($dk[0] === 2) {
			record(log, [ 2, all_of(storage) ]);
			return;
			$dl = undefined;
		} else if ($dk[0] === 3) {
			return;
		} else {
			$dl = undefined;
		}
		$dl;
		const $dp = stage;
		const late = $dp[0] === 1;
		const $dq = change4;
		let $dr = null;
		if ($dq[0] === 0) {
			const at4 = $dq[1];
			const removed = $dq[2];
			const inserted = $dq[3];
			if (late && removed > 0) {
				record(log, [ 2, all_of(storage) ]);
			} else if (late) {
				record(log, [ 0, at4, [  ], arrived_at(storage, at4, inserted) ]);
			} else {
				record(log, [ 0, at4, __clone(leaving.v), arrived_at(storage, at4, inserted) ]);
			}
			$dr = undefined;
		} else if ($dq[0] === 1) {
			const at5 = $dq[1];
			const count3 = $dq[2];
			record(log, [ 0, at5, [  ], arrived_at(storage, at5, count3) ]);
			$dr = undefined;
		} else if ($dq[0] === 3) {
			const at6 = $dq[1];
			const _count = $dq[2];
			if (late) {
				record(log, [ 2, all_of(storage) ]);
			} else {
				record(log, [ 0, at6, __clone(leaving.v), [  ] ]);
			}
			$dr = undefined;
		} else if ($dq[0] === 2) {
			const at7 = $dq[1];
			$dr = record_set(log, storage, leaving, at7, late);
		} else if ($dq[0] === 4) {
			const at8 = $dq[1];
			const _part = $dq[2];
			$dr = record_set(log, storage, leaving, at8, late);
		} else if ($dq[0] === 5) {
			const from = $dq[1];
			const to = $dq[2];
			if (skipping.v > 0) {
				const $du = [ skipping, "v" ];
				skipping.v = $du[0][$du[1]] - 1;
				return;
			}
			if (runs2.v.length === 0) {
				record(log, [ 3, from, 1, to ]);
				return;
			}
			const $dv = __remove_at(runs2.v, 0, "std/src/reactive/lens.vl:1679:47");
			const start = $dv[0];
			const count4 = $dv[1];
			const first2 = $dv[2];
			skipping.v = count4 - 1;
			let $dw = null;
			if (first2 > start) {
				$dw = first2 + 1 - count4;
			} else {
				$dw = first2;
			}
			const landing = $dw;
			record(log, [ 3, start, count4, landing ]);
			$dr = undefined;
		} else {
			$dr = record(log, [ 2, all_of(storage) ]);
		}
		return $dr;
	});
	const feed4 = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release5(feed4);
	}, $cU);
	return feed4;
}
function items(self, $cT) {
	return items_into(self, new10(), $cT);
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
	const length5 = held2.length;
	while (index2 < length5) {
		const $dB = __list_get(held2, index2);
		let $dC = null;
		if ($dB[0] === 0) {
			const op = $dB[1];
			$dC = ops2.push(op);
		} else {
			$dC = undefined;
		}
		$dC;
		index2 = index2 + 1;
	}
	return [ 0, ops2 ];
}
function all2(self) {
	return all_of(self[0].v[0]);
}
function since2(self, cursor6) {
	const $dD = since(self[1], cursor6);
	let $dE = null;
	if ($dD[0] === 0) {
		const recorded = $dD[1];
		$dE = recorded;
	} else {
		$dE = [ [ 2, all2(self[0]) ] ];
	}
	return $dE;
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
	return on_settle(self[0][0].v[2], subscriber);
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
	const value = (($dK) => {
		return (($dL) => {
			return body($dK, $dL);
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
	const holds = new Map();
	const dirty = [  ];
	const consumer = [ 1 ];
	return [ __shared_new(ids), __shared_new(items2), __shared_new(values), __shared_new(holds), __shared_new(dirty), __shared_new(0), __clone(stage), __shared_new(consumer), run ];
}
function keep_hold(core, id, hold) {
	element_holds_allocated_count.v = element_holds_allocated_count.v + 1;
	core[3].v.set(hash2(id), __clone(hold));
}
function admit(core, item3) {
	const id = core[5].v;
	core[5].v = id + 1;
	const $dN = core[8](item3);
	const answer = $dN[0];
	const owner2 = $dN[1];
	const $dO = answer;
	let $dP = null;
	if ($dO[0] === 0) {
		const value = $dO[1];
		const $dQ = owner2;
		let $dR = null;
		if ($dQ[0] === 0) {
			const held2 = $dQ[1];
			$dR = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held2) ] ]);
		} else {
			$dR = undefined;
		}
		$dR;
		$dP = [ id, __clone(value) ];
	} else {
		const instance = $dO[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $dS = consumer.v;
			let $dT = null;
			if ($dS[0] === 0) {
				const downstream = $dS[1];
				$dT = wake(downstream);
			} else {
				$dT = undefined;
			}
			return $dT;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner2 ]);
		$dP = [ id, pull() ];
	}
	return $dP;
}
function seed(core, items2) {
	for (const item3 of items2) {
		const $dY = admit(core, item3);
		const id = $dY[0];
		const value = $dY[1];
		core[0].v.push(id);
		core[1].v.push(__clone(item3));
		core[2].v.push(value);
	}
}
function fresh_list(list2) {
	let copy = [  ];
	for (const item3 of list2) {
		copy.push(__clone(item3));
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
function held_pairs(core, at4, count3) {
	let pairs = [  ];
	let index2 = at4;
	while (index2 < at4 + count3) {
		pairs.push([ __clone(__at(core[1].v, index2, "std/src/reactive/delta.vl:1618:15")), __clone(__at(core[2].v, index2, "std/src/reactive/delta.vl:1618:41")) ]);
		index2 = index2 + 1;
	}
	return pairs;
}
function is_none(self) {
	const $eu = self;
	return $eu[0] === 1;
}
function retire(core, id) {
	const $ed = __map_get(core[3].v, hash2(id));
	let $ee = null;
	if ($ed[0] === 0) {
		const hold = $ed[1];
		core[3].v.delete(hash2(id));
		const $ef = hold[2];
		let $eg = null;
		if ($ef[0] === 0) {
			const handle = $ef[1];
			$eg = detach(handle);
		} else {
			$eg = undefined;
		}
		$eg;
		hold[3]();
		const $eq = hold[4];
		let $er = null;
		if ($eq[0] === 0) {
			const owner2 = $eq[1];
			$er = dispose2(owner2);
		} else {
			$er = undefined;
		}
		$ee = $er;
	} else {
		$ee = undefined;
	}
	return $ee;
}
function all_pairs(core) {
	return held_pairs(core, 0, core[0].v.length);
}
function resolve(core, ops2) {
	let events = [  ];
	for (const op of ops2) {
		const $dZ = op;
		let $ea = null;
		if ($dZ[0] === 0) {
			const at4 = $dZ[1];
			const removed = $dZ[2];
			const inserted = $dZ[3];
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
			for (const item3 of inserted) {
				const $eC = admit(core, item3);
				const id2 = $eC[0];
				const value = $eC[1];
				__insert_at(core[0].v, start + offset, id2, "std/src/reactive/delta.vl:1652:23");
				__insert_at(core[1].v, start + offset, __clone(item3), "std/src/reactive/delta.vl:1653:25");
				__insert_at(core[2].v, start + offset, __clone(value), "std/src/reactive/delta.vl:1654:26");
				arrived.push([ __clone(item3), value ]);
				offset = offset + 1;
			}
			events.push([ 0, start, left, arrived ]);
			$ea = undefined;
		} else if ($dZ[0] === 1) {
			const at5 = $dZ[1];
			const _was = $dZ[2];
			const item4 = $dZ[3];
			const $eD = __list_get(core[0].v, at5);
			let $eE = null;
			if ($eD[0] === 0) {
				const old_id = $eD[1];
				const before = [ __clone(__at(core[1].v, at5, "std/src/reactive/delta.vl:1663:21")), __clone(__at(core[2].v, at5, "std/src/reactive/delta.vl:1663:44")) ];
				retire(core, old_id);
				const $eF = admit(core, item4);
				const id3 = $eF[0];
				const value2 = $eF[1];
				__at_put(core[0].v, at5, id3, "std/src/reactive/delta.vl:1669:7");
				__at_put(core[1].v, at5, __clone(item4), "std/src/reactive/delta.vl:1670:7");
				__at_put(core[2].v, at5, __clone(value2), "std/src/reactive/delta.vl:1671:7");
				events.push([ 1, at5, before, [ __clone(item4), value2 ], true ]);
				$eE = undefined;
			} else {
				$eE = undefined;
			}
			$ea = $eE;
		} else if ($dZ[0] === 2) {
			const items2 = $dZ[1];
			const left2 = all_pairs(core);
			for (const id4 of __clone(core[0].v)) {
				retire(core, id4);
			}
			core[0].v = [  ];
			core[1].v = [  ];
			core[2].v = [  ];
			seed(core, items2);
			events.push([ 2, left2, all_pairs(core) ]);
			$ea = undefined;
		} else {
			const from = $dZ[1];
			const count3 = $dZ[2];
			const to = $dZ[3];
			let lifted_ids = [  ];
			let lifted_items = [  ];
			let lifted_values = [  ];
			let taken2 = 0;
			while (taken2 < count3) {
				lifted_ids.push(__remove_at(core[0].v, from, "std/src/reactive/delta.vl:1694:39"));
				lifted_items.push(__remove_at(core[1].v, from, "std/src/reactive/delta.vl:1695:43"));
				lifted_values.push(__remove_at(core[2].v, from, "std/src/reactive/delta.vl:1696:45"));
				taken2 = taken2 + 1;
			}
			let offset2 = 0;
			while (offset2 < count3) {
				__insert_at(core[0].v, to + offset2, __at(lifted_ids, offset2, "std/src/reactive/delta.vl:1701:43"), "std/src/reactive/delta.vl:1701:23");
				__insert_at(core[1].v, to + offset2, __clone(__at(lifted_items, offset2, "std/src/reactive/delta.vl:1702:45")), "std/src/reactive/delta.vl:1702:25");
				__insert_at(core[2].v, to + offset2, __clone(__at(lifted_values, offset2, "std/src/reactive/delta.vl:1703:46")), "std/src/reactive/delta.vl:1703:26");
				offset2 = offset2 + 1;
			}
			events.push([ 3, from, count3, to ]);
			$ea = undefined;
		}
		$ea;
	}
	const woke = __clone(core[4].v);
	let $eK = null;
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
			const $eG = __map_get(positions, hash2(id6));
			let $eH = null;
			if ($eG[0] === 0) {
				const at6 = $eG[1];
				const $eI = __map_get(core[3].v, hash2(id6));
				let $eJ = null;
				if ($eI[0] === 0) {
					const hold = $eI[1];
					if (hold[0]) {
						const item5 = __clone(__at(core[1].v, at6, "std/src/reactive/delta.vl:1731:19"));
						const before2 = __clone(__at(core[2].v, at6, "std/src/reactive/delta.vl:1732:21"));
						const now = hold[1]();
						__at_put(core[2].v, at6, __clone(now), "std/src/reactive/delta.vl:1734:8");
						events.push([ 1, at6, [ __clone(item5), before2 ], [ item5, now ], false ]);
					}
					$eJ = undefined;
				} else {
					$eJ = undefined;
				}
				$eH = $eJ;
			} else {
				$eH = undefined;
			}
			$eH;
		}
		$eK = undefined;
	}
	$eK;
	return events;
}
function map2(self, fn) {
	let result = [  ];
	for (const item3 of self) {
		result.push(fn(item3));
	}
	return result;
}
function mapped_ops(events) {
	let ops2 = [  ];
	for (const event of events) {
		const $eL = event;
		let $eM = null;
		if ($eL[0] === 0) {
			const at4 = $eL[1];
			const left = $eL[2];
			const arrived = $eL[3];
			ops2.push([ 0, at4, map2(left, (pair) => {
				return __clone(pair[1]);
			}), map2(arrived, (pair) => {
				return __clone(pair[1]);
			}) ]);
			$eM = undefined;
		} else if ($eL[0] === 1) {
			const at5 = $eL[1];
			const before = $eL[2];
			const after = $eL[3];
			const _rerun = $eL[4];
			ops2.push([ 1, at5, __clone(before[1]), __clone(after[1]) ]);
			$eM = undefined;
		} else if ($eL[0] === 2) {
			const _left = $eL[1];
			const now = $eL[2];
			$eM = ops2.push([ 2, map2(now, (pair) => {
				return __clone(pair[1]);
			}) ]);
		} else {
			const from = $eL[1];
			const count3 = $eL[2];
			const to = $eL[3];
			$eM = ops2.push([ 3, from, count3, to ]);
		}
		$eM;
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
	const release11 = upstream[3];
	return () => {
		release_core(core);
		release11();
		return;
	};
}
function open3(self) {
	const transform = self[1];
	const upstream = open2(__clone(self[0]));
	const stage = new4();
	const core = element_core(stage, (item3) => {
		const $dM = run_element(stage, ($dF, $dG) => {
			return transform(item3, $dF, $dG);
		});
		const answer = $dM[0];
		const owner2 = $dM[1];
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
	return [ __shared_new(elements), new8(0, "std/src/reactive/delta.vl:792:68"), __clone(log) ];
}
function of(elements) {
	return list_cell(elements, new10());
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
		const $eT = __list_get(inserted, offset);
		let $eU = null;
		if ($eT[0] === 0) {
			const value = $eT[1];
			$eU = __insert_at(list2, start + offset, value, "std/src/reactive/delta.vl:702:28");
		} else {
			$eU = undefined;
		}
		$eU;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function at2(self) {
	return self[1].v;
}
function set2(self, value, $eW, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $eW);
}
function publish(cell, $eV) {
	set2(cell[1], at2(cell[2]), $eV, "std/src/reactive/delta.vl:799:17");
}
function splice5(self, at4, removed, inserted, $eS) {
	const held2 = size3(self);
	const start = splice_start(held2, at4);
	const taking = splice_count(held2, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	record(self[2], splice_in_place(self[0].v, start, taking, inserted));
	publish(self, $eS);
}
function set_at(self, at4, value, $eX) {
	const $eY = __list_get(self[0].v, at4);
	let $eZ = null;
	if ($eY[0] === 0) {
		const previous = $eY[1];
		__at_put(self[0].v, at4, __clone(value), "std/src/reactive/delta.vl:855:5");
		record(self[2], [ 1, at4, previous, __clone(value) ]);
		publish(self, $eX);
		$eZ = undefined;
	} else {
		$eZ = undefined;
	}
	return $eZ;
}
function set3(self, value, $fa, caller) {
	self[0].v = __clone(value);
	record(self[2], [ 2, __clone(value) ]);
	publish(self, $fa);
}
function move_range(self, from, count3, to, $fb) {
	if (count3 === 0 || from === to) {
		return;
	}
	let lifted = [  ];
	let taken = 0;
	while (taken < count3) {
		lifted.push(__remove_at(self[0].v, from, "std/src/reactive/delta.vl:875:35"));
		taken = taken + 1;
	}
	let offset = 0;
	while (offset < lifted.length) {
		const $fc = __list_get(lifted, offset);
		let $fd = null;
		if ($fc[0] === 0) {
			const value = $fc[1];
			$fd = __insert_at(self[0].v, to + offset, value, "std/src/reactive/delta.vl:881:43");
		} else {
			$fd = undefined;
		}
		$fd;
		offset = offset + 1;
	}
	record(self[2], [ 3, from, count3, to ]);
	publish(self, $fb);
}
function apply_ops(cell, ops2, $eP) {
	for (const op of ops2) {
		const $eQ = op;
		let $eR = null;
		if ($eQ[0] === 0) {
			const at4 = $eQ[1];
			const removed = $eQ[2];
			const inserted = $eQ[3];
			$eR = splice5(cell, at4, removed.length, inserted, $eP);
		} else if ($eQ[0] === 1) {
			const at5 = $eQ[1];
			const _was = $eQ[2];
			const value = $eQ[3];
			$eR = set_at(cell, at5, value, $eP);
		} else if ($eQ[0] === 2) {
			const items2 = $eQ[1];
			$eR = set3(cell, items2, $eP, "std/src/reactive/delta.vl:1325:36");
		} else {
			const from = $eQ[1];
			const count3 = $eQ[2];
			const to = $eQ[3];
			$eR = move_range(cell, from, count3, to, $eP);
		}
		$eR;
	}
}
function take(self, item3, $fk) {
	defer(self, () => {
		dispose(item3, $fk);
		return;
	});
	return __clone(item3);
}
function seal_collection(instance, owned, $eN, $eO, caller) {
	const sealed = of(fresh_list(instance[0]()));
	const drain2 = instance[2];
	if (owned) {
		const refresh = subscriber_of(() => {
			return apply_ops(sealed, drain2(), $eN);
		}, true);
		place_subscriber(refresh[0], caller);
		const refreshed = instance[1](refresh);
		also_releasing(refreshed, instance[3]);
		register_with_owner(refreshed, $eN, $eO);
	} else {
		const cell_global_refresh = () => {
			return apply_ops(sealed, drain2(), $eN);
		};
		const refresh2 = subscriber_of(cell_global_refresh, true);
		place_subscriber(refresh2[0], caller);
		instance[1](refresh2);
	}
	return sealed;
}
function memo(self, $dz, $dA, caller) {
	return [ seal_collection(open3(self), true, $dz, $dA, caller) ];
}
function filter(self, keep) {
	return [ self, keep ];
}
function start_element(self) {
	return [ 0, self ];
}
function admit2(core, item3) {
	const id = core[5].v;
	core[5].v = id + 1;
	const $ft = core[8](item3);
	const answer = $ft[0];
	const owner2 = $ft[1];
	const $fu = answer;
	let $fv = null;
	if ($fu[0] === 0) {
		const value = $fu[1];
		const $fw = owner2;
		let $fx = null;
		if ($fw[0] === 0) {
			const held2 = $fw[1];
			$fx = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held2) ] ]);
		} else {
			$fx = undefined;
		}
		$fx;
		$fv = [ id, __clone(value) ];
	} else {
		const instance = $fu[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $fy = consumer.v;
			let $fz = null;
			if ($fy[0] === 0) {
				const downstream = $fy[1];
				$fz = wake(downstream);
			} else {
				$fz = undefined;
			}
			return $fz;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner2 ]);
		$fv = [ id, pull() ];
	}
	return $fv;
}
function seed2(core, items2) {
	for (const item3 of items2) {
		const $fA = admit2(core, item3);
		const id = $fA[0];
		const value = $fA[1];
		core[0].v.push(id);
		core[1].v.push(__clone(item3));
		core[2].v.push(value);
	}
}
function retire2(core, id) {
	const $fF = __map_get(core[3].v, hash2(id));
	let $fG = null;
	if ($fF[0] === 0) {
		const hold = $fF[1];
		core[3].v.delete(hash2(id));
		const $fH = hold[2];
		let $fI = null;
		if ($fH[0] === 0) {
			const handle = $fH[1];
			$fI = detach(handle);
		} else {
			$fI = undefined;
		}
		$fI;
		hold[3]();
		const $fJ = hold[4];
		let $fK = null;
		if ($fJ[0] === 0) {
			const owner2 = $fJ[1];
			$fK = dispose2(owner2);
		} else {
			$fK = undefined;
		}
		$fG = $fK;
	} else {
		$fG = undefined;
	}
	return $fG;
}
function resolve2(core, ops2) {
	let events = [  ];
	for (const op of ops2) {
		const $fD = op;
		let $fE = null;
		if ($fD[0] === 0) {
			const at4 = $fD[1];
			const removed = $fD[2];
			const inserted = $fD[3];
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
			for (const item3 of inserted) {
				const $fL = admit2(core, item3);
				const id2 = $fL[0];
				const value = $fL[1];
				__insert_at(core[0].v, start + offset, id2, "std/src/reactive/delta.vl:1652:23");
				__insert_at(core[1].v, start + offset, __clone(item3), "std/src/reactive/delta.vl:1653:25");
				__insert_at(core[2].v, start + offset, __clone(value), "std/src/reactive/delta.vl:1654:26");
				arrived.push([ __clone(item3), value ]);
				offset = offset + 1;
			}
			events.push([ 0, start, left, arrived ]);
			$fE = undefined;
		} else if ($fD[0] === 1) {
			const at5 = $fD[1];
			const _was = $fD[2];
			const item4 = $fD[3];
			const $fM = __list_get(core[0].v, at5);
			let $fN = null;
			if ($fM[0] === 0) {
				const old_id = $fM[1];
				const before = [ __clone(__at(core[1].v, at5, "std/src/reactive/delta.vl:1663:21")), __clone(__at(core[2].v, at5, "std/src/reactive/delta.vl:1663:44")) ];
				retire2(core, old_id);
				const $fO = admit2(core, item4);
				const id3 = $fO[0];
				const value2 = $fO[1];
				__at_put(core[0].v, at5, id3, "std/src/reactive/delta.vl:1669:7");
				__at_put(core[1].v, at5, __clone(item4), "std/src/reactive/delta.vl:1670:7");
				__at_put(core[2].v, at5, __clone(value2), "std/src/reactive/delta.vl:1671:7");
				events.push([ 1, at5, before, [ __clone(item4), value2 ], true ]);
				$fN = undefined;
			} else {
				$fN = undefined;
			}
			$fE = $fN;
		} else if ($fD[0] === 2) {
			const items2 = $fD[1];
			const left2 = all_pairs(core);
			for (const id4 of __clone(core[0].v)) {
				retire2(core, id4);
			}
			core[0].v = [  ];
			core[1].v = [  ];
			core[2].v = [  ];
			seed2(core, items2);
			events.push([ 2, left2, all_pairs(core) ]);
			$fE = undefined;
		} else {
			const from = $fD[1];
			const count3 = $fD[2];
			const to = $fD[3];
			let lifted_ids = [  ];
			let lifted_items = [  ];
			let lifted_values = [  ];
			let taken2 = 0;
			while (taken2 < count3) {
				lifted_ids.push(__remove_at(core[0].v, from, "std/src/reactive/delta.vl:1694:39"));
				lifted_items.push(__remove_at(core[1].v, from, "std/src/reactive/delta.vl:1695:43"));
				lifted_values.push(__remove_at(core[2].v, from, "std/src/reactive/delta.vl:1696:45"));
				taken2 = taken2 + 1;
			}
			let offset2 = 0;
			while (offset2 < count3) {
				__insert_at(core[0].v, to + offset2, __at(lifted_ids, offset2, "std/src/reactive/delta.vl:1701:43"), "std/src/reactive/delta.vl:1701:23");
				__insert_at(core[1].v, to + offset2, __clone(__at(lifted_items, offset2, "std/src/reactive/delta.vl:1702:45")), "std/src/reactive/delta.vl:1702:25");
				__insert_at(core[2].v, to + offset2, __clone(__at(lifted_values, offset2, "std/src/reactive/delta.vl:1703:46")), "std/src/reactive/delta.vl:1703:26");
				offset2 = offset2 + 1;
			}
			events.push([ 3, from, count3, to ]);
			$fE = undefined;
		}
		$fE;
	}
	const woke = __clone(core[4].v);
	let $fT = null;
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
			const $fP = __map_get(positions, hash2(id6));
			let $fQ = null;
			if ($fP[0] === 0) {
				const at6 = $fP[1];
				const $fR = __map_get(core[3].v, hash2(id6));
				let $fS = null;
				if ($fR[0] === 0) {
					const hold = $fR[1];
					if (hold[0]) {
						const item5 = __clone(__at(core[1].v, at6, "std/src/reactive/delta.vl:1731:19"));
						const before2 = __clone(__at(core[2].v, at6, "std/src/reactive/delta.vl:1732:21"));
						const now = hold[1]();
						__at_put(core[2].v, at6, __clone(now), "std/src/reactive/delta.vl:1734:8");
						events.push([ 1, at6, [ __clone(item5), before2 ], [ item5, now ], false ]);
					}
					$fS = undefined;
				} else {
					$fS = undefined;
				}
				$fQ = $fS;
			} else {
				$fQ = undefined;
			}
			$fQ;
		}
		$fT = undefined;
	}
	$fT;
	return events;
}
function kept_ops(kept, index2, events, out, payload_follows) {
	let ops2 = [  ];
	for (const event of events) {
		const $fV = event;
		let $fW = null;
		if ($fV[0] === 0) {
			const at4 = $fV[1];
			const left = $fV[2];
			const arrived = $fV[3];
			const position = kept_before(index2, at4);
			let removed = [  ];
			for (const pair of left) {
				const $fX = out(pair);
				let $fY = null;
				if ($fX[0] === 0) {
					const value = $fX[1];
					$fY = removed.push(value);
				} else {
					$fY = undefined;
				}
				$fY;
			}
			let inserted = [  ];
			let flags = [  ];
			for (const pair2 of arrived) {
				const $fZ = out(pair2);
				let $ga = null;
				if ($fZ[0] === 0) {
					const value2 = $fZ[1];
					inserted.push(value2);
					flags.push(true);
					$ga = undefined;
				} else {
					$ga = flags.push(false);
				}
				$ga;
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
			$fW = undefined;
		} else if ($fV[0] === 1) {
			const at5 = $fV[1];
			const before = $fV[2];
			const after = $fV[3];
			const rerun = $fV[4];
			const position2 = kept_before(index2, at5);
			const $gb = [ out(before), out(after) ];
			let $gc = null;
			if ($gb[0][0] === 0 && $gb[1][0] === 0) {
				const was = $gb[0][1];
				const now = $gb[1][1];
				if (rerun || payload_follows) {
					ops2.push([ 1, position2, was, now ]);
				}
				$gc = undefined;
			} else if ($gb[0][0] === 0 && $gb[1][0] === 1) {
				const was2 = $gb[0][1];
				__at_put(kept.v, at5, false, "std/src/reactive/delta.vl:2179:7");
				kept_add(index2, at5, -(1));
				ops2.push([ 0, position2, [ was2 ], [  ] ]);
				$gc = undefined;
			} else if ($gb[0][0] === 1 && $gb[1][0] === 0) {
				const now2 = $gb[1][1];
				__at_put(kept.v, at5, true, "std/src/reactive/delta.vl:2184:7");
				kept_add(index2, at5, 1);
				ops2.push([ 0, position2, [  ], [ now2 ] ]);
				$gc = undefined;
			} else {
				$gc = undefined;
			}
			$fW = $gc;
		} else if ($fV[0] === 2) {
			const _left = $fV[1];
			const now3 = $fV[2];
			let flags2 = [  ];
			let values = [  ];
			for (const pair3 of now3) {
				const $gd = out(pair3);
				let $ge = null;
				if ($gd[0] === 0) {
					const value3 = $gd[1];
					values.push(value3);
					flags2.push(true);
					$ge = undefined;
				} else {
					$ge = flags2.push(false);
				}
				$ge;
			}
			kept.v = flags2;
			rebuild_kept(index2, kept.v);
			ops2.push([ 2, values ]);
			$fW = undefined;
		} else {
			const from = $fV[1];
			const count3 = $fV[2];
			const to = $fV[3];
			const out_from = kept_before(index2, from);
			const moved = kept_before(index2, from + count3) - out_from;
			let lifted = [  ];
			let taken2 = 0;
			while (taken2 < count3) {
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
			$fW = undefined;
		}
		$fW;
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
	const release11 = upstream[3];
	return () => {
		release_core2(core);
		release11();
		return;
	};
}
function open4(self) {
	const keep = self[1];
	const upstream = open2(__clone(self[0]));
	const stage = new4();
	const core = element_core(stage, (item3) => {
		const $fs = run_element(stage, ($fq, $fr) => {
			return keep(item3, $fq, $fr);
		});
		const answer = $fs[0];
		const owner2 = $fs[1];
		return [ start_element(answer), owner2 ];
	});
	seed2(core, upstream[0]());
	const kept = __shared_new(fresh_list(core[2].v));
	const index2 = kept_index(kept.v);
	const drain2 = upstream[2];
	return [ () => {
		let out = [  ];
		let at4 = 0;
		for (const item3 of core[1].v) {
			if (__at(core[2].v, at4, "std/src/reactive/delta.vl:2040:9")) {
				out.push(__clone(item3));
			}
			at4 = at4 + 1;
		}
		return out;
	}, core_attach(core, upstream), () => {
		return kept_ops(kept, index2, resolve2(core, drain2()), (pair) => {
			let $fU = null;
			if (pair[1]) {
				$fU = [ 0, __clone(pair[0]) ];
			} else {
				$fU = [ 1 ];
			}
			return $fU;
		}, false);
	}, core_release2(core, upstream) ];
}
function memo2(self, $dz, $dA, caller) {
	return [ seal_collection(open4(self), true, $dz, $dA, caller) ];
}
function get7(self) {
	return __clone(self[0].v);
}
function get8(self) {
	return get7(self[0]);
}
function landed_one3(self, change4, $H) {
	landed2(self, [ __clone(change4) ], false, $H);
}
function splice6(self, at4, removed, inserted, $y) {
	const held2 = size2(self);
	let $gj = null;
	if (at4 > held2) {
		$gj = held2;
	} else {
		$gj = at4;
	}
	const start = $gj;
	let $gk = null;
	if (start + removed > held2) {
		$gk = held2 - start;
	} else {
		$gk = removed;
	}
	const taking = $gk;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change4 = [ 0, start, taking, arriving ];
	coming_one(self, change4);
	mutate(self, (held3) => {
		return splice2(held3, start, taking, inserted);
	});
	landed_one3(self, change4, $y);
}
function insert_at3(self, at4, value, $x) {
	return splice6(self, at4, 0, [ __clone(value) ], $x);
}
function remove_at2(self, at4, $bg) {
	return splice6(self, at4, 1, [  ], $bg);
}
function replace3(self, at4, value, $aZ) {
	const watch = self[0].v[3];
	let $gD = null;
	if (has2(self, at4)) {
		$gD = [ 2, __clone(at4) ];
	} else if (viewing(watch, (held2) => {
		return follows_items(held2[0]);
	})) {
		return false;
	} else {
		$gD = [ 1, __clone(at4), 1 ];
	}
	const change4 = $gD;
	coming_one(self, change4);
	const write = self[0].v[1][3];
	mutate(self, (held2) => {
		return write(held2, at4, value);
	});
	landed_one3(self, change4, $aZ);
	return true;
}
function get9(self, key2) {
	return __nmap_get(self[0], hash(key2));
}
function remove(self, key2) {
	__nmap_remove(self[0], hash(key2));
}
function entries(self) {
	let keys2 = __nmap_keys(self[0]);
	let values = __nmap_values(self[0]);
	let backwards = [  ];
	while (!(is_empty(keys2))) {
		const $gH = __list_pop(keys2);
		let $gI = null;
		if ($gH[0] === 0) {
			const key2 = $gH[1];
			const $gJ = __list_pop(values);
			let $gK = null;
			if ($gJ[0] === 0) {
				const value = $gJ[1];
				$gK = backwards.push([ key2, value ]);
			} else {
				$gK = undefined;
			}
			$gI = $gK;
		} else {
			$gI = undefined;
		}
		$gI;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $gL = __list_pop(backwards);
		let $gM = null;
		if ($gL[0] === 0) {
			const pair = $gL[1];
			$gM = result.push(pair);
		} else {
			$gM = undefined;
		}
		$gM;
	}
	return result;
}
function release6(self) {
	untap(self[0], self[2]);
	for (const _ of entries(self[1].v)) {
		const $gX = _;
		const _key = $gX[0];
		const id = $gX[1];
		release_id(self[0][0].v[3], id);
	}
	self[1].v = new9();
}
function index_by(self, key2, $gE) {
	const found = __shared_new(new2());
	const leaving = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const view = self[0].v[1][2];
	const key_at = (at4) => {
		let out = [ 1 ];
		peek_storage2(storage, (held2) => {
			return view(held2, at4, (item3) => {
				return out = [ 0, key2(item3) ];
			});
		});
		return out;
	};
	const map4 = (held2, at4) => {
		const id = follow_id2(storage, watch, holds, at4);
		const before = get9(found.v, held2);
		insert(found.v, held2, id);
		const $gF = before;
		if ($gF[0] === 0) {
			release_id(watch, $gF[1]);
		}
		return;
	};
	const unmap = (held2) => {
		const $gG = get9(found.v, held2);
		if ($gG[0] === 0) {
			remove(found.v, held2);
			release_id(watch, $gG[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $gN = _;
			const _held = $gN[0];
			const old = $gN[1];
			release_id(watch, old);
		}
		found.v = new9();
		let at4 = 0;
		const count3 = peek_storage2(storage, (held2) => {
			return count2(held2);
		});
		while (at4 < count3) {
			const $gO = key_at(at4);
			if ($gO[0] === 0) {
				map4($gO[1], at4);
			}
			at4 = at4 + 1;
		}
		return;
	};
	fill();
	const note = (at4, count3) => {
		let gone = [  ];
		let index3 = at4;
		while (index3 < at4 + count3) {
			const $gP = key_at(index3);
			if ($gP[0] === 0) {
				gone.push($gP[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone;
		return;
	};
	const arrive = (at4, count3) => {
		let index3 = at4;
		while (index3 < at4 + count3) {
			const $gQ = key_at(index3);
			if ($gQ[0] === 0) {
				map4($gQ[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change4) => {
		const $gR = change4;
		let $gS = null;
		if ($gR[0] === 0) {
			const at4 = $gR[1];
			const removed = $gR[2];
			const _inserted = $gR[3];
			$gS = note(at4, removed);
		} else if ($gR[0] === 3) {
			const at5 = $gR[1];
			const count3 = $gR[2];
			$gS = note(at5, count3);
		} else if ($gR[0] === 2) {
			const at6 = $gR[1];
			$gS = note(at6, 1);
		} else if ($gR[0] === 4) {
			const at7 = $gR[1];
			const _part = $gR[2];
			$gS = note(at7, 1);
		} else {
			$gS = undefined;
		}
		return $gS;
	}, (change4, stage) => {
		const $gT = stage;
		let $gU = null;
		if ($gT[0] === 0) {
			$gU = undefined;
		} else if ($gT[0] === 3) {
			return;
		} else {
			fill();
			return;
			$gU = undefined;
		}
		$gU;
		const $gV = change4;
		let $gW = null;
		if ($gV[0] === 0) {
			const at4 = $gV[1];
			const _removed = $gV[2];
			const inserted = $gV[3];
			for (const held2 of leaving.v) {
				unmap(held2);
			}
			arrive(at4, inserted);
			$gW = undefined;
		} else if ($gV[0] === 1) {
			const at5 = $gV[1];
			const count3 = $gV[2];
			$gW = arrive(at5, count3);
		} else if ($gV[0] === 3) {
			const _at = $gV[1];
			const _count = $gV[2];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			$gW = undefined;
		} else if ($gV[0] === 2) {
			const at6 = $gV[1];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			arrive(at6, 1);
			$gW = undefined;
		} else if ($gV[0] === 4) {
			const at7 = $gV[1];
			const _part = $gV[2];
			for (const held5 of leaving.v) {
				unmap(held5);
			}
			arrive(at7, 1);
			$gW = undefined;
		} else if ($gV[0] === 5) {
			const _from = $gV[1];
			const _to = $gV[2];
			$gW = undefined;
		} else {
			$gW = fill();
		}
		return $gW;
	});
	const index2 = [ __clone(self), found, tap2 ];
	defer_to_owner(() => {
		return release6(index2);
	}, $gE);
	return index2;
}
function map3(self, fn) {
	const $gY = self;
	let $gZ = null;
	if ($gY[0] === 0) {
		const x = $gY[1];
		$gZ = [ 0, fn(x) ];
	} else {
		$gZ = [ 1 ];
	}
	return $gZ;
}
function find(self, key2) {
	const lens2 = __clone(self[0]);
	return map3(viewing2(self[1], (held2) => {
		return get9(held2, key2);
	}), (id) => {
		return slot_of(lens2, id);
	});
}
function eq7(self, b2) {
	const $hc = self;
	let $hf = null;
	if ($hc[0] === 0) {
		const $hd = b2;
		let $he = null;
		if ($hd[0] === 0) {
			$he = eq5($hc[1], $hd[1]);
		} else {
			$he = false;
		}
		$hf = $he;
	} else {
		const $hg = b2;
		$hf = $hg[0] === 1;
	}
	return $hf;
}
function remove_at3(self, at4, $bg) {
	return splice4(self, at4, 1, [  ], $bg);
}
function index(self) {
	return [ __clone(self) ];
}
function get10(self) {
	return at(self[0]);
}
function shape_cell(self) {
	const watch = self[0].v[3];
	const $hi = watch.v[2];
	const cell = __clone($hi[1]);
	if ($hi[0] === 0) {
		return cell;
	}
	const cell2 = new8(0, "std/src/reactive/lens.vl:1031:26");
	watch.v[2] = [ 0, __clone(cell2) ];
	return cell2;
}
function on_settle7(self, subscriber) {
	return on_settle(shape_cell(self[0][0]), subscriber);
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
function release7(self) {
	untap(self[0], self[3]);
	for (const id of self[1].v) {
		release_id(self[0][0].v[3], id);
	}
	self[1].v = [  ];
}
function rows(self, $hj) {
	const log = new10();
	const held2 = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const fill = () => {
		let ids = [  ];
		let at4 = 0;
		const count3 = peek_storage2(storage, (list2) => {
			return count2(list2);
		});
		while (at4 < count3) {
			ids.push(follow_id(storage, watch, holds, at4));
			at4 = at4 + 1;
		}
		return ids;
	};
	const leave = (at4, count3) => {
		let gone = [  ];
		let taken = 0;
		while (taken < count3) {
			const id = __remove_at(held2.v, at4, "std/src/reactive/lens.vl:1839:27");
			release_id(watch, id);
			gone.push(id);
			taken = taken + 1;
		}
		return gone;
	};
	const arrive = (at4, count3) => {
		let arrived = [  ];
		let index2 = at4;
		while (index2 < at4 + count3) {
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
	}, (change4, _stage) => {
		const $hk = change4;
		let $hl = null;
		if ($hk[0] === 0) {
			const at4 = $hk[1];
			const removed = $hk[2];
			const inserted = $hk[3];
			const gone = leave(at4, removed);
			record(log, [ 0, at4, gone, arrive(at4, inserted) ]);
			$hl = undefined;
		} else if ($hk[0] === 1) {
			const at5 = $hk[1];
			const count3 = $hk[2];
			$hl = record(log, [ 0, at5, [  ], arrive(at5, count3) ]);
		} else if ($hk[0] === 3) {
			const at6 = $hk[1];
			const count4 = $hk[2];
			$hl = record(log, [ 0, at6, leave(at6, count4), [  ] ]);
		} else if ($hk[0] === 5) {
			const from = $hk[1];
			const to = $hk[2];
			const id = __remove_at(held2.v, from, "std/src/reactive/lens.vl:1867:28");
			__insert_at(held2.v, to, id, "std/src/reactive/lens.vl:1868:19");
			record(log, [ 3, from, 1, to ]);
			$hl = undefined;
		} else if ($hk[0] === 6) {
			for (const id2 of held2.v) {
				release_id(watch, id2);
			}
			held2.v = fill();
			record(log, [ 2, __clone(held2.v) ]);
			$hl = undefined;
		} else {
			$hl = undefined;
		}
		return $hl;
	});
	const feed4 = [ __clone(self), held2, __clone(log), tap2 ];
	defer_to_owner(() => {
		return release7(feed4);
	}, $hj);
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
function tapped(self) {
	return self[0].v[4].length;
}
function comp(body) {
	const scope = new4();
	const result = body(scope);
	return [ ...result, scope ];
}
function find2(self, key2) {
	return get9(self, key2);
}
function store(self, key2, value) {
	insert(self, key2, value);
}
function keyed(value) {
	return over3(value, new7(), [ (held2, key2) => {
		const $hq = find2(held2, key2);
		return $hq[0] === 0;
	}, (held2, key2) => {
		return find2(held2, key2);
	}, (held2, key2, read) => {
		const $hr = find2(held2, key2);
		const item3 = [ $hr[1] ];
		if ($hr[0] === 0) {
			read([ item3, 0 ]);
		}
		return;
	}, (held2, key2, value2) => {
		return store(held2, key2, value2);
	}, (held2, key2, with2) => {
		const $hs = find2(held2, key2);
		if ($hs[0] === 0) {

		} else {
			return;
		}
		let item3 = [ $hs[1] ];
		with2([ item3, 0 ]);
		store(held2, key2, item3[0]);
		return;
	} ]);
}
function follows_items2(self) {
	return false;
}
function follow5(self, at4) {
	const $hx = get9(self[0], at4);
	if ($hx[0] === 0) {
		return $hx[1];
	}
	const slot7 = as_i32(self[1].length);
	self[1].push(__clone(at4));
	insert(self[0], at4, slot7);
	return slot7;
}
function follow_id3(storage, watch, holds, at4) {
	if (viewing2(watch, (held2) => {
		return follows_items2(held2[0]);
	}) && !(peek_storage2(storage, (held2) => {
		return holds(held2, at4);
	}))) {
		return -(1);
	}
	const id = follow5(watch.v[0], at4);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] + 1;
	return id;
}
function follow6(self, at4) {
	const core = self[0];
	return slot_of(self, follow_id3(core.v[0], core.v[3], core.v[1][0], at4));
}
function release8(self, slot7) {
	const $hA = __list_get(self[1], as_usize(slot7));
	if ($hA[0] === 0) {
		remove(self[0], $hA[1]);
	}
}
function release_id2(watch, id) {
	if (id < 0) {
		return;
	}
	const entry = as_usize(id);
	const left = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1071:13")[2] - 1;
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1072:2")[2] = left;
	if (left <= 0) {
		release8(watch.v[0], id);
	}
}
function release9(self) {
	release_id2(self[0][0].v[3], self[1]);
}
function slot3(self, at4, $b) {
	const handle = follow6(self, at4);
	defer_to_owner(() => {
		return release9(handle);
	}, $b);
	return handle;
}
function locate3(self, slot7) {
	return __list_get(self[1], as_usize(slot7));
}
function at3(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][0].v[3], (held2) => {
		return locate3(held2[0], self[1]);
	});
}
function get12(self) {
	const $hD = at3(self);
	if ($hD[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek2(self[0], (held2) => {
		return read(held2, $hD[1]);
	});
}
function on_settle8(self, subscriber) {
	return on_settle(slot_cell(self[0][0].v[3], self[1]), subscriber);
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
function apply3(self, change4, wakes) {
	const $hS = change4;
	let $hT = null;
	if ($hS[0] === 0) {
		const key2 = $hS[1];
		const _removed = $hS[2];
		const _inserted = $hS[3];
		const $hU = get9(self[0], key2);
		if ($hU[0] === 0) {
			wakes.push([ 0, $hU[1] ]);
		}
		$hT = undefined;
	} else if ($hS[0] === 1) {
		const key3 = $hS[1];
		const _count = $hS[2];
		const $hV = get9(self[0], key3);
		if ($hV[0] === 0) {
			wakes.push([ 0, $hV[1] ]);
		}
		$hT = undefined;
	} else if ($hS[0] === 2) {
		const key4 = $hS[1];
		const $hW = get9(self[0], key4);
		if ($hW[0] === 0) {
			wakes.push([ 0, $hW[1] ]);
		}
		$hT = undefined;
	} else if ($hS[0] === 3) {
		const key5 = $hS[1];
		const _count2 = $hS[2];
		const $hX = get9(self[0], key5);
		if ($hX[0] === 0) {
			wakes.push([ 0, $hX[1] ]);
		}
		$hT = undefined;
	} else if ($hS[0] === 4) {
		const key6 = $hS[1];
		const part3 = $hS[2];
		const $hY = get9(self[0], key6);
		if ($hY[0] === 0) {
			wakes.push([ 1, $hY[1], part3 ]);
		}
		$hT = undefined;
	} else if ($hS[0] === 5) {
		const _from = $hS[1];
		const _to = $hS[2];
		$hT = undefined;
	} else {
		let slot7 = 0;
		const held2 = as_i32(self[1].length);
		while (slot7 < held2) {
			wakes.push([ 0, slot7 ]);
			slot7 = slot7 + 1;
		}
		$hT = undefined;
	}
	return $hT;
}
function landed3(self, changes, late, $I) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index2 = 0;
	for (const change4 of changes) {
		const $hQ = change4;
		let $hR = null;
		if ($hQ[0] === 0) {
			const _at = $hQ[1];
			const removed = $hQ[2];
			const inserted = $hQ[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$hR = undefined;
		} else if ($hQ[0] === 1) {
			const _at2 = $hQ[1];
			const _count = $hQ[2];
			$hR = resized = true;
		} else if ($hQ[0] === 3) {
			const _at3 = $hQ[1];
			const _count2 = $hQ[2];
			$hR = resized = true;
		} else if ($hQ[0] === 6) {
			$hR = resized = true;
		} else if ($hQ[0] === 5) {
			const _from = $hQ[1];
			const _to = $hQ[2];
			$hR = shaped = true;
		} else {
			$hR = undefined;
		}
		$hR;
		if (watch.v[1].length > 0) {
			apply3(watch.v[0], change4, wakes);
		}
		let $ia = null;
		if (self[0].v[4].length > 0) {
			let $hZ = null;
			if (!(late)) {
				$hZ = [ 0 ];
			} else if (!(several)) {
				$hZ = [ 1 ];
			} else if (index2 === 0) {
				$hZ = [ 2 ];
			} else {
				$hZ = [ 3 ];
			}
			const stage = $hZ;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change4, stage);
			}
			$ia = undefined;
		}
		$ia;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $ib = null;
	if (shaped || resized) {
		$ib = watch.v[2];
	} else {
		$ib = [ 1 ];
	}
	const shape = __clone($ib);
	let $ic = null;
	if (resized) {
		$ic = watch.v[3];
	} else {
		$ic = [ 1 ];
	}
	const sized = __clone($ic);
	let $id = undefined;
	let $ie = undefined;
	if (wakes.length === 0 && (($id = shape), $id[0] === 1) && (($ie = sized), $ie[0] === 1)) {
		notify(published, $I);
		return;
	}
	settle_together(($ao) => {
		for (const wake2 of wakes) {
			const $if = wake2;
			let $ig = null;
			if ($if[0] === 0) {
				const slot7 = $if[1];
				$ig = wake_all(watch, slot7, [ 0, $ao ]);
			} else if ($if[0] === 2) {
				const slot8 = $if[1];
				$ig = wake_all(watch, slot8, [ 0, $ao ]);
			} else {
				const slot9 = $if[1];
				const part3 = $if[2];
				$ig = wake_part(watch, slot9, part3, [ 0, $ao ]);
			}
			$ig;
		}
		const $ij = sized;
		if ($ij[0] === 0) {
			notify($ij[1], [ 0, $ao ]);
		}
		const $ik = shape;
		if ($ik[0] === 0) {
			notify($ik[1], [ 0, $ao ]);
		}
		notify(published, [ 0, $ao ]);
		return;
	}, $I);
}
function landed_one4(self, change4, $H) {
	landed3(self, [ __clone(change4) ], false, $H);
}
function put2(self, key2, value, $hL) {
	const had = peek2(self, (held2) => {
		const $hM = find2(held2, key2);
		return $hM[0] === 0;
	});
	let $hN = null;
	if (had) {
		$hN = [ 2, __clone(key2) ];
	} else {
		$hN = [ 1, __clone(key2), 1 ];
	}
	const change4 = $hN;
	coming_one(self, change4);
	mutate(self, (held2) => {
		return store(held2, key2, value);
	});
	landed_one4(self, change4, $hL);
}
function drop(self, key2) {
	remove(self, key2);
}
function remove2(self, key2, $il) {
	const had = peek2(self, (held2) => {
		const $im = find2(held2, key2);
		return $im[0] === 0;
	});
	if (!(had)) {
		return;
	}
	coming_one(self, [ 3, __clone(key2), 1 ]);
	mutate(self, (held2) => {
		return drop(held2, key2);
	});
	landed_one4(self, [ 3, __clone(key2), 1 ], $il);
}
function push3(self, value, $bh) {
	return splice6(self, size2(self), 0, [ __clone(value) ], $bh);
}
function eq8(self, other) {
	const $ip = [ self, other ];
	let $iq = null;
	if ($ip[0][0] === 0 && $ip[1][0] === 0) {
		const s0 = $ip[0][1];
		const s1 = $ip[0][2];
		const s2 = $ip[0][3];
		const o0 = $ip[1][1];
		const o1 = $ip[1][2];
		const o2 = $ip[1][3];
		$iq = eq2(s0, o0) && s1 === o1 && s2 === o2;
	} else if ($ip[0][0] === 1 && $ip[1][0] === 1) {
		const s02 = $ip[0][1];
		const s12 = $ip[0][2];
		const o02 = $ip[1][1];
		const o12 = $ip[1][2];
		$iq = eq2(s02, o02) && s12 === o12;
	} else if ($ip[0][0] === 2 && $ip[1][0] === 2) {
		const s03 = $ip[0][1];
		const o03 = $ip[1][1];
		$iq = eq2(s03, o03);
	} else if ($ip[0][0] === 3 && $ip[1][0] === 3) {
		const s04 = $ip[0][1];
		const s13 = $ip[0][2];
		const o04 = $ip[1][1];
		const o13 = $ip[1][2];
		$iq = eq2(s04, o04) && s13 === o13;
	} else if ($ip[0][0] === 4 && $ip[1][0] === 4) {
		const s05 = $ip[0][1];
		const s14 = $ip[0][2];
		const o05 = $ip[1][1];
		const o14 = $ip[1][2];
		$iq = eq2(s05, o05) && s14 === o14;
	} else if ($ip[0][0] === 5 && $ip[1][0] === 5) {
		const s06 = $ip[0][1];
		const s15 = $ip[0][2];
		const o06 = $ip[1][1];
		const o15 = $ip[1][2];
		$iq = eq2(s06, o06) && eq2(s15, o15);
	} else if ($ip[0][0] === 6 && $ip[1][0] === 6) {
		$iq = true;
	} else {
		$iq = false;
	}
	return $iq;
}
function eq9(self, b2) {
	if (self.length !== b2.length) {
		return false;
	}
	let index2 = 0;
	for (const item3 of self) {
		if (!(eq8(item3, __at(b2, index2, "std/src/compare.vl:147:17")))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function on_settle9(self, subscriber) {
	return on_settle(sized_cell(self[0]), subscriber);
}
function attach_observer7(self, observer, immediately) {
	const subscription = on_settle9(self, mint_subscriber(() => {
		return observer(get5(self));
	}));
	if (immediately) {
		observer(get5(self));
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
function on_settle10(self, subscriber) {
	return on_settle(shape_cell(self[0][0]), subscriber);
}
function attach_observer8(self, observer, immediately) {
	const subscription = on_settle10(self, mint_subscriber(() => {
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
	const $iz = op;
	let $iA = null;
	if ($iz[0] === 0) {
		const at4 = $iz[1];
		const gone = $iz[2];
		const arrived = $iz[3];
		$iA = [ 0, at4, handles(self, gone), handles(self, arrived) ];
	} else if ($iz[0] === 1) {
		const at5 = $iz[1];
		const old = $iz[2];
		const new11 = $iz[3];
		$iA = [ 1, at5, slot_of(self[0], old), slot_of(self[0], new11) ];
	} else if ($iz[0] === 2) {
		const all5 = $iz[1];
		$iA = [ 2, handles(self, all5) ];
	} else {
		const from = $iz[1];
		const count3 = $iz[2];
		const to = $iz[3];
		$iA = [ 3, from, count3, to ];
	}
	return $iA;
}
function since3(self, cursor6) {
	const $ix = since(self[2], cursor6);
	let $iy = null;
	if ($ix[0] === 0) {
		const recorded = $ix[1];
		let out = [  ];
		for (const op of recorded) {
			out.push(lift(self, op));
		}
		$iy = out;
	} else {
		$iy = [ [ 2, get11(self) ] ];
	}
	return $iy;
}
function eq10(self, other) {
	return self[1] === other[1];
}
function eq11(self, b2) {
	if (self.length !== b2.length) {
		return false;
	}
	let index2 = 0;
	for (const item3 of self) {
		if (!(eq10(item3, __at(b2, index2, "std/src/compare.vl:147:17")))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function row_arrived_reads(op) {
	const $iC = op;
	let $iD = null;
	if ($iC[0] === 0) {
		const _at = $iC[1];
		const _gone = $iC[2];
		const arrived = $iC[3];
		$iD = map2(arrived, (slot7) => {
			return get(slot7);
		});
	} else {
		$iD = [  ];
	}
	return $iD;
}
function eq12(self, b2) {
	const $iE = self;
	let $iH = null;
	if ($iE[0] === 0) {
		const $iF = b2;
		let $iG = null;
		if ($iF[0] === 0) {
			$iG = $iE[1] === $iF[1];
		} else {
			$iG = false;
		}
		$iH = $iG;
	} else {
		const $iI = b2;
		$iH = $iI[0] === 1;
	}
	return $iH;
}
function eq13(self, b2) {
	if (self.length !== b2.length) {
		return false;
	}
	let index2 = 0;
	for (const item3 of self) {
		if (!(eq12(item3, __at(b2, index2, "std/src/compare.vl:147:17")))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function index_by2(self, key2, $gE) {
	const found = __shared_new(new2());
	const leaving = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const view = self[0].v[1][2];
	const key_at = (at4) => {
		let out = [ 1 ];
		peek_storage2(storage, (held2) => {
			return view(held2, at4, (item3) => {
				return out = [ 0, key2(item3) ];
			});
		});
		return out;
	};
	const map4 = (held2, at4) => {
		const id = follow_id(storage, watch, holds, at4);
		const before = get9(found.v, held2);
		insert(found.v, held2, id);
		const $iN = before;
		if ($iN[0] === 0) {
			release_id(watch, $iN[1]);
		}
		return;
	};
	const unmap = (held2) => {
		const $iO = get9(found.v, held2);
		if ($iO[0] === 0) {
			remove(found.v, held2);
			release_id(watch, $iO[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $iP = _;
			const _held = $iP[0];
			const old = $iP[1];
			release_id(watch, old);
		}
		found.v = new9();
		let at4 = 0;
		const count3 = peek_storage2(storage, (held2) => {
			return count2(held2);
		});
		while (at4 < count3) {
			const $iQ = key_at(at4);
			if ($iQ[0] === 0) {
				map4($iQ[1], at4);
			}
			at4 = at4 + 1;
		}
		return;
	};
	fill();
	const note = (at4, count3) => {
		let gone = [  ];
		let index3 = at4;
		while (index3 < at4 + count3) {
			const $iR = key_at(index3);
			if ($iR[0] === 0) {
				gone.push($iR[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone;
		return;
	};
	const arrive = (at4, count3) => {
		let index3 = at4;
		while (index3 < at4 + count3) {
			const $iS = key_at(index3);
			if ($iS[0] === 0) {
				map4($iS[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change4) => {
		const $iT = change4;
		let $iU = null;
		if ($iT[0] === 0) {
			const at4 = $iT[1];
			const removed = $iT[2];
			const _inserted = $iT[3];
			$iU = note(at4, removed);
		} else if ($iT[0] === 3) {
			const at5 = $iT[1];
			const count3 = $iT[2];
			$iU = note(at5, count3);
		} else if ($iT[0] === 2) {
			const at6 = $iT[1];
			$iU = note(at6, 1);
		} else if ($iT[0] === 4) {
			const at7 = $iT[1];
			const _part = $iT[2];
			$iU = note(at7, 1);
		} else {
			$iU = undefined;
		}
		return $iU;
	}, (change4, stage) => {
		const $iV = stage;
		let $iW = null;
		if ($iV[0] === 0) {
			$iW = undefined;
		} else if ($iV[0] === 3) {
			return;
		} else {
			fill();
			return;
			$iW = undefined;
		}
		$iW;
		const $iX = change4;
		let $iY = null;
		if ($iX[0] === 0) {
			const at4 = $iX[1];
			const _removed = $iX[2];
			const inserted = $iX[3];
			for (const held2 of leaving.v) {
				unmap(held2);
			}
			arrive(at4, inserted);
			$iY = undefined;
		} else if ($iX[0] === 1) {
			const at5 = $iX[1];
			const count3 = $iX[2];
			$iY = arrive(at5, count3);
		} else if ($iX[0] === 3) {
			const _at = $iX[1];
			const _count = $iX[2];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			$iY = undefined;
		} else if ($iX[0] === 2) {
			const at6 = $iX[1];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			arrive(at6, 1);
			$iY = undefined;
		} else if ($iX[0] === 4) {
			const at7 = $iX[1];
			const _part = $iX[2];
			for (const held5 of leaving.v) {
				unmap(held5);
			}
			arrive(at7, 1);
			$iY = undefined;
		} else if ($iX[0] === 5) {
			const _from = $iX[1];
			const _to = $iX[2];
			$iY = undefined;
		} else {
			$iY = fill();
		}
		return $iY;
	});
	const index2 = [ __clone(self), found, tap2 ];
	defer_to_owner(() => {
		return release6(index2);
	}, $gE);
	return index2;
}
function find3(self, key2) {
	const lens2 = __clone(self[0]);
	return map3(viewing2(self[1], (held2) => {
		return get9(held2, key2);
	}), (id) => {
		return slot_of(lens2, id);
	});
}
function eq14(self, b2) {
	const $je = self;
	let $jh = null;
	if ($je[0] === 0) {
		const $jf = b2;
		let $jg = null;
		if ($jf[0] === 0) {
			$jg = eq12($je[1], $jf[1]);
		} else {
			$jg = false;
		}
		$jh = $jg;
	} else {
		const $ji = b2;
		$jh = $ji[0] === 1;
	}
	return $jh;
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
		const $jm = [ 0, cell ];
		let $jn = null;
		if ($jm[0] === 0) {
			const live = $jm[1];
			$jn = observer(live.v);
		} else {
			$jn = undefined;
		}
		return $jn;
	}));
}
function get13(self) {
	return __clone(self[0].v);
}
function attach_observer10(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get13(self));
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
function record_put(log, key2, now, leaving, late) {
	const $jA = now;
	if ($jA[0] === 0) {
		const $jC = __clone(key2);
		let $jB = null;
		if (late) {
			$jB = [ 1 ];
		} else {
			$jB = leaving.v;
		}
		record2(log, [ 0, $jC, __clone($jB), __clone($jA[1]) ]);
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
		let out = new9();
		for (const key2 of held_keys(held2)) {
			const $jD = find2(held2, key2);
			if ($jD[0] === 0) {
				insert(out, key2, $jD[1]);
			}
		}
		return out;
	});
}
function release10(self) {
	untap(self[0], self[2]);
}
function entries2(self, $jr) {
	const log = new10();
	const leaving = __shared_new([ 1 ]);
	const storage = __clone(self[0].v[0]);
	const find4 = (key2) => {
		return peek_storage2(storage, (held2) => {
			return find2(held2, key2);
		});
	};
	const tap2 = tap(self, (change4) => {
		const $js = change4;
		let $jt = null;
		if ($js[0] === 0) {
			const key2 = $js[1];
			const _removed = $js[2];
			const _inserted = $js[3];
			$jt = leaving.v = find4(key2);
		} else if ($js[0] === 3) {
			const key3 = $js[1];
			const _count = $js[2];
			$jt = leaving.v = find4(key3);
		} else if ($js[0] === 2) {
			const key4 = $js[1];
			$jt = leaving.v = find4(key4);
		} else if ($js[0] === 4) {
			const key5 = $js[1];
			const _part = $js[2];
			$jt = leaving.v = find4(key5);
		} else {
			$jt = undefined;
		}
		return $jt;
	}, (change4, stage) => {
		const $ju = stage;
		const late = !($ju[0] === 0);
		const $jv = change4;
		let $jw = null;
		if ($jv[0] === 1) {
			const key2 = $jv[1];
			const _count = $jv[2];
			const $jx = find4(key2);
			if ($jx[0] === 0) {
				record2(log, [ 0, __clone(key2), [ 1 ], $jx[1] ]);
			}
			$jw = undefined;
		} else if ($jv[0] === 0) {
			const key3 = $jv[1];
			const _removed = $jv[2];
			const _inserted = $jv[3];
			$jw = record_put(log, key3, find4(key3), leaving, late);
		} else if ($jv[0] === 2) {
			const key4 = $jv[1];
			$jw = record_put(log, key4, find4(key4), leaving, late);
		} else if ($jv[0] === 4) {
			const key5 = $jv[1];
			const _part = $jv[2];
			$jw = record_put(log, key5, find4(key5), leaving, late);
		} else if ($jv[0] === 3) {
			const key6 = $jv[1];
			const _count2 = $jv[2];
			if (late) {
				record2(log, [ 2, entries_of(storage) ]);
			} else {
				const $jI = leaving.v;
				if ($jI[0] === 0) {
					record2(log, [ 1, __clone(key6), $jI[1] ]);
				}
			}
			$jw = undefined;
		} else if ($jv[0] === 5) {
			const _from = $jv[1];
			const _to = $jv[2];
			$jw = undefined;
		} else {
			$jw = record2(log, [ 2, entries_of(storage) ]);
		}
		return $jw;
	});
	const feed4 = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release10(feed4);
	}, $jr);
	return feed4;
}
function cursor4(self) {
	return cursor(self[1]);
}
function update2(self, change4, $cb) {
	const $jJ = self[0].v[6];
	let $jK = null;
	if ($jJ[0] === 0) {
		$jK = [ 0, peek2(self, (held2) => {
			return __clone(held2);
		}) ];
	} else {
		$jK = [ 1 ];
	}
	const before = $jK;
	let out = [ 1 ];
	mutate(self, (held2) => {
		return out = [ 0, change4(held2) ];
	});
	const $jL = out;
	let $jM = null;
	if ($jL[0] === 0) {
		const result = __clone($jL[1][0]);
		const changes = __clone($jL[1][1]);
		audit(self, before, changes);
		landed3(self, changes, true, $cb);
		$jM = result;
	} else {
		$jM = (() => {
			throw __panic("lens: an update\'s write never ran", "std/src/reactive/lens.vl:776:12");
		})();
	}
	return $jM;
}
function all3(self) {
	return entries_of(self[0].v[0]);
}
function since4(self, cursor6) {
	const $jW = since(self[1], cursor6);
	let $jX = null;
	if ($jW[0] === 0) {
		const recorded = $jW[1];
		$jX = recorded;
	} else {
		$jX = [ [ 2, all3(self[0]) ] ];
	}
	return $jX;
}
function over4(value, $jY) {
	return [ sequence(value, $jY) ];
}
function over5(value) {
	return [ keyed(value) ];
}
function slot4(self, at4, $jZ) {
	return slot(self[0], at4, $jZ);
}
function splice7(self, at4, removed, inserted, $ka) {
	splice3(self[0], at4, removed, inserted, $ka);
}
function insert_at4(self, at4, value, $x) {
	return splice7(self, at4, 0, [ __clone(value) ], $x);
}
function size4(self) {
	return size(self[0]);
}
function put3(self, key2, value, $kb) {
	return put2(self[0], key2, value, $kb);
}
function slot5(self, key2, $kc) {
	return slot3(self[0], key2, $kc);
}
function length2(self) {
	return [ __clone(self) ];
}
function length3(self) {
	return length2(self[0]);
}
function get14(self) {
	return peek2(self[0], (held2) => {
		return held_keys(held2).length;
	});
}
function lens(self) {
	return __clone(self[0]);
}
function identity(self) {
	return [ 0, __shared_identity(self[0]) ];
}
function identity2(self) {
	return identity(slot_cell(self[0][0].v[3], self[1]));
}
function push4(self, value, $bh) {
	return splice7(self, size4(self), 0, [ __clone(value) ], $bh);
}
function length4(self) {
	return length(self[0]);
}
function audit2(self, before, changes) {
	const $kr = self[0].v[6];
	if ($kr[0] === 0) {

	} else {
		return;
	}
	const $ks = before;
	if ($ks[0] === 0) {

	} else {
		return;
	}
	const problem = peek(self, (held2) => {
		return $kr[1]($ks[1], held2, changes);
	});
	const $kt = problem;
	if ($kt[0] === 0) {
		(() => {
			throw __panic("lens: the changes do not match the write: " + $kt[1], "std/src/reactive/lens.vl:806:4");
		})();
	}
}
function update3(self, change4, $cb) {
	const $kn = self[0].v[6];
	let $ko = null;
	if ($kn[0] === 0) {
		$ko = [ 0, peek2(self, (held2) => {
			return __clone(held2);
		}) ];
	} else {
		$ko = [ 1 ];
	}
	const before = $ko;
	let out = [ 1 ];
	mutate(self, (held2) => {
		return out = [ 0, change4(held2) ];
	});
	const $kp = out;
	let $kq = null;
	if ($kp[0] === 0) {
		const result = __clone($kp[1][0]);
		const changes = __clone($kp[1][1]);
		audit2(self, before, changes);
		landed(self, changes, true, $cb);
		$kq = result;
	} else {
		$kq = (() => {
			throw __panic("lens: an update\'s write never ran", "std/src/reactive/lens.vl:776:12");
		})();
	}
	return $kq;
}
function key(self) {
	return self[1];
}
function lend4(self, at4, read) {
	const $ku = item(self, at4);
	const item3 = [ $ku[1] ];
	if ($ku[0] === 0) {
		read([ item3, 0 ]);
	}
}
function put4(self, at4, value, $kv) {
	splice(self, at4, 1, [ __clone(value) ], $kv);
}
function change3(self, at4, with2, $kw) {
	const $kx = item(self, at4);
	if ($kx[0] === 0) {

	} else {
		return;
	}
	let item3 = [ $kx[1] ];
	with2([ item3, 0 ]);
	put4(self, at4, item3[0], $kw);
}
function sequence4(value, $a) {
	return over3(value, new5(), [ (held2, at4) => {
		return at4 < count(held2);
	}, (held2, at4) => {
		return item(held2, at4);
	}, (held2, at4, read) => {
		return lend4(held2, at4, read);
	}, (held2, at4, value2) => {
		return put4(held2, at4, value2, $a);
	}, (held2, at4, with2) => {
		return change3(held2, at4, with2, $a);
	} ]);
}
function follow7(self, at4) {
	const core = self[0];
	return slot_of(self, follow_id2(core.v[0], core.v[3], core.v[1][0], at4));
}
function slot6(self, at4, $b) {
	const handle = follow7(self, at4);
	defer_to_owner(() => {
		return release4(handle);
	}, $b);
	return handle;
}
function size5(self) {
	return peek2(self, (held2) => {
		return count(held2);
	});
}
function shift2(self, from, to, $kJ) {
	const $kK = item(self, from);
	if ($kK[0] === 0) {

	} else {
		return;
	}
	splice(self, from, 1, [  ], $kJ);
	splice(self, to, 0, [ $kK[1] ], $kJ);
}
function move_item2(self, from, to, $cJ) {
	const held2 = size5(self);
	if (from >= held2 || from === to) {
		return;
	}
	let $kI = null;
	if (to >= held2) {
		$kI = held2 - 1;
	} else {
		$kI = to;
	}
	const landing = $kI;
	const change4 = [ 5, from, landing ];
	coming_one(self, change4);
	mutate(self, (held3) => {
		return shift2(held3, from, landing, $cJ);
	});
	landed_one3(self, change4, $cJ);
}
function all_of2(storage) {
	return peek_storage2(storage, (held2) => {
		let out = [  ];
		let at4 = 0;
		const count3 = count(held2);
		while (at4 < count3) {
			const $kZ = item(held2, at4);
			if ($kZ[0] === 0) {
				out.push($kZ[1]);
			}
			at4 = at4 + 1;
		}
		return out;
	});
}
function all4(self) {
	return all_of2(self[0].v[0]);
}
function part2(self, name, read) {
	return [ __clone(self), part_cell(self[0][0].v[3], self[1], name), read ];
}
function edit3(self, at4, part3, change4, $bC) {
	if (!(has2(self, at4))) {
		return false;
	}
	coming_one(self, [ 4, __clone(at4), part3 ]);
	const modify = self[0].v[1][4];
	mutate(self, (held2) => {
		return modify(held2, at4, change4);
	});
	landed_one3(self, [ 4, __clone(at4), part3 ], $bC);
	return true;
}
function edit4(self, part3, change4, $bA) {
	const $lo = at(self);
	if ($lo[0] === 0) {

	} else {
		return false;
	}
	return edit3(self[0], $lo[1], part3, change4, $bA);
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
on_change(__clone(bar), (value, $q) => {
	return heard.v.push(__clone(value));
}, "lens-laws.vl:88:19");
insert_at(list, 0, "qux", [ 1 ]);
assert(eq3(get(bar), [ 0, "bar" ]), "L1: the slot still reads its item", "lens-laws.vl:90:2");
assert(eq5(at(bar), [ 0, 2 ]), "L1: and knows where it went", "lens-laws.vl:91:2");
assert(peek2(list, (held2) => {
	return __at(held2, 1, "lens-laws.vl:92:26");
}) === "foo", "position 1 is now foo", "lens-laws.vl:92:2");
assert(heard.v.length === 0, "L4: an insert above a slot does not wake it", "lens-laws.vl:93:2");
const rows2 = sequence([ "a", "b", "c" ], [ 1 ]);
const c = slot(rows2, 2, [ 1 ]);
const runs = __shared_new(0);
on_change(__clone(c), (_value, $aU) => {
	const $aV = [ runs, "v" ];
	return runs.v = $aV[0][$aV[1]] + 1;
}, "lens-laws.vl:100:17");
batch(($aW) => {
	insert_at(rows2, 0, "z", [ 0, $aW ]);
	set(c, "C!", [ 0, $aW ]);
	set(c, "C!!", [ 0, $aW ]);
	assert(runs.v === 0, "L3: nothing runs inside the turn", "lens-laws.vl:105:3");
	return;
}, [ 1 ]);
assert(eq(all(rows2), [ "z", "a", "b", "C!!" ]), "L2: the writes landed on c, not on b", "lens-laws.vl:107:2");
assert(runs.v === 1, "L3: one run for the turn", "lens-laws.vl:108:2");
remove_at(rows2, 3, [ 1 ]);
assert(eq3(get(c), [ 1 ]), "L6: the slot reads None", "lens-laws.vl:112:2");
assert(runs.v === 2, "L6: and woke once to say so", "lens-laws.vl:113:2");
push(rows2, "d", [ 1 ]);
assert(eq3(get(c), [ 1 ]), "L1: a new item at the old position is not the slot\'s item", "lens-laws.vl:115:2");
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
on_change2(__clone(title), (_text, $bp) => {
	const $bq = [ titles, "v" ];
	return titles.v = $bq[0][$bq[1]] + 1;
}, "lens-laws.vl:127:17");
on_change2(status, (_text, $bw) => {
	const $bx = [ statuses, "v" ];
	return statuses.v = $bx[0][$bx[1]] + 1;
}, "lens-laws.vl:128:18");
on_change2(part(first, "title", (task) => {
	return task[0];
}), (_text, $by) => {
	const $bz = [ others, "v" ];
	return others.v = $bz[0][$bz[1]] + 1;
}, "lens-laws.vl:129:50");
edit2(second, "title", (task) => {
	return task[0] = "B";
}, [ 1 ]);
assert(eq3(get2(title), [ 0, "B" ]), "the part reads the edit", "lens-laws.vl:131:2");
assert(titles.v === 1 && statuses.v === 0 && others.v === 0, "L4: only the edited part woke", "lens-laws.vl:132:2");
insert_at2(board, 0, [ "new", "open" ], [ 1 ]);
edit2(second, "status", (task) => {
	return task[1] = "done";
}, [ 1 ]);
assert(peek2(board, (held2) => {
	return __at(held2, 2, "lens-laws.vl:138:27")[1];
}) === "done", "L2: the edit followed the row", "lens-laws.vl:138:2");
assert(titles.v === 1 && statuses.v === 1, "L4: only the status woke", "lens-laws.vl:139:2");
update(board, (held2) => {
	__replace(held2, tasks());
	return [ undefined, [ [ 6 ] ] ];
}, [ 1 ]);
assert(eq6(get3(second), [ 1 ]), "L7: a positional slot dies at a reset", "lens-laws.vl:146:2");
const whole = __shared_new(0);
const lengths = __shared_new(0);
on_change3(__clone(board), (_all, $cB) => {
	const $cC = [ whole, "v" ];
	return whole.v = $cC[0][$cC[1]] + 1;
}, "lens-laws.vl:151:17");
on_change4(length(board), (_n, $cD) => {
	const $cE = [ lengths, "v" ];
	return lengths.v = $cE[0][$cE[1]] + 1;
}, "lens-laws.vl:152:26");
replace2(board, 0, [ "x", "open" ], [ 1 ]);
assert(whole.v === 1 && lengths.v === 0, "a replace wakes the whole value, not the length", "lens-laws.vl:154:2");
push2(board, [ "y", "open" ], [ 1 ]);
assert(whole.v === 2 && lengths.v === 1, "a push wakes both", "lens-laws.vl:159:2");
const order = sequence([ "a", "b", "c", "d" ], [ 1 ]);
const d = slot(order, 3, [ 1 ]);
const d_runs = __shared_new(0);
on_change(__clone(d), (_value, $cH) => {
	const $cI = [ d_runs, "v" ];
	return d_runs.v = $cI[0][$cI[1]] + 1;
}, "lens-laws.vl:165:13");
move_item(order, 3, 0, [ 1 ]);
assert(eq(all(order), [ "d", "a", "b", "c" ]), "the move happened", "lens-laws.vl:167:2");
assert(eq5(at(d), [ 0, 0 ]) && eq3(get(d), [ 0, "d" ]) && d_runs.v === 0, "the slot went with its item, silently", "lens-laws.vl:168:2");
move_item(order, 0, 9, [ 1 ]);
assert(eq(all(order), [ "a", "b", "c", "d" ]) && eq5(at(d), [ 0, 3 ]), "a move past the end lands at the end", "lens-laws.vl:173:2");
const people2 = sequence([ "ann", "bob", "cy", "dee" ], [ 1 ]);
const bob2 = slot(people2, 1, [ 1 ]);
const dee2 = slot(people2, 3, [ 1 ]);
const cy2 = slot(people2, 2, [ 1 ]);
reconcile_by_key(people2, [ "dee", "zed", "bob", "ann" ], (name) => {
	return name[0][name[1]];
}, [ 1 ]);
assert(eq(all(people2), [ "dee", "zed", "bob", "ann" ]), "the list became the new list", "lens-laws.vl:184:2");
assert(eq5(at(bob2), [ 0, 2 ]) && eq5(at(dee2), [ 0, 0 ]), "kept items keep their slots", "lens-laws.vl:185:2");
assert(eq3(get(cy2), [ 1 ]), "an item that left ends its slot", "lens-laws.vl:186:2");
const numbers = sequence3([ 1, 2, 3, 4 ], [ 1 ]);
const doubled = memo(map(items(numbers, [ 1 ]), (n, $dx, $dy) => {
	return n * 2;
}), [ 1 ], [ 1 ], "lens-laws.vl:190:47");
const evens = memo2(filter(items(numbers, [ 1 ]), (n, $fo, $fp) => {
	return n % 2 === 0;
}), [ 1 ], [ 1 ], "lens-laws.vl:191:53");
assert(eq(get8(doubled), [ 2, 4, 6, 8 ]) && eq(get8(evens), [ 2, 4 ]), "the operators seed from the lens", "lens-laws.vl:192:2");
insert_at3(numbers, 1, 10, [ 1 ]);
remove_at2(numbers, 0, [ 1 ]);
replace3(numbers, 3, 5, [ 1 ]);
assert(eq(all2(numbers), [ 10, 2, 3, 5 ]), "the writes landed", "lens-laws.vl:199:2");
assert(eq(get8(doubled), [ 20, 4, 6, 10 ]), "map follows the lens", "lens-laws.vl:200:2");
assert(eq(get8(evens), [ 10, 2 ]), "filter follows the lens", "lens-laws.vl:201:2");
const tabs = sequence2(tasks(), [ 1 ]);
const by_title = index_by(tabs, (task) => {
	return task[0];
}, [ 1 ]);
const b = find(by_title, "b");
assert(eq7(map3(b, (slot7) => {
	return at(slot7);
}), [ 0, [ 0, 1 ] ]), "the index finds the slot of the item under a key", "lens-laws.vl:207:2");
insert_at2(tabs, 0, [ "first", "open" ], [ 1 ]);
remove_at3(tabs, 3, [ 1 ]);
assert(eq7(map3(find(by_title, "first"), (slot7) => {
	return at(slot7);
}), [ 0, [ 0, 0 ] ]), "an inserted item is indexed", "lens-laws.vl:213:2");
assert(eq7(map3(find(by_title, "c"), (slot7) => {
	return at(slot7);
}), [ 1 ]), "a deleted item leaves the index", "lens-laws.vl:217:2");
assert(eq7(map3(find(by_title, "b"), (slot7) => {
	return at(slot7);
}), [ 0, [ 0, 2 ] ]), "and the others are found where they are now", "lens-laws.vl:218:2");
edit(tabs, 2, "title", (task) => {
	return task[0] = "B";
}, [ 1 ]);
assert(eq7(map3(find(by_title, "b"), (slot7) => {
	return at(slot7);
}), [ 1 ]) && eq7(map3(find(by_title, "B"), (slot7) => {
	return at(slot7);
}), [ 0, [ 0, 2 ] ]), "an edit that changes the key re-indexes the item", "lens-laws.vl:223:2");
const places = __shared_new([  ]);
const first_tab = slot2(tabs, 0, [ 1 ]);
on_change5(index(first_tab), (place, $hh) => {
	return places.v.push(__clone(place));
}, "lens-laws.vl:235:29");
insert_at2(tabs, 0, [ "zero", "open" ], [ 1 ]);
assert(places.v.length === 1 && eq5(at(first_tab), [ 0, 1 ]), "a slot\'s position is a source that follows the shifts", "lens-laws.vl:237:2");
const held = sequence([ "p", "q", "r" ], [ 1 ]);
const one2 = slot(held, 1, [ 1 ]);
const two = slot(held, 1, [ 1 ]);
assert(one2[1] === two[1], "two asks for one item are one slot", "lens-laws.vl:246:2");
release3(one2);
assert(eq3(get(two), [ 0, "q" ]) && eq5(at(two), [ 0, 1 ]), "the slot lives while a handle holds it", "lens-laws.vl:248:2");
release3(two);
assert(eq3(get(two), [ 1 ]) && eq5(at(two), [ 1 ]), "the last release ends it", "lens-laws.vl:250:2");
insert_at(held, 0, "o", [ 1 ]);
assert(eq3(get(slot(held, 2, [ 1 ])), [ 0, "q" ]) && slot(held, 2, [ 1 ])[1] !== one2[1], "a new ask makes a new slot; the old entry is free", "lens-laws.vl:252:2");
const feed = rows(held, [ 1 ]);
assert(get11(feed).length === 4, "rows() holds a slot per item", "lens-laws.vl:257:2");
release7(feed);
assert(tapped(held) === 0, "releasing the feed lets its tap and its slots go", "lens-laws.vl:259:2");
const scoped = sequence([ "m", "n" ], [ 1 ]);
const $hn = comp(($hm) => {
	return [ slot(scoped, 1, [ 0, $hm ]), rows(scoped, [ 0, $hm ]) ];
});
const inner = $hn[0];
const feed2 = $hn[1];
const owner = $hn[2];
assert(eq3(get(inner), [ 0, "n" ]) && tapped(scoped) === 1, "alive while the owner is", "lens-laws.vl:264:2");
dispose2(owner);
assert(eq3(get(inner), [ 1 ]) && tapped(scoped) === 0, "released with the owner", "lens-laws.vl:266:2");
assert(get11(feed2).length === 0, "the feed let its slots go", "lens-laws.vl:267:2");
const one_write = sequence3([ 1, 2, 3 ], [ 1 ]);
const feed3 = items(one_write, [ 1 ]);
const cursor5 = cursor2(feed3);
splice6(one_write, 0, 1, [ 9 ], [ 1 ]);
const ops = since2(feed3, cursor5);
assert(ops.length === 1, "a replacing splice is one op", "lens-laws.vl:277:2");
const $ho = __at(ops, 0, "lens-laws.vl:279:3");
assert($ho[0] === 0 && $ho[1] === 0 && eq($ho[2], [ 1 ]) && eq($ho[3], [ 9 ]), "and it carries what left and what arrived", "lens-laws.vl:278:2");
splice6(one_write, 1, 2, [ 7, 8, 6 ], [ 1 ]);
const again = since2(feed3, cursor5);
let $hp = undefined;
assert(again.length === 1 && (($hp = __at(again, 0, "lens-laws.vl:289:7")), $hp[0] === 0) && $hp[1] === 1 && eq($hp[2], [ 2, 3 ]) && eq($hp[3], [ 7, 8, 6 ]), "so is a splice that takes and inserts different counts", "lens-laws.vl:287:2");
const ages = new9();
const people = keyed(ages);
const ada = slot3(people, "ada", [ 1 ]);
const bob = slot3(people, "bob", [ 1 ]);
const ada_runs = __shared_new(0);
const bob_runs = __shared_new(0);
on_change6(__clone(ada), (_age, $hB) => {
	const $hC = [ ada_runs, "v" ];
	return ada_runs.v = $hC[0][$hC[1]] + 1;
}, "lens-laws.vl:303:15");
on_change6(bob, (_age, $hJ) => {
	const $hK = [ bob_runs, "v" ];
	return bob_runs.v = $hK[0][$hK[1]] + 1;
}, "lens-laws.vl:304:15");
assert(eq3(get12(ada), [ 1 ]), "a key\'s slot exists before the key", "lens-laws.vl:305:2");
put2(people, "ada", 36, [ 1 ]);
assert(eq3(get12(ada), [ 0, 36 ]) && ada_runs.v === 1 && bob_runs.v === 0, "only ada\'s slot woke", "lens-laws.vl:307:2");
remove2(people, "ada", [ 1 ]);
assert(eq3(get12(ada), [ 1 ]) && ada_runs.v === 2, "the slot reads the removal", "lens-laws.vl:312:2");
put2(people, "ada", 37, [ 1 ]);
assert(eq3(get12(ada), [ 0, 37 ]) && ada_runs.v === 3, "and follows the key when it returns", "lens-laws.vl:314:2");
const spliced = sequence3([ 1, 2, 3, 4 ], [ 1 ]);
const heard_changes = __shared_new([  ]);
const before_changes = __shared_new(0);
tap(spliced, (_change) => {
	const $in = [ before_changes, "v" ];
	return before_changes.v = $in[0][$in[1]] + 1;
}, (change4, stage) => {
	const $io = stage;
	assert($io[0] === 0, "a shape write is exact", "lens-laws.vl:321:3");
	heard_changes.v.push(__clone(change4));
	return;
});
splice6(spliced, 1, 2, [ 7, 8, 9 ], [ 1 ]);
push3(spliced, 5, [ 1 ]);
remove_at2(spliced, 0, [ 1 ]);
const one_each = [ [ 0, 1, 2, 3 ], [ 0, 5, 0, 1 ], [ 0, 0, 1, 0 ] ];
assert(eq9(heard_changes.v, one_each), "R16: each splice is one Splice change", "lens-laws.vl:334:2");
assert(before_changes.v === 3, "and `before` heard each once, ahead of the write", "lens-laws.vl:335:2");
const table = sequence([ "a", "b", "c", "d", "e" ], [ 1 ]);
const b_slot = slot(table, 1, [ 1 ]);
const e_slot = slot(table, 4, [ 1 ]);
splice3(table, 1, 2, [ "x", "y", "z" ], [ 1 ]);
assert(eq3(get(b_slot), [ 1 ]) && eq3(get(e_slot), [ 0, "e" ]) && eq5(at(e_slot), [ 0, 5 ]), "the splice moved the slots in one step", "lens-laws.vl:343:2");
assert(eq3(get(slot(table, 1, [ 1 ])), [ 0, "x" ]), "an arrival gets a slot of its own", "lens-laws.vl:347:2");
const sizes = __shared_new(0);
const shapes = __shared_new(0);
on_change7(length(table), (_n, $ir) => {
	const $is = [ sizes, "v" ];
	return sizes.v = $is[0][$is[1]] + 1;
}, "lens-laws.vl:352:29");
on_change8(index(slot(table, 0, [ 1 ])), (_at, $iu) => {
	const $iv = [ shapes, "v" ];
	return shapes.v = $iv[0][$iv[1]] + 1;
}, "lens-laws.vl:353:37");
splice3(table, 0, 1, [ "A" ], [ 1 ]);
assert(sizes.v === 0 && shapes.v === 1, "an equal-count splice leaves the length asleep and moves the shape", "lens-laws.vl:355:2");
splice3(table, 0, 1, [ "A", "A2" ], [ 1 ]);
assert(sizes.v === 1 && shapes.v === 2, "a growing splice wakes both", "lens-laws.vl:360:2");
const rowed = sequence([ "p", "q", "r" ], [ 1 ]);
const rows3 = rows(rowed, [ 1 ]);
const row_cursor = cursor3(rows3);
const q_row = __clone(__at(get11(rows3), 1, "lens-laws.vl:367:14"));
splice3(rowed, 1, 1, [ "Q", "Q2" ], [ 1 ]);
const row_ops = since3(rows3, row_cursor);
assert(row_ops.length === 1, "a replacing splice is one row op", "lens-laws.vl:370:2");
const $iB = __at(row_ops, 0, "lens-laws.vl:372:3");
assert($iB[0] === 0 && $iB[1] === 1 && eq11($iB[2], [ q_row ]) && $iB[3].length === 2, "with the slot that left and the two that arrived", "lens-laws.vl:371:2");
assert(eq13(row_arrived_reads(__at(row_ops, 0, "lens-laws.vl:379:21")), [ [ 0, "Q" ], [ 0, "Q2" ] ]), "the arrived slots read the arrivals", "lens-laws.vl:378:2");
release7(rows3);
const indexed = sequence([ "k", "l" ], [ 1 ]);
const by_name = index_by2(indexed, (name) => {
	return name[0][name[1]];
}, [ 1 ]);
const l_found = find3(by_name, "l");
assert(eq14(map3(__clone(l_found), (slot7) => {
	return get(slot7);
}), [ 0, [ 0, "l" ] ]), "the index holds l\'s slot", "lens-laws.vl:388:2");
release6(by_name);
assert(eq14(map3(l_found, (slot7) => {
	return get(slot7);
}), [ 0, [ 1 ] ]) && tapped(indexed) === 0, "released, the index let its tap and its slots go", "lens-laws.vl:390:2");
const trigger = new8(0, "lens-laws.vl:397:28");
const echo = sequence3([ 0 ], [ 1 ]);
const echo_runs = __shared_new(0);
on_change9(__clone(echo), (_all, $jj) => {
	const $jk = [ echo_runs, "v" ];
	return echo_runs.v = $jk[0][$jk[1]] + 1;
}, "lens-laws.vl:400:19");
on_change10(__clone(trigger), (value, $jl) => {
	replace3(echo, 0, value, [ 1 ]);
	push3(echo, value, [ 1 ]);
	return;
}, "lens-laws.vl:401:23");
batch(($jo) => {
	return set2(trigger, 1, [ 0, $jo ], "lens-laws.vl:405:19");
}, [ 1 ]);
assert(eq(all2(echo), [ 1, 1 ]), "the observer\'s writes landed", "lens-laws.vl:406:2");
assert(echo_runs.v === 1, "and settled with the drain they ran in: one run for two writes", "lens-laws.vl:407:2");
const scores = new9();
const board2 = keyed(scores);
put2(board2, "ada", 1, [ 1 ]);
const ada_score = slot3(board2, "ada", [ 1 ]);
const ada_wakes = __shared_new(0);
on_change6(__clone(ada_score), (_score, $jp) => {
	const $jq = [ ada_wakes, "v" ];
	return ada_wakes.v = $jq[0][$jq[1]] + 1;
}, "lens-laws.vl:415:23");
const score_feed = entries2(board2, [ 1 ]);
const score_cursor = cursor4(score_feed);
update2(board2, (held2) => {
	insert(held2, "ada", 2);
	return [ undefined, [ [ 0, "ada", 1, 1 ] ] ];
}, [ 1 ]);
assert(eq3(get12(ada_score), [ 0, 2 ]) && ada_wakes.v === 1, "a keyed splice wakes the key\'s slot", "lens-laws.vl:422:2");
assert(since4(score_feed, score_cursor).length === 1, "and is one map op", "lens-laws.vl:426:2");
const wrapped = [ over4([ "x", "y" ], [ 1 ]), over5(ages_of()) ];
const y_slot = slot4(wrapped[0], 1, [ 1 ]);
insert_at4(wrapped[0], 0, "w", [ 1 ]);
assert(eq3(get(y_slot), [ 0, "y" ]) && eq5(at(y_slot), [ 0, 2 ]) && size4(wrapped[0]) === 3, "a SequenceLens is the sequence lens", "lens-laws.vl:435:2");
put3(wrapped[1], "cy", 9, [ 1 ]);
assert(eq3(get12(slot5(wrapped[1], "cy", [ 1 ])), [ 0, 9 ]) && get14(length3(wrapped[1])) === 2, "a KeyedLens is the keyed lens", "lens-laws.vl:440:2");
assert(eq(all(lens(wrapped[0])), [ "w", "x", "y" ]), "lens() hands out the full lens", "lens-laws.vl:444:2");
const lazy = sequence([ "a", "b" ], [ 1 ]);
const first_handle = slot(lazy, 1, [ 1 ]);
const second_handle = slot(lazy, 1, [ 1 ]);
const lazy_runs = __shared_new(0);
on_change(__clone(first_handle), (_value, $kh) => {
	const $ki = [ lazy_runs, "v" ];
	return lazy_runs.v = $ki[0][$ki[1]] + 1;
}, "lens-laws.vl:453:25");
on_change(__clone(second_handle), (_value, $kj) => {
	const $kk = [ lazy_runs, "v" ];
	return lazy_runs.v = $kk[0][$kk[1]] + 1;
}, "lens-laws.vl:454:26");
assert(eq3(identity2(first_handle), identity2(second_handle)), "S3: one slot, one cell, made on the first listen", "lens-laws.vl:455:2");
replace(lazy, 1, "B", [ 1 ]);
assert(lazy_runs.v === 2, "S3: both listeners ride the slot\'s one cell", "lens-laws.vl:460:2");
const counted = over4([ "a" ], [ 1 ]);
const copied = __clone(counted);
push4(counted, "b", [ 1 ]);
const counts = __shared_new(0);
on_change7(length4(copied), (_n, $kl) => {
	const $km = [ counts, "v" ];
	return counts.v = $km[0][$km[1]] + 1;
}, "lens-laws.vl:468:32");
push4(counted, "c", [ 1 ]);
assert(counts.v === 1 && get5(length4(copied)) === 3, "S3: a length asked through one handle hears a write through another", "lens-laws.vl:470:2");
const empty = [  ];
const grown = sequence(empty, [ 1 ]);
const grown_rows = rows(grown, [ 1 ]);
update3(grown, (held2) => {
	held2.push("x");
	held2.push("y");
	return [ undefined, [ [ 1, 0, 1 ], [ 1, 0, 1 ] ] ];
}, [ 1 ]);
const grown_slots = get11(grown_rows);
assert(grown_slots.length === 2 && key(__at(grown_slots, 0, "lens-laws.vl:487:29")) !== key(__at(grown_slots, 1, "lens-laws.vl:487:53")), "S3: the second change moved the slot the first change\'s tap made", "lens-laws.vl:486:2");
assert(eq3(get(__at(grown_slots, 0, "lens-laws.vl:491:3")), [ 0, "x" ]) && eq3(get(__at(grown_slots, 1, "lens-laws.vl:491:40")), [ 0, "y" ]), "S3: and each row reads its own item", "lens-laws.vl:490:2");
release7(grown_rows);
const bag = sequence4([ [ "p", "q", "r", "s" ] ], [ 1 ]);
const p_slot = slot6(bag, 0, [ 1 ]);
move_item2(bag, 0, 2, [ 1 ]);
assert(eq(all4(bag), [ "q", "r", "p", "s" ]), "S3: the default shift moved the item", "lens-laws.vl:501:2");
assert(eq5(at(p_slot), [ 0, 2 ]) && eq3(get3(p_slot), [ 0, "p" ]), "S3: and its slot went with it", "lens-laws.vl:502:2");
move_item2(bag, 3, 0, [ 1 ]);
assert(eq(all4(bag), [ "s", "q", "r", "p" ]), "S3: leftward too", "lens-laws.vl:504:2");
const p_width = part2(slot6(bag, 3, [ 1 ]), "width", (line) => {
	return line[0][line[1]].length;
});
assert(eq5(get2(p_width), [ 0, 1 ]), "S2: a part reads through a supertrait\'s default lend", "lens-laws.vl:511:2");
edit4(slot6(bag, 3, [ 1 ]), "width", (line) => {
	return line[0][line[1]] = "pp";
}, [ 1 ]);
assert(eq5(get2(p_width), [ 0, 2 ]) && eq(all4(bag), [ "s", "q", "r", "pp" ]), "S2: and an edit writes through change\'s default", "lens-laws.vl:513:2");
console.log("laws: all passed");
//# sourceMappingURL=lens-laws.mjs.map
