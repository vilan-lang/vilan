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
function home_page($aF, $aG) {
	return __vilan_chunks.fn.home_page($aF, $aG);
}
function docs_page(page, $aJ, $aK) {
	return __vilan_chunks.fn.docs_page(page, $aJ, $aK);
}
function not_found_page($aN, $aO) {
	return __vilan_chunks.fn.not_found_page($aN, $aO);
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
function get_owner($au) {
	return $au;
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
		window.addEventListener("popstate", () => {
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
function bind_text(self, source, $ap, $aq) {
	const element = __clone(self[0]);
	$ar(source, (value) => {
		element.textContent = value;
		return;
	}, $ap, $aq);
	return __clone(self);
}
function chunk_pending() {
	return chunk_pending_signal;
}
function chunk_failure() {
	return chunk_error_signal;
}
function set_chunk_pending(busy, $bw) {
	if ($an(chunk_pending_signal) !== busy) {
		$bx(chunk_pending_signal, busy, $bw);
	}
}
function clear_chunk_error($bn) {
	const $bo = $aA(chunk_error_signal);
	let $bp = null;
	if ($bo[0] === 0) {
		const _reason = $bo[1];
		$bp = $bq(chunk_error_signal, [ 1 ], $bn);
	} else {
		$bp = undefined;
	}
	return $bp;
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
	const $bS = $bR([ 1 ], ($bP) => {
		return $bQ(body);
	});
	const built = $bS[0];
	const root = $bS[1];
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
function panel(title, body, $aH, $aI) {
	return $ai($ai(view("section"), text(view("h2"), title), $aH, $aI), text(view("p"), body), $aH, $aI);
}
function app(route2, $S, $T) {
	$aP(route2);
	return $aR($ai($ai($ai(view("main"), $ai($ai(view("nav"), $X("Home", [ 0 ], $S, $T), $S, $T), $X("Docs", [ 1, 1 ], $S, $T), $S, $T), $S, $T), bind_text(class2(view("p"), "pending"), $am(pending(), (busy) => {
		let $al = null;
		if (busy) {
			$al = "...";
		} else {
			$al = "";
		}
		return $al;
	}, $S, [ 0, $T ]), $S, $T), $S, $T), bind_text(class2(view("p"), "failed"), $az(chunk_error(), (reason) => {
		const $aw = reason;
		let $ax = null;
		if ($aw[0] === 0) {
			const text2 = $aw[1];
			let $ay = null;
			if (text2.length > 0) {
				$ay = "!";
			} else {
				$ay = "?";
			}
			$ax = $ay;
		} else {
			$ax = "";
		}
		return $ax;
	}, $S, [ 0, $T ]), $S, $T), $S, $T), route2, (current, $aC) => {
		const $aD = current;
		let $aE = null;
		if ($aD[0] === 0) {
			$aE = home_page($S, $aC);
		} else if ($aD[0] === 1) {
			const page = $aD[1];
			$aE = docs_page(page, $S, $aC);
		} else {
			$aE = not_found_page($S, $aC);
		}
		return $aE;
	}, $S, $T);
}
function eq(self, other) {
	const $bb = [ self, other ];
	let $bc = null;
	if ($bb[0][0] === 0 && $bb[1][0] === 0) {
		$bc = true;
	} else if ($bb[0][0] === 1 && $bb[1][0] === 1) {
		const s0 = $bb[0][1];
		const o0 = $bb[1][1];
		$bc = s0 === o0;
	} else if ($bb[0][0] === 2 && $bb[1][0] === 2) {
		$bc = true;
	} else {
		$bc = false;
	}
	return $bc;
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
function $av(self, observer) {
	const subscription = $F(self, observer);
	observer($x(self));
	return subscription;
}
function $ar(self, observer, $as, $at) {
	$K(get_owner($at), $av(self, observer), $as);
}
function $aA(self) {
	return self[0].v;
}
function $aB(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $az(self, transform, $v, $w) {
	const derived = $a(transform($aA(self)));
	register_with_owner($aB(self, (value) => {
		$f(derived, transform(value), $v);
		return;
	}), $v, $w);
	return derived;
}
function $aQ(self) {
	return self[0].v;
}
function $aP(source) {
	__chunk_preload(__chunk_arm($aQ(source)));
}
function $bi(owner, body) {
	return body(owner);
}
function $bl(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $bm(self) {
	return self[0].v;
}
function $bk(self, observer) {
	const subscription = $bl(self, observer);
	observer($bm(self));
	return subscription;
}
function $bj(self, observer, $as, $at) {
	$K(get_owner($at), $bk(self, observer), $as);
}
function $aU(self, source, render, $aV, $aW) {
	const element = __clone(self[0]);
	const last_value = __shared_new([ 1 ]);
	const live_view = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($aW), () => {
		const $aX = live_owner.v;
		let $aY = null;
		if ($aX[0] === 1) {
			$aY = $aX;
		} else {
			$aY = [ 0, dispose2($aX[1]) ];
		}
		$aY;
		return;
	});
	$bj(source, (value) => {
		const $aZ = last_value.v;
		let $ba = null;
		if ($aZ[0] === 0) {
			const previous = $aZ[1];
			$ba = eq(previous, value);
		} else {
			$ba = false;
		}
		const unchanged = $ba;
		if (!(unchanged)) {
			const $bd = live_owner.v;
			let $be = null;
			if ($bd[0] === 1) {
				$be = $bd;
			} else {
				$be = [ 0, dispose2($bd[1]) ];
			}
			$be;
			const $bf = live_view.v;
			let $bg = null;
			if ($bf[0] === 0) {
				const built = $bf[1];
				$bg = built[0].remove();
			} else {
				$bg = undefined;
			}
			$bg;
			const owner = new3();
			const built2 = $bi(owner, ($bh) => {
				return render(value, $bh);
			});
			element.appendChild(built2[0]);
			last_value.v = [ 0, __clone(value) ];
			live_view.v = [ 0, built2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $aV, $aW);
	return __clone(self);
}
function $br(self, $i) {
	const $bs = $i;
	let $bt = null;
	if ($bs[0] === 0) {
		const turn = $bs[1];
		$bt = enqueue(turn, self[1].v);
	} else {
		const $bu = $m(draining_turns.v);
		let $bv = null;
		if ($bu[0] === 0) {
			const draining = $bu[1];
			$bv = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bv = undefined;
		}
		$bt = $bv;
	}
	return $bt;
}
function $bq(self, value, $g) {
	self[0].v = value;
	$br(self, $g);
}
function $by(self, $i) {
	const $bz = $i;
	let $bA = null;
	if ($bz[0] === 0) {
		const turn = $bz[1];
		$bA = enqueue(turn, self[1].v);
	} else {
		const $bB = $m(draining_turns.v);
		let $bC = null;
		if ($bB[0] === 0) {
			const draining = $bB[1];
			$bC = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bC = undefined;
		}
		$bA = $bC;
	}
	return $bA;
}
function $bx(self, value, $g) {
	self[0].v = value;
	$by(self, $g);
}
function $bG(self, $i) {
	const $bH = $i;
	let $bI = null;
	if ($bH[0] === 0) {
		const turn = $bH[1];
		$bI = enqueue(turn, self[1].v);
	} else {
		const $bJ = $m(draining_turns.v);
		let $bK = null;
		if ($bJ[0] === 0) {
			const draining = $bJ[1];
			$bK = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bK = undefined;
		}
		$bI = $bK;
	}
	return $bI;
}
function $bF(self, value, $g) {
	self[0].v = value;
	$bG(self, $g);
}
function $bN(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $bO(self) {
	return self[0].v;
}
function $bM(self, observer) {
	const subscription = $bN(self, observer);
	observer($bO(self));
	return subscription;
}
function $bL(self, observer, $as, $at) {
	$K(get_owner($at), $bM(self, observer), $as);
}
function $aR(self, source, render, $aS, $aT) {
	const gated = $y($aQ(source));
	const wired2 = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value) => {
		$z(gated, value, $aS);
		if (!(wired2.v)) {
			wired2.v = true;
			$aU(self, gated, render, $aS, $aT);
		}
		return;
	};
	$bL(source, (value) => {
		const mine = generation.v + 1;
		generation.v = mine;
		clear_chunk_error($aS);
		const arm = __chunk_arm(value);
		if (__chunk_ready(arm)) {
			set_chunk_pending(false, $aS);
			advance(value);
		} else {
			set_chunk_pending(true, $aS);
			__chunk_load(arm, () => {
				return $q([ 1 ], ($bD) => {
					if (generation.v === mine) {
						set_chunk_pending(false, [ 0, $bD ]);
						advance(value);
					}
					return;
				});
			}, (reason) => {
				return $q([ 1 ], ($bE) => {
					if (generation.v === mine) {
						set_chunk_pending(false, [ 0, $bE ]);
						$bF(chunk_error_signal, [ 0, reason ], [ 0, $bE ]);
					}
					return;
				});
			});
		}
		return;
	}, $aS, $aT);
	return __clone(self);
}
function $bQ(body) {
	const scope = new3();
	const result = body(scope);
	return [ result, scope ];
}
function $bR(policy, body) {
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
