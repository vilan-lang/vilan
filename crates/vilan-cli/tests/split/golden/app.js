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
function home_page($bR, $bS) {
	return __vilan_chunks.fn.home_page($bR, $bS);
}
function docs_page(page, $bV, $bW) {
	return __vilan_chunks.fn.docs_page(page, $bV, $bW);
}
function not_found_page($bZ, $ca) {
	return __vilan_chunks.fn.not_found_page($bZ, $ca);
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
function dispose(self, $aX) {
	const $aY = $aX;
	let $aZ = null;
	if ($aY[0] === 0) {
		const established = $aY[1];
		$aZ = [ 0, established ];
	} else {
		$aZ = last(draining_turns.v);
	}
	const ambient = $aZ;
	release_under(self, ambient);
}
function detach(handle) {
	const $at = last(releasing_turns.v);
	let $au = null;
	if ($at[0] === 0) {
		const at_release = $at[1];
		$au = at_release;
	} else {
		$au = last(draining_turns.v);
	}
	const turn2 = $au;
	release_under(handle, turn2);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $av = [ 0, handle[0] ];
	let $aw = null;
	if ($av[0] === 0) {
		const subscribers = $av[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aw = undefined;
	} else {
		$aw = undefined;
	}
	$aw;
	const $ax = ambient;
	let $ay = null;
	if ($ax[0] === 0) {
		const turn2 = $ax[1];
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
		$ay = undefined;
	} else {
		$ay = undefined;
	}
	$ay;
	const $az = handle[3].v;
	let $aA = null;
	if ($az[0] === 0) {
		const release = $az[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aA = undefined;
	} else {
		$aA = undefined;
	}
	return $aA;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $ba = null;
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
		$ba = undefined;
	}
	return $ba;
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
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
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
					throw __panic(message2, "std/src/reactive.vl:1291:27");
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
function get_owner($bF) {
	return $bF;
}
function register_with_owner(subscription, $aS, $aT) {
	const $aU = $aT;
	let $aV = null;
	if ($aU[0] === 0) {
		const owner = $aU[1];
		$aV = take(owner, subscription, $aS);
	} else {
		$aV = __clone(subscription);
	}
	return $aV;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker() {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ] ]) ];
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
			__at_put(kept, index, true, "std/src/reactive.vl:1708:5");
			next.push(__clone(__at(held, index, "std/src/reactive.vl:1709:15")));
			$an = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$an = undefined;
		}
		$an;
		position = position + 1;
	}
	lists.v[2] = next;
	let index2 = 0;
	for (const edge of held) {
		if (!(__at(kept, index2, "std/src/reactive.vl:1723:7"))) {
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
		if (position < held.length && !(__at(kept, position, "std/src/reactive.vl:1746:9")) && same_identity(__at(held, position, "std/src/reactive.vl:1747:22")[0], wanted)) {
			return [ 0, position ];
		}
		let index = 0;
		while (index < held.length) {
			if (!(__at(kept, index, "std/src/reactive.vl:1752:9")) && same_identity(__at(held, index, "std/src/reactive.vl:1752:38")[0], wanted)) {
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
	return subscriber_of(() => {
		const connecting = tracker[0].v[3];
		tracker[0].v[2] = true;
		if (connecting) {
			tracker[0].v[4] = true;
		} else {
			wake(target);
		}
		return;
	}, true);
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $aE = tracker[0].v[6];
	let $aF = null;
	if ($aE[0] === 0) {
		const lists = $aE[1];
		$aF = lists;
	} else {
		return;
		$aF = undefined;
	}
	const lists2 = $aF;
	let $aG = null;
	if (!(is_empty(lists2.v[1]))) {
		const read = __clone(lists2.v[1]);
		lists2.v[1] = [  ];
		tracker[0].v[3] = true;
		let edges = [  ];
		for (const dependency of read) {
			edges.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
		}
		lists2.v[2] = edges;
		tracker[0].v[3] = false;
		if (tracker[0].v[4]) {
			tracker[0].v[4] = false;
			wake(target);
		}
		$aG = undefined;
	}
	return $aG;
}
function forget_reads(tracker) {
	const $aH = tracker[0].v[6];
	let $aI = null;
	if ($aH[0] === 0) {
		const lists = $aH[1];
		$aI = lists;
	} else {
		return;
		$aI = undefined;
	}
	const lists2 = $aI;
	let $aJ = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aJ = undefined;
	}
	$aJ;
	if (!(is_empty(lists2.v[1]))) {
		lists2.v[1] = [  ];
	}
}
function detach_tracker(tracker) {
	tracker[0].v[5] = [ 1 ];
	forget_reads(tracker);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $aQ = previous;
		let $aR = null;
		if ($aQ[0] === 0) {
			const earlier = $aQ[1];
			$aR = earlier();
		} else {
			$aR = undefined;
		}
		return $aR;
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
		set(entry_signal, [ __router_path(), __router_state(), false ], $b, "std/src/web/router.vl:101:16");
		set2(path_signal, __router_path(), $b, "std/src/web/router.vl:106:15");
		__dom_window().addEventListener("popstate", () => {
			return turn([ 1 ], ($s) => {
				set(entry_signal, [ __router_path(), __router_state(), true ], [ 0, $s ], "std/src/web/router.vl:108:17");
				set2(path_signal, __router_path(), [ 0, $s ], "std/src/web/router.vl:113:16");
				return;
			});
		});
	}
}
function current_path($a) {
	ensure_wired($a);
	return path_signal;
}
function navigate(path, $br) {
	navigate_with(path, "", $br);
}
function navigate_with(path, state, $bs) {
	ensure_wired($bs);
	history.pushState(state, "", path);
	set(entry_signal, [ path, state, false ], $bs, "std/src/web/router.vl:178:15");
	set2(path_signal, path, $bs, "std/src/web/router.vl:179:14");
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
	let $bg = null;
	if (is_svg_tag(tag)) {
		$bg = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$bg = [ document.createElement(tag) ];
	}
	return $bg;
}
function is_svg_tag(tag) {
	const $be = tag;
	let $bf = null;
	if ($be === "svg") {
		$bf = true;
	} else if ($be === "path") {
		$bf = true;
	} else if ($be === "circle") {
		$bf = true;
	} else if ($be === "ellipse") {
		$bf = true;
	} else if ($be === "rect") {
		$bf = true;
	} else if ($be === "line") {
		$bf = true;
	} else if ($be === "polyline") {
		$bf = true;
	} else if ($be === "polygon") {
		$bf = true;
	} else if ($be === "g") {
		$bf = true;
	} else if ($be === "defs") {
		$bf = true;
	} else if ($be === "use") {
		$bf = true;
	} else if ($be === "symbol") {
		$bf = true;
	} else if ($be === "marker") {
		$bf = true;
	} else if ($be === "pattern") {
		$bf = true;
	} else if ($be === "mask") {
		$bf = true;
	} else if ($be === "clipPath") {
		$bf = true;
	} else if ($be === "linearGradient") {
		$bf = true;
	} else if ($be === "radialGradient") {
		$bf = true;
	} else if ($be === "stop") {
		$bf = true;
	} else if ($be === "text") {
		$bf = true;
	} else if ($be === "tspan") {
		$bf = true;
	} else if ($be === "textPath") {
		$bf = true;
	} else if ($be === "filter") {
		$bf = true;
	} else if ($be === "foreignObject") {
		$bf = true;
	} else if ($be === "feGaussianBlur") {
		$bf = true;
	} else if ($be === "feColorMatrix") {
		$bf = true;
	} else if ($be === "feOffset") {
		$bf = true;
	} else if ($be === "feMerge") {
		$bf = true;
	} else if ($be === "feMergeNode") {
		$bf = true;
	} else if ($be === "feFlood") {
		$bf = true;
	} else if ($be === "feComposite") {
		$bf = true;
	} else if ($be === "feBlend") {
		$bf = true;
	} else if ($be === "feDropShadow") {
		$bf = true;
	} else {
		$bf = false;
	}
	return $bf;
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
		return turn([ 1 ], ($bt) => {
			return (() => {
				return handler(dispatched, $bt, [ 1 ]);
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
function set_chunk_pending(busy, $cp) {
	if (get(chunk_pending_signal) !== busy) {
		set3(chunk_pending_signal, busy, $cp, "std/src/browser/web/ui.vl:656:24");
	}
}
function clear_chunk_error($cg) {
	const $ch = get(chunk_error_signal);
	let $ci = null;
	if ($ch[0] === 0) {
		const _reason = $ch[1];
		$ci = set3(chunk_error_signal, [ 1 ], $cg, "std/src/browser/web/ui.vl:664:43");
	} else {
		$ci = undefined;
	}
	return $ci;
}
function open(parent) {
	const anchor = document.createTextNode("");
	parent[0].appendChild(anchor);
	return [ anchor, __shared_new([  ]), __shared_new([  ]) ];
}
function host(self) {
	return self[0].parentNode;
}
function cut_row(self, row, end) {
	const range = document.createRange();
	range.setStartAfter(row[0]);
	range.setEndBefore(end);
	return range.extractContents();
}
function drop_row(self, row) {
	row[0].remove();
}
function hold_rows(self, rows) {
	self[2].v = __clone(rows);
}
function close(self) {
	for (const view2 of self[1].v) {
		view2[0].remove();
	}
	self[1].v = [  ];
	const rows = __clone(self[2].v);
	let at = 0;
	for (const row of rows) {
		let $cI = null;
		if (at + 1 < rows.length) {
			$cI = __at(rows, at + 1, "std/src/browser/web/ui.vl:866:39")[0];
		} else {
			$cI = self[0];
		}
		const end = __clone($cI);
		cut_row(self, row, end);
		drop_row(self, row);
		at = at + 1;
	}
	self[2].v = [  ];
	self[0].remove();
}
function place(self, parent) {
	parent[0].appendChild(self[0]);
}
function apply(self, parent, name) {
	parent[0].setAttribute(name, self);
}
function mount_target(id) {
	const element = document.getElementById(id);
	if (__is_null(element)) {
		(() => {
			throw __panic("mount: no element with id \'" + id + "\'", "std/src/browser/web/ui.vl:2386:3");
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
	const $da = turn([ 1 ], ($cZ) => {
		return comp(body);
	});
	const built = $da[0];
	const root = $da[1];
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
	const $bl = route2;
	let $bm = null;
	if ($bl[0] === 0) {
		$bm = "/";
	} else if ($bl[0] === 1) {
		const page = $bl[1];
		$bm = "/docs/" + page;
	} else {
		$bm = "/404";
	}
	return $bm;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $bT, $bU) {
	return child(child(view("section"), text(view("h2"), title), $bT, $bU), text(view("p"), body), $bT, $bU);
}
function app(route2, $bc, $bd) {
	chunk_preload(route2);
	return child2(child(child(child(view("main"), child(child(view("nav"), link("Home", [ 0 ], $bc, $bd), $bc, $bd), link("Docs", [ 1, 1 ], $bc, $bd), $bc, $bd), $bc, $bd), bind_text(class2(view("p"), "pending"), derive(pending(), (busy, $bw, $bx, $by) => {
		let $bz = null;
		if (busy) {
			$bz = "...";
		} else {
			$bz = "";
		}
		return $bz;
	}), $bc, $bd), $bc, $bd), bind_text2(class2(view("p"), "failed"), derive(chunk_error(), (reason, $bI, $bJ, $bK) => {
		const $bL = reason;
		let $bM = null;
		if ($bL[0] === 0) {
			const text2 = $bL[1];
			let $bN = null;
			if (text2.length > 0) {
				$bN = "!";
			} else {
				$bN = "?";
			}
			$bM = $bN;
		} else {
			$bM = "";
		}
		return $bM;
	}), $bc, $bd), $bc, $bd), swap_split(__clone(route2), (current, $bO) => {
		const $bP = current;
		let $bQ = null;
		if ($bP[0] === 0) {
			$bQ = home_page($bc, $bO);
		} else if ($bP[0] === 1) {
			const page = $bP[1];
			$bQ = docs_page(page, $bc, $bO);
		} else {
			$bQ = not_found_page($bc, $bO);
		}
		return $bQ;
	}), $bc, $bd);
}
function eq(self, other) {
	const $cM = [ self, other ];
	let $cN = null;
	if ($cM[0][0] === 0 && $cM[1][0] === 0) {
		$cN = true;
	} else if ($cM[0][0] === 1 && $cM[1][0] === 1) {
		const s0 = $cM[0][1];
		const o0 = $cM[1][1];
		$cN = s0 === o0;
	} else if ($cM[0][0] === 2 && $cM[1][0] === 2) {
		$cN = true;
	} else {
		$cN = false;
	}
	return $cN;
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
function derive(self, transform) {
	return [ self, transform ];
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
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1402:11");
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
		value = run_once(runs, tracker, ($aB, $aC, $aD) => {
			return body($aB, $aC, $aD);
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
	const upstream = start(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new3();
	const tracker = new_tracker();
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
function take(self, item, $aW) {
	defer(self, () => {
		dispose(item, $aW);
		return;
	});
	return __clone(item);
}
function cell(self, $z, $A) {
	const instance = start2(self);
	const pull = instance[0];
	const cached = new4(pull(), "std/src/reactive.vl:2347:28");
	const refreshed = instance[1](subscriber_of(() => {
		set3(cached, pull(), $z, "std/src/reactive.vl:2351:11");
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $z, $A);
	return cached;
}
function attr(self, name, value, $bn, $bo) {
	apply(value, self, name, $bn, $bo);
	return __clone(self);
}
function link_to(self, route2, $bj, $bk) {
	const path = to_path(route2);
	return on_event(attr(attr(self, "href", path, $bj, $bk), "draggable", "false", $bj, $bk), "click", (event, $bp, $bq) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $bp ]);
		}
		return;
	});
}
function link(label, route2, $bh, $bi) {
	return text(link_to(view("a"), route2, $bh, $bi), label);
}
function child(self, content, $bu, $bv) {
	place(content, self, $bu, $bv);
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
	const $bG = nursery(run);
	let $bH = null;
	if ($bG[0] === 0) {
		const nursery2 = $bG[1];
		$bH = (($ac) => {
			return (($ad) => {
				return body($ac, $ad);
			})(nursery2);
		})(run);
	} else {
		$bH = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1402:11");
		})();
	}
	return $bH;
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
		value = run_once2(runs, tracker, ($aB, $aC, $aD) => {
			return body($aB, $aC, $aD);
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
	const upstream = start3(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new3();
	const tracker = new_tracker();
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
function observe_flow(flow, observer, immediately) {
	const instance = start4(flow);
	const pull = instance[0];
	if (!(immediately)) {
		pull();
	}
	const subscription = instance[1](mint_subscriber(() => {
		return observer(pull());
	}));
	also_releasing(subscription, instance[2]);
	if (immediately) {
		observer(pull());
	}
	return subscription;
}
function sub(self, observer) {
	return observe_flow(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function follow(flow, observer, $bD, $bE) {
	take(get_owner($bE), sub(flow, observer), $bD);
}
function bind_text(self, source, $bA, $bB) {
	const element = __clone(self[0]);
	follow(source, (value, $bC) => {
		element.textContent = value;
		return;
	}, $bA, $bB);
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
	const upstream = start5(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new3();
	const tracker = new_tracker();
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
function observe_flow2(flow, observer, immediately) {
	const instance = start6(flow);
	const pull = instance[0];
	if (!(immediately)) {
		pull();
	}
	const subscription = instance[1](mint_subscriber(() => {
		return observer(pull());
	}));
	also_releasing(subscription, instance[2]);
	if (immediately) {
		observer(pull());
	}
	return subscription;
}
function sub2(self, observer) {
	return observe_flow2(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function follow2(flow, observer, $bD, $bE) {
	take(get_owner($bE), sub2(flow, observer), $bD);
}
function bind_text2(self, source, $bA, $bB) {
	const element = __clone(self[0]);
	follow2(source, (value, $bC) => {
		element.textContent = value;
		return;
	}, $bA, $bB);
	return __clone(self);
}
function chunk_preload(source) {
	__chunk_preload(__chunk_arm(get(source)));
}
function observe(signal, observer) {
	const cell2 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $cy = [ 0, cell2 ];
		let $cz = null;
		if ($cy[0] === 0) {
			const live = $cy[1];
			$cz = observer(live.v);
		} else {
			$cz = undefined;
		}
		return $cz;
	}));
}
function attach_observer(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function sub3(self, observer) {
	return attach_observer(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function follow3(flow, observer, $bD, $bE) {
	take(get_owner($bE), sub3(flow, observer), $bD);
}
function swap_split(source, render, $cb) {
	const gated = new4(get(source), "std/src/browser/web/ui.vl:1211:41");
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance2 = (value, $cc) => {
		armed.v = true;
		set3(gated, value, [ 0, $cc ], "std/src/browser/web/ui.vl:1223:9");
		return;
	};
	const wire = ($cd) => {
		follow3(__clone(source), (value, $ce) => {
			return turn([ 1 ], ($cf) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $cf ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $cf ]);
					advance2(value, $cf);
				} else {
					set_chunk_pending(true, [ 0, $cf ]);
					__chunk_load(arm, () => {
						return turn([ 1 ], ($cw) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $cw ]);
								advance2(value, $cw);
							}
							return;
						});
					}, (reason) => {
						return turn([ 1 ], ($cx) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $cx ]);
								set3(chunk_error_signal, [ 0, reason ], [ 0, $cx ], "std/src/browser/web/ui.vl:1258:26");
							}
							return;
						});
					});
				}
				return;
			});
		}, $cb, $cd);
		return;
	};
	return [ [ 1 ], render, [ 0, __clone(gated) ], armed, wire ];
}
function open_row_before(self, content, end, $cV, $cW) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(content, [ __clone(staging) ], $cV, $cW);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function open_row(self, content, $cT, $cU) {
	return open_row_before(self, content, self[0], $cT, $cU);
}
function run_with_owner(owner, body) {
	return body(owner);
}
function place_swap(parent, source, render, armed, $cE, $cF) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($cF), () => {
		const $cG = live_owner.v;
		let $cH = null;
		if ($cG[0] === 1) {
			$cH = $cG;
		} else {
			$cH = [ 0, dispose2($cG[1]) ];
		}
		$cH;
		close(region);
		return;
	});
	follow3(source, (value, $cJ) => {
		const $cK = last_value.v;
		let $cL = null;
		if ($cK[0] === 0) {
			const previous = $cK[1];
			$cL = eq(previous, value);
		} else {
			$cL = false;
		}
		const unchanged = $cL;
		if (armed.v && !(unchanged)) {
			const $cO = live_owner.v;
			let $cP = null;
			if ($cO[0] === 1) {
				$cP = $cO;
			} else {
				$cP = [ 0, dispose2($cO[1]) ];
			}
			$cP;
			const $cQ = live_row.v;
			let $cR = null;
			if ($cQ[0] === 0) {
				const row = $cQ[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$cR = undefined;
			} else {
				$cR = undefined;
			}
			$cR;
			const owner = new3();
			const row2 = run_with_owner(owner, ($cS) => {
				return open_row(region, render(value, $cS), $cE, $cS);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $cE, $cF);
}
function place2(self, parent, $cA, $cB) {
	self[4]($cB);
	const render = self[1];
	const armed = self[3];
	const $cC = self[0];
	let $cD = null;
	if ($cC[0] === 0) {
		const source = $cC[1];
		$cD = place_swap(parent, __clone(source), render, armed, $cA, $cB);
	} else {
		const $cX = self[2];
		let $cY = null;
		if ($cX[0] === 0) {
			const gated = $cX[1];
			$cY = place_swap(parent, __clone(gated), render, armed, $cA, $cB);
		} else {
			$cY = undefined;
		}
		$cD = $cY;
	}
	return $cD;
}
function child2(self, content, $bu, $bv) {
	place2(content, self, $bu, $bv);
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
} ]);
const waking = __shared_new([ 1 ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const run_nurseries_allocated_count = __shared_new(0);
const path_signal = new4("", "std/src/web/router.vl:83:48");
const entry_signal = new4([ "", "", false ], "std/src/web/router.vl:85:58");
const wired = __shared_new(false);
const chunk_pending_signal = new4(false, "std/src/browser/web/ui.vl:602:58");
const chunk_error_signal = new4([ 1 ], "std/src/browser/web/ui.vl:607:63");
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
}), [ 1 ], [ 1 ]);
mount_root("app", ($bb) => {
	return app(route, [ 1 ], $bb);
});
//# sourceMappingURL=app.js.map
