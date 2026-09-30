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
function home_page($bM, $bN) {
	return __vilan_chunks.fn.home_page($bM, $bN);
}
function docs_page(page, $bQ, $bR) {
	return __vilan_chunks.fn.docs_page(page, $bQ, $bR);
}
function not_found_page($bU, $bV) {
	return __vilan_chunks.fn.not_found_page($bU, $bV);
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
function dispose(self, $aq) {
	const $ar = $aq;
	let $as = null;
	if ($ar[0] === 0) {
		const established = $ar[1];
		$as = [ 0, established ];
	} else {
		$as = $n(draining_turns.v);
	}
	const ambient = $as;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $at = [ 0, handle[0] ];
	let $au = null;
	if ($at[0] === 0) {
		const subscribers = $at[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$au = undefined;
	} else {
		$au = undefined;
	}
	$au;
	const $av = ambient;
	let $aw = null;
	if ($av[0] === 0) {
		const turn = $av[1];
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
		$aw = undefined;
	} else {
		$aw = undefined;
	}
	$aw;
	const $ax = handle[3].v;
	let $ay = null;
	if ($ax[0] === 0) {
		const release = $ax[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ay = undefined;
	} else {
		$ay = undefined;
	}
	return $ay;
}
function new3() {
	return [ __shared_new([ 0, [ 1 ], [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aB = null;
	if (is_disposed(self)) {
		cleanup();
	} else {
		const $az = self[0].v[1];
		let $aA = null;
		if ($az[0] === 0) {
			const list = $az[1];
			$aA = list.v.push(cleanup);
		} else {
			owner_lists_allocated_count.v = owner_lists_allocated_count.v + 1;
			self[0].v[1] = [ 0, __shared_new([ cleanup ]) ];
			$aA = undefined;
		}
		$aB = $aA;
	}
	return $aB;
}
function renew(self, with_nursery) {
	const live = [ self[0], self[0].v[0] ];
	dispose2(live);
	const next = self[0].v[0];
	if (with_nursery) {
		self[0].v[2] = [ 0, detached_nursery() ];
	}
	return [ self[0], next ];
}
function nursery(self) {
	let $W = null;
	if (is_disposed(self)) {
		$W = [ 1 ];
	} else {
		$W = self[0].v[2];
	}
	return $W;
}
function dispose2(self) {
	let $V = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, [ 1 ], [ 1 ] ];
		const $L = held[2];
		let $M = null;
		if ($L[0] === 0) {
			const nursery2 = $L[1];
			$M = nursery2.cancel();
		} else {
			$M = undefined;
		}
		$M;
		const $N = held[1];
		let $O = null;
		if ($N[0] === 0) {
			const cleanups = $N[1];
			let failure = [ 1 ];
			for (const cleanup of cleanups.v) {
				const $P = __guarded(cleanup);
				let $Q = null;
				if ($P[0] === 0) {
					const message = $P[1];
					if ($R(failure)) {
						failure = [ 0, message ];
					}
					$Q = undefined;
				} else {
					$Q = undefined;
				}
				$Q;
			}
			const $T = failure;
			let $U = null;
			if ($T[0] === 0) {
				const message2 = $T[1];
				$U = (() => {
					throw message2;
				})();
			} else {
				$U = undefined;
			}
			$O = $U;
		} else {
			$O = undefined;
		}
		$V = $O;
	}
	return $V;
}
function get_owner($bj) {
	return $bj;
}
function register_with_owner(subscription, $ak, $al) {
	const $am = $al;
	let $an = null;
	if ($am[0] === 0) {
		const owner = $am[1];
		$an = $ao(owner, subscription, $ak);
	} else {
		$an = __clone(subscription);
	}
	return $an;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $ai = previous;
		let $aj = null;
		if ($ai[0] === 0) {
			const earlier = $ai[1];
			$aj = earlier();
		} else {
			$aj = undefined;
		}
		return $aj;
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
function navigate(path, $aU) {
	ensure_wired($aU);
	history.pushState("", "", path);
	$f(path_signal, path, $aU);
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
	let $aH = null;
	if (is_svg_tag(tag)) {
		$aH = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$aH = [ document.createElement(tag) ];
	}
	return $aH;
}
function is_svg_tag(tag) {
	const $aF = tag;
	let $aG = null;
	if ($aF === "svg") {
		$aG = true;
	} else if ($aF === "path") {
		$aG = true;
	} else if ($aF === "circle") {
		$aG = true;
	} else if ($aF === "ellipse") {
		$aG = true;
	} else if ($aF === "rect") {
		$aG = true;
	} else if ($aF === "line") {
		$aG = true;
	} else if ($aF === "polyline") {
		$aG = true;
	} else if ($aF === "polygon") {
		$aG = true;
	} else if ($aF === "g") {
		$aG = true;
	} else if ($aF === "defs") {
		$aG = true;
	} else if ($aF === "use") {
		$aG = true;
	} else if ($aF === "symbol") {
		$aG = true;
	} else if ($aF === "marker") {
		$aG = true;
	} else if ($aF === "pattern") {
		$aG = true;
	} else if ($aF === "mask") {
		$aG = true;
	} else if ($aF === "clipPath") {
		$aG = true;
	} else if ($aF === "linearGradient") {
		$aG = true;
	} else if ($aF === "radialGradient") {
		$aG = true;
	} else if ($aF === "stop") {
		$aG = true;
	} else if ($aF === "text") {
		$aG = true;
	} else if ($aF === "tspan") {
		$aG = true;
	} else if ($aF === "textPath") {
		$aG = true;
	} else if ($aF === "filter") {
		$aG = true;
	} else if ($aF === "foreignObject") {
		$aG = true;
	} else if ($aF === "feGaussianBlur") {
		$aG = true;
	} else if ($aF === "feColorMatrix") {
		$aG = true;
	} else if ($aF === "feOffset") {
		$aG = true;
	} else if ($aF === "feMerge") {
		$aG = true;
	} else if ($aF === "feMergeNode") {
		$aG = true;
	} else if ($aF === "feFlood") {
		$aG = true;
	} else if ($aF === "feComposite") {
		$aG = true;
	} else if ($aF === "feBlend") {
		$aG = true;
	} else if ($aF === "feDropShadow") {
		$aG = true;
	} else {
		$aG = false;
	}
	return $aG;
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
		return $t([ 1 ], ($aV) => {
			return handler(dispatched, $aV);
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
function set_chunk_pending(busy, $cm) {
	if ($F(chunk_pending_signal) !== busy) {
		$cg(chunk_pending_signal, busy, $cm);
	}
}
function clear_chunk_error($cd) {
	const $ce = $F(chunk_error_signal);
	let $cf = null;
	if ($ce[0] === 0) {
		const _reason = $ce[1];
		$cf = $cg(chunk_error_signal, [ 1 ], $cd);
	} else {
		$cf = undefined;
	}
	return $cf;
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
		let $cN = null;
		if (at + 1 < rows.length) {
			$cN = __at(rows, at + 1)[0];
		} else {
			$cN = self[0];
		}
		const end = __clone($cN);
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
	const $dj = $t([ 1 ], ($dg) => {
		return $dh(body);
	});
	const built = $dj[0];
	const root = $dj[1];
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
	const $aO = route2;
	let $aP = null;
	if ($aO[0] === 0) {
		$aP = "/";
	} else if ($aO[0] === 1) {
		const page = $aO[1];
		$aP = "/docs/" + page;
	} else {
		$aP = "/404";
	}
	return $aP;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $bO, $bP) {
	return $aW($aW(view("section"), text(view("h2"), title), $bO, $bP), text(view("p"), body), $bO, $bP);
}
function app(route2, $aD, $aE) {
	$bW(route2);
	return $cC($aW($aW($aW(view("main"), $aW($aW(view("nav"), $aI("Home", [ 0 ], $aD, $aE), $aD, $aE), $aI("Docs", [ 1, 1 ], $aD, $aE), $aD, $aE), $aD, $aE), $bd(class2(view("p"), "pending"), $z(pending(), (busy, $aZ, $ba) => {
		let $bb = null;
		if (busy) {
			$bb = "...";
		} else {
			$bb = "";
		}
		return $bb;
	}), $aD, $aE), $aD, $aE), $bA(class2(view("p"), "failed"), $z(chunk_error(), (reason, $bu, $bv) => {
		const $bw = reason;
		let $bx = null;
		if ($bw[0] === 0) {
			const text2 = $bw[1];
			let $by = null;
			if (text2.length > 0) {
				$by = "!";
			} else {
				$by = "?";
			}
			$bx = $by;
		} else {
			$bx = "";
		}
		return $bx;
	}), $aD, $aE), $aD, $aE), $bY(__clone(route2), (current, $bJ) => {
		const $bK = current;
		let $bL = null;
		if ($bK[0] === 0) {
			$bL = home_page($aD, $bJ);
		} else if ($bK[0] === 1) {
			const page = $bK[1];
			$bL = docs_page(page, $aD, $bJ);
		} else {
			$bL = not_found_page($aD, $bJ);
		}
		return $bL;
	}), $aD, $aE);
}
function eq(self, other) {
	const $cQ = [ self, other ];
	let $cR = null;
	if ($cQ[0][0] === 0 && $cQ[1][0] === 0) {
		$cR = true;
	} else if ($cQ[0][0] === 1 && $cQ[1][0] === 1) {
		const s0 = $cQ[0][1];
		const o0 = $cQ[1][1];
		$cR = s0 === o0;
	} else if ($cQ[0][0] === 2 && $cQ[1][0] === 2) {
		$cR = true;
	} else {
		$cR = false;
	}
	return $cR;
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
function $R(self) {
	const $S = self;
	return $S[0] === 1;
}
function $K(runs, body) {
	const run = renew(runs, true);
	const $X = nursery(run);
	let $Y = null;
	if ($X[0] === 0) {
		const nursery2 = $X[1];
		$Y = (($Z) => {
			return (($aa) => {
				return body($Z, $aa);
			})(nursery2);
		})(run);
	} else {
		$Y = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $Y;
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
function $ad(self, $i) {
	const $ae = $i;
	let $af = null;
	if ($ae[0] === 0) {
		const turn = $ae[1];
		$af = enqueue(turn, self[1].v);
	} else {
		const $ag = $n(draining_turns.v);
		let $ah = null;
		if ($ag[0] === 0) {
			const draining = $ag[1];
			$ah = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$ah = undefined;
		}
		$af = $ah;
	}
	return $af;
}
function $ac(self, value, $g) {
	self[0].v = __clone(value);
	$ad(self, $g);
}
function $ao(self, item, $ap) {
	defer(self, () => {
		dispose(item, $ap);
		return;
	});
	return __clone(item);
}
function $A(self, $B, $C) {
	const instance = $D(self);
	const pull = instance[0];
	const cached = $a(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$ac(cached, pull(), $B);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $B, $C);
	return cached;
}
function $aQ(self, name, value, $aR, $aS) {
	apply(__clone(value), self, name, $aR, $aS);
	return __clone(self);
}
function $aL(self, route2, $aM, $aN) {
	const path = to_path(route2);
	return on_event($aQ($aQ(self, "href", path, $aM, $aN), "draggable", "false", $aM, $aN), "click", (event, $aT) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $aT ]);
		}
		return;
	});
}
function $aI(label, route2, $aJ, $aK) {
	return text($aL(view("a"), route2, $aJ, $aK), label);
}
function $aW(self, content, $aX, $aY) {
	place(__clone(content), self, $aX, $aY);
	return __clone(self);
}
function $bp(self, subscriber) {
	return $H(self, subscriber);
}
function $bn(self) {
	return [ () => {
		return $F(self);
	}, (subscriber) => {
		return $bp(self, subscriber);
	}, () => {
		return;
	} ];
}
function $br(runs, body) {
	const run = renew(runs, true);
	const $bs = nursery(run);
	let $bt = null;
	if ($bs[0] === 0) {
		const nursery2 = $bs[1];
		$bt = (($Z) => {
			return (($aa) => {
				return body($Z, $aa);
			})(nursery2);
		})(run);
	} else {
		$bt = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $bt;
}
function $bm(self) {
	const transform = self[1];
	const upstream = $bn(__clone(self[0]));
	const pull = upstream[0];
	const runs = new3();
	return [ () => {
		const value = pull();
		return $br(runs, ($I, $J) => {
			return transform(value, $I, $J);
		});
	}, upstream[1], () => {
		release_runs(runs);
		upstream[2]();
		return;
	} ];
}
function $bl(flow, observer, immediately) {
	const instance = $bm(flow);
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
function $bk(self, observer) {
	return $bl(self, observer, true);
}
function $bg(flow, observer, $bh, $bi) {
	$ao(get_owner($bi), $bk(flow, observer), $bh);
}
function $bd(self, source, $be, $bf) {
	const element = __clone(self[0]);
	$bg(source, (value) => {
		element.textContent = value;
		return;
	}, $be, $bf);
	return __clone(self);
}
function $bF(self) {
	return [ () => {
		return $F(self);
	}, (subscriber) => {
		return $bp(self, subscriber);
	}, () => {
		return;
	} ];
}
function $bE(self) {
	const transform = self[1];
	const upstream = $bF(__clone(self[0]));
	const pull = upstream[0];
	const runs = new3();
	return [ () => {
		const value = pull();
		return $br(runs, ($I, $J) => {
			return transform(value, $I, $J);
		});
	}, upstream[1], () => {
		release_runs(runs);
		upstream[2]();
		return;
	} ];
}
function $bD(flow, observer, immediately) {
	const instance = $bE(flow);
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
function $bC(self, observer) {
	return $bD(self, observer, true);
}
function $bB(flow, observer, $bh, $bi) {
	$ao(get_owner($bi), $bC(flow, observer), $bh);
}
function $bA(self, source, $be, $bf) {
	const element = __clone(self[0]);
	$bB(source, (value) => {
		element.textContent = value;
		return;
	}, $be, $bf);
	return __clone(self);
}
function $bW(source) {
	__chunk_preload(__chunk_arm($F(source)));
}
function $cg(self, value, $g) {
	self[0].v = __clone(value);
	$ad(self, $g);
}
function $cy(signal, observer) {
	const cell = signal[0];
	return $H(signal, mint_subscriber(() => {
		const $cz = [ 0, cell ];
		let $cA = null;
		if ($cz[0] === 0) {
			const live = $cz[1];
			$cA = observer(live.v);
		} else {
			$cA = undefined;
		}
		return $cA;
	}));
}
function $cx(self, observer, immediately) {
	const subscription = $cy(self, observer);
	if (immediately) {
		observer($F(self));
	}
	return subscription;
}
function $cw(self, observer) {
	return $cx(self, observer, true);
}
function $cv(flow, observer, $bh, $bi) {
	$ao(get_owner($bi), $cw(__clone(flow), observer), $bh);
}
function $bY(source, render, $bZ) {
	const gated = $a($F(source));
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value, $ca) => {
		armed.v = true;
		$ac(gated, value, [ 0, $ca ]);
		return;
	};
	const wire = ($cb) => {
		$cv(__clone(source), (value) => {
			return $t([ 1 ], ($cc) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $cc ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $cc ]);
					advance(value, $cc);
				} else {
					set_chunk_pending(true, [ 0, $cc ]);
					__chunk_load(arm, () => {
						return $t([ 1 ], ($ct) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $ct ]);
								advance(value, $ct);
							}
							return;
						});
					}, (reason) => {
						return $t([ 1 ], ($cu) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $cu ]);
								$cg(chunk_error_signal, [ 0, reason ], [ 0, $cu ]);
							}
							return;
						});
					});
				}
				return;
			});
		}, $bZ, $cb);
		return;
	};
	return [ [ 1 ], render, [ 0, __clone(gated) ], armed, wire ];
}
function $da(self, content, end, $db, $dc) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(__clone(content), [ __clone(staging) ], $db, $dc);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function $cX(self, content, $cY, $cZ) {
	return $da(self, __clone(content), self[0], $cY, $cZ);
}
function $dd(owner, body) {
	return body(owner);
}
function $cI(parent, source, render, armed, $cJ, $cK) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($cK), () => {
		const $cL = live_owner.v;
		let $cM = null;
		if ($cL[0] === 1) {
			$cM = $cL;
		} else {
			$cM = [ 0, dispose2($cL[1]) ];
		}
		$cM;
		close(region);
		return;
	});
	$cv(__clone(source), (value) => {
		const $cO = last_value.v;
		let $cP = null;
		if ($cO[0] === 0) {
			const previous = $cO[1];
			$cP = eq(previous, value);
		} else {
			$cP = false;
		}
		const unchanged = $cP;
		if (armed.v && !(unchanged)) {
			const $cS = live_owner.v;
			let $cT = null;
			if ($cS[0] === 1) {
				$cT = $cS;
			} else {
				$cT = [ 0, dispose2($cS[1]) ];
			}
			$cT;
			const $cU = live_row.v;
			let $cV = null;
			if ($cU[0] === 0) {
				const row = $cU[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$cV = undefined;
			} else {
				$cV = undefined;
			}
			$cV;
			const owner = new3();
			const row2 = $dd(owner, ($cW) => {
				return $cX(region, render(value, $cW), $cJ, $cW);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $cJ, $cK);
}
function $cD(self, parent, $cE, $cF) {
	self[4]($cF);
	const render = self[1];
	const armed = self[3];
	const $cG = self[0];
	let $cH = null;
	if ($cG[0] === 0) {
		const source = $cG[1];
		$cH = $cI(parent, __clone(source), render, armed, $cE, $cF);
	} else {
		const $de = self[2];
		let $df = null;
		if ($de[0] === 0) {
			const gated = $de[1];
			$df = $cI(parent, __clone(gated), render, armed, $cE, $cF);
		} else {
			$df = undefined;
		}
		$cH = $df;
	}
	return $cH;
}
function $cC(self, content, $aX, $aY) {
	$cD(__clone(content), self, $aX, $aY);
	return __clone(self);
}
function $dh(body) {
	const scope = new3();
	const result = body(scope);
	return [ result, scope ];
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
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
__vilan_chunks.fn.$aI = $aI;
__vilan_chunks.fn.$aW = $aW;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $A($z(current_path([ 1 ]), (path, $u, $v) => {
	return parse(path);
}), [ 1 ], [ 1 ]);
mount_root("app", ($aC) => {
	return app(route, [ 1 ], $aC);
});
