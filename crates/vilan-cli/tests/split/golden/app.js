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
function home_page($bK, $bL) {
	return __vilan_chunks.fn.home_page($bK, $bL);
}
function docs_page(page, $bO, $bP) {
	return __vilan_chunks.fn.docs_page(page, $bO, $bP);
}
function not_found_page($bS, $bT) {
	return __vilan_chunks.fn.not_found_page($bS, $bT);
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
	const $ai = turn2;
	let $aj = null;
	if ($ai[0] === 0) {
		const ambient = $ai[1];
		$aj = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $ak = last(draining_turns.v);
		let $al = null;
		if ($ak[0] === 0) {
			const draining = $ak[1];
			$al = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$al = undefined;
		}
		$aj = $al;
	}
	return $aj;
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
function dispose(self, $aR) {
	const $aS = $aR;
	let $aT = null;
	if ($aS[0] === 0) {
		const established = $aS[1];
		$aT = [ 0, established ];
	} else {
		$aT = last(draining_turns.v);
	}
	const ambient = $aT;
	release_under(self, ambient);
}
function detach(handle) {
	const $an = last(releasing_turns.v);
	let $ao = null;
	if ($an[0] === 0) {
		const at_release = $an[1];
		$ao = at_release;
	} else {
		$ao = last(draining_turns.v);
	}
	const turn2 = $ao;
	release_under(handle, turn2);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ap = [ 0, handle[0] ];
	let $aq = null;
	if ($ap[0] === 0) {
		const subscribers = $ap[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aq = undefined;
	} else {
		$aq = undefined;
	}
	$aq;
	const $ar = ambient;
	let $as = null;
	if ($ar[0] === 0) {
		const turn2 = $ar[1];
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
		$as = undefined;
	} else {
		$as = undefined;
	}
	$as;
	const $at = handle[3].v;
	let $au = null;
	if ($at[0] === 0) {
		const release = $at[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$au = undefined;
	} else {
		$au = undefined;
	}
	return $au;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aU = null;
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
		$aU = undefined;
	}
	return $aU;
}
function renew(self) {
	const $G = self[0].v[3];
	let $H = null;
	if ($G[0] === 0) {
		const nursery2 = __clone($G[1]);
		let $I = null;
		if (has_spawned(nursery2)) {
			$I = [ 1 ];
		} else {
			$I = [ 0, nursery2 ];
		}
		$H = $I;
	} else {
		$H = [ 1 ];
	}
	const carried = $H;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $T = null;
	if (is_disposed(self)) {
		$T = [ 1 ];
	} else {
		$T = self[0].v[3];
	}
	return $T;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $S = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $J = held[3];
		let $K = null;
		if ($J[0] === 0) {
			const nursery2 = $J[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$K = undefined;
		} else {
			$K = undefined;
		}
		$K;
		let $R = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $M = __guarded(cleanup);
				let $N = null;
				if ($M[0] === 0) {
					const message = $M[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$N = undefined;
				} else {
					$N = undefined;
				}
				$N;
			}
			const $P = failure;
			let $Q = null;
			if ($P[0] === 0) {
				const message2 = $P[1];
				$Q = (() => {
					throw __panic(message2, "std/src/reactive.vl:1291:27");
				})();
			} else {
				$Q = undefined;
			}
			$R = $Q;
		}
		$S = $R;
	}
	return $S;
}
function get_owner($by) {
	return $by;
}
function register_with_owner(subscription, $aM, $aN) {
	const $aO = $aN;
	let $aP = null;
	if ($aO[0] === 0) {
		const owner = $aO[1];
		$aP = take(owner, subscription, $aM);
	} else {
		$aP = __clone(subscription);
	}
	return $aP;
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
	const $B = tracker[0].v[6];
	let $C = null;
	if ($B[0] === 0) {
		const lists = $B[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$C = undefined;
	} else {
		$C = undefined;
	}
	$C;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $Y = tracker[0].v[6];
	let $Z = null;
	if ($Y[0] === 0) {
		const lists = $Y[1];
		$Z = lists;
	} else {
		return;
		$Z = undefined;
	}
	const lists2 = $Z;
	const $aa = tracker[0].v[5];
	let $ab = null;
	if ($aa[0] === 0) {
		const target = __clone($aa[1]);
		$ab = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$ab = undefined;
	}
	$ab;
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
		const $ag = reusable(held, kept, dependency[0], position);
		let $ah = null;
		if ($ag[0] === 0) {
			const index = $ag[1];
			__at_put(kept, index, true, "std/src/reactive.vl:1708:5");
			next.push(__clone(__at(held, index, "std/src/reactive.vl:1709:15")));
			$ah = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$ah = undefined;
		}
		$ah;
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
	const $ac = identity;
	let $ad = null;
	if ($ac[0] === 0) {
		const wanted = $ac[1];
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
		$ad = [ 1 ];
	} else {
		$ad = [ 1 ];
	}
	return $ad;
}
function same_identity(identity, wanted) {
	const $ae = identity;
	let $af = null;
	if ($ae[0] === 0) {
		const held = $ae[1];
		$af = held === wanted;
	} else {
		$af = false;
	}
	return $af;
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
	const $ay = tracker[0].v[6];
	let $az = null;
	if ($ay[0] === 0) {
		const lists = $ay[1];
		$az = lists;
	} else {
		return;
		$az = undefined;
	}
	const lists2 = $az;
	let $aA = null;
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
		$aA = undefined;
	}
	return $aA;
}
function forget_reads(tracker) {
	const $aB = tracker[0].v[6];
	let $aC = null;
	if ($aB[0] === 0) {
		const lists = $aB[1];
		$aC = lists;
	} else {
		return;
		$aC = undefined;
	}
	const lists2 = $aC;
	let $aD = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aD = undefined;
	}
	$aD;
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
		const $aK = previous;
		let $aL = null;
		if ($aK[0] === 0) {
			const earlier = $aK[1];
			$aL = earlier();
		} else {
			$aL = undefined;
		}
		return $aL;
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
		set(path_signal, __router_path(), $b, "std/src/web/router.vl:71:15");
		__dom_window().addEventListener("popstate", () => {
			return turn([ 1 ], ($m) => {
				set(path_signal, __router_path(), [ 0, $m ], "std/src/web/router.vl:73:16");
				return;
			});
		});
	}
}
function current_path($a) {
	ensure_wired($a);
	return path_signal;
}
function navigate(path, $bl) {
	ensure_wired($bl);
	history.pushState("", "", path);
	set(path_signal, path, $bl, "std/src/web/router.vl:91:14");
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
	let $ba = null;
	if (is_svg_tag(tag)) {
		$ba = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$ba = [ document.createElement(tag) ];
	}
	return $ba;
}
function is_svg_tag(tag) {
	const $aY = tag;
	let $aZ = null;
	if ($aY === "svg") {
		$aZ = true;
	} else if ($aY === "path") {
		$aZ = true;
	} else if ($aY === "circle") {
		$aZ = true;
	} else if ($aY === "ellipse") {
		$aZ = true;
	} else if ($aY === "rect") {
		$aZ = true;
	} else if ($aY === "line") {
		$aZ = true;
	} else if ($aY === "polyline") {
		$aZ = true;
	} else if ($aY === "polygon") {
		$aZ = true;
	} else if ($aY === "g") {
		$aZ = true;
	} else if ($aY === "defs") {
		$aZ = true;
	} else if ($aY === "use") {
		$aZ = true;
	} else if ($aY === "symbol") {
		$aZ = true;
	} else if ($aY === "marker") {
		$aZ = true;
	} else if ($aY === "pattern") {
		$aZ = true;
	} else if ($aY === "mask") {
		$aZ = true;
	} else if ($aY === "clipPath") {
		$aZ = true;
	} else if ($aY === "linearGradient") {
		$aZ = true;
	} else if ($aY === "radialGradient") {
		$aZ = true;
	} else if ($aY === "stop") {
		$aZ = true;
	} else if ($aY === "text") {
		$aZ = true;
	} else if ($aY === "tspan") {
		$aZ = true;
	} else if ($aY === "textPath") {
		$aZ = true;
	} else if ($aY === "filter") {
		$aZ = true;
	} else if ($aY === "foreignObject") {
		$aZ = true;
	} else if ($aY === "feGaussianBlur") {
		$aZ = true;
	} else if ($aY === "feColorMatrix") {
		$aZ = true;
	} else if ($aY === "feOffset") {
		$aZ = true;
	} else if ($aY === "feMerge") {
		$aZ = true;
	} else if ($aY === "feMergeNode") {
		$aZ = true;
	} else if ($aY === "feFlood") {
		$aZ = true;
	} else if ($aY === "feComposite") {
		$aZ = true;
	} else if ($aY === "feBlend") {
		$aZ = true;
	} else if ($aY === "feDropShadow") {
		$aZ = true;
	} else {
		$aZ = false;
	}
	return $aZ;
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
		return turn([ 1 ], ($bm) => {
			return (() => {
				return handler(dispatched, $bm, [ 1 ]);
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
function set_chunk_pending(busy, $ci) {
	if (get(chunk_pending_signal) !== busy) {
		set3(chunk_pending_signal, busy, $ci, "std/src/browser/web/ui.vl:628:24");
	}
}
function clear_chunk_error($bZ) {
	const $ca = get(chunk_error_signal);
	let $cb = null;
	if ($ca[0] === 0) {
		const _reason = $ca[1];
		$cb = set3(chunk_error_signal, [ 1 ], $bZ, "std/src/browser/web/ui.vl:636:43");
	} else {
		$cb = undefined;
	}
	return $cb;
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
		let $cB = null;
		if (at + 1 < rows.length) {
			$cB = __at(rows, at + 1, "std/src/browser/web/ui.vl:838:39")[0];
		} else {
			$cB = self[0];
		}
		const end = __clone($cB);
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
			throw __panic("mount: no element with id \'" + id + "\'", "std/src/browser/web/ui.vl:2332:3");
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
	const $cT = turn([ 1 ], ($cS) => {
		return comp(body);
	});
	const built = $cT[0];
	const root = $cT[1];
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
	let $s = null;
	if (__at(parts, 0, "app.vl:38:5") === "docs" && parts.length === 2) {
		const $q = __parse_i32(__at(parts, 1, "app.vl:39:9"));
		let $r = null;
		if ($q[0] === 0) {
			const page = $q[1];
			return [ 1, page ];
		} else {
			$r = undefined;
		}
		$s = $r;
	}
	$s;
	return [ 2 ];
}
function href(route2) {
	const $bf = route2;
	let $bg = null;
	if ($bf[0] === 0) {
		$bg = "/";
	} else if ($bf[0] === 1) {
		const page = $bf[1];
		$bg = "/docs/" + page;
	} else {
		$bg = "/404";
	}
	return $bg;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $bM, $bN) {
	return child(child(view("section"), text(view("h2"), title), $bM, $bN), text(view("p"), body), $bM, $bN);
}
function app(route2, $aW, $aX) {
	chunk_preload(route2);
	return child2(child(child(child(view("main"), child(child(view("nav"), link("Home", [ 0 ], $aW, $aX), $aW, $aX), link("Docs", [ 1, 1 ], $aW, $aX), $aW, $aX), $aW, $aX), bind_text(class2(view("p"), "pending"), derive(pending(), (busy, $bp, $bq, $br) => {
		let $bs = null;
		if (busy) {
			$bs = "...";
		} else {
			$bs = "";
		}
		return $bs;
	}), $aW, $aX), $aW, $aX), bind_text2(class2(view("p"), "failed"), derive(chunk_error(), (reason, $bB, $bC, $bD) => {
		const $bE = reason;
		let $bF = null;
		if ($bE[0] === 0) {
			const text2 = $bE[1];
			let $bG = null;
			if (text2.length > 0) {
				$bG = "!";
			} else {
				$bG = "?";
			}
			$bF = $bG;
		} else {
			$bF = "";
		}
		return $bF;
	}), $aW, $aX), $aW, $aX), swap_split(__clone(route2), (current, $bH) => {
		const $bI = current;
		let $bJ = null;
		if ($bI[0] === 0) {
			$bJ = home_page($aW, $bH);
		} else if ($bI[0] === 1) {
			const page = $bI[1];
			$bJ = docs_page(page, $aW, $bH);
		} else {
			$bJ = not_found_page($aW, $bH);
		}
		return $bJ;
	}), $aW, $aX);
}
function eq(self, other) {
	const $cF = [ self, other ];
	let $cG = null;
	if ($cF[0][0] === 0 && $cF[1][0] === 0) {
		$cG = true;
	} else if ($cF[0][0] === 1 && $cF[1][0] === 1) {
		const s0 = $cF[0][1];
		const o0 = $cF[1][1];
		$cG = s0 === o0;
	} else if ($cF[0][0] === 2 && $cF[1][0] === 2) {
		$cG = true;
	} else {
		$cG = false;
	}
	return $cG;
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
	const $L = self;
	return $L[0] === 1;
}
function run_body(runs, body) {
	const run = renew(runs);
	const $U = nursery(run);
	let $V = null;
	if ($U[0] === 0) {
		const nursery2 = $U[1];
		$V = (($W) => {
			return (($X) => {
				return body($W, $X);
			})(nursery2);
		})(run);
	} else {
		$V = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1402:11");
		})();
	}
	return $V;
}
function run_once(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body(runs, ($D, $E) => {
		return (($F) => {
			return body($D, $F, $E);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs, tracker, body) {
	let value = run_once(runs, tracker, ($y, $z, $A) => {
		return body($y, $z, $A);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs, tracker, ($av, $aw, $ax) => {
			return body($av, $aw, $ax);
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
		return run_tracked(runs, tracker, ($v, $w, $x) => {
			return transform(value, $v, $w, $x);
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
function notify2(self, $d) {
	const traced = tracing.v;
	let $aE = null;
	if (traced) {
		$aE = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$aE = "";
	}
	const cause = $aE;
	const $aF = $d;
	let $aG = null;
	if ($aF[0] === 0) {
		const turn2 = $aF[1];
		$aG = queue_wakes(turn2, __clone(self[1].v), traced, cause);
	} else {
		const $aH = last(draining_turns.v);
		let $aI = null;
		if ($aH[0] === 0) {
			const draining = $aH[1];
			$aI = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $aJ = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$aJ = undefined;
				}
				$aJ;
			}
			$aI = undefined;
		}
		$aG = $aI;
	}
	return $aG;
}
function set2(self, value, $c, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $c);
}
function take(self, item, $aQ) {
	defer(self, () => {
		dispose(item, $aQ);
		return;
	});
	return __clone(item);
}
function cell(self, $t, $u) {
	const instance = start2(self);
	const pull = instance[0];
	const cached = new4(pull(), "std/src/reactive.vl:2347:28");
	const refreshed = instance[1](subscriber_of(() => {
		set2(cached, pull(), $t, "std/src/reactive.vl:2351:11");
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $t, $u);
	return cached;
}
function attr(self, name, value, $bh, $bi) {
	apply(value, self, name, $bh, $bi);
	return __clone(self);
}
function link_to(self, route2, $bd, $be) {
	const path = to_path(route2);
	return on_event(attr(attr(self, "href", path, $bd, $be), "draggable", "false", $bd, $be), "click", (event, $bj, $bk) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $bj ]);
		}
		return;
	});
}
function link(label, route2, $bb, $bc) {
	return text(link_to(view("a"), route2, $bb, $bc), label);
}
function child(self, content, $bn, $bo) {
	place(content, self, $bn, $bo);
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
	const $bz = nursery(run);
	let $bA = null;
	if ($bz[0] === 0) {
		const nursery2 = $bz[1];
		$bA = (($W) => {
			return (($X) => {
				return body($W, $X);
			})(nursery2);
		})(run);
	} else {
		$bA = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1402:11");
		})();
	}
	return $bA;
}
function run_once2(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body2(runs, ($D, $E) => {
		return (($F) => {
			return body($D, $F, $E);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked2(runs, tracker, body) {
	let value = run_once2(runs, tracker, ($y, $z, $A) => {
		return body($y, $z, $A);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once2(runs, tracker, ($av, $aw, $ax) => {
			return body($av, $aw, $ax);
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
		return run_tracked2(runs, tracker, ($v, $w, $x) => {
			return transform(value, $v, $w, $x);
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
function follow(flow, observer, $bw, $bx) {
	take(get_owner($bx), sub(flow, observer), $bw);
}
function bind_text(self, source, $bt, $bu) {
	const element = __clone(self[0]);
	follow(source, (value, $bv) => {
		element.textContent = value;
		return;
	}, $bt, $bu);
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
		return run_tracked2(runs, tracker, ($v, $w, $x) => {
			return transform(value, $v, $w, $x);
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
function follow2(flow, observer, $bw, $bx) {
	take(get_owner($bx), sub2(flow, observer), $bw);
}
function bind_text2(self, source, $bt, $bu) {
	const element = __clone(self[0]);
	follow2(source, (value, $bv) => {
		element.textContent = value;
		return;
	}, $bt, $bu);
	return __clone(self);
}
function chunk_preload(source) {
	__chunk_preload(__chunk_arm(get(source)));
}
function set3(self, value, $c, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $c);
}
function observe(signal, observer) {
	const cell2 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $cr = [ 0, cell2 ];
		let $cs = null;
		if ($cr[0] === 0) {
			const live = $cr[1];
			$cs = observer(live.v);
		} else {
			$cs = undefined;
		}
		return $cs;
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
function follow3(flow, observer, $bw, $bx) {
	take(get_owner($bx), sub3(flow, observer), $bw);
}
function swap_split(source, render, $bU) {
	const gated = new4(get(source), "std/src/browser/web/ui.vl:1183:41");
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance2 = (value, $bV) => {
		armed.v = true;
		set2(gated, value, [ 0, $bV ], "std/src/browser/web/ui.vl:1195:9");
		return;
	};
	const wire = ($bW) => {
		follow3(__clone(source), (value, $bX) => {
			return turn([ 1 ], ($bY) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $bY ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $bY ]);
					advance2(value, $bY);
				} else {
					set_chunk_pending(true, [ 0, $bY ]);
					__chunk_load(arm, () => {
						return turn([ 1 ], ($cp) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $cp ]);
								advance2(value, $cp);
							}
							return;
						});
					}, (reason) => {
						return turn([ 1 ], ($cq) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $cq ]);
								set3(chunk_error_signal, [ 0, reason ], [ 0, $cq ], "std/src/browser/web/ui.vl:1230:26");
							}
							return;
						});
					});
				}
				return;
			});
		}, $bU, $bW);
		return;
	};
	return [ [ 1 ], render, [ 0, __clone(gated) ], armed, wire ];
}
function open_row_before(self, content, end, $cO, $cP) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(content, [ __clone(staging) ], $cO, $cP);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function open_row(self, content, $cM, $cN) {
	return open_row_before(self, content, self[0], $cM, $cN);
}
function run_with_owner(owner, body) {
	return body(owner);
}
function place_swap(parent, source, render, armed, $cx, $cy) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($cy), () => {
		const $cz = live_owner.v;
		let $cA = null;
		if ($cz[0] === 1) {
			$cA = $cz;
		} else {
			$cA = [ 0, dispose2($cz[1]) ];
		}
		$cA;
		close(region);
		return;
	});
	follow3(source, (value, $cC) => {
		const $cD = last_value.v;
		let $cE = null;
		if ($cD[0] === 0) {
			const previous = $cD[1];
			$cE = eq(previous, value);
		} else {
			$cE = false;
		}
		const unchanged = $cE;
		if (armed.v && !(unchanged)) {
			const $cH = live_owner.v;
			let $cI = null;
			if ($cH[0] === 1) {
				$cI = $cH;
			} else {
				$cI = [ 0, dispose2($cH[1]) ];
			}
			$cI;
			const $cJ = live_row.v;
			let $cK = null;
			if ($cJ[0] === 0) {
				const row = $cJ[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$cK = undefined;
			} else {
				$cK = undefined;
			}
			$cK;
			const owner = new3();
			const row2 = run_with_owner(owner, ($cL) => {
				return open_row(region, render(value, $cL), $cx, $cL);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $cx, $cy);
}
function place2(self, parent, $ct, $cu) {
	self[4]($cu);
	const render = self[1];
	const armed = self[3];
	const $cv = self[0];
	let $cw = null;
	if ($cv[0] === 0) {
		const source = $cv[1];
		$cw = place_swap(parent, __clone(source), render, armed, $ct, $cu);
	} else {
		const $cQ = self[2];
		let $cR = null;
		if ($cQ[0] === 0) {
			const gated = $cQ[1];
			$cR = place_swap(parent, __clone(gated), render, armed, $ct, $cu);
		} else {
			$cR = undefined;
		}
		$cw = $cR;
	}
	return $cw;
}
function child2(self, content, $bn, $bo) {
	place2(content, self, $bn, $bo);
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
const path_signal = new4("", "std/src/web/router.vl:59:48");
const wired = __shared_new(false);
const chunk_pending_signal = new4(false, "std/src/browser/web/ui.vl:574:58");
const chunk_error_signal = new4([ 1 ], "std/src/browser/web/ui.vl:579:63");
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
const route = cell(derive(current_path([ 1 ]), (path, $n, $o, $p) => {
	return parse(path);
}), [ 1 ], [ 1 ]);
mount_root("app", ($aV) => {
	return app(route, [ 1 ], $aV);
});
//# sourceMappingURL=app.js.map
