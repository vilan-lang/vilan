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
function home_page($aY, $aZ) {
	return __vilan_chunks.fn.home_page($aY, $aZ);
}
function docs_page(page, $bc, $bd) {
	return __vilan_chunks.fn.docs_page(page, $bc, $bd);
}
function not_found_page($bg, $bh) {
	return __vilan_chunks.fn.not_found_page($bg, $bh);
}
function hash(self) {
	return __hash(self);
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		if (!(turn[1].v.has(key))) {
			turn[1].v.set(key, true);
			turn[0].v.push(__clone(subscriber));
		}
	}
	if (turn[3].v && !(turn[4].v) && !(turn[2].v)) {
		turn[4].v = true;
		queueMicrotask(() => {
			turn[4].v = false;
			drain(turn);
			return;
		});
	}
}
function drain(turn) {
	if (!(turn[2].v)) {
		turn[2].v = true;
		draining_turns.v.push(__clone(turn));
		__with_finally(() => {
			let budget = 100000;
			while (!($l(turn[0].v)) && budget > 0) {
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[1].v = new Map();
				for (const subscriber of wave) {
					if (subscriber[2].v) {
						subscriber[1]();
					}
					budget = budget - 1;
				}
			}
			return;
		}, () => {
			__list_pop(draining_turns.v);
			turn[2].v = false;
			return;
		});
	}
}
function dispose(self, $P) {
	self[2].v = false;
	const $Q = [ 0, self[0] ];
	let $R = null;
	if ($Q[0] === 0) {
		const subscribers = $Q[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$R = undefined;
	} else {
		$R = undefined;
	}
	$R;
	const $S = $P;
	let $T = null;
	if ($S[0] === 0) {
		const established = $S[1];
		$T = [ 0, established ];
	} else {
		$T = $m(draining_turns.v);
	}
	const ambient = $T;
	const $U = ambient;
	let $V = null;
	if ($U[0] === 0) {
		const turn = $U[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$V = undefined;
	} else {
		$V = undefined;
	}
	$V;
	const $W = self[3].v;
	let $X = null;
	if ($W[0] === 0) {
		const release = $W[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$X = undefined;
	} else {
		$X = undefined;
	}
	return $X;
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
	let $cl = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $cf = __guarded(cleanup);
			let $cg = null;
			if ($cf[0] === 0) {
				const message = $cf[1];
				if ($ch(failure)) {
					failure = [ 0, message ];
				}
				$cg = undefined;
			} else {
				$cg = undefined;
			}
			$cg;
		}
		self[0].v = [  ];
		const $cj = failure;
		let $ck = null;
		if ($cj[0] === 0) {
			const message2 = $cj[1];
			$ck = (() => {
				throw message2;
			})();
		} else {
			$ck = undefined;
		}
		$cl = $ck;
	}
	return $cl;
}
function get_owner($aL) {
	return $aL;
}
function register_with_owner(subscription, $J, $K) {
	const $L = $K;
	let $M = null;
	if ($L[0] === 0) {
		const owner = $L[1];
		$M = $N(owner, subscription, $J);
	} else {
		$M = __clone(subscription);
	}
	return $M;
}
function ensure_wired($e) {
	if (!(wired.v)) {
		wired.v = true;
		$f(path_signal, __router_path(), $e);
		__dom_window().addEventListener("popstate", () => {
			return $q([ 1 ], ($p) => {
				$f(path_signal, __router_path(), [ 0, $p ]);
				return;
			});
		});
	}
}
function current_path($d) {
	ensure_wired($d);
	return path_signal;
}
function navigate(path, $aq) {
	ensure_wired($aq);
	history.pushState("", "", path);
	$f(path_signal, path, $aq);
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
	let $ad = null;
	if (is_svg_tag(tag)) {
		$ad = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$ad = [ document.createElement(tag) ];
	}
	return $ad;
}
function is_svg_tag(tag) {
	const $ab = tag;
	let $ac = null;
	if ($ab === "svg") {
		$ac = true;
	} else if ($ab === "path") {
		$ac = true;
	} else if ($ab === "circle") {
		$ac = true;
	} else if ($ab === "ellipse") {
		$ac = true;
	} else if ($ab === "rect") {
		$ac = true;
	} else if ($ab === "line") {
		$ac = true;
	} else if ($ab === "polyline") {
		$ac = true;
	} else if ($ab === "polygon") {
		$ac = true;
	} else if ($ab === "g") {
		$ac = true;
	} else if ($ab === "defs") {
		$ac = true;
	} else if ($ab === "use") {
		$ac = true;
	} else if ($ab === "symbol") {
		$ac = true;
	} else if ($ab === "marker") {
		$ac = true;
	} else if ($ab === "pattern") {
		$ac = true;
	} else if ($ab === "mask") {
		$ac = true;
	} else if ($ab === "clipPath") {
		$ac = true;
	} else if ($ab === "linearGradient") {
		$ac = true;
	} else if ($ab === "radialGradient") {
		$ac = true;
	} else if ($ab === "stop") {
		$ac = true;
	} else if ($ab === "text") {
		$ac = true;
	} else if ($ab === "tspan") {
		$ac = true;
	} else if ($ab === "textPath") {
		$ac = true;
	} else if ($ab === "filter") {
		$ac = true;
	} else if ($ab === "foreignObject") {
		$ac = true;
	} else if ($ab === "feGaussianBlur") {
		$ac = true;
	} else if ($ab === "feColorMatrix") {
		$ac = true;
	} else if ($ab === "feOffset") {
		$ac = true;
	} else if ($ab === "feMerge") {
		$ac = true;
	} else if ($ab === "feMergeNode") {
		$ac = true;
	} else if ($ab === "feFlood") {
		$ac = true;
	} else if ($ab === "feComposite") {
		$ac = true;
	} else if ($ab === "feBlend") {
		$ac = true;
	} else if ($ab === "feDropShadow") {
		$ac = true;
	} else {
		$ac = false;
	}
	return $ac;
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
		return $q([ 1 ], ($ar) => {
			return handler(dispatched, $ar);
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
function set_chunk_pending(busy, $bF) {
	if ($x(chunk_pending_signal) !== busy) {
		$bG(chunk_pending_signal, busy, $bF);
	}
}
function clear_chunk_error($bw) {
	const $bx = $x(chunk_error_signal);
	let $by = null;
	if ($bx[0] === 0) {
		const _reason = $bx[1];
		$by = $bz(chunk_error_signal, [ 1 ], $bw);
	} else {
		$by = undefined;
	}
	return $by;
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
		let $cm = null;
		if (at + 1 < rows.length) {
			$cm = __at(rows, at + 1)[0];
		} else {
			$cm = self[0];
		}
		const end = $cm;
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
	const $cG = $q([ 1 ], ($cD) => {
		return $cE(body);
	});
	const built = $cG[0];
	const root = $cG[1];
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
	let $t = null;
	if (__at(parts, 0) === "docs" && parts.length === 2) {
		const $r = __parse_i32(__at(parts, 1));
		let $s = null;
		if ($r[0] === 0) {
			const page = $r[1];
			return [ 1, page ];
		} else {
			$s = undefined;
		}
		$t = $s;
	}
	$t;
	return [ 2 ];
}
function href(route2) {
	const $ak = route2;
	let $al = null;
	if ($ak[0] === 0) {
		$al = "/";
	} else if ($ak[0] === 1) {
		const page = $ak[1];
		$al = "/docs/" + page;
	} else {
		$al = "/404";
	}
	return $al;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $ba, $bb) {
	return $as($as(view("section"), text(view("h2"), title), $ba, $bb), text(view("p"), body), $ba, $bb);
}
function app(route2, $Z, $aa) {
	$bi(route2);
	return $bU($as($as($as(view("main"), $as($as(view("nav"), $ae("Home", [ 0 ], $Z, $aa), $Z, $aa), $ae("Docs", [ 1, 1 ], $Z, $aa), $Z, $aa), $Z, $aa), $aC(class2(view("p"), "pending"), $aw(pending(), (busy) => {
		let $av = null;
		if (busy) {
			$av = "...";
		} else {
			$av = "";
		}
		return $av;
	}, $Z, [ 0, $aa ]), $Z, $aa), $Z, $aa), $aC(class2(view("p"), "failed"), $aP(chunk_error(), (reason) => {
		const $aM = reason;
		let $aN = null;
		if ($aM[0] === 0) {
			const text2 = $aM[1];
			let $aO = null;
			if (text2.length > 0) {
				$aO = "!";
			} else {
				$aO = "?";
			}
			$aN = $aO;
		} else {
			$aN = "";
		}
		return $aN;
	}, $Z, [ 0, $aa ]), $Z, $aa), $Z, $aa), $bk(route2, (current, $aV) => {
		const $aW = current;
		let $aX = null;
		if ($aW[0] === 0) {
			$aX = home_page($Z, $aV);
		} else if ($aW[0] === 1) {
			const page = $aW[1];
			$aX = docs_page(page, $Z, $aV);
		} else {
			$aX = not_found_page($Z, $aV);
		}
		return $aX;
	}), $Z, $aa);
}
function eq(self, other) {
	const $cp = [ self, other ];
	let $cq = null;
	if ($cp[0][0] === 0 && $cp[1][0] === 0) {
		$cq = true;
	} else if ($cp[0][0] === 1 && $cp[1][0] === 1) {
		const s0 = $cp[0][1];
		const o0 = $cp[1][1];
		$cq = s0 === o0;
	} else if ($cp[0][0] === 2 && $cp[1][0] === 2) {
		$cq = true;
	} else {
		$cq = false;
	}
	return $cq;
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $l(self) {
	return self.length === 0;
}
function $m(self) {
	return __list_get(self, self.length - 1);
}
function $h(self, $i) {
	const $j = $i;
	let $k = null;
	if ($j[0] === 0) {
		const turn = $j[1];
		$k = enqueue(turn, self[1].v);
	} else {
		const $n = $m(draining_turns.v);
		let $o = null;
		if ($n[0] === 0) {
			const draining = $n[1];
			$o = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$o = undefined;
		}
		$k = $o;
	}
	return $k;
}
function $f(self, value, $g) {
	self[0].v = __clone(value);
	$h(self, $g);
}
function $q(policy, body) {
	const fresh = new2();
	const result = body(fresh);
	drain(fresh);
	fresh[3].v = true;
	return result;
}
function $x(self) {
	return __clone(self[0].v);
}
function $A(self, $i) {
	const $B = $i;
	let $C = null;
	if ($B[0] === 0) {
		const turn = $B[1];
		$C = enqueue(turn, self[1].v);
	} else {
		const $D = $m(draining_turns.v);
		let $E = null;
		if ($D[0] === 0) {
			const draining = $D[1];
			$E = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$E = undefined;
		}
		$C = $E;
	}
	return $C;
}
function $z(self, value, $g) {
	self[0].v = __clone(value);
	$A(self, $g);
}
function $G(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	signal[1].v.push([ id, () => {
		const $H = [ 0, cell ];
		let $I = null;
		if ($H[0] === 0) {
			const live2 = $H[1];
			$I = observer(live2.v);
		} else {
			$I = undefined;
		}
		return $I;
	}, live ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $F(self, observer) {
	return $G(self, observer);
}
function $N(self, item, $O) {
	if (self[1].v) {
		dispose(item, $O);
	} else {
		self[0].v.push(() => {
			dispose(item, $O);
			return;
		});
	}
	return __clone(item);
}
function $u(self, transform, $v, $w) {
	const derived = $a(transform($x(self)));
	register_with_owner($F(self, (value) => {
		$z(derived, transform(value), $v);
		return;
	}), $v, $w);
	return derived;
}
function $am(self, name, value, $an, $ao) {
	apply(value, self, name, $an, $ao);
	return __clone(self);
}
function $ah(self, route2, $ai, $aj) {
	const path = to_path(route2);
	return on_event($am($am(self, "href", path, $ai, $aj), "draggable", "false", $ai, $aj), "click", (event, $ap) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $ap ]);
		}
		return;
	});
}
function $ae(label, route2, $af, $ag) {
	return text($ah(view("a"), route2, $af, $ag), label);
}
function $as(self, content, $at, $au) {
	place(content, self, $at, $au);
	return __clone(self);
}
function $az(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	signal[1].v.push([ id, () => {
		const $aA = [ 0, cell ];
		let $aB = null;
		if ($aA[0] === 0) {
			const live2 = $aA[1];
			$aB = observer(live2.v);
		} else {
			$aB = undefined;
		}
		return $aB;
	}, live ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $ay(self, observer) {
	return $az(self, observer);
}
function $aw(self, transform, $v, $w) {
	const derived = $a(transform($x(self)));
	register_with_owner($ay(self, (value) => {
		$z(derived, transform(value), $v);
		return;
	}), $v, $w);
	return derived;
}
function $aI(self, observer, $aJ, $aK) {
	$N(get_owner($aK), $F(self, observer), $aJ);
}
function $aF(self, observer, $aG, $aH) {
	$aI(self, observer, $aG, $aH);
	observer($x(self));
}
function $aC(self, source, $aD, $aE) {
	const element = __clone(self[0]);
	$aF(source, (value) => {
		element.textContent = value;
		return;
	}, $aD, $aE);
	return __clone(self);
}
function $aS(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	signal[1].v.push([ id, () => {
		const $aT = [ 0, cell ];
		let $aU = null;
		if ($aT[0] === 0) {
			const live2 = $aT[1];
			$aU = observer(live2.v);
		} else {
			$aU = undefined;
		}
		return $aU;
	}, live ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $aR(self, observer) {
	return $aS(self, observer);
}
function $aP(self, transform, $v, $w) {
	const derived = $a(transform($x(self)));
	register_with_owner($aR(self, (value) => {
		$z(derived, transform(value), $v);
		return;
	}), $v, $w);
	return derived;
}
function $bi(source) {
	__chunk_preload(__chunk_arm($x(source)));
}
function $bp(self, $i) {
	const $bq = $i;
	let $br = null;
	if ($bq[0] === 0) {
		const turn = $bq[1];
		$br = enqueue(turn, self[1].v);
	} else {
		const $bs = $m(draining_turns.v);
		let $bt = null;
		if ($bs[0] === 0) {
			const draining = $bs[1];
			$bt = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bt = undefined;
		}
		$br = $bt;
	}
	return $br;
}
function $bo(self, value, $g) {
	self[0].v = __clone(value);
	$bp(self, $g);
}
function $bA(self, $i) {
	const $bB = $i;
	let $bC = null;
	if ($bB[0] === 0) {
		const turn = $bB[1];
		$bC = enqueue(turn, self[1].v);
	} else {
		const $bD = $m(draining_turns.v);
		let $bE = null;
		if ($bD[0] === 0) {
			const draining = $bD[1];
			$bE = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bE = undefined;
		}
		$bC = $bE;
	}
	return $bC;
}
function $bz(self, value, $g) {
	self[0].v = __clone(value);
	$bA(self, $g);
}
function $bH(self, $i) {
	const $bI = $i;
	let $bJ = null;
	if ($bI[0] === 0) {
		const turn = $bI[1];
		$bJ = enqueue(turn, self[1].v);
	} else {
		const $bK = $m(draining_turns.v);
		let $bL = null;
		if ($bK[0] === 0) {
			const draining = $bK[1];
			$bL = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bL = undefined;
		}
		$bJ = $bL;
	}
	return $bJ;
}
function $bG(self, value, $g) {
	self[0].v = __clone(value);
	$bH(self, $g);
}
function $bR(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	signal[1].v.push([ id, () => {
		const $bS = [ 0, cell ];
		let $bT = null;
		if ($bS[0] === 0) {
			const live2 = $bS[1];
			$bT = observer(live2.v);
		} else {
			$bT = undefined;
		}
		return $bT;
	}, live ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $bQ(self, observer) {
	return $bR(self, observer);
}
function $bP(self, observer, $aJ, $aK) {
	$N(get_owner($aK), $bQ(self, observer), $aJ);
}
function $bO(self, observer, $aG, $aH) {
	$bP(self, observer, $aG, $aH);
	observer($x(self));
}
function $bk(source, render, $bl) {
	const gated = $a($x(source));
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value, $bn) => {
		armed.v = true;
		$bo(gated, value, [ 0, $bn ]);
		return;
	};
	const wire = ($bu) => {
		$bO(source, (value) => {
			return $q([ 1 ], ($bv) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $bv ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $bv ]);
					advance(value, $bv);
				} else {
					set_chunk_pending(true, [ 0, $bv ]);
					__chunk_load(arm, () => {
						return $q([ 1 ], ($bM) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bM ]);
								advance(value, $bM);
							}
							return;
						});
					}, (reason) => {
						return $q([ 1 ], ($bN) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bN ]);
								$bz(chunk_error_signal, [ 0, reason ], [ 0, $bN ]);
							}
							return;
						});
					});
				}
				return;
			});
		}, $bl, $bu);
		return;
	};
	return [ __clone(source), render, [ 0, __clone(gated) ], armed, wire ];
}
function $ch(self) {
	const $ci = self;
	return $ci[0] === 1;
}
function $cz(self, content, end, $cA, $cB) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(content, [ __clone(staging) ], $cA, $cB);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function $cw(self, content, $cx, $cy) {
	return $cz(self, content, self[0], $cx, $cy);
}
function $cC(owner, body) {
	return body(owner);
}
function $ca(parent, source, render, armed, $cb, $cc) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($cc), () => {
		const $cd = live_owner.v;
		let $ce = null;
		if ($cd[0] === 1) {
			$ce = $cd;
		} else {
			$ce = [ 0, dispose2($cd[1]) ];
		}
		$ce;
		close(region);
		return;
	});
	$bO(source, (value) => {
		const $cn = last_value.v;
		let $co = null;
		if ($cn[0] === 0) {
			const previous = $cn[1];
			$co = eq(previous, value);
		} else {
			$co = false;
		}
		const unchanged = $co;
		if (armed.v && !(unchanged)) {
			const $cr = live_owner.v;
			let $cs = null;
			if ($cr[0] === 1) {
				$cs = $cr;
			} else {
				$cs = [ 0, dispose2($cr[1]) ];
			}
			$cs;
			const $ct = live_row.v;
			let $cu = null;
			if ($ct[0] === 0) {
				const row = $ct[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$cu = undefined;
			} else {
				$cu = undefined;
			}
			$cu;
			const owner = new3();
			const row2 = $cC(owner, ($cv) => {
				return $cw(region, render(value, $cv), $cb, $cv);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $cb, $cc);
}
function $bV(self, parent, $bW, $bX) {
	self[4]($bX);
	const $bY = self[2];
	let $bZ = null;
	if ($bY[0] === 0) {
		const gated = $bY[1];
		$bZ = $ca(parent, gated, self[1], self[3], $bW, $bX);
	} else {
		$bZ = $ca(parent, self[0], self[1], self[3], $bW, $bX);
	}
	return $bZ;
}
function $bU(self, content, $at, $au) {
	$bV(content, self, $at, $au);
	return __clone(self);
}
function $cE(body) {
	const scope = new3();
	const result = body(scope);
	return [ result, scope ];
}
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
__vilan_chunks.fn.$ae = $ae;
__vilan_chunks.fn.$as = $as;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $u(current_path([ 1 ]), parse, [ 1 ], [ 1 ]);
mount_root("app", ($Y) => {
	return app(route, [ 1 ], $Y);
});
