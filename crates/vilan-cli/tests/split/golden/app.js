function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_put(list, index, value, location) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __chunk_arm(value) {
	return Array.isArray(value) ? value[0] : -1;
}
function __chunk_load(arm, then, failed) {
	const chunks = __chunk_registry();
	if (chunks.url[arm] === undefined || chunks.loaded[arm] === true) {
		then();
		return;
	}
	let inflight = chunks.pending[arm];
	if (inflight === undefined) {
		const url = chunks.url[arm];
		const specifier = chunks.base === "" ? "./" + url : new URL(url, chunks.base).href;
		inflight = import(specifier).then(() => {
			chunks.loaded[arm] = true;
			delete chunks.pending[arm];
		}, (error) => {
			delete chunks.pending[arm];
			console.error("[vilan] route chunk " + url + " failed to load", error);
			throw error;
		});
		chunks.pending[arm] = inflight;
	}
	inflight.then(then, (error) => {
		failed(String(error));
	});
}
function __chunk_preload(arm) {
	__chunk_load(arm, () => {}, () => {});
}
function __chunk_ready(arm) {
	const chunks = __chunk_registry();
	return chunks.url[arm] === undefined || chunks.loaded[arm] === true;
}
function __chunk_registry() {
	let chunks = globalThis.__vilan_chunks;
	if (chunks === undefined) {
		let base = "";
		if (typeof document !== "undefined" && document.currentScript && document.currentScript.src) {
			base = document.currentScript.src;
		}
		chunks = { fn: Object.create(null), url: Object.create(null), loaded: Object.create(null), pending: Object.create(null), base: base };
		globalThis.__vilan_chunks = chunks;
	}
	return chunks;
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __dom_remove_run(from, before) {
	for (let node = from; node !== null && node !== before; ) {
		const next = node.nextSibling;
		node.remove();
		node = next;
	}
}
function __dom_run_holds(nodes, node) {
	for (let walk = node; walk !== null; walk = walk.parentNode) {
		if (nodes.has(walk)) return true;
	}
	return false;
}
function __dom_run_nodes(from, before) {
	const nodes = new Set();
	for (let node = from; node !== null && node !== before; node = node.nextSibling) nodes.add(node);
	return nodes;
}
function __dom_window() {
	return window;
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
function __hmr_active() {
	return typeof globalThis.__VILAN_HMR__ !== "undefined";
}
function __insert_at(list, index, value, location) {
	if (index >= 0 && index < list.length) return void list.splice(index, 0, value);
	if (index === list.length) return void list.push(value);
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __is_null(value) {
	return value === null || value === undefined;
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
function __parse_i32(text) {
	const trimmed = text.trim();
	const value = Number(trimmed);
	return /^[+-]?[0-9]+$/.test(trimmed) && value >= -2147483648 && value <= 2147483647 ? [ 0, value ] : [ 1 ];
}
function __router_path() {
	return location.pathname;
}
function __router_state() {
	return typeof history.state === "string" ? history.state : "";
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
const __vilan_chunks = __chunk_registry();
function home_page($bT, $bU) {
	return __vilan_chunks.fn.home_page($bT, $bU);
}
function docs_page(page, $bX, $bY) {
	return __vilan_chunks.fn.docs_page(page, $bX, $bY);
}
function not_found_page($cb, $cc) {
	return __vilan_chunks.fn.not_found_page($cb, $cc);
}
function hash(self) {
	return __hash(self);
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
function new2() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return is_empty(self[0].v) && is_empty(self[1].v);
}
function enqueue(turn2, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $h = null;
		if (subscriber[3]) {
			if (!(turn2[3].v.has(key))) {
				turn2[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn2, subscriber[0]);
				}
				turn2[1].v.push(__clone(subscriber));
			}
			$h = undefined;
		} else if (!(turn2[2].v.has(key))) {
			turn2[2].v.set(key, true);
			if (tracing.v) {
				trace_hooks.v[3](turn2, subscriber[0]);
			}
			let index = turn2[0].v.length;
			while (index > 0 && __at(turn2[0].v, index - 1, "std/src/reactive.vl:421:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn2[0].v, index, __clone(subscriber), "std/src/reactive.vl:424:25");
		}
		$h;
	}
	if (turn2[5].v && !(turn2[6].v) && !(turn2[4].v)) {
		turn2[6].v = true;
		queueMicrotask(() => {
			turn2[6].v = false;
			drain(turn2);
			return;
		});
	}
}
function drain(turn2) {
	if (!(turn2[4].v)) {
		turn2[4].v = true;
		draining_turns.v.push(__clone(turn2));
		__with_finally(() => {
			let budget = 100000;
			while (!(is_quiescent(turn2)) && budget > 0) {
				while (!(is_empty(turn2[1].v)) && budget > 0) {
					const derivations = turn2[1].v;
					turn2[1].v = [  ];
					turn2[3].v = new Map();
					for (const subscriber of derivations) {
						if (tracing.v) {
							trace_hooks.v[4](turn2, subscriber);
						} else if (subscriber[2].v) {
							subscriber[1]();
						}
						budget = budget - 1;
					}
				}
				const wave = turn2[0].v;
				turn2[0].v = [  ];
				turn2[2].v = new Map();
				for (const subscriber2 of wave) {
					if (tracing.v) {
						trace_hooks.v[4](turn2, subscriber2);
					} else if (subscriber2[2].v) {
						subscriber2[1]();
					}
					budget = budget - 1;
				}
			}
			return;
		}, () => {
			__list_pop(draining_turns.v);
			turn2[4].v = false;
			return;
		});
	}
}
function defer_subscriber(turn2, subscriber) {
	const $ao = turn2;
	let $ap = null;
	if ($ao[0] === 0) {
		const ambient = $ao[1];
		$ap = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $aq = last(draining_turns.v);
		let $ar = null;
		if ($aq[0] === 0) {
			const draining = $aq[1];
			$ar = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$ar = undefined;
		}
		$ap = $ar;
	}
	return $ap;
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
function queue_wakes(turn2, subscribers, traced, cause) {
	if (traced) {
		waking.v = [ 0, cause ];
		enqueue(turn2, subscribers);
		waking.v = [ 1 ];
	} else {
		enqueue(turn2, subscribers);
	}
}
function dispose(self, $aZ) {
	const $ba = $aZ;
	let $bb = null;
	if ($ba[0] === 0) {
		const established = $ba[1];
		$bb = [ 0, established ];
	} else {
		$bb = last(draining_turns.v);
	}
	const ambient = $bb;
	release_under(self, ambient);
}
function detach(handle) {
	const $av = last(releasing_turns.v);
	let $aw = null;
	if ($av[0] === 0) {
		const at_release = $av[1];
		$aw = at_release;
	} else {
		$aw = last(draining_turns.v);
	}
	const turn2 = $aw;
	release_under(handle, turn2);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ax = [ 0, handle[0] ];
	let $ay = null;
	if ($ax[0] === 0) {
		const subscribers = $ax[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$ay = undefined;
	} else {
		$ay = undefined;
	}
	$ay;
	const $az = ambient;
	let $aA = null;
	if ($az[0] === 0) {
		const turn2 = $az[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn2[0].v) {
			if (subscriber2[0] !== handle[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn2[0].v = kept_pending;
		turn2[2].v.delete(hash(handle[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn2[1].v) {
			if (subscriber3[0] !== handle[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn2[1].v = kept_derived;
		turn2[3].v.delete(hash(handle[1]));
		if (tracing.v) {
			trace_hooks.v[7](turn2, handle[1]);
		}
		$aA = undefined;
	} else {
		$aA = undefined;
	}
	$aA;
	const $aB = handle[3].v;
	let $aC = null;
	if ($aB[0] === 0) {
		const release = $aB[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aC = undefined;
	} else {
		$aC = undefined;
	}
	return $aC;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $bc = null;
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
		$bc = undefined;
	}
	return $bc;
}
function renew(self) {
	const $M = self[0].v[3];
	let $N = null;
	if ($M[0] === 0) {
		const nursery2 = __clone($M[1]);
		let $O = null;
		if (has_spawned(nursery2)) {
			$O = [ 1 ];
		} else {
			$O = [ 0, nursery2 ];
		}
		$N = $O;
	} else {
		$N = [ 1 ];
	}
	const carried = $N;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $Z = null;
	if (is_disposed(self)) {
		$Z = [ 1 ];
	} else {
		$Z = self[0].v[3];
	}
	return $Z;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $Y = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $P = held[3];
		let $Q = null;
		if ($P[0] === 0) {
			const nursery2 = $P[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$Q = undefined;
		} else {
			$Q = undefined;
		}
		$Q;
		let $X = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $S = __guarded(cleanup);
				let $T = null;
				if ($S[0] === 0) {
					const message = $S[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$T = undefined;
				} else {
					$T = undefined;
				}
				$T;
			}
			const $V = failure;
			let $W = null;
			if ($V[0] === 0) {
				const message2 = $V[1];
				$W = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$W = undefined;
			}
			$X = $W;
		}
		$Y = $X;
	}
	return $Y;
}
function get_owner($bH) {
	return $bH;
}
function register_with_owner(subscription, $aU, $aV) {
	const $aW = $aV;
	let $aX = null;
	if ($aW[0] === 0) {
		const owner = $aW[1];
		$aX = take(owner, subscription, $aU);
	} else {
		$aX = __clone(subscription);
	}
	return $aX;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker(site) {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ], site ]) ];
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $H = tracker[0].v[6];
	let $I = null;
	if ($H[0] === 0) {
		const lists = $H[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$I = undefined;
	} else {
		$I = undefined;
	}
	$I;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $ae = tracker[0].v[6];
	let $af = null;
	if ($ae[0] === 0) {
		const lists = $ae[1];
		$af = lists;
	} else {
		return;
		$af = undefined;
	}
	const lists2 = $af;
	const $ag = tracker[0].v[5];
	let $ah = null;
	if ($ag[0] === 0) {
		const target = __clone($ag[1]);
		$ah = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$ah = undefined;
	}
	$ah;
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
		const $am = reusable(held, kept, dependency[0], position);
		let $an = null;
		if ($am[0] === 0) {
			const index = $am[1];
			__at_put(kept, index, true, "std/src/reactive.vl:1715:5");
			next.push(__clone(__at(held, index, "std/src/reactive.vl:1716:15")));
			$an = undefined;
		} else {
			next.push([ __clone(dependency[0]), dependency[1](relay_for(tracker, target)) ]);
			$an = undefined;
		}
		$an;
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
function reusable(held, kept, identity, position) {
	const $ai = identity;
	let $aj = null;
	if ($ai[0] === 0) {
		const wanted = $ai[1];
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
		$aj = [ 1 ];
	} else {
		$aj = [ 1 ];
	}
	return $aj;
}
function same_identity(identity, wanted) {
	const $ak = identity;
	let $al = null;
	if ($ak[0] === 0) {
		const held = $ak[1];
		$al = held === wanted;
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
	const $as = tracker[0].v[7];
	let $at = null;
	if ($as[0] === 0) {
		const site = __clone($as[1]);
		$at = place_subscriber(relay[0], site);
	} else {
		$at = undefined;
	}
	$at;
	return relay;
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $aG = tracker[0].v[6];
	let $aH = null;
	if ($aG[0] === 0) {
		const lists = $aG[1];
		$aH = lists;
	} else {
		return;
		$aH = undefined;
	}
	const lists2 = $aH;
	let $aI = null;
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
		$aI = undefined;
	}
	return $aI;
}
function forget_reads(tracker) {
	const $aJ = tracker[0].v[6];
	let $aK = null;
	if ($aJ[0] === 0) {
		const lists = $aJ[1];
		$aK = lists;
	} else {
		return;
		$aK = undefined;
	}
	const lists2 = $aK;
	let $aL = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aL = undefined;
	}
	$aL;
	if (!(is_empty(lists2.v[1]))) {
		lists2.v[1] = [  ];
	}
}
function detach_tracker(tracker) {
	tracker[0].v[5] = [ 1 ];
	forget_reads(tracker);
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
function also_releasing(handle, release) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release();
		const $aS = previous;
		let $aT = null;
		if ($aS[0] === 0) {
			const earlier = $aS[1];
			$aT = earlier();
		} else {
			$aT = undefined;
		}
		return $aT;
	} ];
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function on_teardown(cleanup) {
	if (__hmr_active()) {
		__hmr_register_teardown(cleanup);
	}
}
function ensure_wired($b) {
	if (!(wired.v)) {
		wired.v = true;
		set(entry_signal, [ __router_path(), __router_state(), false ], $b, "std/src/web/router.vl:95:16");
		set2(path_signal, __router_path(), $b, "std/src/web/router.vl:100:15");
		__dom_window().addEventListener("popstate", () => {
			return turn([ 1 ], ($s) => {
				set(entry_signal, [ __router_path(), __router_state(), true ], [ 0, $s ], "std/src/web/router.vl:102:17");
				set2(path_signal, __router_path(), [ 0, $s ], "std/src/web/router.vl:107:16");
				return;
			});
		});
	}
}
function current_path($a) {
	ensure_wired($a);
	return path_signal;
}
function navigate(path, $bt) {
	navigate_with(path, "", $bt);
}
function navigate_with(path, state, $bu) {
	ensure_wired($bu);
	history.pushState(state, "", path);
	set(entry_signal, [ path, state, false ], $bu, "std/src/web/router.vl:172:15");
	set2(path_signal, path, $bu, "std/src/web/router.vl:173:14");
}
function segments(path) {
	let parts = [  ];
	for (const part of path.split("/")) {
		if (part !== "") {
			parts.push(part);
		}
	}
	return parts;
}
function plain_left_click(event) {
	const no_modifiers = !(event.metaKey) && !(event.ctrlKey) && !(event.shiftKey) && !(event.altKey);
	return event.button === 0 && no_modifiers;
}
function pending() {
	return chunk_pending();
}
function chunk_error() {
	return chunk_failure();
}
function view(tag) {
	let $bi = null;
	if (is_svg_tag(tag)) {
		$bi = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$bi = [ document.createElement(tag) ];
	}
	return $bi;
}
function is_svg_tag(tag) {
	const $bg = tag;
	let $bh = null;
	if ($bg === "svg") {
		$bh = true;
	} else if ($bg === "path") {
		$bh = true;
	} else if ($bg === "circle") {
		$bh = true;
	} else if ($bg === "ellipse") {
		$bh = true;
	} else if ($bg === "rect") {
		$bh = true;
	} else if ($bg === "line") {
		$bh = true;
	} else if ($bg === "polyline") {
		$bh = true;
	} else if ($bg === "polygon") {
		$bh = true;
	} else if ($bg === "g") {
		$bh = true;
	} else if ($bg === "defs") {
		$bh = true;
	} else if ($bg === "use") {
		$bh = true;
	} else if ($bg === "symbol") {
		$bh = true;
	} else if ($bg === "marker") {
		$bh = true;
	} else if ($bg === "pattern") {
		$bh = true;
	} else if ($bg === "mask") {
		$bh = true;
	} else if ($bg === "clipPath") {
		$bh = true;
	} else if ($bg === "linearGradient") {
		$bh = true;
	} else if ($bg === "radialGradient") {
		$bh = true;
	} else if ($bg === "stop") {
		$bh = true;
	} else if ($bg === "text") {
		$bh = true;
	} else if ($bg === "tspan") {
		$bh = true;
	} else if ($bg === "textPath") {
		$bh = true;
	} else if ($bg === "filter") {
		$bh = true;
	} else if ($bg === "foreignObject") {
		$bh = true;
	} else if ($bg === "feGaussianBlur") {
		$bh = true;
	} else if ($bg === "feColorMatrix") {
		$bh = true;
	} else if ($bg === "feOffset") {
		$bh = true;
	} else if ($bg === "feMerge") {
		$bh = true;
	} else if ($bg === "feMergeNode") {
		$bh = true;
	} else if ($bg === "feFlood") {
		$bh = true;
	} else if ($bg === "feComposite") {
		$bh = true;
	} else if ($bg === "feBlend") {
		$bh = true;
	} else if ($bg === "feDropShadow") {
		$bh = true;
	} else {
		$bh = false;
	}
	return $bh;
}
function text(self, content) {
	self[0].textContent = content;
	return __clone(self);
}
function class2(self, name) {
	self[0].setAttribute("class", name);
	return __clone(self);
}
function on_event(self, event, handler) {
	self[0].addEventListener(event, (dispatched) => {
		return turn([ 1 ], ($bv) => {
			return (() => {
				return handler(dispatched, $bv, [ 1 ]);
			})();
		});
	});
	return __clone(self);
}
function chunk_pending() {
	return chunk_pending_signal;
}
function chunk_failure() {
	return chunk_error_signal;
}
function set_chunk_pending(busy, $cr) {
	if (get(chunk_pending_signal) !== busy) {
		set3(chunk_pending_signal, busy, $cr, "std/src/browser/web/ui.vl:669:24");
	}
}
function clear_chunk_error($ci) {
	const $cj = get(chunk_error_signal);
	let $ck = null;
	if ($cj[0] === 0) {
		const _reason = $cj[1];
		$ck = set3(chunk_error_signal, [ 1 ], $ci, "std/src/browser/web/ui.vl:677:43");
	} else {
		$ck = undefined;
	}
	return $ck;
}
function open(parent) {
	const anchor = document.createTextNode("");
	parent[0].appendChild(anchor);
	return [ anchor, __shared_new([  ]), __shared_new([  ]) ];
}
function host(self) {
	return self[0].parentNode;
}
function retire_row(self, row, end, disposal) {
	while_leaving(row[0], end, disposal);
	__dom_remove_run(row[0], end);
}
function hold_rows(self, rows) {
	self[2].v = __clone(rows);
}
function close(self) {
	if (!(is_leaving(self[0]))) {
		for (const view2 of self[1].v) {
			view2[0].remove();
		}
		const $cM = first(self[2].v);
		let $cN = null;
		if ($cM[0] === 0) {
			const first2 = $cM[1];
			$cN = __dom_remove_run(first2[0], self[0]);
		} else {
			$cN = undefined;
		}
		$cN;
		self[0].remove();
	}
	self[1].v = [  ];
	self[2].v = [  ];
}
function close_with(self, disposal) {
	const $cK = first(self[2].v);
	let $cL = null;
	if ($cK[0] === 0) {
		const first2 = $cK[1];
		if (is_leaving(self[0])) {
			disposal();
		} else {
			while_leaving(first2[0], self[0], disposal);
		}
		$cL = undefined;
	} else {
		$cL = disposal();
	}
	$cL;
	close(self);
}
function is_leaving(node) {
	let at = 0;
	while (at < leaving_runs.v.length) {
		if (__dom_run_holds(__at(leaving_runs.v, at, "std/src/browser/web/ui.vl:975:16"), node)) {
			return true;
		}
		at = at + 1;
	}
	return false;
}
function while_leaving(from, before, disposal) {
	leaving_runs.v.push(__dom_run_nodes(from, before));
	__with_finally(disposal, () => {
		__list_pop(leaving_runs.v);
		return;
	});
}
function place(self, parent, caller) {
	parent[0].appendChild(self[0]);
}
function apply(self, parent, name, caller) {
	parent[0].setAttribute(name, self);
}
function mount_target(id) {
	const element = document.getElementById(id);
	if (__is_null(element)) {
		(() => {
			throw __panic("mount: no element with id \'" + id + "\'", "std/src/browser/web/ui.vl:2553:3");
		})();
	}
	return element;
}
function mount(id, view2) {
	const element = mount_target(id);
	element.replaceChildren();
	element.appendChild(view2[0]);
}
function mount_root(id, body) {
	const $dh = turn([ 1 ], ($dg) => {
		return comp(body);
	});
	const built = $dh[0];
	const root = $dh[1];
	mount(id, built);
	if (__hmr_active()) {
		const element = document.getElementById(id);
		on_teardown(() => {
			dispose2(root);
			element.replaceChildren();
			return;
		});
	}
	return root;
}
function parse(path) {
	const parts = segments(path);
	if (parts.length === 0) {
		return [ 0 ];
	}
	let $y = null;
	if (__at(parts, 0, "app.vl:38:5") === "docs" && parts.length === 2) {
		const $w = __parse_i32(__at(parts, 1, "app.vl:39:9"));
		let $x = null;
		if ($w[0] === 0) {
			const page = $w[1];
			return [ 1, page ];
		} else {
			$x = undefined;
		}
		$y = $x;
	}
	$y;
	return [ 2 ];
}
function href(route2) {
	const $bn = route2;
	let $bo = null;
	if ($bn[0] === 0) {
		$bo = "/";
	} else if ($bn[0] === 1) {
		const page = $bn[1];
		$bo = "/docs/" + page;
	} else {
		$bo = "/404";
	}
	return $bo;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $bV, $bW) {
	return child(child(view("section"), text(view("h2"), title), $bV, $bW, "app.vl:78:18"), text(view("p"), body), $bV, $bW, "app.vl:78:48");
}
function app(route2, $be, $bf) {
	chunk_preload(route2);
	return child2(child(child(child(view("main"), child(child(view("nav"), link("Home", [ 0 ], $be, $bf), $be, $bf, "app.vl:102:22"), link("Docs", [ 1, 1 ], $be, $bf), $be, $bf, "app.vl:102:55"), $be, $bf, "app.vl:102:4"), bind_text(class2(view("p"), "pending"), derive(pending(), (busy, $by, $bz, $bA) => {
		let $bB = null;
		if (busy) {
			$bB = "...";
		} else {
			$bB = "";
		}
		return $bB;
	}, "app.vl:111:25"), $be, $bf, "app.vl:111:5"), $be, $bf, "app.vl:109:4"), bind_text2(class2(view("p"), "failed"), derive(chunk_error(), (reason, $bK, $bL, $bM) => {
		const $bN = reason;
		let $bO = null;
		if ($bN[0] === 0) {
			const text2 = $bN[1];
			let $bP = null;
			if (text2.length > 0) {
				$bP = "!";
			} else {
				$bP = "?";
			}
			$bO = $bP;
		} else {
			$bO = "";
		}
		return $bO;
	}, "app.vl:112:60"), $be, $bf, "app.vl:112:36"), $be, $bf, "app.vl:112:4"), swap_split(__clone(route2), (current, $bQ) => {
		const $bR = current;
		let $bS = null;
		if ($bR[0] === 0) {
			$bS = home_page($be, $bQ);
		} else if ($bR[0] === 1) {
			const page = $bR[1];
			$bS = docs_page(page, $be, $bQ);
		} else {
			$bS = not_found_page($be, $bQ);
		}
		return $bS;
	}), $be, $bf, "app.vl:116:4");
}
function eq(self, other) {
	const $cR = [ self, other ];
	let $cS = null;
	if ($cR[0][0] === 0 && $cR[1][0] === 0) {
		$cS = true;
	} else if ($cR[0][0] === 1 && $cR[1][0] === 1) {
		const s0 = $cR[0][1];
		const o0 = $cR[1][1];
		$cS = s0 === o0;
	} else if ($cR[0][0] === 2 && $cR[1][0] === 2) {
		$cS = true;
	} else {
		$cS = false;
	}
	return $cS;
}
function new4(value, caller) {
	let subscribers = [  ];
	const cell2 = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell2[0]), caller);
	}
	return cell2;
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $i = null;
	if (is_empty(self)) {
		$i = [ 1 ];
	} else {
		$i = __list_get(self, self.length - 1);
	}
	return $i;
}
function notify(self, $d) {
	const traced = tracing.v;
	let $e = null;
	if (traced) {
		$e = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$e = "";
	}
	const cause = $e;
	const $f = $d;
	let $g = null;
	if ($f[0] === 0) {
		const turn2 = $f[1];
		$g = queue_wakes(turn2, __clone(self[1].v), traced, cause);
	} else {
		const $j = last(draining_turns.v);
		let $k = null;
		if ($j[0] === 0) {
			const draining = $j[1];
			$k = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $l = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$l = undefined;
				}
				$l;
			}
			$k = undefined;
		}
		$g = $k;
	}
	return $g;
}
function set(self, value, $c, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $c);
}
function notify2(self, $d) {
	const traced = tracing.v;
	let $m = null;
	if (traced) {
		$m = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$m = "";
	}
	const cause = $m;
	const $n = $d;
	let $o = null;
	if ($n[0] === 0) {
		const turn2 = $n[1];
		$o = queue_wakes(turn2, __clone(self[1].v), traced, cause);
	} else {
		const $p = last(draining_turns.v);
		let $q = null;
		if ($p[0] === 0) {
			const draining = $p[1];
			$q = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $r = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$r = undefined;
				}
				$r;
			}
			$q = undefined;
		}
		$o = $q;
	}
	return $o;
}
function set2(self, value, $c, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $c);
}
function turn(policy, body) {
	const fresh = new2();
	const result = body(fresh);
	drain(fresh);
	fresh[5].v = true;
	return result;
}
function derive(self, transform, caller) {
	return [ self, transform, caller ];
}
function get(self) {
	return __clone(self[0].v);
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function on_settle(self, subscriber) {
	return attach(self, subscriber);
}
function start(self) {
	return [ () => {
		return get(self);
	}, (subscriber) => {
		return on_settle(self, subscriber);
	}, () => {
		return;
	} ];
}
function is_none(self) {
	const $R = self;
	return $R[0] === 1;
}
function run_body(runs, body) {
	const run = renew(runs);
	const $aa = nursery(run);
	let $ab = null;
	if ($aa[0] === 0) {
		const nursery2 = $aa[1];
		$ab = (($ac) => {
			return (($ad) => {
				return body($ac, $ad);
			})(nursery2);
		})(run);
	} else {
		$ab = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $ab;
}
function run_once(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body(runs, ($J, $K) => {
		return (($L) => {
			return body($J, $L, $K);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs, tracker, body) {
	let value = run_once(runs, tracker, ($E, $F, $G) => {
		return body($E, $F, $G);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs, tracker, ($aD, $aE, $aF) => {
			return body($aD, $aE, $aF);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function start2(self) {
	const transform = self[1];
	const site = __clone(self[2]);
	const upstream = start(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new3();
	const tracker = new_tracker([ 0, site ]);
	return [ () => {
		const value = pull();
		return run_tracked(runs, tracker, ($B, $C, $D) => {
			return transform(value, $B, $C, $D);
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
function set3(self, value, $c, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $c);
}
function take(self, item, $aY) {
	defer(self, () => {
		dispose(item, $aY);
		return;
	});
	return __clone(item);
}
function cell(self, $z, $A, caller) {
	const instance = start2(self);
	const pull = instance[0];
	const cached = new4(pull(), caller);
	const refresh = subscriber_of(() => {
		set3(cached, pull(), $z, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed = instance[1](refresh);
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $z, $A);
	return cached;
}
function attr(self, name, value, $bp, $bq, caller) {
	apply(value, self, name, $bp, $bq, caller);
	return __clone(self);
}
function link_to(self, route2, $bl, $bm) {
	const path = to_path(route2);
	return on_event(attr(attr(self, "href", path, $bl, $bm, "std/src/web/router.vl:415:8"), "draggable", "false", $bl, $bm, "std/src/web/router.vl:415:27"), "click", (event, $br, $bs) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $br ]);
		}
		return;
	});
}
function link(label, route2, $bj, $bk) {
	return text(link_to(view("a"), route2, $bj, $bk), label);
}
function child(self, content, $bw, $bx, caller) {
	place(content, self, $bw, $bx, caller);
	return __clone(self);
}
function on_settle2(self, subscriber) {
	return attach(self, subscriber);
}
function start3(self) {
	return [ () => {
		return get(self);
	}, (subscriber) => {
		return on_settle2(self, subscriber);
	}, () => {
		return;
	} ];
}
function run_body2(runs, body) {
	const run = renew(runs);
	const $bI = nursery(run);
	let $bJ = null;
	if ($bI[0] === 0) {
		const nursery2 = $bI[1];
		$bJ = (($ac) => {
			return (($ad) => {
				return body($ac, $ad);
			})(nursery2);
		})(run);
	} else {
		$bJ = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $bJ;
}
function run_once2(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body2(runs, ($J, $K) => {
		return (($L) => {
			return body($J, $L, $K);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked2(runs, tracker, body) {
	let value = run_once2(runs, tracker, ($E, $F, $G) => {
		return body($E, $F, $G);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once2(runs, tracker, ($aD, $aE, $aF) => {
			return body($aD, $aE, $aF);
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
	const runs = new3();
	const tracker = new_tracker([ 0, site ]);
	return [ () => {
		const value = pull();
		return run_tracked2(runs, tracker, ($B, $C, $D) => {
			return transform(value, $B, $C, $D);
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
function follow(flow, observer, $bF, $bG, caller) {
	take(get_owner($bG), sub(flow, observer, caller), $bF);
}
function bind_text(self, source, $bC, $bD, caller) {
	const element = __clone(self[0]);
	follow(source, (value, $bE) => {
		element.textContent = value;
		return;
	}, $bC, $bD, caller);
	return __clone(self);
}
function start5(self) {
	return [ () => {
		return get(self);
	}, (subscriber) => {
		return on_settle2(self, subscriber);
	}, () => {
		return;
	} ];
}
function start6(self) {
	const transform = self[1];
	const site = __clone(self[2]);
	const upstream = start5(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new3();
	const tracker = new_tracker([ 0, site ]);
	return [ () => {
		const value = pull();
		return run_tracked2(runs, tracker, ($B, $C, $D) => {
			return transform(value, $B, $C, $D);
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
function sub2(self, observer, caller) {
	return observe_flow2(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true, caller);
}
function follow2(flow, observer, $bF, $bG, caller) {
	take(get_owner($bG), sub2(flow, observer, caller), $bF);
}
function bind_text2(self, source, $bC, $bD, caller) {
	const element = __clone(self[0]);
	follow2(source, (value, $bE) => {
		element.textContent = value;
		return;
	}, $bC, $bD, caller);
	return __clone(self);
}
function chunk_preload(source) {
	__chunk_preload(__chunk_arm(get(source)));
}
function observe(signal, observer) {
	const cell2 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $cA = [ 0, cell2 ];
		let $cB = null;
		if ($cA[0] === 0) {
			const live = $cA[1];
			$cB = observer(live.v);
		} else {
			$cB = undefined;
		}
		return $cB;
	}));
}
function attach_observer(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function sub3(self, observer, caller) {
	return place_subscription(attach_observer(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true), caller);
}
function follow3(flow, observer, $bF, $bG, caller) {
	take(get_owner($bG), sub3(flow, observer, caller), $bF);
}
function swap_split(source, render, $cd) {
	const gated = new4(get(source), "std/src/browser/web/ui.vl:1337:41");
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance2 = (value, $ce) => {
		armed.v = true;
		set3(gated, value, [ 0, $ce ], "std/src/browser/web/ui.vl:1349:9");
		return;
	};
	const wire = ($cf) => {
		follow3(__clone(source), (value, $cg) => {
			return turn([ 1 ], ($ch) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $ch ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $ch ]);
					advance2(value, $ch);
				} else {
					set_chunk_pending(true, [ 0, $ch ]);
					__chunk_load(arm, () => {
						return turn([ 1 ], ($cy) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $cy ]);
								advance2(value, $cy);
							}
							return;
						});
					}, (reason) => {
						return turn([ 1 ], ($cz) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $cz ]);
								set3(chunk_error_signal, [ 0, reason ], [ 0, $cz ], "std/src/browser/web/ui.vl:1384:26");
							}
							return;
						});
					});
				}
				return;
			});
		}, $cd, $cf, "std/src/browser/web/ui.vl:1361:3");
		return;
	};
	return [ [ 1 ], render, [ 0, __clone(gated) ], armed, wire ];
}
function first(self) {
	return __list_get(self, 0);
}
function open_row_before(self, content, end, $dc, $dd) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(content, [ __clone(staging) ], $dc, $dd, "std/src/browser/web/ui.vl:801:11");
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function open_row(self, content, $da, $db) {
	return open_row_before(self, content, self[0], $da, $db);
}
function run_with_owner(owner, body) {
	return body(owner);
}
function place_swap(parent, source, render, armed, $cG, $cH, caller) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($cH), () => {
		return close_with(region, () => {
			const $cI = live_owner.v;
			let $cJ = null;
			if ($cI[0] === 1) {
				$cJ = $cI;
			} else {
				$cJ = [ 0, dispose2($cI[1]) ];
			}
			$cJ;
			return;
		});
	});
	follow3(source, (value, $cO) => {
		const $cP = last_value.v;
		let $cQ = null;
		if ($cP[0] === 0) {
			const previous = $cP[1];
			$cQ = eq(previous, value);
		} else {
			$cQ = false;
		}
		const unchanged = $cQ;
		if (armed.v && !(unchanged)) {
			const $cT = live_row.v;
			let $cU = null;
			if ($cT[0] === 0) {
				const row = $cT[1];
				$cU = retire_row(region, row, region[0], () => {
					const $cV = live_owner.v;
					let $cW = null;
					if ($cV[0] === 1) {
						$cW = $cV;
					} else {
						$cW = [ 0, dispose2($cV[1]) ];
					}
					$cW;
					return;
				});
			} else {
				const $cX = live_owner.v;
				let $cY = null;
				if ($cX[0] === 1) {
					$cY = $cX;
				} else {
					$cY = [ 0, dispose2($cX[1]) ];
				}
				$cY;
				$cU = undefined;
			}
			$cU;
			const owner = new3();
			const row2 = run_with_owner(owner, ($cZ) => {
				return open_row(region, render(value, $cZ), $cG, $cZ);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $cG, $cH, caller);
}
function place2(self, parent, $cC, $cD, caller) {
	self[4]($cD);
	const render = self[1];
	const armed = self[3];
	const $cE = self[0];
	let $cF = null;
	if ($cE[0] === 0) {
		const source = $cE[1];
		$cF = place_swap(parent, __clone(source), render, armed, $cC, $cD, caller);
	} else {
		const $de = self[2];
		let $df = null;
		if ($de[0] === 0) {
			const gated = $de[1];
			$df = place_swap(parent, __clone(gated), render, armed, $cC, $cD, caller);
		} else {
			$df = undefined;
		}
		$cF = $df;
	}
	return $cF;
}
function child2(self, content, $bw, $bx, caller) {
	place2(content, self, $bw, $bx, caller);
	return __clone(self);
}
function comp(body) {
	const scope = new3();
	const result = body(scope);
	return [ result, scope ];
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
const path_signal = new4("", "std/src/web/router.vl:77:48");
const entry_signal = new4([ "", "", false ], "std/src/web/router.vl:79:58");
const wired = __shared_new(false);
const chunk_pending_signal = new4(false, "std/src/browser/web/ui.vl:615:58");
const chunk_error_signal = new4([ 1 ], "std/src/browser/web/ui.vl:620:63");
const leaving_runs = __shared_new([  ]);
const BASE = announce("BASE", 2);
const SCALED = announce("SCALED", BASE * 3);
const LABEL = "scale " + SCALED;
__vilan_chunks.url[0] = "app.Route_Home.js";
__vilan_chunks.url[1] = "app.Route_Docs.js";
__vilan_chunks.url[2] = "app.Route_NotFound.js";
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.child = child;
__vilan_chunks.fn.link = link;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = cell(derive(current_path([ 1 ]), (path, $t, $u, $v) => {
	return parse(path);
}, "app.vl:124:29"), [ 1 ], [ 1 ], "app.vl:124:56");
mount_root("app", ($bd) => {
	return app(route, [ 1 ], $bd);
});
//# sourceMappingURL=app.js.map
