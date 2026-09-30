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
function home_page($cv, $cw) {
	return __vilan_chunks.fn.home_page($cv, $cw);
}
function docs_page(page, $cz, $cA) {
	return __vilan_chunks.fn.docs_page(page, $cz, $cA);
}
function not_found_page($cD, $cE) {
	return __vilan_chunks.fn.not_found_page($cD, $cE);
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
	const $au = turn;
	let $av = null;
	if ($au[0] === 0) {
		const ambient = $au[1];
		$av = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $aw = $n(draining_turns.v);
		let $ax = null;
		if ($aw[0] === 0) {
			const draining = $aw[1];
			$ax = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$ax = undefined;
		}
		$av = $ax;
	}
	return $av;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $bd) {
	const $be = $bd;
	let $bf = null;
	if ($be[0] === 0) {
		const established = $be[1];
		$bf = [ 0, established ];
	} else {
		$bf = $n(draining_turns.v);
	}
	const ambient = $bf;
	release_under(self, ambient);
}
function detach(handle) {
	const $aB = $n(releasing_turns.v);
	let $aC = null;
	if ($aB[0] === 0) {
		const at_release = $aB[1];
		$aC = at_release;
	} else {
		$aC = $n(draining_turns.v);
	}
	const turn = $aC;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $aD = [ 0, handle[0] ];
	let $aE = null;
	if ($aD[0] === 0) {
		const subscribers = $aD[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aE = undefined;
	} else {
		$aE = undefined;
	}
	$aE;
	const $aF = ambient;
	let $aG = null;
	if ($aF[0] === 0) {
		const turn = $aF[1];
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
		$aG = undefined;
	} else {
		$aG = undefined;
	}
	$aG;
	const $aH = handle[3].v;
	let $aI = null;
	if ($aH[0] === 0) {
		const release = $aH[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aI = undefined;
	} else {
		$aI = undefined;
	}
	return $aI;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $bg = null;
	if (is_disposed(self)) {
		cleanup();
	} else {
		const held = __clone(self[0].v);
		if (held[2]) {
			held[1].v.push(cleanup);
		} else {
			owner_lists_allocated_count.v = owner_lists_allocated_count.v + 1;
			self[0].v = [ held[0], __shared_new([ cleanup ]), true, held[3] ];
		}
		$bg = undefined;
	}
	return $bg;
}
function renew(self, with_nursery) {
	const live = [ self[0], self[0].v[0] ];
	dispose2(live);
	const next = self[0].v[0];
	if (with_nursery) {
		const held = self[0].v;
		self[0].v = [ held[0], held[1], held[2], [ 0, detached_nursery() ] ];
	}
	return [ self[0], next ];
}
function nursery(self) {
	let $ag = null;
	if (is_disposed(self)) {
		$ag = [ 1 ];
	} else {
		$ag = self[0].v[3];
	}
	return $ag;
}
function dispose2(self) {
	let $af = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, [ 1 ] ];
		const $W = held[3];
		let $X = null;
		if ($W[0] === 0) {
			const nursery2 = $W[1];
			$X = nursery2.cancel();
		} else {
			$X = undefined;
		}
		$X;
		let $ae = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $Y = __guarded(cleanup);
				let $Z = null;
				if ($Y[0] === 0) {
					const message = $Y[1];
					if ($aa(failure)) {
						failure = [ 0, message ];
					}
					$Z = undefined;
				} else {
					$Z = undefined;
				}
				$Z;
			}
			const $ac = failure;
			let $ad = null;
			if ($ac[0] === 0) {
				const message2 = $ac[1];
				$ad = (() => {
					throw message2;
				})();
			} else {
				$ad = undefined;
			}
			$ae = $ad;
		}
		$af = $ae;
	}
	return $af;
}
function get_owner($bP) {
	return $bP;
}
function register_with_owner(subscription, $aX, $aY) {
	const $aZ = $aY;
	let $ba = null;
	if ($aZ[0] === 0) {
		const owner = $aZ[1];
		$ba = $bb(owner, subscription, $aX);
	} else {
		$ba = __clone(subscription);
	}
	return $ba;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker() {
	return [ __shared_new([ 0, false, false, false, false ]), __shared_new([  ]), __shared_new([  ]), __shared_new([  ]), __shared_new([ 1 ]) ];
}
function opened(held) {
	return [ held[0] + 1, true, false, held[3], held[4] ];
}
function closed(held) {
	return [ held[0], false, held[2], held[3], held[4] ];
}
function dirtied(held) {
	return [ held[0], held[1], true, held[3], held[4] || held[3] ];
}
function connecting(held, now) {
	return [ held[0], held[1], held[2], now, held[4] ];
}
function answered(held) {
	return [ held[0], held[1], held[2], held[3], false ];
}
function open_run(tracker) {
	const next = opened(tracker[0].v);
	const epoch = next[0];
	tracker[0].v = next;
	if (!($m(tracker[1].v))) {
		tracker[1].v = [  ];
	}
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v = closed(tracker[0].v);
	const $al = tracker[4].v;
	let $am = null;
	if ($al[0] === 0) {
		const target = $al[1];
		$am = reconnect(tracker, target);
	} else {
		if (!($m(tracker[1].v)) || !($m(tracker[2].v))) {
			tracker[2].v = __clone(tracker[1].v);
		}
		$am = undefined;
	}
	$am;
	if (!($m(tracker[1].v))) {
		tracker[1].v = [  ];
	}
}
function reconnect(tracker, target) {
	if ($m(tracker[1].v) && $m(tracker[3].v)) {
		return;
	}
	const held = __clone(tracker[3].v);
	const reading = __clone(tracker[1].v);
	let kept = [  ];
	for (const _edge of held) {
		kept.push(false);
	}
	let next = [  ];
	tracker[0].v = connecting(tracker[0].v, true);
	let position = 0;
	for (const dependency of reading) {
		const $as = reusable(held, kept, dependency[0], position);
		let $at = null;
		if ($as[0] === 0) {
			const index = $as[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$at = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$at = undefined;
		}
		$at;
		position = position + 1;
	}
	tracker[3].v = next;
	let index2 = 0;
	for (const edge of held) {
		if (!(__at(kept, index2))) {
			detach(edge[1]);
		}
		index2 = index2 + 1;
	}
	tracker[0].v = connecting(tracker[0].v, false);
}
function reusable(held, kept, identity, position) {
	const $ao = identity;
	let $ap = null;
	if ($ao[0] === 0) {
		const wanted = $ao[1];
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
		$ap = [ 1 ];
	} else {
		$ap = [ 1 ];
	}
	return $ap;
}
function same_identity(identity, wanted) {
	const $aq = identity;
	let $ar = null;
	if ($aq[0] === 0) {
		const held = $aq[1];
		$ar = held === wanted;
	} else {
		$ar = false;
	}
	return $ar;
}
function relay_for(tracker, target) {
	return subscriber_of(() => {
		const held = tracker[0].v;
		tracker[0].v = dirtied(held);
		if (!(held[3])) {
			wake(target);
		}
		return;
	}, true);
}
function attach_tracker(tracker, target) {
	tracker[4].v = [ 0, __clone(target) ];
	let $aM = null;
	if (!($m(tracker[2].v))) {
		const read = tracker[2].v;
		tracker[2].v = [  ];
		tracker[0].v = connecting(tracker[0].v, true);
		let edges = [  ];
		for (const dependency of read) {
			edges.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
		}
		tracker[3].v = edges;
		tracker[0].v = connecting(tracker[0].v, false);
		if (tracker[0].v[4]) {
			tracker[0].v = answered(tracker[0].v);
			wake(target);
		}
		$aM = undefined;
	}
	return $aM;
}
function forget_reads(tracker) {
	let $aN = null;
	if (!($m(tracker[3].v))) {
		const edges = tracker[3].v;
		tracker[3].v = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aN = undefined;
	}
	$aN;
	if (!($m(tracker[2].v))) {
		tracker[2].v = [  ];
	}
}
function detach_tracker(tracker) {
	tracker[4].v = [ 1 ];
	forget_reads(tracker);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $aV = previous;
		let $aW = null;
		if ($aV[0] === 0) {
			const earlier = $aV[1];
			$aW = earlier();
		} else {
			$aW = undefined;
		}
		return $aW;
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
function navigate(path, $bz) {
	ensure_wired($bz);
	history.pushState("", "", path);
	$f(path_signal, path, $bz);
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
function detached_nursery() {
	return __nursery_new_detached();
}
function view(tag) {
	let $bm = null;
	if (is_svg_tag(tag)) {
		$bm = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$bm = [ document.createElement(tag) ];
	}
	return $bm;
}
function is_svg_tag(tag) {
	const $bk = tag;
	let $bl = null;
	if ($bk === "svg") {
		$bl = true;
	} else if ($bk === "path") {
		$bl = true;
	} else if ($bk === "circle") {
		$bl = true;
	} else if ($bk === "ellipse") {
		$bl = true;
	} else if ($bk === "rect") {
		$bl = true;
	} else if ($bk === "line") {
		$bl = true;
	} else if ($bk === "polyline") {
		$bl = true;
	} else if ($bk === "polygon") {
		$bl = true;
	} else if ($bk === "g") {
		$bl = true;
	} else if ($bk === "defs") {
		$bl = true;
	} else if ($bk === "use") {
		$bl = true;
	} else if ($bk === "symbol") {
		$bl = true;
	} else if ($bk === "marker") {
		$bl = true;
	} else if ($bk === "pattern") {
		$bl = true;
	} else if ($bk === "mask") {
		$bl = true;
	} else if ($bk === "clipPath") {
		$bl = true;
	} else if ($bk === "linearGradient") {
		$bl = true;
	} else if ($bk === "radialGradient") {
		$bl = true;
	} else if ($bk === "stop") {
		$bl = true;
	} else if ($bk === "text") {
		$bl = true;
	} else if ($bk === "tspan") {
		$bl = true;
	} else if ($bk === "textPath") {
		$bl = true;
	} else if ($bk === "filter") {
		$bl = true;
	} else if ($bk === "foreignObject") {
		$bl = true;
	} else if ($bk === "feGaussianBlur") {
		$bl = true;
	} else if ($bk === "feColorMatrix") {
		$bl = true;
	} else if ($bk === "feOffset") {
		$bl = true;
	} else if ($bk === "feMerge") {
		$bl = true;
	} else if ($bk === "feMergeNode") {
		$bl = true;
	} else if ($bk === "feFlood") {
		$bl = true;
	} else if ($bk === "feComposite") {
		$bl = true;
	} else if ($bk === "feBlend") {
		$bl = true;
	} else if ($bk === "feDropShadow") {
		$bl = true;
	} else {
		$bl = false;
	}
	return $bl;
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
		return $t([ 1 ], ($bA) => {
			return handler(dispatched, $bA);
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
function set_chunk_pending(busy, $cV) {
	if ($G(chunk_pending_signal) !== busy) {
		$cP(chunk_pending_signal, busy, $cV);
	}
}
function clear_chunk_error($cM) {
	const $cN = $G(chunk_error_signal);
	let $cO = null;
	if ($cN[0] === 0) {
		const _reason = $cN[1];
		$cO = $cP(chunk_error_signal, [ 1 ], $cM);
	} else {
		$cO = undefined;
	}
	return $cO;
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
		let $dw = null;
		if (at + 1 < rows.length) {
			$dw = __at(rows, at + 1)[0];
		} else {
			$dw = self[0];
		}
		const end = __clone($dw);
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
	const $dS = $t([ 1 ], ($dP) => {
		return $dQ(body);
	});
	const built = $dS[0];
	const root = $dS[1];
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
	const $bt = route2;
	let $bu = null;
	if ($bt[0] === 0) {
		$bu = "/";
	} else if ($bt[0] === 1) {
		const page = $bt[1];
		$bu = "/docs/" + page;
	} else {
		$bu = "/404";
	}
	return $bu;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $cx, $cy) {
	return $bB($bB(view("section"), text(view("h2"), title), $cx, $cy), text(view("p"), body), $cx, $cy);
}
function app(route2, $bi, $bj) {
	$cF(route2);
	return $dl($bB($bB($bB(view("main"), $bB($bB(view("nav"), $bn("Home", [ 0 ], $bi, $bj), $bi, $bj), $bn("Docs", [ 1, 1 ], $bi, $bj), $bi, $bj), $bi, $bj), $bJ(class2(view("p"), "pending"), $A(pending(), (busy, $bE, $bF, $bG) => {
		let $bH = null;
		if (busy) {
			$bH = "...";
		} else {
			$bH = "";
		}
		return $bH;
	}), $bi, $bj), $bi, $bj), $cj(class2(view("p"), "failed"), $A(chunk_error(), (reason, $cc, $cd, $ce) => {
		const $cf = reason;
		let $cg = null;
		if ($cf[0] === 0) {
			const text2 = $cf[1];
			let $ch = null;
			if (text2.length > 0) {
				$ch = "!";
			} else {
				$ch = "?";
			}
			$cg = $ch;
		} else {
			$cg = "";
		}
		return $cg;
	}), $bi, $bj), $bi, $bj), $cH(__clone(route2), (current, $cs) => {
		const $ct = current;
		let $cu = null;
		if ($ct[0] === 0) {
			$cu = home_page($bi, $cs);
		} else if ($ct[0] === 1) {
			const page = $ct[1];
			$cu = docs_page(page, $bi, $cs);
		} else {
			$cu = not_found_page($bi, $cs);
		}
		return $cu;
	}), $bi, $bj);
}
function eq(self, other) {
	const $dz = [ self, other ];
	let $dA = null;
	if ($dz[0][0] === 0 && $dz[1][0] === 0) {
		$dA = true;
	} else if ($dz[0][0] === 1 && $dz[1][0] === 1) {
		const s0 = $dz[0][1];
		const o0 = $dz[1][1];
		$dA = s0 === o0;
	} else if ($dz[0][0] === 2 && $dz[1][0] === 2) {
		$dA = true;
	} else {
		$dA = false;
	}
	return $dA;
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
	return [ __clone(self), transform ];
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
function $aa(self) {
	const $ab = self;
	return $ab[0] === 1;
}
function $V(runs, body) {
	const run = renew(runs, true);
	const $ah = nursery(run);
	let $ai = null;
	if ($ah[0] === 0) {
		const nursery2 = $ah[1];
		$ai = (($aj) => {
			return (($ak) => {
				return body($aj, $ak);
			})(nursery2);
		})(run);
	} else {
		$ai = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $ai;
}
function $Q(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = $V(runs, ($S, $T) => {
		return (($U) => {
			return body($S, $U, $T);
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
		tracker[0].v = answered(tracker[0].v);
		value = $Q(runs, tracker, ($aJ, $aK, $aL) => {
			return body($aJ, $aK, $aL);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v = answered(tracker[0].v);
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
function $aQ(self, $i) {
	const $aR = $i;
	let $aS = null;
	if ($aR[0] === 0) {
		const turn = $aR[1];
		$aS = enqueue(turn, __clone(self[1].v));
	} else {
		const $aT = $n(draining_turns.v);
		let $aU = null;
		if ($aT[0] === 0) {
			const draining = $aT[1];
			$aU = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$aU = undefined;
		}
		$aS = $aU;
	}
	return $aS;
}
function $aP(self, value, $g) {
	self[0].v = __clone(value);
	$aQ(self, $g);
}
function $bb(self, item, $bc) {
	defer(self, () => {
		dispose(item, $bc);
		return;
	});
	return __clone(item);
}
function $B(self, $C, $D) {
	const instance = $E(self);
	const pull = instance[0];
	const cached = $a(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$aP(cached, pull(), $C);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $C, $D);
	return cached;
}
function $bv(self, name, value, $bw, $bx) {
	apply(__clone(value), self, name, $bw, $bx);
	return __clone(self);
}
function $bq(self, route2, $br, $bs) {
	const path = to_path(route2);
	return on_event($bv($bv(self, "href", path, $br, $bs), "draggable", "false", $br, $bs), "click", (event, $by) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $by ]);
		}
		return;
	});
}
function $bn(label, route2, $bo, $bp) {
	return text($bq(view("a"), route2, $bo, $bp), label);
}
function $bB(self, content, $bC, $bD) {
	place(__clone(content), self, $bC, $bD);
	return __clone(self);
}
function $bV(self, subscriber) {
	return $I(self, subscriber);
}
function $bT(self) {
	return [ () => {
		return $G(self);
	}, (subscriber) => {
		return $bV(self, subscriber);
	}, () => {
		return;
	} ];
}
function $bZ(runs, body) {
	const run = renew(runs, true);
	const $ca = nursery(run);
	let $cb = null;
	if ($ca[0] === 0) {
		const nursery2 = $ca[1];
		$cb = (($aj) => {
			return (($ak) => {
				return body($aj, $ak);
			})(nursery2);
		})(run);
	} else {
		$cb = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $cb;
}
function $bY(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = $bZ(runs, ($S, $T) => {
		return (($U) => {
			return body($S, $U, $T);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function $bX(runs, tracker, body) {
	let value = $bY(runs, tracker, ($N, $O, $P) => {
		return body($N, $O, $P);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v = answered(tracker[0].v);
		value = $bY(runs, tracker, ($aJ, $aK, $aL) => {
			return body($aJ, $aK, $aL);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v = answered(tracker[0].v);
	}
	return value;
}
function $bS(self) {
	const transform = self[1];
	const upstream = $bT(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new3();
	const tracker = new_tracker();
	return [ () => {
		const value = pull();
		return $bX(runs, tracker, ($J, $K, $L) => {
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
function $bR(flow, observer, immediately) {
	const instance = $bS(flow);
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
function $bQ(self, observer) {
	return $bR(self, observer, true);
}
function $bM(flow, observer, $bN, $bO) {
	$bb(get_owner($bO), $bQ(flow, observer), $bN);
}
function $bJ(self, source, $bK, $bL) {
	const element = __clone(self[0]);
	$bM(source, (value) => {
		element.textContent = value;
		return;
	}, $bK, $bL);
	return __clone(self);
}
function $co(self) {
	return [ () => {
		return $G(self);
	}, (subscriber) => {
		return $bV(self, subscriber);
	}, () => {
		return;
	} ];
}
function $cn(self) {
	const transform = self[1];
	const upstream = $co(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new3();
	const tracker = new_tracker();
	return [ () => {
		const value = pull();
		return $bX(runs, tracker, ($J, $K, $L) => {
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
function $cm(flow, observer, immediately) {
	const instance = $cn(flow);
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
function $cl(self, observer) {
	return $cm(self, observer, true);
}
function $ck(flow, observer, $bN, $bO) {
	$bb(get_owner($bO), $cl(flow, observer), $bN);
}
function $cj(self, source, $bK, $bL) {
	const element = __clone(self[0]);
	$ck(source, (value) => {
		element.textContent = value;
		return;
	}, $bK, $bL);
	return __clone(self);
}
function $cF(source) {
	__chunk_preload(__chunk_arm($G(source)));
}
function $cP(self, value, $g) {
	self[0].v = __clone(value);
	$aQ(self, $g);
}
function $dh(signal, observer) {
	const cell = signal[0];
	return $I(signal, mint_subscriber(() => {
		const $di = [ 0, cell ];
		let $dj = null;
		if ($di[0] === 0) {
			const live = $di[1];
			$dj = observer(live.v);
		} else {
			$dj = undefined;
		}
		return $dj;
	}));
}
function $dg(self, observer, immediately) {
	const subscription = $dh(self, observer);
	if (immediately) {
		observer($G(self));
	}
	return subscription;
}
function $df(self, observer) {
	return $dg(self, observer, true);
}
function $de(flow, observer, $bN, $bO) {
	$bb(get_owner($bO), $df(__clone(flow), observer), $bN);
}
function $cH(source, render, $cI) {
	const gated = $a($G(source));
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value, $cJ) => {
		armed.v = true;
		$aP(gated, value, [ 0, $cJ ]);
		return;
	};
	const wire = ($cK) => {
		$de(__clone(source), (value) => {
			return $t([ 1 ], ($cL) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $cL ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $cL ]);
					advance(value, $cL);
				} else {
					set_chunk_pending(true, [ 0, $cL ]);
					__chunk_load(arm, () => {
						return $t([ 1 ], ($dc) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $dc ]);
								advance(value, $dc);
							}
							return;
						});
					}, (reason) => {
						return $t([ 1 ], ($dd) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $dd ]);
								$cP(chunk_error_signal, [ 0, reason ], [ 0, $dd ]);
							}
							return;
						});
					});
				}
				return;
			});
		}, $cI, $cK);
		return;
	};
	return [ [ 1 ], render, [ 0, __clone(gated) ], armed, wire ];
}
function $dJ(self, content, end, $dK, $dL) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(__clone(content), [ __clone(staging) ], $dK, $dL);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function $dG(self, content, $dH, $dI) {
	return $dJ(self, __clone(content), self[0], $dH, $dI);
}
function $dM(owner, body) {
	return body(owner);
}
function $dr(parent, source, render, armed, $ds, $dt) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($dt), () => {
		const $du = live_owner.v;
		let $dv = null;
		if ($du[0] === 1) {
			$dv = $du;
		} else {
			$dv = [ 0, dispose2($du[1]) ];
		}
		$dv;
		close(region);
		return;
	});
	$de(__clone(source), (value) => {
		const $dx = last_value.v;
		let $dy = null;
		if ($dx[0] === 0) {
			const previous = $dx[1];
			$dy = eq(previous, value);
		} else {
			$dy = false;
		}
		const unchanged = $dy;
		if (armed.v && !(unchanged)) {
			const $dB = live_owner.v;
			let $dC = null;
			if ($dB[0] === 1) {
				$dC = $dB;
			} else {
				$dC = [ 0, dispose2($dB[1]) ];
			}
			$dC;
			const $dD = live_row.v;
			let $dE = null;
			if ($dD[0] === 0) {
				const row = $dD[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$dE = undefined;
			} else {
				$dE = undefined;
			}
			$dE;
			const owner = new3();
			const row2 = $dM(owner, ($dF) => {
				return $dG(region, render(value, $dF), $ds, $dF);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $ds, $dt);
}
function $dm(self, parent, $dn, $do) {
	self[4]($do);
	const render = self[1];
	const armed = self[3];
	const $dp = self[0];
	let $dq = null;
	if ($dp[0] === 0) {
		const source = $dp[1];
		$dq = $dr(parent, __clone(source), render, armed, $dn, $do);
	} else {
		const $dN = self[2];
		let $dO = null;
		if ($dN[0] === 0) {
			const gated = $dN[1];
			$dO = $dr(parent, __clone(gated), render, armed, $dn, $do);
		} else {
			$dO = undefined;
		}
		$dq = $dO;
	}
	return $dq;
}
function $dl(self, content, $bC, $bD) {
	$dm(__clone(content), self, $bC, $bD);
	return __clone(self);
}
function $dQ(body) {
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
__vilan_chunks.fn.$bB = $bB;
__vilan_chunks.fn.$bn = $bn;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $B($A(current_path([ 1 ]), (path, $u, $v, $w) => {
	return parse(path);
}), [ 1 ], [ 1 ]);
mount_root("app", ($bh) => {
	return app(route, [ 1 ], $bh);
});
