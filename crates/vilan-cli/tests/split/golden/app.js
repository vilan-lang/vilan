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
function home_page($bG, $bH) {
	return __vilan_chunks.fn.home_page($bG, $bH);
}
function docs_page(page, $bK, $bL) {
	return __vilan_chunks.fn.docs_page(page, $bK, $bL);
}
function not_found_page($bO, $bP) {
	return __vilan_chunks.fn.not_found_page($bO, $bP);
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
		let $g = null;
		if (subscriber[3]) {
			if (!(turn2[3].v.has(key))) {
				turn2[3].v.set(key, true);
				turn2[1].v.push(__clone(subscriber));
			}
			$g = undefined;
		} else if (!(turn2[2].v.has(key))) {
			turn2[2].v.set(key, true);
			let index = turn2[0].v.length;
			while (index > 0 && __at(turn2[0].v, index - 1, "std/src/reactive.vl:414:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn2[0].v, index, __clone(subscriber), "std/src/reactive.vl:417:25");
		}
		$g;
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
						if (subscriber[2].v) {
							subscriber[1]();
						}
						budget = budget - 1;
					}
				}
				const wave = turn2[0].v;
				turn2[0].v = [  ];
				turn2[2].v = new Map();
				for (const subscriber2 of wave) {
					if (subscriber2[2].v) {
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
	const $ag = turn2;
	let $ah = null;
	if ($ag[0] === 0) {
		const ambient = $ag[1];
		$ah = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $ai = last(draining_turns.v);
		let $aj = null;
		if ($ai[0] === 0) {
			const draining = $ai[1];
			$aj = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$aj = undefined;
		}
		$ah = $aj;
	}
	return $ah;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $aN) {
	const $aO = $aN;
	let $aP = null;
	if ($aO[0] === 0) {
		const established = $aO[1];
		$aP = [ 0, established ];
	} else {
		$aP = last(draining_turns.v);
	}
	const ambient = $aP;
	release_under(self, ambient);
}
function detach(handle) {
	const $al = last(releasing_turns.v);
	let $am = null;
	if ($al[0] === 0) {
		const at_release = $al[1];
		$am = at_release;
	} else {
		$am = last(draining_turns.v);
	}
	const turn2 = $am;
	release_under(handle, turn2);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $an = [ 0, handle[0] ];
	let $ao = null;
	if ($an[0] === 0) {
		const subscribers = $an[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$ao = undefined;
	} else {
		$ao = undefined;
	}
	$ao;
	const $ap = ambient;
	let $aq = null;
	if ($ap[0] === 0) {
		const turn2 = $ap[1];
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
		$aq = undefined;
	} else {
		$aq = undefined;
	}
	$aq;
	const $ar = handle[3].v;
	let $as = null;
	if ($ar[0] === 0) {
		const release = $ar[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$as = undefined;
	} else {
		$as = undefined;
	}
	return $as;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aQ = null;
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
		$aQ = undefined;
	}
	return $aQ;
}
function renew(self) {
	const $E = self[0].v[3];
	let $F = null;
	if ($E[0] === 0) {
		const nursery2 = __clone($E[1]);
		let $G = null;
		if (has_spawned(nursery2)) {
			$G = [ 1 ];
		} else {
			$G = [ 0, nursery2 ];
		}
		$F = $G;
	} else {
		$F = [ 1 ];
	}
	const carried = $F;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $R = null;
	if (is_disposed(self)) {
		$R = [ 1 ];
	} else {
		$R = self[0].v[3];
	}
	return $R;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $Q = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $H = held[3];
		let $I = null;
		if ($H[0] === 0) {
			const nursery2 = $H[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$I = undefined;
		} else {
			$I = undefined;
		}
		$I;
		let $P = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $K = __guarded(cleanup);
				let $L = null;
				if ($K[0] === 0) {
					const message = $K[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$L = undefined;
				} else {
					$L = undefined;
				}
				$L;
			}
			const $N = failure;
			let $O = null;
			if ($N[0] === 0) {
				const message2 = $N[1];
				$O = (() => {
					throw __panic(message2, "std/src/reactive.vl:1207:27");
				})();
			} else {
				$O = undefined;
			}
			$P = $O;
		}
		$Q = $P;
	}
	return $Q;
}
function get_owner($bu) {
	return $bu;
}
function register_with_owner(subscription, $aI, $aJ) {
	const $aK = $aJ;
	let $aL = null;
	if ($aK[0] === 0) {
		const owner = $aK[1];
		$aL = take(owner, subscription, $aI);
	} else {
		$aL = __clone(subscription);
	}
	return $aL;
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
	const $z = tracker[0].v[6];
	let $A = null;
	if ($z[0] === 0) {
		const lists = $z[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$A = undefined;
	} else {
		$A = undefined;
	}
	$A;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $W = tracker[0].v[6];
	let $X = null;
	if ($W[0] === 0) {
		const lists = $W[1];
		$X = lists;
	} else {
		return;
		$X = undefined;
	}
	const lists2 = $X;
	const $Y = tracker[0].v[5];
	let $Z = null;
	if ($Y[0] === 0) {
		const target = __clone($Y[1]);
		$Z = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$Z = undefined;
	}
	$Z;
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
		const $ae = reusable(held, kept, dependency[0], position);
		let $af = null;
		if ($ae[0] === 0) {
			const index = $ae[1];
			__at_put(kept, index, true, "std/src/reactive.vl:1624:5");
			next.push(__clone(__at(held, index, "std/src/reactive.vl:1625:15")));
			$af = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$af = undefined;
		}
		$af;
		position = position + 1;
	}
	lists.v[2] = next;
	let index2 = 0;
	for (const edge of held) {
		if (!(__at(kept, index2, "std/src/reactive.vl:1639:7"))) {
			detach(edge[1]);
		}
		index2 = index2 + 1;
	}
	tracker[0].v[3] = false;
}
function reusable(held, kept, identity, position) {
	const $aa = identity;
	let $ab = null;
	if ($aa[0] === 0) {
		const wanted = $aa[1];
		if (position < held.length && !(__at(kept, position, "std/src/reactive.vl:1662:9")) && same_identity(__at(held, position, "std/src/reactive.vl:1663:22")[0], wanted)) {
			return [ 0, position ];
		}
		let index = 0;
		while (index < held.length) {
			if (!(__at(kept, index, "std/src/reactive.vl:1668:9")) && same_identity(__at(held, index, "std/src/reactive.vl:1668:38")[0], wanted)) {
				return [ 0, index ];
			}
			index = index + 1;
		}
		$ab = [ 1 ];
	} else {
		$ab = [ 1 ];
	}
	return $ab;
}
function same_identity(identity, wanted) {
	const $ac = identity;
	let $ad = null;
	if ($ac[0] === 0) {
		const held = $ac[1];
		$ad = held === wanted;
	} else {
		$ad = false;
	}
	return $ad;
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
	const $aw = tracker[0].v[6];
	let $ax = null;
	if ($aw[0] === 0) {
		const lists = $aw[1];
		$ax = lists;
	} else {
		return;
		$ax = undefined;
	}
	const lists2 = $ax;
	let $ay = null;
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
		$ay = undefined;
	}
	return $ay;
}
function forget_reads(tracker) {
	const $az = tracker[0].v[6];
	let $aA = null;
	if ($az[0] === 0) {
		const lists = $az[1];
		$aA = lists;
	} else {
		return;
		$aA = undefined;
	}
	const lists2 = $aA;
	let $aB = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aB = undefined;
	}
	$aB;
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
		const $aG = previous;
		let $aH = null;
		if ($aG[0] === 0) {
			const earlier = $aG[1];
			$aH = earlier();
		} else {
			$aH = undefined;
		}
		return $aH;
	} ];
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function ensure_wired($b) {
	if (!(wired.v)) {
		wired.v = true;
		set(path_signal, __router_path(), $b);
		__dom_window().addEventListener("popstate", () => {
			return turn([ 1 ], ($k) => {
				set(path_signal, __router_path(), [ 0, $k ]);
				return;
			});
		});
	}
}
function current_path($a) {
	ensure_wired($a);
	return path_signal;
}
function navigate(path, $bh) {
	ensure_wired($bh);
	history.pushState("", "", path);
	set(path_signal, path, $bh);
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
	let $aW = null;
	if (is_svg_tag(tag)) {
		$aW = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$aW = [ document.createElement(tag) ];
	}
	return $aW;
}
function is_svg_tag(tag) {
	const $aU = tag;
	let $aV = null;
	if ($aU === "svg") {
		$aV = true;
	} else if ($aU === "path") {
		$aV = true;
	} else if ($aU === "circle") {
		$aV = true;
	} else if ($aU === "ellipse") {
		$aV = true;
	} else if ($aU === "rect") {
		$aV = true;
	} else if ($aU === "line") {
		$aV = true;
	} else if ($aU === "polyline") {
		$aV = true;
	} else if ($aU === "polygon") {
		$aV = true;
	} else if ($aU === "g") {
		$aV = true;
	} else if ($aU === "defs") {
		$aV = true;
	} else if ($aU === "use") {
		$aV = true;
	} else if ($aU === "symbol") {
		$aV = true;
	} else if ($aU === "marker") {
		$aV = true;
	} else if ($aU === "pattern") {
		$aV = true;
	} else if ($aU === "mask") {
		$aV = true;
	} else if ($aU === "clipPath") {
		$aV = true;
	} else if ($aU === "linearGradient") {
		$aV = true;
	} else if ($aU === "radialGradient") {
		$aV = true;
	} else if ($aU === "stop") {
		$aV = true;
	} else if ($aU === "text") {
		$aV = true;
	} else if ($aU === "tspan") {
		$aV = true;
	} else if ($aU === "textPath") {
		$aV = true;
	} else if ($aU === "filter") {
		$aV = true;
	} else if ($aU === "foreignObject") {
		$aV = true;
	} else if ($aU === "feGaussianBlur") {
		$aV = true;
	} else if ($aU === "feColorMatrix") {
		$aV = true;
	} else if ($aU === "feOffset") {
		$aV = true;
	} else if ($aU === "feMerge") {
		$aV = true;
	} else if ($aU === "feMergeNode") {
		$aV = true;
	} else if ($aU === "feFlood") {
		$aV = true;
	} else if ($aU === "feComposite") {
		$aV = true;
	} else if ($aU === "feBlend") {
		$aV = true;
	} else if ($aU === "feDropShadow") {
		$aV = true;
	} else {
		$aV = false;
	}
	return $aV;
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
		return turn([ 1 ], ($bi) => {
			return (() => {
				return handler(dispatched, $bi, [ 1 ]);
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
function set_chunk_pending(busy, $cc) {
	if (get(chunk_pending_signal) !== busy) {
		set3(chunk_pending_signal, busy, $cc);
	}
}
function clear_chunk_error($bV) {
	const $bW = get(chunk_error_signal);
	let $bX = null;
	if ($bW[0] === 0) {
		const _reason = $bW[1];
		$bX = set3(chunk_error_signal, [ 1 ], $bV);
	} else {
		$bX = undefined;
	}
	return $bX;
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
		let $ct = null;
		if (at + 1 < rows.length) {
			$ct = __at(rows, at + 1, "std/src/browser/web/ui.vl:829:39")[0];
		} else {
			$ct = self[0];
		}
		const end = __clone($ct);
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
			throw __panic("mount: no element with id \'" + id + "\'", "std/src/browser/web/ui.vl:2262:3");
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
	const $cL = turn([ 1 ], ($cK) => {
		return comp(body);
	});
	const built = $cL[0];
	const root = $cL[1];
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
function on_teardown(cleanup) {
	if (__hmr_active()) {
		__hmr_register_teardown(cleanup);
	}
}
function parse(path) {
	const parts = segments(path);
	if (parts.length === 0) {
		return [ 0 ];
	}
	let $q = null;
	if (__at(parts, 0, "app.vl:38:5") === "docs" && parts.length === 2) {
		const $o = __parse_i32(__at(parts, 1, "app.vl:39:9"));
		let $p = null;
		if ($o[0] === 0) {
			const page = $o[1];
			return [ 1, page ];
		} else {
			$p = undefined;
		}
		$q = $p;
	}
	$q;
	return [ 2 ];
}
function href(route2) {
	const $bb = route2;
	let $bc = null;
	if ($bb[0] === 0) {
		$bc = "/";
	} else if ($bb[0] === 1) {
		const page = $bb[1];
		$bc = "/docs/" + page;
	} else {
		$bc = "/404";
	}
	return $bc;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $bI, $bJ) {
	return child(child(view("section"), text(view("h2"), title), $bI, $bJ), text(view("p"), body), $bI, $bJ);
}
function app(route2, $aS, $aT) {
	chunk_preload(route2);
	return child2(child(child(child(view("main"), child(child(view("nav"), link("Home", [ 0 ], $aS, $aT), $aS, $aT), link("Docs", [ 1, 1 ], $aS, $aT), $aS, $aT), $aS, $aT), bind_text(class2(view("p"), "pending"), derive(pending(), (busy, $bl, $bm, $bn) => {
		let $bo = null;
		if (busy) {
			$bo = "...";
		} else {
			$bo = "";
		}
		return $bo;
	}), $aS, $aT), $aS, $aT), bind_text2(class2(view("p"), "failed"), derive(chunk_error(), (reason, $bx, $by, $bz) => {
		const $bA = reason;
		let $bB = null;
		if ($bA[0] === 0) {
			const text2 = $bA[1];
			let $bC = null;
			if (text2.length > 0) {
				$bC = "!";
			} else {
				$bC = "?";
			}
			$bB = $bC;
		} else {
			$bB = "";
		}
		return $bB;
	}), $aS, $aT), $aS, $aT), swap_split(__clone(route2), (current, $bD) => {
		const $bE = current;
		let $bF = null;
		if ($bE[0] === 0) {
			$bF = home_page($aS, $bD);
		} else if ($bE[0] === 1) {
			const page = $bE[1];
			$bF = docs_page(page, $aS, $bD);
		} else {
			$bF = not_found_page($aS, $bD);
		}
		return $bF;
	}), $aS, $aT);
}
function eq(self, other) {
	const $cx = [ self, other ];
	let $cy = null;
	if ($cx[0][0] === 0 && $cx[1][0] === 0) {
		$cy = true;
	} else if ($cx[0][0] === 1 && $cx[1][0] === 1) {
		const s0 = $cx[0][1];
		const o0 = $cx[1][1];
		$cy = s0 === o0;
	} else if ($cx[0][0] === 2 && $cx[1][0] === 2) {
		$cy = true;
	} else {
		$cy = false;
	}
	return $cy;
}
function new4(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $h = null;
	if (is_empty(self)) {
		$h = [ 1 ];
	} else {
		$h = __list_get(self, self.length - 1);
	}
	return $h;
}
function notify(self, $d) {
	const $e = $d;
	let $f = null;
	if ($e[0] === 0) {
		const turn2 = $e[1];
		$f = enqueue(turn2, __clone(self[1].v));
	} else {
		const $i = last(draining_turns.v);
		let $j = null;
		if ($i[0] === 0) {
			const draining = $i[1];
			$j = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$j = undefined;
		}
		$f = $j;
	}
	return $f;
}
function set(self, value, $c) {
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
	const $J = self;
	return $J[0] === 1;
}
function run_body(runs, body) {
	const run = renew(runs);
	const $S = nursery(run);
	let $T = null;
	if ($S[0] === 0) {
		const nursery2 = $S[1];
		$T = (($U) => {
			return (($V) => {
				return body($U, $V);
			})(nursery2);
		})(run);
	} else {
		$T = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1318:11");
		})();
	}
	return $T;
}
function run_once(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body(runs, ($B, $C) => {
		return (($D) => {
			return body($B, $D, $C);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs, tracker, body) {
	let value = run_once(runs, tracker, ($w, $x, $y) => {
		return body($w, $x, $y);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs, tracker, ($at, $au, $av) => {
			return body($at, $au, $av);
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
		return run_tracked(runs, tracker, ($t, $u, $v) => {
			return transform(value, $t, $u, $v);
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
	const $aC = $d;
	let $aD = null;
	if ($aC[0] === 0) {
		const turn2 = $aC[1];
		$aD = enqueue(turn2, __clone(self[1].v));
	} else {
		const $aE = last(draining_turns.v);
		let $aF = null;
		if ($aE[0] === 0) {
			const draining = $aE[1];
			$aF = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$aF = undefined;
		}
		$aD = $aF;
	}
	return $aD;
}
function set2(self, value, $c) {
	self[0].v = __clone(value);
	notify2(self, $c);
}
function take(self, item, $aM) {
	defer(self, () => {
		dispose(item, $aM);
		return;
	});
	return __clone(item);
}
function cell(self, $r, $s) {
	const instance = start2(self);
	const pull = instance[0];
	const cached = new4(pull());
	const refreshed = instance[1](subscriber_of(() => {
		set2(cached, pull(), $r);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $r, $s);
	return cached;
}
function attr(self, name, value, $bd, $be) {
	apply(value, self, name, $bd, $be);
	return __clone(self);
}
function link_to(self, route2, $aZ, $ba) {
	const path = to_path(route2);
	return on_event(attr(attr(self, "href", path, $aZ, $ba), "draggable", "false", $aZ, $ba), "click", (event, $bf, $bg) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $bf ]);
		}
		return;
	});
}
function link(label, route2, $aX, $aY) {
	return text(link_to(view("a"), route2, $aX, $aY), label);
}
function child(self, content, $bj, $bk) {
	place(content, self, $bj, $bk);
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
	const $bv = nursery(run);
	let $bw = null;
	if ($bv[0] === 0) {
		const nursery2 = $bv[1];
		$bw = (($U) => {
			return (($V) => {
				return body($U, $V);
			})(nursery2);
		})(run);
	} else {
		$bw = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1318:11");
		})();
	}
	return $bw;
}
function run_once2(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body2(runs, ($B, $C) => {
		return (($D) => {
			return body($B, $D, $C);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked2(runs, tracker, body) {
	let value = run_once2(runs, tracker, ($w, $x, $y) => {
		return body($w, $x, $y);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once2(runs, tracker, ($at, $au, $av) => {
			return body($at, $au, $av);
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
		return run_tracked2(runs, tracker, ($t, $u, $v) => {
			return transform(value, $t, $u, $v);
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
function follow(flow, observer, $bs, $bt) {
	take(get_owner($bt), sub(flow, observer), $bs);
}
function bind_text(self, source, $bp, $bq) {
	const element = __clone(self[0]);
	follow(source, (value, $br) => {
		element.textContent = value;
		return;
	}, $bp, $bq);
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
		return run_tracked2(runs, tracker, ($t, $u, $v) => {
			return transform(value, $t, $u, $v);
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
function follow2(flow, observer, $bs, $bt) {
	take(get_owner($bt), sub2(flow, observer), $bs);
}
function bind_text2(self, source, $bp, $bq) {
	const element = __clone(self[0]);
	follow2(source, (value, $br) => {
		element.textContent = value;
		return;
	}, $bp, $bq);
	return __clone(self);
}
function chunk_preload(source) {
	__chunk_preload(__chunk_arm(get(source)));
}
function set3(self, value, $c) {
	self[0].v = __clone(value);
	notify2(self, $c);
}
function observe(signal, observer) {
	const cell2 = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $cj = [ 0, cell2 ];
		let $ck = null;
		if ($cj[0] === 0) {
			const live = $cj[1];
			$ck = observer(live.v);
		} else {
			$ck = undefined;
		}
		return $ck;
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
function follow3(flow, observer, $bs, $bt) {
	take(get_owner($bt), sub3(flow, observer), $bs);
}
function swap_split(source, render, $bQ) {
	const gated = new4(get(source));
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance2 = (value, $bR) => {
		armed.v = true;
		set2(gated, value, [ 0, $bR ]);
		return;
	};
	const wire = ($bS) => {
		follow3(__clone(source), (value, $bT) => {
			return turn([ 1 ], ($bU) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $bU ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $bU ]);
					advance2(value, $bU);
				} else {
					set_chunk_pending(true, [ 0, $bU ]);
					__chunk_load(arm, () => {
						return turn([ 1 ], ($ch) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $ch ]);
								advance2(value, $ch);
							}
							return;
						});
					}, (reason) => {
						return turn([ 1 ], ($ci) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $ci ]);
								set3(chunk_error_signal, [ 0, reason ], [ 0, $ci ]);
							}
							return;
						});
					});
				}
				return;
			});
		}, $bQ, $bS);
		return;
	};
	return [ [ 1 ], render, [ 0, __clone(gated) ], armed, wire ];
}
function open_row_before(self, content, end, $cG, $cH) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(content, [ __clone(staging) ], $cG, $cH);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function open_row(self, content, $cE, $cF) {
	return open_row_before(self, content, self[0], $cE, $cF);
}
function run_with_owner(owner, body) {
	return body(owner);
}
function place_swap(parent, source, render, armed, $cp, $cq) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($cq), () => {
		const $cr = live_owner.v;
		let $cs = null;
		if ($cr[0] === 1) {
			$cs = $cr;
		} else {
			$cs = [ 0, dispose2($cr[1]) ];
		}
		$cs;
		close(region);
		return;
	});
	follow3(source, (value, $cu) => {
		const $cv = last_value.v;
		let $cw = null;
		if ($cv[0] === 0) {
			const previous = $cv[1];
			$cw = eq(previous, value);
		} else {
			$cw = false;
		}
		const unchanged = $cw;
		if (armed.v && !(unchanged)) {
			const $cz = live_owner.v;
			let $cA = null;
			if ($cz[0] === 1) {
				$cA = $cz;
			} else {
				$cA = [ 0, dispose2($cz[1]) ];
			}
			$cA;
			const $cB = live_row.v;
			let $cC = null;
			if ($cB[0] === 0) {
				const row = $cB[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$cC = undefined;
			} else {
				$cC = undefined;
			}
			$cC;
			const owner = new3();
			const row2 = run_with_owner(owner, ($cD) => {
				return open_row(region, render(value, $cD), $cp, $cD);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $cp, $cq);
}
function place2(self, parent, $cl, $cm) {
	self[4]($cm);
	const render = self[1];
	const armed = self[3];
	const $cn = self[0];
	let $co = null;
	if ($cn[0] === 0) {
		const source = $cn[1];
		$co = place_swap(parent, __clone(source), render, armed, $cl, $cm);
	} else {
		const $cI = self[2];
		let $cJ = null;
		if ($cI[0] === 0) {
			const gated = $cI[1];
			$cJ = place_swap(parent, __clone(gated), render, armed, $cl, $cm);
		} else {
			$cJ = undefined;
		}
		$co = $cJ;
	}
	return $co;
}
function child2(self, content, $bj, $bk) {
	place2(content, self, $bj, $bk);
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
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const run_nurseries_allocated_count = __shared_new(0);
const path_signal = new4("");
const wired = __shared_new(false);
const chunk_pending_signal = new4(false);
const chunk_error_signal = new4([ 1 ]);
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
const route = cell(derive(current_path([ 1 ]), (path, $l, $m, $n) => {
	return parse(path);
}), [ 1 ], [ 1 ]);
mount_root("app", ($aR) => {
	return app(route, [ 1 ], $aR);
});
