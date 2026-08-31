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
const __vilan_chunks = __chunk_registry();
function home_page($aG, $aH) {
	return __vilan_chunks.fn.home_page($aG, $aH);
}
function docs_page(page, $aK, $aL) {
	return __vilan_chunks.fn.docs_page(page, $aK, $aL);
}
function not_found_page($aO, $aP) {
	return __vilan_chunks.fn.not_found_page($aO, $aP);
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		let seen = false;
		for (const queued of turn[0].v) {
			if (queued[0] === subscriber[0]) {
				seen = true;
			}
		}
		if (!(seen)) {
			turn[0].v.push(__clone(subscriber));
		}
	}
	if (turn[2].v && !(turn[3].v) && !(turn[1].v)) {
		turn[3].v = true;
		queueMicrotask(() => {
			turn[3].v = false;
			drain(turn);
			return;
		});
	}
}
function drain(turn) {
	if (!(turn[1].v)) {
		turn[1].v = true;
		draining_turns.v.push(__clone(turn));
		let budget = 100000;
		while (!($l(turn[0].v)) && budget > 0) {
			const wave = turn[0].v;
			turn[0].v = [  ];
			for (const subscriber of wave) {
				subscriber[1]();
				budget = budget - 1;
			}
		}
		__list_pop(draining_turns.v);
		turn[1].v = false;
	}
}
function dispose(self, $M) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $N = $M;
	let $O = null;
	if ($N[0] === 0) {
		const turn = $N[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		$O = undefined;
	} else {
		$O = undefined;
	}
	$O;
	const $P = self[2].v;
	let $Q = null;
	if ($P[0] === 0) {
		const release = $P[1];
		self[2].v = [ 1 ];
		release();
		$Q = undefined;
	} else {
		$Q = undefined;
	}
	return $Q;
}
function new3() {
	return [ __shared_new([  ]) ];
}
function defer(self, cleanup) {
	self[0].v.push(cleanup);
}
function dispose2(self) {
	for (const cleanup of self[0].v) {
		cleanup();
	}
	self[0].v = [  ];
}
function get_owner($av) {
	return $av;
}
function register_with_owner(subscription, $G, $H) {
	const $I = $H;
	let $J = null;
	if ($I[0] === 0) {
		const owner = $I[1];
		$J = $K(owner, subscription, $G);
	} else {
		$J = __clone(subscription);
	}
	return $J;
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
function navigate(path, $ag) {
	ensure_wired($ag);
	history.pushState("", "", path);
	$f(path_signal, path, $ag);
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
	let $W = null;
	if (is_svg_tag(tag)) {
		$W = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$W = [ document.createElement(tag) ];
	}
	return $W;
}
function is_svg_tag(tag) {
	const $U = tag;
	let $V = null;
	if ($U === "svg") {
		$V = true;
	} else if ($U === "path") {
		$V = true;
	} else if ($U === "circle") {
		$V = true;
	} else if ($U === "ellipse") {
		$V = true;
	} else if ($U === "rect") {
		$V = true;
	} else if ($U === "line") {
		$V = true;
	} else if ($U === "polyline") {
		$V = true;
	} else if ($U === "polygon") {
		$V = true;
	} else if ($U === "g") {
		$V = true;
	} else if ($U === "defs") {
		$V = true;
	} else if ($U === "use") {
		$V = true;
	} else if ($U === "symbol") {
		$V = true;
	} else if ($U === "marker") {
		$V = true;
	} else if ($U === "pattern") {
		$V = true;
	} else if ($U === "mask") {
		$V = true;
	} else if ($U === "clipPath") {
		$V = true;
	} else if ($U === "linearGradient") {
		$V = true;
	} else if ($U === "radialGradient") {
		$V = true;
	} else if ($U === "stop") {
		$V = true;
	} else if ($U === "text") {
		$V = true;
	} else if ($U === "tspan") {
		$V = true;
	} else if ($U === "textPath") {
		$V = true;
	} else if ($U === "filter") {
		$V = true;
	} else if ($U === "foreignObject") {
		$V = true;
	} else if ($U === "feGaussianBlur") {
		$V = true;
	} else if ($U === "feColorMatrix") {
		$V = true;
	} else if ($U === "feOffset") {
		$V = true;
	} else if ($U === "feMerge") {
		$V = true;
	} else if ($U === "feMergeNode") {
		$V = true;
	} else if ($U === "feFlood") {
		$V = true;
	} else if ($U === "feComposite") {
		$V = true;
	} else if ($U === "feBlend") {
		$V = true;
	} else if ($U === "feDropShadow") {
		$V = true;
	} else {
		$V = false;
	}
	return $V;
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
		return $q([ 1 ], ($ah) => {
			return handler(dispatched, $ah);
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
function set_chunk_pending(busy, $bx) {
	if ($an(chunk_pending_signal) !== busy) {
		$by(chunk_pending_signal, busy, $bx);
	}
}
function clear_chunk_error($bo) {
	const $bp = $aB(chunk_error_signal);
	let $bq = null;
	if ($bp[0] === 0) {
		const _reason = $bp[1];
		$bq = $br(chunk_error_signal, [ 1 ], $bo);
	} else {
		$bq = undefined;
	}
	return $bq;
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
	const $bT = $bS([ 1 ], ($bQ) => {
		return $bR(body);
	});
	const built = $bT[0];
	const root = $bT[1];
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
	const $aa = route2;
	let $ab = null;
	if ($aa[0] === 0) {
		$ab = "/";
	} else if ($aa[0] === 1) {
		const page = $aa[1];
		$ab = "/docs/" + page;
	} else {
		$ab = "/404";
	}
	return $ab;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $aI, $aJ) {
	return $ai($ai(view("section"), text(view("h2"), title), $aI, $aJ), text(view("p"), body), $aI, $aJ);
}
function app(route2, $S, $T) {
	$aQ(route2);
	return $aS($ai($ai($ai(view("main"), $ai($ai(view("nav"), $X("Home", [ 0 ], $S, $T), $S, $T), $X("Docs", [ 1, 1 ], $S, $T), $S, $T), $S, $T), $ap(class2(view("p"), "pending"), $am(pending(), (busy) => {
		let $al = null;
		if (busy) {
			$al = "...";
		} else {
			$al = "";
		}
		return $al;
	}, $S, [ 0, $T ]), $S, $T), $S, $T), $ap(class2(view("p"), "failed"), $aA(chunk_error(), (reason) => {
		const $ax = reason;
		let $ay = null;
		if ($ax[0] === 0) {
			const text2 = $ax[1];
			let $az = null;
			if (text2.length > 0) {
				$az = "!";
			} else {
				$az = "?";
			}
			$ay = $az;
		} else {
			$ay = "";
		}
		return $ay;
	}, $S, [ 0, $T ]), $S, $T), $S, $T), route2, (current, $aD) => {
		const $aE = current;
		let $aF = null;
		if ($aE[0] === 0) {
			$aF = home_page($S, $aD);
		} else if ($aE[0] === 1) {
			const page = $aE[1];
			$aF = docs_page(page, $S, $aD);
		} else {
			$aF = not_found_page($S, $aD);
		}
		return $aF;
	}, $S, $T);
}
function eq(self, other) {
	const $bc = [ self, other ];
	let $bd = null;
	if ($bc[0][0] === 0 && $bc[1][0] === 0) {
		$bd = true;
	} else if ($bc[0][0] === 1 && $bc[1][0] === 1) {
		const s0 = $bc[0][1];
		const o0 = $bc[1][1];
		$bd = s0 === o0;
	} else if ($bc[0][0] === 2 && $bc[1][0] === 2) {
		$bd = true;
	} else {
		$bd = false;
	}
	return $bd;
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $c(value) {
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
				subscriber[1]();
			}
			$o = undefined;
		}
		$k = $o;
	}
	return $k;
}
function $f(self, value, $g) {
	self[0].v = value;
	$h(self, $g);
}
function $q(policy, body) {
	const fresh = new2();
	const result = body(fresh);
	drain(fresh);
	fresh[2].v = true;
	return result;
}
function $x(self) {
	return self[0].v;
}
function $y(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
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
				subscriber[1]();
			}
			$E = undefined;
		}
		$C = $E;
	}
	return $C;
}
function $z(self, value, $g) {
	self[0].v = value;
	$A(self, $g);
}
function $F(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $K(self, item, $L) {
	self[0].v.push(() => {
		dispose(item, $L);
		return;
	});
	return __clone(item);
}
function $u(self, transform, $v, $w) {
	const derived = $y(transform($x(self)));
	register_with_owner($F(self, (value) => {
		$z(derived, transform(value), $v);
		return;
	}), $v, $w);
	return derived;
}
function $ac(self, name, value, $ad, $ae) {
	apply(value, self, name, $ad, $ae);
	return __clone(self);
}
function $X(label, route2, $Y, $Z) {
	const path = to_path(route2);
	return on_event(text($ac(view("a"), "href", path, $Y, $Z), label), "click", (event, $af) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $af ]);
		}
		return;
	});
}
function $ai(self, content, $aj, $ak) {
	place(content, self, $aj, $ak);
	return __clone(self);
}
function $an(self) {
	return self[0].v;
}
function $ao(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $am(self, transform, $v, $w) {
	const derived = $a(transform($an(self)));
	register_with_owner($ao(self, (value) => {
		$f(derived, transform(value), $v);
		return;
	}), $v, $w);
	return derived;
}
function $aw(self, observer) {
	const subscription = $F(self, observer);
	observer($x(self));
	return subscription;
}
function $as(self, observer, $at, $au) {
	$K(get_owner($au), $aw(self, observer), $at);
}
function $ap(self, source, $aq, $ar) {
	const element = __clone(self[0]);
	$as(source, (value) => {
		element.textContent = value;
		return;
	}, $aq, $ar);
	return __clone(self);
}
function $aB(self) {
	return self[0].v;
}
function $aC(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $aA(self, transform, $v, $w) {
	const derived = $a(transform($aB(self)));
	register_with_owner($aC(self, (value) => {
		$f(derived, transform(value), $v);
		return;
	}), $v, $w);
	return derived;
}
function $aR(self) {
	return self[0].v;
}
function $aQ(source) {
	__chunk_preload(__chunk_arm($aR(source)));
}
function $bj(owner, body) {
	return body(owner);
}
function $bm(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $bn(self) {
	return self[0].v;
}
function $bl(self, observer) {
	const subscription = $bm(self, observer);
	observer($bn(self));
	return subscription;
}
function $bk(self, observer, $at, $au) {
	$K(get_owner($au), $bl(self, observer), $at);
}
function $aV(self, source, render, $aW, $aX) {
	const element = __clone(self[0]);
	const last_value = __shared_new([ 1 ]);
	const live_view = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($aX), () => {
		const $aY = live_owner.v;
		let $aZ = null;
		if ($aY[0] === 1) {
			$aZ = $aY;
		} else {
			$aZ = [ 0, dispose2($aY[1]) ];
		}
		$aZ;
		return;
	});
	$bk(source, (value) => {
		const $ba = last_value.v;
		let $bb = null;
		if ($ba[0] === 0) {
			const previous = $ba[1];
			$bb = eq(previous, value);
		} else {
			$bb = false;
		}
		const unchanged = $bb;
		if (!(unchanged)) {
			const $be = live_owner.v;
			let $bf = null;
			if ($be[0] === 1) {
				$bf = $be;
			} else {
				$bf = [ 0, dispose2($be[1]) ];
			}
			$bf;
			const $bg = live_view.v;
			let $bh = null;
			if ($bg[0] === 0) {
				const built = $bg[1];
				$bh = built[0].remove();
			} else {
				$bh = undefined;
			}
			$bh;
			const owner = new3();
			const built2 = $bj(owner, ($bi) => {
				return render(value, $bi);
			});
			element.appendChild(built2[0]);
			last_value.v = [ 0, __clone(value) ];
			live_view.v = [ 0, built2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $aW, $aX);
	return __clone(self);
}
function $bs(self, $i) {
	const $bt = $i;
	let $bu = null;
	if ($bt[0] === 0) {
		const turn = $bt[1];
		$bu = enqueue(turn, self[1].v);
	} else {
		const $bv = $m(draining_turns.v);
		let $bw = null;
		if ($bv[0] === 0) {
			const draining = $bv[1];
			$bw = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bw = undefined;
		}
		$bu = $bw;
	}
	return $bu;
}
function $br(self, value, $g) {
	self[0].v = value;
	$bs(self, $g);
}
function $bz(self, $i) {
	const $bA = $i;
	let $bB = null;
	if ($bA[0] === 0) {
		const turn = $bA[1];
		$bB = enqueue(turn, self[1].v);
	} else {
		const $bC = $m(draining_turns.v);
		let $bD = null;
		if ($bC[0] === 0) {
			const draining = $bC[1];
			$bD = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bD = undefined;
		}
		$bB = $bD;
	}
	return $bB;
}
function $by(self, value, $g) {
	self[0].v = value;
	$bz(self, $g);
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
				subscriber[1]();
			}
			$bL = undefined;
		}
		$bJ = $bL;
	}
	return $bJ;
}
function $bG(self, value, $g) {
	self[0].v = value;
	$bH(self, $g);
}
function $bO(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $bP(self) {
	return self[0].v;
}
function $bN(self, observer) {
	const subscription = $bO(self, observer);
	observer($bP(self));
	return subscription;
}
function $bM(self, observer, $at, $au) {
	$K(get_owner($au), $bN(self, observer), $at);
}
function $aS(self, source, render, $aT, $aU) {
	const gated = $y($aR(source));
	const wired2 = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value) => {
		$z(gated, value, $aT);
		if (!(wired2.v)) {
			wired2.v = true;
			$aV(self, gated, render, $aT, $aU);
		}
		return;
	};
	$bM(source, (value) => {
		const mine = generation.v + 1;
		generation.v = mine;
		clear_chunk_error($aT);
		const arm = __chunk_arm(value);
		if (__chunk_ready(arm)) {
			set_chunk_pending(false, $aT);
			advance(value);
		} else {
			set_chunk_pending(true, $aT);
			__chunk_load(arm, () => {
				return $q([ 1 ], ($bE) => {
					if (generation.v === mine) {
						set_chunk_pending(false, [ 0, $bE ]);
						advance(value);
					}
					return;
				});
			}, (reason) => {
				return $q([ 1 ], ($bF) => {
					if (generation.v === mine) {
						set_chunk_pending(false, [ 0, $bF ]);
						$bG(chunk_error_signal, [ 0, reason ], [ 0, $bF ]);
					}
					return;
				});
			});
		}
		return;
	}, $aT, $aU);
	return __clone(self);
}
function $bR(body) {
	const scope = new3();
	const result = body(scope);
	return [ result, scope ];
}
function $bS(policy, body) {
	const fresh = new2();
	const result = body(fresh);
	drain(fresh);
	fresh[2].v = true;
	return result;
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const path_signal = $a("");
const wired = __shared_new(false);
const chunk_pending_signal = $b(false);
const chunk_error_signal = $c([ 1 ]);
const BASE = announce("BASE", 2);
const SCALED = announce("SCALED", BASE * 3);
const LABEL = "scale " + SCALED;
__vilan_chunks.url[0] = "app.Route_Home.js";
__vilan_chunks.url[1] = "app.Route_Docs.js";
__vilan_chunks.url[2] = "app.Route_NotFound.js";
__vilan_chunks.fn.$X = $X;
__vilan_chunks.fn.$ai = $ai;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $u(current_path([ 1 ]), parse, [ 1 ], [ 1 ]);
mount_root("app", ($R) => {
	return app(route, [ 1 ], $R);
});
