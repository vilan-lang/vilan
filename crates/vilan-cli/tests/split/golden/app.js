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
function home_page($bh, $bi) {
	return __vilan_chunks.fn.home_page($bh, $bi);
}
function docs_page(page, $bl, $bm) {
	return __vilan_chunks.fn.docs_page(page, $bl, $bm);
}
function not_found_page($bp, $bq) {
	return __vilan_chunks.fn.not_found_page($bp, $bq);
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
function dispose(self, $V) {
	const $W = $V;
	let $X = null;
	if ($W[0] === 0) {
		const established = $W[1];
		$X = [ 0, established ];
	} else {
		$X = $n(draining_turns.v);
	}
	const ambient = $X;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $Y = [ 0, handle[0] ];
	let $Z = null;
	if ($Y[0] === 0) {
		const subscribers = $Y[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$Z = undefined;
	} else {
		$Z = undefined;
	}
	$Z;
	const $aa = ambient;
	let $ab = null;
	if ($aa[0] === 0) {
		const turn = $aa[1];
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
		$ab = undefined;
	} else {
		$ab = undefined;
	}
	$ab;
	const $ac = handle[3].v;
	let $ad = null;
	if ($ac[0] === 0) {
		const release = $ac[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ad = undefined;
	} else {
		$ad = undefined;
	}
	return $ad;
}
function new3() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function defer(self, cleanup) {
	if (self[1].v) {
		cleanup();
	} else {
		self[0].v.push(cleanup);
	}
}
function dispose2(self) {
	let $co = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $ci = __guarded(cleanup);
			let $cj = null;
			if ($ci[0] === 0) {
				const message = $ci[1];
				if ($ck(failure)) {
					failure = [ 0, message ];
				}
				$cj = undefined;
			} else {
				$cj = undefined;
			}
			$cj;
		}
		self[0].v = [  ];
		const $cm = failure;
		let $cn = null;
		if ($cm[0] === 0) {
			const message2 = $cm[1];
			$cn = (() => {
				throw message2;
			})();
		} else {
			$cn = undefined;
		}
		$co = $cn;
	}
	return $co;
}
function get_owner($aJ) {
	return $aJ;
}
function register_with_owner(subscription, $P, $Q) {
	const $R = $Q;
	let $S = null;
	if ($R[0] === 0) {
		const owner = $R[1];
		$S = $T(owner, subscription, $P);
	} else {
		$S = __clone(subscription);
	}
	return $S;
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $N = previous;
		let $O = null;
		if ($N[0] === 0) {
			const earlier = $N[1];
			$O = earlier();
		} else {
			$O = undefined;
		}
		return $O;
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
function navigate(path, $aw) {
	ensure_wired($aw);
	history.pushState("", "", path);
	$f(path_signal, path, $aw);
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
	let $aj = null;
	if (is_svg_tag(tag)) {
		$aj = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$aj = [ document.createElement(tag) ];
	}
	return $aj;
}
function is_svg_tag(tag) {
	const $ah = tag;
	let $ai = null;
	if ($ah === "svg") {
		$ai = true;
	} else if ($ah === "path") {
		$ai = true;
	} else if ($ah === "circle") {
		$ai = true;
	} else if ($ah === "ellipse") {
		$ai = true;
	} else if ($ah === "rect") {
		$ai = true;
	} else if ($ah === "line") {
		$ai = true;
	} else if ($ah === "polyline") {
		$ai = true;
	} else if ($ah === "polygon") {
		$ai = true;
	} else if ($ah === "g") {
		$ai = true;
	} else if ($ah === "defs") {
		$ai = true;
	} else if ($ah === "use") {
		$ai = true;
	} else if ($ah === "symbol") {
		$ai = true;
	} else if ($ah === "marker") {
		$ai = true;
	} else if ($ah === "pattern") {
		$ai = true;
	} else if ($ah === "mask") {
		$ai = true;
	} else if ($ah === "clipPath") {
		$ai = true;
	} else if ($ah === "linearGradient") {
		$ai = true;
	} else if ($ah === "radialGradient") {
		$ai = true;
	} else if ($ah === "stop") {
		$ai = true;
	} else if ($ah === "text") {
		$ai = true;
	} else if ($ah === "tspan") {
		$ai = true;
	} else if ($ah === "textPath") {
		$ai = true;
	} else if ($ah === "filter") {
		$ai = true;
	} else if ($ah === "foreignObject") {
		$ai = true;
	} else if ($ah === "feGaussianBlur") {
		$ai = true;
	} else if ($ah === "feColorMatrix") {
		$ai = true;
	} else if ($ah === "feOffset") {
		$ai = true;
	} else if ($ah === "feMerge") {
		$ai = true;
	} else if ($ah === "feMergeNode") {
		$ai = true;
	} else if ($ah === "feFlood") {
		$ai = true;
	} else if ($ah === "feComposite") {
		$ai = true;
	} else if ($ah === "feBlend") {
		$ai = true;
	} else if ($ah === "feDropShadow") {
		$ai = true;
	} else {
		$ai = false;
	}
	return $ai;
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
		return $t([ 1 ], ($ax) => {
			return handler(dispatched, $ax);
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
function set_chunk_pending(busy, $bH) {
	if ($D(chunk_pending_signal) !== busy) {
		$bB(chunk_pending_signal, busy, $bH);
	}
}
function clear_chunk_error($by) {
	const $bz = $D(chunk_error_signal);
	let $bA = null;
	if ($bz[0] === 0) {
		const _reason = $bz[1];
		$bA = $bB(chunk_error_signal, [ 1 ], $by);
	} else {
		$bA = undefined;
	}
	return $bA;
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
		let $cp = null;
		if (at + 1 < rows.length) {
			$cp = __at(rows, at + 1)[0];
		} else {
			$cp = self[0];
		}
		const end = __clone($cp);
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
	const $cL = $t([ 1 ], ($cI) => {
		return $cJ(body);
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
	let $w = null;
	if (__at(parts, 0) === "docs" && parts.length === 2) {
		const $u = __parse_i32(__at(parts, 1));
		let $v = null;
		if ($u[0] === 0) {
			const page = $u[1];
			return [ 1, page ];
		} else {
			$v = undefined;
		}
		$w = $v;
	}
	$w;
	return [ 2 ];
}
function href(route2) {
	const $aq = route2;
	let $ar = null;
	if ($aq[0] === 0) {
		$ar = "/";
	} else if ($aq[0] === 1) {
		const page = $aq[1];
		$ar = "/docs/" + page;
	} else {
		$ar = "/404";
	}
	return $ar;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $bj, $bk) {
	return $ay($ay(view("section"), text(view("h2"), title), $bj, $bk), text(view("p"), body), $bj, $bk);
}
function app(route2, $af, $ag) {
	$br(route2);
	return $bX($ay($ay($ay(view("main"), $ay($ay(view("nav"), $ak("Home", [ 0 ], $af, $ag), $af, $ag), $ak("Docs", [ 1, 1 ], $af, $ag), $af, $ag), $af, $ag), $aD(class2(view("p"), "pending"), $x(pending(), (busy) => {
		let $aB = null;
		if (busy) {
			$aB = "...";
		} else {
			$aB = "";
		}
		return $aB;
	}), $af, $ag), $af, $ag), $aV(class2(view("p"), "failed"), $x(chunk_error(), (reason) => {
		const $aR = reason;
		let $aS = null;
		if ($aR[0] === 0) {
			const text2 = $aR[1];
			let $aT = null;
			if (text2.length > 0) {
				$aT = "!";
			} else {
				$aT = "?";
			}
			$aS = $aT;
		} else {
			$aS = "";
		}
		return $aS;
	}), $af, $ag), $af, $ag), $bt(__clone(route2), (current, $be) => {
		const $bf = current;
		let $bg = null;
		if ($bf[0] === 0) {
			$bg = home_page($af, $be);
		} else if ($bf[0] === 1) {
			const page = $bf[1];
			$bg = docs_page(page, $af, $be);
		} else {
			$bg = not_found_page($af, $be);
		}
		return $bg;
	}), $af, $ag);
}
function eq(self, other) {
	const $cs = [ self, other ];
	let $ct = null;
	if ($cs[0][0] === 0 && $cs[1][0] === 0) {
		$ct = true;
	} else if ($cs[0][0] === 1 && $cs[1][0] === 1) {
		const s0 = $cs[0][1];
		const o0 = $cs[1][1];
		$ct = s0 === o0;
	} else if ($cs[0][0] === 2 && $cs[1][0] === 2) {
		$ct = true;
	} else {
		$ct = false;
	}
	return $ct;
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
function $x(self, transform) {
	return [ __clone(self), transform ];
}
function $D(self) {
	return __clone(self[0].v);
}
function $F(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $E(self, subscriber) {
	return $F(self, subscriber);
}
function $C(self) {
	return [ () => {
		return $D(self);
	}, (subscriber) => {
		return $E(self, subscriber);
	}, () => {
		return;
	} ];
}
function $B(self) {
	const transform = self[1];
	const upstream = $C(__clone(self[0]));
	const pull = upstream[0];
	return [ () => {
		return transform(pull());
	}, upstream[1], upstream[2] ];
}
function $I(self, $i) {
	const $J = $i;
	let $K = null;
	if ($J[0] === 0) {
		const turn = $J[1];
		$K = enqueue(turn, self[1].v);
	} else {
		const $L = $n(draining_turns.v);
		let $M = null;
		if ($L[0] === 0) {
			const draining = $L[1];
			$M = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$M = undefined;
		}
		$K = $M;
	}
	return $K;
}
function $H(self, value, $g) {
	self[0].v = __clone(value);
	$I(self, $g);
}
function $T(self, item, $U) {
	if (self[1].v) {
		dispose(item, $U);
	} else {
		self[0].v.push(() => {
			dispose(item, $U);
			return;
		});
	}
	return __clone(item);
}
function $y(self, $z, $A) {
	const instance = $B(self);
	const pull = instance[0];
	const cached = $a(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$H(cached, pull(), $z);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $z, $A);
	return cached;
}
function $as(self, name, value, $at, $au) {
	apply(__clone(value), self, name, $at, $au);
	return __clone(self);
}
function $an(self, route2, $ao, $ap) {
	const path = to_path(route2);
	return on_event($as($as(self, "href", path, $ao, $ap), "draggable", "false", $ao, $ap), "click", (event, $av) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $av ]);
		}
		return;
	});
}
function $ak(label, route2, $al, $am) {
	return text($an(view("a"), route2, $al, $am), label);
}
function $ay(self, content, $az, $aA) {
	place(__clone(content), self, $az, $aA);
	return __clone(self);
}
function $aP(self, subscriber) {
	return $F(self, subscriber);
}
function $aN(self) {
	return [ () => {
		return $D(self);
	}, (subscriber) => {
		return $aP(self, subscriber);
	}, () => {
		return;
	} ];
}
function $aM(self) {
	const transform = self[1];
	const upstream = $aN(__clone(self[0]));
	const pull = upstream[0];
	return [ () => {
		return transform(pull());
	}, upstream[1], upstream[2] ];
}
function $aL(flow, observer, immediately) {
	const instance = $aM(flow);
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
function $aK(self, observer) {
	return $aL(self, observer, true);
}
function $aG(self, observer, $aH, $aI) {
	$T(get_owner($aI), $aK(self, observer), $aH);
}
function $aD(self, source, $aE, $aF) {
	const element = __clone(self[0]);
	$aG(source, (value) => {
		element.textContent = value;
		return;
	}, $aE, $aF);
	return __clone(self);
}
function $ba(self) {
	return [ () => {
		return $D(self);
	}, (subscriber) => {
		return $aP(self, subscriber);
	}, () => {
		return;
	} ];
}
function $aZ(self) {
	const transform = self[1];
	const upstream = $ba(__clone(self[0]));
	const pull = upstream[0];
	return [ () => {
		return transform(pull());
	}, upstream[1], upstream[2] ];
}
function $aY(flow, observer, immediately) {
	const instance = $aZ(flow);
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
function $aX(self, observer) {
	return $aY(self, observer, true);
}
function $aW(self, observer, $aH, $aI) {
	$T(get_owner($aI), $aX(self, observer), $aH);
}
function $aV(self, source, $aE, $aF) {
	const element = __clone(self[0]);
	$aW(source, (value) => {
		element.textContent = value;
		return;
	}, $aE, $aF);
	return __clone(self);
}
function $br(source) {
	__chunk_preload(__chunk_arm($D(source)));
}
function $bB(self, value, $g) {
	self[0].v = __clone(value);
	$I(self, $g);
}
function $bT(signal, observer) {
	const cell = signal[0];
	return $F(signal, mint_subscriber(() => {
		const $bU = [ 0, cell ];
		let $bV = null;
		if ($bU[0] === 0) {
			const live = $bU[1];
			$bV = observer(live.v);
		} else {
			$bV = undefined;
		}
		return $bV;
	}));
}
function $bS(self, observer, immediately) {
	const subscription = $bT(self, observer);
	if (immediately) {
		observer($D(self));
	}
	return subscription;
}
function $bR(self, observer) {
	return $bS(self, observer, true);
}
function $bQ(self, observer, $aH, $aI) {
	$T(get_owner($aI), $bR(self, observer), $aH);
}
function $bt(source, render, $bu) {
	const gated = $a($D(source));
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value, $bv) => {
		armed.v = true;
		$H(gated, value, [ 0, $bv ]);
		return;
	};
	const wire = ($bw) => {
		$bQ(__clone(source), (value) => {
			return $t([ 1 ], ($bx) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $bx ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $bx ]);
					advance(value, $bx);
				} else {
					set_chunk_pending(true, [ 0, $bx ]);
					__chunk_load(arm, () => {
						return $t([ 1 ], ($bO) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bO ]);
								advance(value, $bO);
							}
							return;
						});
					}, (reason) => {
						return $t([ 1 ], ($bP) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bP ]);
								$bB(chunk_error_signal, [ 0, reason ], [ 0, $bP ]);
							}
							return;
						});
					});
				}
				return;
			});
		}, $bu, $bw);
		return;
	};
	return [ [ 1 ], render, [ 0, __clone(gated) ], armed, wire ];
}
function $ck(self) {
	const $cl = self;
	return $cl[0] === 1;
}
function $cC(self, content, end, $cD, $cE) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(__clone(content), [ __clone(staging) ], $cD, $cE);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function $cz(self, content, $cA, $cB) {
	return $cC(self, __clone(content), self[0], $cA, $cB);
}
function $cF(owner, body) {
	return body(owner);
}
function $cd(parent, source, render, armed, $ce, $cf) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($cf), () => {
		const $cg = live_owner.v;
		let $ch = null;
		if ($cg[0] === 1) {
			$ch = $cg;
		} else {
			$ch = [ 0, dispose2($cg[1]) ];
		}
		$ch;
		close(region);
		return;
	});
	$bQ(__clone(source), (value) => {
		const $cq = last_value.v;
		let $cr = null;
		if ($cq[0] === 0) {
			const previous = $cq[1];
			$cr = eq(previous, value);
		} else {
			$cr = false;
		}
		const unchanged = $cr;
		if (armed.v && !(unchanged)) {
			const $cu = live_owner.v;
			let $cv = null;
			if ($cu[0] === 1) {
				$cv = $cu;
			} else {
				$cv = [ 0, dispose2($cu[1]) ];
			}
			$cv;
			const $cw = live_row.v;
			let $cx = null;
			if ($cw[0] === 0) {
				const row = $cw[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$cx = undefined;
			} else {
				$cx = undefined;
			}
			$cx;
			const owner = new3();
			const row2 = $cF(owner, ($cy) => {
				return $cz(region, render(value, $cy), $ce, $cy);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $ce, $cf);
}
function $bY(self, parent, $bZ, $ca) {
	self[4]($ca);
	const render = self[1];
	const armed = self[3];
	const $cb = self[0];
	let $cc = null;
	if ($cb[0] === 0) {
		const source = $cb[1];
		$cc = $cd(parent, __clone(source), render, armed, $bZ, $ca);
	} else {
		const $cG = self[2];
		let $cH = null;
		if ($cG[0] === 0) {
			const gated = $cG[1];
			$cH = $cd(parent, __clone(gated), render, armed, $bZ, $ca);
		} else {
			$cH = undefined;
		}
		$cc = $cH;
	}
	return $cc;
}
function $bX(self, content, $az, $aA) {
	$bY(__clone(content), self, $az, $aA);
	return __clone(self);
}
function $cJ(body) {
	const scope = new3();
	const result = body(scope);
	return [ result, scope ];
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
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
__vilan_chunks.fn.$ak = $ak;
__vilan_chunks.fn.$ay = $ay;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $y($x(current_path([ 1 ]), parse), [ 1 ], [ 1 ]);
mount_root("app", ($ae) => {
	return app(route, [ 1 ], $ae);
});
