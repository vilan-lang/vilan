function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __at_put(list, index, value) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
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
function __insert_at(list, index, value) {
	if (index >= 0 && index < list.length) return void list.splice(index, 0, value);
	if (index === list.length) return void list.push(value);
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
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
	throw typeof winner.error === "string" ? winner.error + " (in task spawned in " + winner.origin + ")" : winner.error;
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
function home_page($cI, $cJ) {
	return __vilan_chunks.fn.home_page($cI, $cJ);
}
function docs_page(page, $cM, $cN) {
	return __vilan_chunks.fn.docs_page(page, $cM, $cN);
}
function not_found_page($cQ, $cR) {
	return __vilan_chunks.fn.not_found_page($cQ, $cR);
}
function hash(self) {
	return __hash(self);
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function mint_subscriber(notify) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify, derived);
}
function subscriber_of(notify, derived) {
	return [ fresh_id(), notify, __shared_new(true), derived ];
}
function new2() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return $m(self[0].v) && $m(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $l = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$l = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$l;
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
				while (!($m(turn[1].v)) && budget > 0) {
					const derivations = turn[1].v;
					turn[1].v = [  ];
					turn[3].v = new Map();
					for (const subscriber of derivations) {
						if (subscriber[2].v) {
							subscriber[1]();
						}
						budget = budget - 1;
					}
				}
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[2].v = new Map();
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
			turn[4].v = false;
			return;
		});
	}
}
function defer_subscriber(turn, subscriber) {
	const $aD = turn;
	let $aE = null;
	if ($aD[0] === 0) {
		const ambient = $aD[1];
		$aE = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $aF = $n(draining_turns.v);
		let $aG = null;
		if ($aF[0] === 0) {
			const draining = $aF[1];
			$aG = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$aG = undefined;
		}
		$aE = $aG;
	}
	return $aE;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $bq) {
	const $br = $bq;
	let $bs = null;
	if ($br[0] === 0) {
		const established = $br[1];
		$bs = [ 0, established ];
	} else {
		$bs = $n(draining_turns.v);
	}
	const ambient = $bs;
	release_under(self, ambient);
}
function detach(handle) {
	const $aK = $n(releasing_turns.v);
	let $aL = null;
	if ($aK[0] === 0) {
		const at_release = $aK[1];
		$aL = at_release;
	} else {
		$aL = $n(draining_turns.v);
	}
	const turn = $aL;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $aM = [ 0, handle[0] ];
	let $aN = null;
	if ($aM[0] === 0) {
		const subscribers = $aM[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aN = undefined;
	} else {
		$aN = undefined;
	}
	$aN;
	const $aO = ambient;
	let $aP = null;
	if ($aO[0] === 0) {
		const turn = $aO[1];
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
		$aP = undefined;
	} else {
		$aP = undefined;
	}
	$aP;
	const $aQ = handle[3].v;
	let $aR = null;
	if ($aQ[0] === 0) {
		const release = $aQ[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aR = undefined;
	} else {
		$aR = undefined;
	}
	return $aR;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $bt = null;
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
		$bt = undefined;
	}
	return $bt;
}
function renew(self) {
	const $Y = self[0].v[3];
	let $Z = null;
	if ($Y[0] === 0) {
		const nursery2 = __clone($Y[1]);
		let $aa = null;
		if (has_spawned(nursery2)) {
			$aa = [ 1 ];
		} else {
			$aa = [ 0, nursery2 ];
		}
		$Z = $aa;
	} else {
		$Z = [ 1 ];
	}
	const carried = $Z;
	advance([ self[0], self[0].v[0] ], carried);
	if ($ad(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $an = null;
	if (is_disposed(self)) {
		$an = [ 1 ];
	} else {
		$an = self[0].v[3];
	}
	return $an;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $am = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $ab = held[3];
		let $ac = null;
		if ($ab[0] === 0) {
			const nursery2 = $ab[1];
			if ($ad(carried)) {
				nursery2.cancel();
			}
			$ac = undefined;
		} else {
			$ac = undefined;
		}
		$ac;
		let $al = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $af = __guarded(cleanup);
				let $ag = null;
				if ($af[0] === 0) {
					const message = $af[1];
					if ($ad(failure)) {
						failure = [ 0, message ];
					}
					$ag = undefined;
				} else {
					$ag = undefined;
				}
				$ag;
			}
			const $aj = failure;
			let $ak = null;
			if ($aj[0] === 0) {
				const message2 = $aj[1];
				$ak = (() => {
					throw message2;
				})();
			} else {
				$ak = undefined;
			}
			$al = $ak;
		}
		$am = $al;
	}
	return $am;
}
function get_owner($cc) {
	return $cc;
}
function register_with_owner(subscription, $bk, $bl) {
	const $bm = $bl;
	let $bn = null;
	if ($bm[0] === 0) {
		const owner = $bm[1];
		$bn = $bo(owner, subscription, $bk);
	} else {
		$bn = __clone(subscription);
	}
	return $bn;
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
	const $R = tracker[0].v[6];
	let $S = null;
	if ($R[0] === 0) {
		const lists = $R[1];
		if (!($m(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$S = undefined;
	} else {
		$S = undefined;
	}
	$S;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $as = tracker[0].v[6];
	let $at = null;
	if ($as[0] === 0) {
		const lists = $as[1];
		$at = lists;
	} else {
		return;
		$at = undefined;
	}
	const lists2 = $at;
	const $au = tracker[0].v[5];
	let $av = null;
	if ($au[0] === 0) {
		const target = __clone($au[1]);
		$av = reconnect(tracker, lists2, target);
	} else {
		if (!($m(lists2.v[0])) || !($m(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$av = undefined;
	}
	$av;
	if (!($m(lists2.v[0]))) {
		lists2.v[0] = [  ];
	}
}
function reconnect(tracker, lists, target) {
	if ($m(lists.v[0]) && $m(lists.v[2])) {
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
		const $aB = reusable(held, kept, dependency[0], position);
		let $aC = null;
		if ($aB[0] === 0) {
			const index = $aB[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$aC = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$aC = undefined;
		}
		$aC;
		position = position + 1;
	}
	lists.v[2] = next;
	let index2 = 0;
	for (const edge of held) {
		if (!(__at(kept, index2))) {
			detach(edge[1]);
		}
		index2 = index2 + 1;
	}
	tracker[0].v[3] = false;
}
function reusable(held, kept, identity, position) {
	const $ax = identity;
	let $ay = null;
	if ($ax[0] === 0) {
		const wanted = $ax[1];
		if (position < held.length && !(__at(kept, position)) && same_identity(__at(held, position)[0], wanted)) {
			return [ 0, position ];
		}
		let index = 0;
		while (index < held.length) {
			if (!(__at(kept, index)) && same_identity(__at(held, index)[0], wanted)) {
				return [ 0, index ];
			}
			index = index + 1;
		}
		$ay = [ 1 ];
	} else {
		$ay = [ 1 ];
	}
	return $ay;
}
function same_identity(identity, wanted) {
	const $az = identity;
	let $aA = null;
	if ($az[0] === 0) {
		const held = $az[1];
		$aA = held === wanted;
	} else {
		$aA = false;
	}
	return $aA;
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
	const $aV = tracker[0].v[6];
	let $aW = null;
	if ($aV[0] === 0) {
		const lists = $aV[1];
		$aW = lists;
	} else {
		return;
		$aW = undefined;
	}
	const lists2 = $aW;
	let $aX = null;
	if (!($m(lists2.v[1]))) {
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
		$aX = undefined;
	}
	return $aX;
}
function forget_reads(tracker) {
	const $aY = tracker[0].v[6];
	let $aZ = null;
	if ($aY[0] === 0) {
		const lists = $aY[1];
		$aZ = lists;
	} else {
		return;
		$aZ = undefined;
	}
	const lists2 = $aZ;
	let $ba = null;
	if (!($m(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$ba = undefined;
	}
	$ba;
	if (!($m(lists2.v[1]))) {
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
		const $bi = previous;
		let $bj = null;
		if ($bi[0] === 0) {
			const earlier = $bi[1];
			$bj = earlier();
		} else {
			$bj = undefined;
		}
		return $bj;
	} ];
}
function ensure_wired($e) {
	if (!(wired.v)) {
		wired.v = true;
		$f(path_signal, __router_path(), $e);
		__dom_window().addEventListener("popstate", () => {
			return $t([ 1 ], ($s) => {
				$f(path_signal, __router_path(), [ 0, $s ]);
				return;
			});
		});
	}
}
function current_path($d) {
	ensure_wired($d);
	return path_signal;
}
function navigate(path, $bM) {
	ensure_wired($bM);
	history.pushState("", "", path);
	$f(path_signal, path, $bM);
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
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function view(tag) {
	let $bz = null;
	if (is_svg_tag(tag)) {
		$bz = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$bz = [ document.createElement(tag) ];
	}
	return $bz;
}
function is_svg_tag(tag) {
	const $bx = tag;
	let $by = null;
	if ($bx === "svg") {
		$by = true;
	} else if ($bx === "path") {
		$by = true;
	} else if ($bx === "circle") {
		$by = true;
	} else if ($bx === "ellipse") {
		$by = true;
	} else if ($bx === "rect") {
		$by = true;
	} else if ($bx === "line") {
		$by = true;
	} else if ($bx === "polyline") {
		$by = true;
	} else if ($bx === "polygon") {
		$by = true;
	} else if ($bx === "g") {
		$by = true;
	} else if ($bx === "defs") {
		$by = true;
	} else if ($bx === "use") {
		$by = true;
	} else if ($bx === "symbol") {
		$by = true;
	} else if ($bx === "marker") {
		$by = true;
	} else if ($bx === "pattern") {
		$by = true;
	} else if ($bx === "mask") {
		$by = true;
	} else if ($bx === "clipPath") {
		$by = true;
	} else if ($bx === "linearGradient") {
		$by = true;
	} else if ($bx === "radialGradient") {
		$by = true;
	} else if ($bx === "stop") {
		$by = true;
	} else if ($bx === "text") {
		$by = true;
	} else if ($bx === "tspan") {
		$by = true;
	} else if ($bx === "textPath") {
		$by = true;
	} else if ($bx === "filter") {
		$by = true;
	} else if ($bx === "foreignObject") {
		$by = true;
	} else if ($bx === "feGaussianBlur") {
		$by = true;
	} else if ($bx === "feColorMatrix") {
		$by = true;
	} else if ($bx === "feOffset") {
		$by = true;
	} else if ($bx === "feMerge") {
		$by = true;
	} else if ($bx === "feMergeNode") {
		$by = true;
	} else if ($bx === "feFlood") {
		$by = true;
	} else if ($bx === "feComposite") {
		$by = true;
	} else if ($bx === "feBlend") {
		$by = true;
	} else if ($bx === "feDropShadow") {
		$by = true;
	} else {
		$by = false;
	}
	return $by;
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
		return $t([ 1 ], ($bN) => {
			return handler(dispatched, $bN);
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
function set_chunk_pending(busy, $di) {
	if ($G(chunk_pending_signal) !== busy) {
		$dc(chunk_pending_signal, busy, $di);
	}
}
function clear_chunk_error($cZ) {
	const $da = $G(chunk_error_signal);
	let $db = null;
	if ($da[0] === 0) {
		const _reason = $da[1];
		$db = $dc(chunk_error_signal, [ 1 ], $cZ);
	} else {
		$db = undefined;
	}
	return $db;
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
		let $dJ = null;
		if (at + 1 < rows.length) {
			$dJ = __at(rows, at + 1)[0];
		} else {
			$dJ = self[0];
		}
		const end = __clone($dJ);
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
			throw "mount: no element with id \'" + id + "\'";
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
	const $ef = $t([ 1 ], ($ec) => {
		return $ed(body);
	});
	const built = $ef[0];
	const root = $ef[1];
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
	let $z = null;
	if (__at(parts, 0) === "docs" && parts.length === 2) {
		const $x = __parse_i32(__at(parts, 1));
		let $y = null;
		if ($x[0] === 0) {
			const page = $x[1];
			return [ 1, page ];
		} else {
			$y = undefined;
		}
		$z = $y;
	}
	$z;
	return [ 2 ];
}
function href(route2) {
	const $bG = route2;
	let $bH = null;
	if ($bG[0] === 0) {
		$bH = "/";
	} else if ($bG[0] === 1) {
		const page = $bG[1];
		$bH = "/docs/" + page;
	} else {
		$bH = "/404";
	}
	return $bH;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $cK, $cL) {
	return $bO($bO(view("section"), text(view("h2"), title), $cK, $cL), text(view("p"), body), $cK, $cL);
}
function app(route2, $bv, $bw) {
	$cS(route2);
	return $dy($bO($bO($bO(view("main"), $bO($bO(view("nav"), $bA("Home", [ 0 ], $bv, $bw), $bv, $bw), $bA("Docs", [ 1, 1 ], $bv, $bw), $bv, $bw), $bv, $bw), $bW(class2(view("p"), "pending"), $A(pending(), (busy, $bR, $bS, $bT) => {
		let $bU = null;
		if (busy) {
			$bU = "...";
		} else {
			$bU = "";
		}
		return $bU;
	}), $bv, $bw), $bv, $bw), $cw(class2(view("p"), "failed"), $A(chunk_error(), (reason, $cp, $cq, $cr) => {
		const $cs = reason;
		let $ct = null;
		if ($cs[0] === 0) {
			const text2 = $cs[1];
			let $cu = null;
			if (text2.length > 0) {
				$cu = "!";
			} else {
				$cu = "?";
			}
			$ct = $cu;
		} else {
			$ct = "";
		}
		return $ct;
	}), $bv, $bw), $bv, $bw), $cU(__clone(route2), (current, $cF) => {
		const $cG = current;
		let $cH = null;
		if ($cG[0] === 0) {
			$cH = home_page($bv, $cF);
		} else if ($cG[0] === 1) {
			const page = $cG[1];
			$cH = docs_page(page, $bv, $cF);
		} else {
			$cH = not_found_page($bv, $cF);
		}
		return $cH;
	}), $bv, $bw);
}
function eq(self, other) {
	const $dM = [ self, other ];
	let $dN = null;
	if ($dM[0][0] === 0 && $dM[1][0] === 0) {
		$dN = true;
	} else if ($dM[0][0] === 1 && $dM[1][0] === 1) {
		const s0 = $dM[0][1];
		const o0 = $dM[1][1];
		$dN = s0 === o0;
	} else if ($dM[0][0] === 2 && $dM[1][0] === 2) {
		$dN = true;
	} else {
		$dN = false;
	}
	return $dN;
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $m(self) {
	return self.length === 0;
}
function $n(self) {
	let $p = null;
	if ($m(self)) {
		$p = [ 1 ];
	} else {
		$p = __list_get(self, self.length - 1);
	}
	return $p;
}
function $h(self, $i) {
	const $j = $i;
	let $k = null;
	if ($j[0] === 0) {
		const turn = $j[1];
		$k = enqueue(turn, __clone(self[1].v));
	} else {
		const $q = $n(draining_turns.v);
		let $r = null;
		if ($q[0] === 0) {
			const draining = $q[1];
			$r = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$r = undefined;
		}
		$k = $r;
	}
	return $k;
}
function $f(self, value, $g) {
	self[0].v = __clone(value);
	$h(self, $g);
}
function $t(policy, body) {
	const fresh = new2();
	const result = body(fresh);
	drain(fresh);
	fresh[5].v = true;
	return result;
}
function $A(self, transform) {
	return [ self, transform ];
}
function $G(self) {
	return __clone(self[0].v);
}
function $I(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $H(self, subscriber) {
	return $I(self, subscriber);
}
function $F(self) {
	return [ () => {
		return $G(self);
	}, (subscriber) => {
		return $H(self, subscriber);
	}, () => {
		return;
	} ];
}
function $ad(self) {
	const $ae = self;
	return $ae[0] === 1;
}
function $X(runs, body) {
	const run = renew(runs);
	const $ao = nursery(run);
	let $ap = null;
	if ($ao[0] === 0) {
		const nursery2 = $ao[1];
		$ap = (($aq) => {
			return (($ar) => {
				return body($aq, $ar);
			})(nursery2);
		})(run);
	} else {
		$ap = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $ap;
}
function $Q(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = $X(runs, ($U, $V) => {
		return (($W) => {
			return body($U, $W, $V);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function $M(runs, tracker, body) {
	let value = $Q(runs, tracker, ($N, $O, $P) => {
		return body($N, $O, $P);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = $Q(runs, tracker, ($aS, $aT, $aU) => {
			return body($aS, $aT, $aU);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function $E(self) {
	const transform = self[1];
	const upstream = $F(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new3();
	const tracker = new_tracker();
	return [ () => {
		const value = pull();
		return $M(runs, tracker, ($J, $K, $L) => {
			return transform(value, $J, $K, $L);
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
function $bd(self, $i) {
	const $be = $i;
	let $bf = null;
	if ($be[0] === 0) {
		const turn = $be[1];
		$bf = enqueue(turn, __clone(self[1].v));
	} else {
		const $bg = $n(draining_turns.v);
		let $bh = null;
		if ($bg[0] === 0) {
			const draining = $bg[1];
			$bh = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bh = undefined;
		}
		$bf = $bh;
	}
	return $bf;
}
function $bc(self, value, $g) {
	self[0].v = __clone(value);
	$bd(self, $g);
}
function $bo(self, item, $bp) {
	defer(self, () => {
		dispose(item, $bp);
		return;
	});
	return __clone(item);
}
function $B(self, $C, $D) {
	const instance = $E(self);
	const pull = instance[0];
	const cached = $a(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$bc(cached, pull(), $C);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $C, $D);
	return cached;
}
function $bI(self, name, value, $bJ, $bK) {
	apply(value, self, name, $bJ, $bK);
	return __clone(self);
}
function $bD(self, route2, $bE, $bF) {
	const path = to_path(route2);
	return on_event($bI($bI(self, "href", path, $bE, $bF), "draggable", "false", $bE, $bF), "click", (event, $bL) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $bL ]);
		}
		return;
	});
}
function $bA(label, route2, $bB, $bC) {
	return text($bD(view("a"), route2, $bB, $bC), label);
}
function $bO(self, content, $bP, $bQ) {
	place(content, self, $bP, $bQ);
	return __clone(self);
}
function $ci(self, subscriber) {
	return $I(self, subscriber);
}
function $cg(self) {
	return [ () => {
		return $G(self);
	}, (subscriber) => {
		return $ci(self, subscriber);
	}, () => {
		return;
	} ];
}
function $cm(runs, body) {
	const run = renew(runs);
	const $cn = nursery(run);
	let $co = null;
	if ($cn[0] === 0) {
		const nursery2 = $cn[1];
		$co = (($aq) => {
			return (($ar) => {
				return body($aq, $ar);
			})(nursery2);
		})(run);
	} else {
		$co = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $co;
}
function $cl(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = $cm(runs, ($U, $V) => {
		return (($W) => {
			return body($U, $W, $V);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function $ck(runs, tracker, body) {
	let value = $cl(runs, tracker, ($N, $O, $P) => {
		return body($N, $O, $P);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = $cl(runs, tracker, ($aS, $aT, $aU) => {
			return body($aS, $aT, $aU);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function $cf(self) {
	const transform = self[1];
	const upstream = $cg(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new3();
	const tracker = new_tracker();
	return [ () => {
		const value = pull();
		return $ck(runs, tracker, ($J, $K, $L) => {
			return transform(value, $J, $K, $L);
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
function $ce(flow, observer, immediately) {
	const instance = $cf(flow);
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
function $cd(self, observer) {
	return $ce(self, observer, true);
}
function $bZ(flow, observer, $ca, $cb) {
	$bo(get_owner($cb), $cd(flow, observer), $ca);
}
function $bW(self, source, $bX, $bY) {
	const element = __clone(self[0]);
	$bZ(source, (value) => {
		element.textContent = value;
		return;
	}, $bX, $bY);
	return __clone(self);
}
function $cB(self) {
	return [ () => {
		return $G(self);
	}, (subscriber) => {
		return $ci(self, subscriber);
	}, () => {
		return;
	} ];
}
function $cA(self) {
	const transform = self[1];
	const upstream = $cB(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new3();
	const tracker = new_tracker();
	return [ () => {
		const value = pull();
		return $ck(runs, tracker, ($J, $K, $L) => {
			return transform(value, $J, $K, $L);
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
function $cz(flow, observer, immediately) {
	const instance = $cA(flow);
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
function $cy(self, observer) {
	return $cz(self, observer, true);
}
function $cx(flow, observer, $ca, $cb) {
	$bo(get_owner($cb), $cy(flow, observer), $ca);
}
function $cw(self, source, $bX, $bY) {
	const element = __clone(self[0]);
	$cx(source, (value) => {
		element.textContent = value;
		return;
	}, $bX, $bY);
	return __clone(self);
}
function $cS(source) {
	__chunk_preload(__chunk_arm($G(source)));
}
function $dc(self, value, $g) {
	self[0].v = __clone(value);
	$bd(self, $g);
}
function $du(signal, observer) {
	const cell = signal[0];
	return $I(signal, mint_subscriber(() => {
		const $dv = [ 0, cell ];
		let $dw = null;
		if ($dv[0] === 0) {
			const live = $dv[1];
			$dw = observer(live.v);
		} else {
			$dw = undefined;
		}
		return $dw;
	}));
}
function $dt(self, observer, immediately) {
	const subscription = $du(self, observer);
	if (immediately) {
		observer($G(self));
	}
	return subscription;
}
function $ds(self, observer) {
	return $dt(self, observer, true);
}
function $dr(flow, observer, $ca, $cb) {
	$bo(get_owner($cb), $ds(flow, observer), $ca);
}
function $cU(source, render, $cV) {
	const gated = $a($G(source));
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance2 = (value, $cW) => {
		armed.v = true;
		$bc(gated, value, [ 0, $cW ]);
		return;
	};
	const wire = ($cX) => {
		$dr(__clone(source), (value) => {
			return $t([ 1 ], ($cY) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $cY ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $cY ]);
					advance2(value, $cY);
				} else {
					set_chunk_pending(true, [ 0, $cY ]);
					__chunk_load(arm, () => {
						return $t([ 1 ], ($dp) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $dp ]);
								advance2(value, $dp);
							}
							return;
						});
					}, (reason) => {
						return $t([ 1 ], ($dq) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $dq ]);
								$dc(chunk_error_signal, [ 0, reason ], [ 0, $dq ]);
							}
							return;
						});
					});
				}
				return;
			});
		}, $cV, $cX);
		return;
	};
	return [ [ 1 ], render, [ 0, __clone(gated) ], armed, wire ];
}
function $dW(self, content, end, $dX, $dY) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(content, [ __clone(staging) ], $dX, $dY);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function $dT(self, content, $dU, $dV) {
	return $dW(self, content, self[0], $dU, $dV);
}
function $dZ(owner, body) {
	return body(owner);
}
function $dE(parent, source, render, armed, $dF, $dG) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($dG), () => {
		const $dH = live_owner.v;
		let $dI = null;
		if ($dH[0] === 1) {
			$dI = $dH;
		} else {
			$dI = [ 0, dispose2($dH[1]) ];
		}
		$dI;
		close(region);
		return;
	});
	$dr(source, (value) => {
		const $dK = last_value.v;
		let $dL = null;
		if ($dK[0] === 0) {
			const previous = $dK[1];
			$dL = eq(previous, value);
		} else {
			$dL = false;
		}
		const unchanged = $dL;
		if (armed.v && !(unchanged)) {
			const $dO = live_owner.v;
			let $dP = null;
			if ($dO[0] === 1) {
				$dP = $dO;
			} else {
				$dP = [ 0, dispose2($dO[1]) ];
			}
			$dP;
			const $dQ = live_row.v;
			let $dR = null;
			if ($dQ[0] === 0) {
				const row = $dQ[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$dR = undefined;
			} else {
				$dR = undefined;
			}
			$dR;
			const owner = new3();
			const row2 = $dZ(owner, ($dS) => {
				return $dT(region, render(value, $dS), $dF, $dS);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $dF, $dG);
}
function $dz(self, parent, $dA, $dB) {
	self[4]($dB);
	const render = self[1];
	const armed = self[3];
	const $dC = self[0];
	let $dD = null;
	if ($dC[0] === 0) {
		const source = $dC[1];
		$dD = $dE(parent, __clone(source), render, armed, $dA, $dB);
	} else {
		const $ea = self[2];
		let $eb = null;
		if ($ea[0] === 0) {
			const gated = $ea[1];
			$eb = $dE(parent, __clone(gated), render, armed, $dA, $dB);
		} else {
			$eb = undefined;
		}
		$dD = $eb;
	}
	return $dD;
}
function $dy(self, content, $bP, $bQ) {
	$dz(content, self, $bP, $bQ);
	return __clone(self);
}
function $ed(body) {
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
const path_signal = $a("");
const wired = __shared_new(false);
const chunk_pending_signal = $a(false);
const chunk_error_signal = $a([ 1 ]);
const BASE = announce("BASE", 2);
const SCALED = announce("SCALED", BASE * 3);
const LABEL = "scale " + SCALED;
__vilan_chunks.url[0] = "app.Route_Home.js";
__vilan_chunks.url[1] = "app.Route_Docs.js";
__vilan_chunks.url[2] = "app.Route_NotFound.js";
__vilan_chunks.fn.$bA = $bA;
__vilan_chunks.fn.$bO = $bO;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $B($A(current_path([ 1 ]), (path, $u, $v, $w) => {
	return parse(path);
}), [ 1 ], [ 1 ]);
mount_root("app", ($bu) => {
	return app(route, [ 1 ], $bu);
});
