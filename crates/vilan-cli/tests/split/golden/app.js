function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
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
function home_page($bJ, $bK) {
	return __vilan_chunks.fn.home_page($bJ, $bK);
}
function docs_page(page, $bN, $bO) {
	return __vilan_chunks.fn.docs_page(page, $bN, $bO);
}
function not_found_page($bR, $bS) {
	return __vilan_chunks.fn.not_found_page($bR, $bS);
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
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function dispose(self, $ap) {
	const $aq = $ap;
	let $ar = null;
	if ($aq[0] === 0) {
		const established = $aq[1];
		$ar = [ 0, established ];
	} else {
		$ar = $n(draining_turns.v);
	}
	const ambient = $ar;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $as = [ 0, handle[0] ];
	let $at = null;
	if ($as[0] === 0) {
		const subscribers = $as[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$at = undefined;
	} else {
		$at = undefined;
	}
	$at;
	const $au = ambient;
	let $av = null;
	if ($au[0] === 0) {
		const turn = $au[1];
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
		$av = undefined;
	} else {
		$av = undefined;
	}
	$av;
	const $aw = handle[3].v;
	let $ax = null;
	if ($aw[0] === 0) {
		const release = $aw[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ax = undefined;
	} else {
		$ax = undefined;
	}
	return $ax;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $ay = null;
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
		$ay = undefined;
	}
	return $ay;
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
	let $V = null;
	if (is_disposed(self)) {
		$V = [ 1 ];
	} else {
		$V = self[0].v[3];
	}
	return $V;
}
function dispose2(self) {
	let $U = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, [ 1 ] ];
		const $L = held[3];
		let $M = null;
		if ($L[0] === 0) {
			const nursery2 = $L[1];
			$M = nursery2.cancel();
		} else {
			$M = undefined;
		}
		$M;
		let $T = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $N = __guarded(cleanup);
				let $O = null;
				if ($N[0] === 0) {
					const message = $N[1];
					if ($P(failure)) {
						failure = [ 0, message ];
					}
					$O = undefined;
				} else {
					$O = undefined;
				}
				$O;
			}
			const $R = failure;
			let $S = null;
			if ($R[0] === 0) {
				const message2 = $R[1];
				$S = (() => {
					throw message2;
				})();
			} else {
				$S = undefined;
			}
			$T = $S;
		}
		$U = $T;
	}
	return $U;
}
function get_owner($bg) {
	return $bg;
}
function register_with_owner(subscription, $aj, $ak) {
	const $al = $ak;
	let $am = null;
	if ($al[0] === 0) {
		const owner = $al[1];
		$am = $an(owner, subscription, $aj);
	} else {
		$am = __clone(subscription);
	}
	return $am;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $ah = previous;
		let $ai = null;
		if ($ah[0] === 0) {
			const earlier = $ah[1];
			$ai = earlier();
		} else {
			$ai = undefined;
		}
		return $ai;
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
function navigate(path, $aR) {
	ensure_wired($aR);
	history.pushState("", "", path);
	$f(path_signal, path, $aR);
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
	let $aE = null;
	if (is_svg_tag(tag)) {
		$aE = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$aE = [ document.createElement(tag) ];
	}
	return $aE;
}
function is_svg_tag(tag) {
	const $aC = tag;
	let $aD = null;
	if ($aC === "svg") {
		$aD = true;
	} else if ($aC === "path") {
		$aD = true;
	} else if ($aC === "circle") {
		$aD = true;
	} else if ($aC === "ellipse") {
		$aD = true;
	} else if ($aC === "rect") {
		$aD = true;
	} else if ($aC === "line") {
		$aD = true;
	} else if ($aC === "polyline") {
		$aD = true;
	} else if ($aC === "polygon") {
		$aD = true;
	} else if ($aC === "g") {
		$aD = true;
	} else if ($aC === "defs") {
		$aD = true;
	} else if ($aC === "use") {
		$aD = true;
	} else if ($aC === "symbol") {
		$aD = true;
	} else if ($aC === "marker") {
		$aD = true;
	} else if ($aC === "pattern") {
		$aD = true;
	} else if ($aC === "mask") {
		$aD = true;
	} else if ($aC === "clipPath") {
		$aD = true;
	} else if ($aC === "linearGradient") {
		$aD = true;
	} else if ($aC === "radialGradient") {
		$aD = true;
	} else if ($aC === "stop") {
		$aD = true;
	} else if ($aC === "text") {
		$aD = true;
	} else if ($aC === "tspan") {
		$aD = true;
	} else if ($aC === "textPath") {
		$aD = true;
	} else if ($aC === "filter") {
		$aD = true;
	} else if ($aC === "foreignObject") {
		$aD = true;
	} else if ($aC === "feGaussianBlur") {
		$aD = true;
	} else if ($aC === "feColorMatrix") {
		$aD = true;
	} else if ($aC === "feOffset") {
		$aD = true;
	} else if ($aC === "feMerge") {
		$aD = true;
	} else if ($aC === "feMergeNode") {
		$aD = true;
	} else if ($aC === "feFlood") {
		$aD = true;
	} else if ($aC === "feComposite") {
		$aD = true;
	} else if ($aC === "feBlend") {
		$aD = true;
	} else if ($aC === "feDropShadow") {
		$aD = true;
	} else {
		$aD = false;
	}
	return $aD;
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
		return $t([ 1 ], ($aS) => {
			return handler(dispatched, $aS);
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
function set_chunk_pending(busy, $cj) {
	if ($F(chunk_pending_signal) !== busy) {
		$cd(chunk_pending_signal, busy, $cj);
	}
}
function clear_chunk_error($ca) {
	const $cb = $F(chunk_error_signal);
	let $cc = null;
	if ($cb[0] === 0) {
		const _reason = $cb[1];
		$cc = $cd(chunk_error_signal, [ 1 ], $ca);
	} else {
		$cc = undefined;
	}
	return $cc;
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
		let $cK = null;
		if (at + 1 < rows.length) {
			$cK = __at(rows, at + 1)[0];
		} else {
			$cK = self[0];
		}
		const end = __clone($cK);
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
	const $dg = $t([ 1 ], ($dd) => {
		return $de(body);
	});
	const built = $dg[0];
	const root = $dg[1];
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
	let $y = null;
	if (__at(parts, 0) === "docs" && parts.length === 2) {
		const $w = __parse_i32(__at(parts, 1));
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
	const $aL = route2;
	let $aM = null;
	if ($aL[0] === 0) {
		$aM = "/";
	} else if ($aL[0] === 1) {
		const page = $aL[1];
		$aM = "/docs/" + page;
	} else {
		$aM = "/404";
	}
	return $aM;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $bL, $bM) {
	return $aT($aT(view("section"), text(view("h2"), title), $bL, $bM), text(view("p"), body), $bL, $bM);
}
function app(route2, $aA, $aB) {
	$bT(route2);
	return $cz($aT($aT($aT(view("main"), $aT($aT(view("nav"), $aF("Home", [ 0 ], $aA, $aB), $aA, $aB), $aF("Docs", [ 1, 1 ], $aA, $aB), $aA, $aB), $aA, $aB), $ba(class2(view("p"), "pending"), $z(pending(), (busy, $aW, $aX) => {
		let $aY = null;
		if (busy) {
			$aY = "...";
		} else {
			$aY = "";
		}
		return $aY;
	}), $aA, $aB), $aA, $aB), $bx(class2(view("p"), "failed"), $z(chunk_error(), (reason, $br, $bs) => {
		const $bt = reason;
		let $bu = null;
		if ($bt[0] === 0) {
			const text2 = $bt[1];
			let $bv = null;
			if (text2.length > 0) {
				$bv = "!";
			} else {
				$bv = "?";
			}
			$bu = $bv;
		} else {
			$bu = "";
		}
		return $bu;
	}), $aA, $aB), $aA, $aB), $bV(__clone(route2), (current, $bG) => {
		const $bH = current;
		let $bI = null;
		if ($bH[0] === 0) {
			$bI = home_page($aA, $bG);
		} else if ($bH[0] === 1) {
			const page = $bH[1];
			$bI = docs_page(page, $aA, $bG);
		} else {
			$bI = not_found_page($aA, $bG);
		}
		return $bI;
	}), $aA, $aB);
}
function eq(self, other) {
	const $cN = [ self, other ];
	let $cO = null;
	if ($cN[0][0] === 0 && $cN[1][0] === 0) {
		$cO = true;
	} else if ($cN[0][0] === 1 && $cN[1][0] === 1) {
		const s0 = $cN[0][1];
		const o0 = $cN[1][1];
		$cO = s0 === o0;
	} else if ($cN[0][0] === 2 && $cN[1][0] === 2) {
		$cO = true;
	} else {
		$cO = false;
	}
	return $cO;
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
		$k = enqueue(turn, self[1].v);
	} else {
		const $q = $n(draining_turns.v);
		let $r = null;
		if ($q[0] === 0) {
			const draining = $q[1];
			$r = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
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
function $z(self, transform) {
	return [ __clone(self), transform ];
}
function $F(self) {
	return __clone(self[0].v);
}
function $H(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $G(self, subscriber) {
	return $H(self, subscriber);
}
function $E(self) {
	return [ () => {
		return $F(self);
	}, (subscriber) => {
		return $G(self, subscriber);
	}, () => {
		return;
	} ];
}
function $P(self) {
	const $Q = self;
	return $Q[0] === 1;
}
function $K(runs, body) {
	const run = renew(runs, true);
	const $W = nursery(run);
	let $X = null;
	if ($W[0] === 0) {
		const nursery2 = $W[1];
		$X = (($Y) => {
			return (($Z) => {
				return body($Y, $Z);
			})(nursery2);
		})(run);
	} else {
		$X = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $X;
}
function $D(self) {
	const transform = self[1];
	const upstream = $E(__clone(self[0]));
	const pull = upstream[0];
	const runs = new3();
	return [ () => {
		const value = pull();
		return $K(runs, ($I, $J) => {
			return transform(value, $I, $J);
		});
	}, upstream[1], () => {
		release_runs(runs);
		upstream[2]();
		return;
	} ];
}
function $ac(self, $i) {
	const $ad = $i;
	let $ae = null;
	if ($ad[0] === 0) {
		const turn = $ad[1];
		$ae = enqueue(turn, self[1].v);
	} else {
		const $af = $n(draining_turns.v);
		let $ag = null;
		if ($af[0] === 0) {
			const draining = $af[1];
			$ag = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$ag = undefined;
		}
		$ae = $ag;
	}
	return $ae;
}
function $ab(self, value, $g) {
	self[0].v = __clone(value);
	$ac(self, $g);
}
function $an(self, item, $ao) {
	defer(self, () => {
		dispose(item, $ao);
		return;
	});
	return __clone(item);
}
function $A(self, $B, $C) {
	const instance = $D(self);
	const pull = instance[0];
	const cached = $a(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$ab(cached, pull(), $B);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $B, $C);
	return cached;
}
function $aN(self, name, value, $aO, $aP) {
	apply(__clone(value), self, name, $aO, $aP);
	return __clone(self);
}
function $aI(self, route2, $aJ, $aK) {
	const path = to_path(route2);
	return on_event($aN($aN(self, "href", path, $aJ, $aK), "draggable", "false", $aJ, $aK), "click", (event, $aQ) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $aQ ]);
		}
		return;
	});
}
function $aF(label, route2, $aG, $aH) {
	return text($aI(view("a"), route2, $aG, $aH), label);
}
function $aT(self, content, $aU, $aV) {
	place(__clone(content), self, $aU, $aV);
	return __clone(self);
}
function $bm(self, subscriber) {
	return $H(self, subscriber);
}
function $bk(self) {
	return [ () => {
		return $F(self);
	}, (subscriber) => {
		return $bm(self, subscriber);
	}, () => {
		return;
	} ];
}
function $bo(runs, body) {
	const run = renew(runs, true);
	const $bp = nursery(run);
	let $bq = null;
	if ($bp[0] === 0) {
		const nursery2 = $bp[1];
		$bq = (($Y) => {
			return (($Z) => {
				return body($Y, $Z);
			})(nursery2);
		})(run);
	} else {
		$bq = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $bq;
}
function $bj(self) {
	const transform = self[1];
	const upstream = $bk(__clone(self[0]));
	const pull = upstream[0];
	const runs = new3();
	return [ () => {
		const value = pull();
		return $bo(runs, ($I, $J) => {
			return transform(value, $I, $J);
		});
	}, upstream[1], () => {
		release_runs(runs);
		upstream[2]();
		return;
	} ];
}
function $bi(flow, observer, immediately) {
	const instance = $bj(flow);
	const pull = instance[0];
	const subscription = instance[1](mint_subscriber(() => {
		return observer(pull());
	}));
	also_releasing(subscription, instance[2]);
	if (immediately) {
		observer(pull());
	}
	return subscription;
}
function $bh(self, observer) {
	return $bi(self, observer, true);
}
function $bd(flow, observer, $be, $bf) {
	$an(get_owner($bf), $bh(flow, observer), $be);
}
function $ba(self, source, $bb, $bc) {
	const element = __clone(self[0]);
	$bd(source, (value) => {
		element.textContent = value;
		return;
	}, $bb, $bc);
	return __clone(self);
}
function $bC(self) {
	return [ () => {
		return $F(self);
	}, (subscriber) => {
		return $bm(self, subscriber);
	}, () => {
		return;
	} ];
}
function $bB(self) {
	const transform = self[1];
	const upstream = $bC(__clone(self[0]));
	const pull = upstream[0];
	const runs = new3();
	return [ () => {
		const value = pull();
		return $bo(runs, ($I, $J) => {
			return transform(value, $I, $J);
		});
	}, upstream[1], () => {
		release_runs(runs);
		upstream[2]();
		return;
	} ];
}
function $bA(flow, observer, immediately) {
	const instance = $bB(flow);
	const pull = instance[0];
	const subscription = instance[1](mint_subscriber(() => {
		return observer(pull());
	}));
	also_releasing(subscription, instance[2]);
	if (immediately) {
		observer(pull());
	}
	return subscription;
}
function $bz(self, observer) {
	return $bA(self, observer, true);
}
function $by(flow, observer, $be, $bf) {
	$an(get_owner($bf), $bz(flow, observer), $be);
}
function $bx(self, source, $bb, $bc) {
	const element = __clone(self[0]);
	$by(source, (value) => {
		element.textContent = value;
		return;
	}, $bb, $bc);
	return __clone(self);
}
function $bT(source) {
	__chunk_preload(__chunk_arm($F(source)));
}
function $cd(self, value, $g) {
	self[0].v = __clone(value);
	$ac(self, $g);
}
function $cv(signal, observer) {
	const cell = signal[0];
	return $H(signal, mint_subscriber(() => {
		const $cw = [ 0, cell ];
		let $cx = null;
		if ($cw[0] === 0) {
			const live = $cw[1];
			$cx = observer(live.v);
		} else {
			$cx = undefined;
		}
		return $cx;
	}));
}
function $cu(self, observer, immediately) {
	const subscription = $cv(self, observer);
	if (immediately) {
		observer($F(self));
	}
	return subscription;
}
function $ct(self, observer) {
	return $cu(self, observer, true);
}
function $cs(flow, observer, $be, $bf) {
	$an(get_owner($bf), $ct(__clone(flow), observer), $be);
}
function $bV(source, render, $bW) {
	const gated = $a($F(source));
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value, $bX) => {
		armed.v = true;
		$ab(gated, value, [ 0, $bX ]);
		return;
	};
	const wire = ($bY) => {
		$cs(__clone(source), (value) => {
			return $t([ 1 ], ($bZ) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $bZ ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $bZ ]);
					advance(value, $bZ);
				} else {
					set_chunk_pending(true, [ 0, $bZ ]);
					__chunk_load(arm, () => {
						return $t([ 1 ], ($cq) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $cq ]);
								advance(value, $cq);
							}
							return;
						});
					}, (reason) => {
						return $t([ 1 ], ($cr) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $cr ]);
								$cd(chunk_error_signal, [ 0, reason ], [ 0, $cr ]);
							}
							return;
						});
					});
				}
				return;
			});
		}, $bW, $bY);
		return;
	};
	return [ [ 1 ], render, [ 0, __clone(gated) ], armed, wire ];
}
function $cX(self, content, end, $cY, $cZ) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(__clone(content), [ __clone(staging) ], $cY, $cZ);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function $cU(self, content, $cV, $cW) {
	return $cX(self, __clone(content), self[0], $cV, $cW);
}
function $da(owner, body) {
	return body(owner);
}
function $cF(parent, source, render, armed, $cG, $cH) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($cH), () => {
		const $cI = live_owner.v;
		let $cJ = null;
		if ($cI[0] === 1) {
			$cJ = $cI;
		} else {
			$cJ = [ 0, dispose2($cI[1]) ];
		}
		$cJ;
		close(region);
		return;
	});
	$cs(__clone(source), (value) => {
		const $cL = last_value.v;
		let $cM = null;
		if ($cL[0] === 0) {
			const previous = $cL[1];
			$cM = eq(previous, value);
		} else {
			$cM = false;
		}
		const unchanged = $cM;
		if (armed.v && !(unchanged)) {
			const $cP = live_owner.v;
			let $cQ = null;
			if ($cP[0] === 1) {
				$cQ = $cP;
			} else {
				$cQ = [ 0, dispose2($cP[1]) ];
			}
			$cQ;
			const $cR = live_row.v;
			let $cS = null;
			if ($cR[0] === 0) {
				const row = $cR[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$cS = undefined;
			} else {
				$cS = undefined;
			}
			$cS;
			const owner = new3();
			const row2 = $da(owner, ($cT) => {
				return $cU(region, render(value, $cT), $cG, $cT);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $cG, $cH);
}
function $cA(self, parent, $cB, $cC) {
	self[4]($cC);
	const render = self[1];
	const armed = self[3];
	const $cD = self[0];
	let $cE = null;
	if ($cD[0] === 0) {
		const source = $cD[1];
		$cE = $cF(parent, __clone(source), render, armed, $cB, $cC);
	} else {
		const $db = self[2];
		let $dc = null;
		if ($db[0] === 0) {
			const gated = $db[1];
			$dc = $cF(parent, __clone(gated), render, armed, $cB, $cC);
		} else {
			$dc = undefined;
		}
		$cE = $dc;
	}
	return $cE;
}
function $cz(self, content, $aU, $aV) {
	$cA(__clone(content), self, $aU, $aV);
	return __clone(self);
}
function $de(body) {
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
__vilan_chunks.fn.$aF = $aF;
__vilan_chunks.fn.$aT = $aT;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $A($z(current_path([ 1 ]), (path, $u, $v) => {
	return parse(path);
}), [ 1 ], [ 1 ]);
mount_root("app", ($az) => {
	return app(route, [ 1 ], $az);
});
