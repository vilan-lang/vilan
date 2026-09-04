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
const __vilan_chunks = __chunk_registry();
function home_page($aJ, $aK) {
	return __vilan_chunks.fn.home_page($aJ, $aK);
}
function docs_page(page, $aN, $aO) {
	return __vilan_chunks.fn.docs_page(page, $aN, $aO);
}
function not_found_page($aR, $aS) {
	return __vilan_chunks.fn.not_found_page($aR, $aS);
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
		let budget = 100000;
		while (!($l(turn[0].v)) && budget > 0) {
			const wave = turn[0].v;
			turn[0].v = [  ];
			turn[1].v = new Map();
			for (const subscriber of wave) {
				subscriber[1]();
				budget = budget - 1;
			}
		}
		__list_pop(draining_turns.v);
		turn[2].v = false;
	}
}
function dispose(self, $N) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $O = $N;
	let $P = null;
	if ($O[0] === 0) {
		const turn = $O[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$P = undefined;
	} else {
		$P = undefined;
	}
	$P;
	const $Q = self[2].v;
	let $R = null;
	if ($Q[0] === 0) {
		const release = $Q[1];
		self[2].v = [ 1 ];
		release();
		$R = undefined;
	} else {
		$R = undefined;
	}
	return $R;
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
function get_owner($ax) {
	return $ax;
}
function register_with_owner(subscription, $H, $I) {
	const $J = $I;
	let $K = null;
	if ($J[0] === 0) {
		const owner = $J[1];
		$K = $L(owner, subscription, $H);
	} else {
		$K = __clone(subscription);
	}
	return $K;
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
function navigate(path, $ah) {
	ensure_wired($ah);
	history.pushState("", "", path);
	$f(path_signal, path, $ah);
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
	let $X = null;
	if (is_svg_tag(tag)) {
		$X = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$X = [ document.createElement(tag) ];
	}
	return $X;
}
function is_svg_tag(tag) {
	const $V = tag;
	let $W = null;
	if ($V === "svg") {
		$W = true;
	} else if ($V === "path") {
		$W = true;
	} else if ($V === "circle") {
		$W = true;
	} else if ($V === "ellipse") {
		$W = true;
	} else if ($V === "rect") {
		$W = true;
	} else if ($V === "line") {
		$W = true;
	} else if ($V === "polyline") {
		$W = true;
	} else if ($V === "polygon") {
		$W = true;
	} else if ($V === "g") {
		$W = true;
	} else if ($V === "defs") {
		$W = true;
	} else if ($V === "use") {
		$W = true;
	} else if ($V === "symbol") {
		$W = true;
	} else if ($V === "marker") {
		$W = true;
	} else if ($V === "pattern") {
		$W = true;
	} else if ($V === "mask") {
		$W = true;
	} else if ($V === "clipPath") {
		$W = true;
	} else if ($V === "linearGradient") {
		$W = true;
	} else if ($V === "radialGradient") {
		$W = true;
	} else if ($V === "stop") {
		$W = true;
	} else if ($V === "text") {
		$W = true;
	} else if ($V === "tspan") {
		$W = true;
	} else if ($V === "textPath") {
		$W = true;
	} else if ($V === "filter") {
		$W = true;
	} else if ($V === "foreignObject") {
		$W = true;
	} else if ($V === "feGaussianBlur") {
		$W = true;
	} else if ($V === "feColorMatrix") {
		$W = true;
	} else if ($V === "feOffset") {
		$W = true;
	} else if ($V === "feMerge") {
		$W = true;
	} else if ($V === "feMergeNode") {
		$W = true;
	} else if ($V === "feFlood") {
		$W = true;
	} else if ($V === "feComposite") {
		$W = true;
	} else if ($V === "feBlend") {
		$W = true;
	} else if ($V === "feDropShadow") {
		$W = true;
	} else {
		$W = false;
	}
	return $W;
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
		return $q([ 1 ], ($ai) => {
			return handler(dispatched, $ai);
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
	if ($x(chunk_pending_signal) !== busy) {
		$bI(chunk_pending_signal, busy, $bH);
	}
}
function clear_chunk_error($by) {
	const $bz = $x(chunk_error_signal);
	let $bA = null;
	if ($bz[0] === 0) {
		const _reason = $bz[1];
		$bA = $bB(chunk_error_signal, [ 1 ], $by);
	} else {
		$bA = undefined;
	}
	return $bA;
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
	const $cc = $q([ 1 ], ($bZ) => {
		return $ca(body);
	});
	const built = $cc[0];
	const root = $cc[1];
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
	const $ab = route2;
	let $ac = null;
	if ($ab[0] === 0) {
		$ac = "/";
	} else if ($ab[0] === 1) {
		const page = $ab[1];
		$ac = "/docs/" + page;
	} else {
		$ac = "/404";
	}
	return $ac;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $aL, $aM) {
	return $aj($aj(view("section"), text(view("h2"), title), $aL, $aM), text(view("p"), body), $aL, $aM);
}
function app(route2, $T, $U) {
	$aT(route2);
	return $aV($aj($aj($aj(view("main"), $aj($aj(view("nav"), $Y("Home", [ 0 ], $T, $U), $T, $U), $Y("Docs", [ 1, 1 ], $T, $U), $T, $U), $T, $U), $ar(class2(view("p"), "pending"), $an(pending(), (busy) => {
		let $am = null;
		if (busy) {
			$am = "...";
		} else {
			$am = "";
		}
		return $am;
	}, $T, [ 0, $U ]), $T, $U), $T, $U), $ar(class2(view("p"), "failed"), $aC(chunk_error(), (reason) => {
		const $az = reason;
		let $aA = null;
		if ($az[0] === 0) {
			const text2 = $az[1];
			let $aB = null;
			if (text2.length > 0) {
				$aB = "!";
			} else {
				$aB = "?";
			}
			$aA = $aB;
		} else {
			$aA = "";
		}
		return $aA;
	}, $T, [ 0, $U ]), $T, $U), $T, $U), route2, (current, $aG) => {
		const $aH = current;
		let $aI = null;
		if ($aH[0] === 0) {
			$aI = home_page($T, $aG);
		} else if ($aH[0] === 1) {
			const page = $aH[1];
			$aI = docs_page(page, $T, $aG);
		} else {
			$aI = not_found_page($T, $aG);
		}
		return $aI;
	}, $T, $U);
}
function eq(self, other) {
	const $bm = [ self, other ];
	let $bn = null;
	if ($bm[0][0] === 0 && $bm[1][0] === 0) {
		$bn = true;
	} else if ($bm[0][0] === 1 && $bm[1][0] === 1) {
		const s0 = $bm[0][1];
		const o0 = $bm[1][1];
		$bn = s0 === o0;
	} else if ($bm[0][0] === 2 && $bm[1][0] === 2) {
		$bn = true;
	} else {
		$bn = false;
	}
	return $bn;
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
	fresh[3].v = true;
	return result;
}
function $x(self) {
	return self[0].v;
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
function $G(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $F(self, observer) {
	return $G(self, observer);
}
function $L(self, item, $M) {
	self[0].v.push(() => {
		dispose(item, $M);
		return;
	});
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
function $ad(self, name, value, $ae, $af) {
	apply(value, self, name, $ae, $af);
	return __clone(self);
}
function $Y(label, route2, $Z, $aa) {
	const path = to_path(route2);
	return on_event(text($ad(view("a"), "href", path, $Z, $aa), label), "click", (event, $ag) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $ag ]);
		}
		return;
	});
}
function $aj(self, content, $ak, $al) {
	place(content, self, $ak, $al);
	return __clone(self);
}
function $ap(self, observer) {
	return $G(self, observer);
}
function $an(self, transform, $v, $w) {
	const derived = $a(transform($x(self)));
	register_with_owner($ap(self, (value) => {
		$z(derived, transform(value), $v);
		return;
	}), $v, $w);
	return derived;
}
function $ay(self, observer) {
	const subscription = $G(self, observer);
	observer($x(self));
	return subscription;
}
function $au(self, observer, $av, $aw) {
	$L(get_owner($aw), $ay(self, observer), $av);
}
function $ar(self, source, $as, $at) {
	const element = __clone(self[0]);
	$au(source, (value) => {
		element.textContent = value;
		return;
	}, $as, $at);
	return __clone(self);
}
function $aC(self, transform, $v, $w) {
	const derived = $a(transform($x(self)));
	register_with_owner($ap(self, (value) => {
		$z(derived, transform(value), $v);
		return;
	}), $v, $w);
	return derived;
}
function $aT(source) {
	__chunk_preload(__chunk_arm($x(source)));
}
function $ba(self, $i) {
	const $bb = $i;
	let $bc = null;
	if ($bb[0] === 0) {
		const turn = $bb[1];
		$bc = enqueue(turn, self[1].v);
	} else {
		const $bd = $m(draining_turns.v);
		let $be = null;
		if ($bd[0] === 0) {
			const draining = $bd[1];
			$be = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$be = undefined;
		}
		$bc = $be;
	}
	return $bc;
}
function $aZ(self, value, $g) {
	self[0].v = value;
	$ba(self, $g);
}
function $bt(owner, body) {
	return body(owner);
}
function $bu(self, observer, $av, $aw) {
	$L(get_owner($aw), $ay(self, observer), $av);
}
function $bf(self, source, render, $bg, $bh) {
	const element = __clone(self[0]);
	const last_value = __shared_new([ 1 ]);
	const live_view = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($bh), () => {
		const $bi = live_owner.v;
		let $bj = null;
		if ($bi[0] === 1) {
			$bj = $bi;
		} else {
			$bj = [ 0, dispose2($bi[1]) ];
		}
		$bj;
		return;
	});
	$bu(source, (value) => {
		const $bk = last_value.v;
		let $bl = null;
		if ($bk[0] === 0) {
			const previous = $bk[1];
			$bl = eq(previous, value);
		} else {
			$bl = false;
		}
		const unchanged = $bl;
		if (!(unchanged)) {
			const $bo = live_owner.v;
			let $bp = null;
			if ($bo[0] === 1) {
				$bp = $bo;
			} else {
				$bp = [ 0, dispose2($bo[1]) ];
			}
			$bp;
			const $bq = live_view.v;
			let $br = null;
			if ($bq[0] === 0) {
				const built = $bq[1];
				$br = built[0].remove();
			} else {
				$br = undefined;
			}
			$br;
			const owner = new3();
			const built2 = $bt(owner, ($bs) => {
				return render(value, $bs);
			});
			element.appendChild(built2[0]);
			last_value.v = [ 0, __clone(value) ];
			live_view.v = [ 0, built2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $bg, $bh);
	return __clone(self);
}
function $bC(self, $i) {
	const $bD = $i;
	let $bE = null;
	if ($bD[0] === 0) {
		const turn = $bD[1];
		$bE = enqueue(turn, self[1].v);
	} else {
		const $bF = $m(draining_turns.v);
		let $bG = null;
		if ($bF[0] === 0) {
			const draining = $bF[1];
			$bG = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bG = undefined;
		}
		$bE = $bG;
	}
	return $bE;
}
function $bB(self, value, $g) {
	self[0].v = value;
	$bC(self, $g);
}
function $bJ(self, $i) {
	const $bK = $i;
	let $bL = null;
	if ($bK[0] === 0) {
		const turn = $bK[1];
		$bL = enqueue(turn, self[1].v);
	} else {
		const $bM = $m(draining_turns.v);
		let $bN = null;
		if ($bM[0] === 0) {
			const draining = $bM[1];
			$bN = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bN = undefined;
		}
		$bL = $bN;
	}
	return $bL;
}
function $bI(self, value, $g) {
	self[0].v = value;
	$bJ(self, $g);
}
function $bR(self, $i) {
	const $bS = $i;
	let $bT = null;
	if ($bS[0] === 0) {
		const turn = $bS[1];
		$bT = enqueue(turn, self[1].v);
	} else {
		const $bU = $m(draining_turns.v);
		let $bV = null;
		if ($bU[0] === 0) {
			const draining = $bU[1];
			$bV = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bV = undefined;
		}
		$bT = $bV;
	}
	return $bT;
}
function $bQ(self, value, $g) {
	self[0].v = value;
	$bR(self, $g);
}
function $aV(self, source, render, $aW, $aX) {
	const gated = $a($x(source));
	const wired2 = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value) => {
		$aZ(gated, value, $aW);
		if (!(wired2.v)) {
			wired2.v = true;
			$bf(self, gated, render, $aW, $aX);
		}
		return;
	};
	$bu(source, (value) => {
		const mine = generation.v + 1;
		generation.v = mine;
		clear_chunk_error($aW);
		const arm = __chunk_arm(value);
		if (__chunk_ready(arm)) {
			set_chunk_pending(false, $aW);
			advance(value);
		} else {
			set_chunk_pending(true, $aW);
			__chunk_load(arm, () => {
				return $q([ 1 ], ($bO) => {
					if (generation.v === mine) {
						set_chunk_pending(false, [ 0, $bO ]);
						advance(value);
					}
					return;
				});
			}, (reason) => {
				return $q([ 1 ], ($bP) => {
					if (generation.v === mine) {
						set_chunk_pending(false, [ 0, $bP ]);
						$bQ(chunk_error_signal, [ 0, reason ], [ 0, $bP ]);
					}
					return;
				});
			});
		}
		return;
	}, $aW, $aX);
	return __clone(self);
}
function $ca(body) {
	const scope = new3();
	const result = body(scope);
	return [ result, scope ];
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
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
__vilan_chunks.fn.$Y = $Y;
__vilan_chunks.fn.$aj = $aj;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $u(current_path([ 1 ]), parse, [ 1 ], [ 1 ]);
mount_root("app", ($S) => {
	return app(route, [ 1 ], $S);
});
