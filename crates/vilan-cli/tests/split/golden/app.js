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
function home_page($av, $aw) {
	return __vilan_chunks.fn.home_page($av, $aw);
}
function docs_page(page, $az, $aA) {
	return __vilan_chunks.fn.docs_page(page, $az, $aA);
}
function not_found_page($aD, $aE) {
	return __vilan_chunks.fn.not_found_page($aD, $aE);
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
function dispose(self, $ak) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $al = $ak;
	let $am = null;
	if ($al[0] === 0) {
		const turn = $al[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		$am = undefined;
	} else {
		$am = undefined;
	}
	return $am;
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
function get_owner($ag) {
	return $ag;
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
function navigate(path, $T) {
	ensure_wired($T);
	history.pushState("", "", path);
	$f(path_signal, path, $T);
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
	let $J = null;
	if (is_svg_tag(tag)) {
		$J = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$J = [ document.createElement(tag) ];
	}
	return $J;
}
function is_svg_tag(tag) {
	const $H = tag;
	let $I = null;
	if ($H === "svg") {
		$I = true;
	} else if ($H === "path") {
		$I = true;
	} else if ($H === "circle") {
		$I = true;
	} else if ($H === "ellipse") {
		$I = true;
	} else if ($H === "rect") {
		$I = true;
	} else if ($H === "line") {
		$I = true;
	} else if ($H === "polyline") {
		$I = true;
	} else if ($H === "polygon") {
		$I = true;
	} else if ($H === "g") {
		$I = true;
	} else if ($H === "defs") {
		$I = true;
	} else if ($H === "use") {
		$I = true;
	} else if ($H === "symbol") {
		$I = true;
	} else if ($H === "marker") {
		$I = true;
	} else if ($H === "pattern") {
		$I = true;
	} else if ($H === "mask") {
		$I = true;
	} else if ($H === "clipPath") {
		$I = true;
	} else if ($H === "linearGradient") {
		$I = true;
	} else if ($H === "radialGradient") {
		$I = true;
	} else if ($H === "stop") {
		$I = true;
	} else if ($H === "text") {
		$I = true;
	} else if ($H === "tspan") {
		$I = true;
	} else if ($H === "textPath") {
		$I = true;
	} else if ($H === "filter") {
		$I = true;
	} else if ($H === "foreignObject") {
		$I = true;
	} else if ($H === "feGaussianBlur") {
		$I = true;
	} else if ($H === "feColorMatrix") {
		$I = true;
	} else if ($H === "feOffset") {
		$I = true;
	} else if ($H === "feMerge") {
		$I = true;
	} else if ($H === "feMergeNode") {
		$I = true;
	} else if ($H === "feFlood") {
		$I = true;
	} else if ($H === "feComposite") {
		$I = true;
	} else if ($H === "feBlend") {
		$I = true;
	} else if ($H === "feDropShadow") {
		$I = true;
	} else {
		$I = false;
	}
	return $I;
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
		return $q([ 1 ], ($U) => {
			return handler(dispatched, $U);
		});
	});
	return __clone(self);
}
function bind_text(self, source, $ab, $ac) {
	const element = __clone(self[0]);
	$ad(source, (value) => {
		element.textContent = value;
		return;
	}, $ab, $ac);
	return __clone(self);
}
function chunk_pending() {
	return chunk_pending_signal;
}
function chunk_failure() {
	return chunk_error_signal;
}
function set_chunk_pending(busy, $bl) {
	if ($aa(chunk_pending_signal) !== busy) {
		$bm(chunk_pending_signal, busy, $bl);
	}
}
function clear_chunk_error($bc) {
	const $bd = $ar(chunk_error_signal);
	let $be = null;
	if ($bd[0] === 0) {
		const _reason = $bd[1];
		$be = $bf(chunk_error_signal, [ 1 ], $bc);
	} else {
		$be = undefined;
	}
	return $be;
}
function place(self, parent) {
	parent[0].appendChild(self[0]);
}
function apply(self, parent, name) {
	parent[0].setAttribute(name, self);
}
function mount(id, view2) {
	const element = document.getElementById(id);
	element.replaceChildren();
	element.appendChild(view2[0]);
}
function mount_root(id, body) {
	const $bG = $bF([ 1 ], ($bD) => {
		return $bE(body);
	});
	const built = $bG[0];
	const root = $bG[1];
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
	const $N = route2;
	let $O = null;
	if ($N[0] === 0) {
		$O = "/";
	} else if ($N[0] === 1) {
		const page = $N[1];
		$O = "/docs/" + page;
	} else {
		$O = "/404";
	}
	return $O;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $ax, $ay) {
	return $V($V(view("section"), text(view("h2"), title), $ax, $ay), text(view("p"), body), $ax, $ay);
}
function app(route2, $F, $G) {
	$aF(route2);
	return $aH($V($V($V(view("main"), $V($V(view("nav"), $K("Home", [ 0 ], $F, $G), $F, $G), $K("Docs", [ 1, 1 ], $F, $G), $F, $G), $F, $G), bind_text(class2(view("p"), "pending"), $Z(pending(), (busy) => {
		let $Y = null;
		if (busy) {
			$Y = "...";
		} else {
			$Y = "";
		}
		return $Y;
	}, $F), $F, $G), $F, $G), bind_text(class2(view("p"), "failed"), $aq(chunk_error(), (reason) => {
		const $an = reason;
		let $ao = null;
		if ($an[0] === 0) {
			const text2 = $an[1];
			let $ap = null;
			if (text2.length > 0) {
				$ap = "!";
			} else {
				$ap = "?";
			}
			$ao = $ap;
		} else {
			$ao = "";
		}
		return $ao;
	}, $F), $F, $G), $F, $G), route2, (current, $as) => {
		const $at = current;
		let $au = null;
		if ($at[0] === 0) {
			$au = home_page($F, $as);
		} else if ($at[0] === 1) {
			const page = $at[1];
			$au = docs_page(page, $F, $as);
		} else {
			$au = not_found_page($F, $as);
		}
		return $au;
	}, $F, $G);
}
function eq(self, other) {
	const $aR = [ self, other ];
	let $aS = null;
	if ($aR[0][0] === 0 && $aR[1][0] === 0) {
		$aS = true;
	} else if ($aR[0][0] === 1 && $aR[1][0] === 1) {
		const s0 = $aR[0][1];
		const o0 = $aR[1][1];
		$aS = s0 === o0;
	} else if ($aR[0][0] === 2 && $aR[1][0] === 2) {
		$aS = true;
	} else {
		$aS = false;
	}
	return $aS;
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
function $w(self) {
	return self[0].v;
}
function $x(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $z(self, $i) {
	const $A = $i;
	let $B = null;
	if ($A[0] === 0) {
		const turn = $A[1];
		$B = enqueue(turn, self[1].v);
	} else {
		const $C = $m(draining_turns.v);
		let $D = null;
		if ($C[0] === 0) {
			const draining = $C[1];
			$D = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$D = undefined;
		}
		$B = $D;
	}
	return $B;
}
function $y(self, value, $g) {
	self[0].v = value;
	$z(self, $g);
}
function $u(self, transform, $v) {
	const derived = $x(transform($w(self)));
	self[1].v.push([ fresh_id(), () => {
		$y(derived, transform($w(self)), $v);
		return;
	} ]);
	return derived;
}
function $P(self, name, value, $Q, $R) {
	apply(value, self, name, $Q, $R);
	return __clone(self);
}
function $K(label, route2, $L, $M) {
	const path = to_path(route2);
	return on_event(text($P(view("a"), "href", path, $L, $M), label), "click", (event, $S) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $S ]);
		}
		return;
	});
}
function $V(self, content, $W, $X) {
	place(content, self, $W, $X);
	return __clone(self);
}
function $aa(self) {
	return self[0].v;
}
function $Z(self, transform, $v) {
	const derived = $a(transform($aa(self)));
	self[1].v.push([ fresh_id(), () => {
		$f(derived, transform($aa(self)), $v);
		return;
	} ]);
	return derived;
}
function $ah(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($w(self));
		return;
	} ]);
	observer($w(self));
	return [ self[1], id ];
}
function $ai(self, item, $aj) {
	self[0].v.push(() => {
		dispose(item, $aj);
		return;
	});
	return __clone(item);
}
function $ad(self, observer, $ae, $af) {
	$ai(get_owner($af), $ah(self, observer), $ae);
}
function $ar(self) {
	return self[0].v;
}
function $aq(self, transform, $v) {
	const derived = $a(transform($ar(self)));
	self[1].v.push([ fresh_id(), () => {
		$f(derived, transform($ar(self)), $v);
		return;
	} ]);
	return derived;
}
function $aG(self) {
	return self[0].v;
}
function $aF(source) {
	__chunk_preload(__chunk_arm($aG(source)));
}
function $aY(owner, body) {
	return body(owner);
}
function $bb(self) {
	return self[0].v;
}
function $ba(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($bb(self));
		return;
	} ]);
	observer($bb(self));
	return [ self[1], id ];
}
function $aZ(self, observer, $ae, $af) {
	$ai(get_owner($af), $ba(self, observer), $ae);
}
function $aK(self, source, render, $aL, $aM) {
	const element = __clone(self[0]);
	const last_value = __shared_new([ 1 ]);
	const live_view = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($aM), () => {
		const $aN = live_owner.v;
		let $aO = null;
		if ($aN[0] === 1) {
			$aO = $aN;
		} else {
			$aO = [ 0, dispose2($aN[1]) ];
		}
		$aO;
		return;
	});
	$aZ(source, (value) => {
		const $aP = last_value.v;
		let $aQ = null;
		if ($aP[0] === 0) {
			const previous = $aP[1];
			$aQ = eq(previous, value);
		} else {
			$aQ = false;
		}
		const unchanged = $aQ;
		if (!(unchanged)) {
			const $aT = live_owner.v;
			let $aU = null;
			if ($aT[0] === 1) {
				$aU = $aT;
			} else {
				$aU = [ 0, dispose2($aT[1]) ];
			}
			$aU;
			const $aV = live_view.v;
			let $aW = null;
			if ($aV[0] === 0) {
				const built = $aV[1];
				$aW = built[0].remove();
			} else {
				$aW = undefined;
			}
			$aW;
			const owner = new3();
			const built2 = $aY(owner, ($aX) => {
				return render(value, $aX);
			});
			element.appendChild(built2[0]);
			last_value.v = [ 0, __clone(value) ];
			live_view.v = [ 0, __clone(built2) ];
			live_owner.v = [ 0, __clone(owner) ];
		}
		return;
	}, $aL, $aM);
	return __clone(self);
}
function $bg(self, $i) {
	const $bh = $i;
	let $bi = null;
	if ($bh[0] === 0) {
		const turn = $bh[1];
		$bi = enqueue(turn, self[1].v);
	} else {
		const $bj = $m(draining_turns.v);
		let $bk = null;
		if ($bj[0] === 0) {
			const draining = $bj[1];
			$bk = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bk = undefined;
		}
		$bi = $bk;
	}
	return $bi;
}
function $bf(self, value, $g) {
	self[0].v = value;
	$bg(self, $g);
}
function $bn(self, $i) {
	const $bo = $i;
	let $bp = null;
	if ($bo[0] === 0) {
		const turn = $bo[1];
		$bp = enqueue(turn, self[1].v);
	} else {
		const $bq = $m(draining_turns.v);
		let $br = null;
		if ($bq[0] === 0) {
			const draining = $bq[1];
			$br = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$br = undefined;
		}
		$bp = $br;
	}
	return $bp;
}
function $bm(self, value, $g) {
	self[0].v = value;
	$bn(self, $g);
}
function $bv(self, $i) {
	const $bw = $i;
	let $bx = null;
	if ($bw[0] === 0) {
		const turn = $bw[1];
		$bx = enqueue(turn, self[1].v);
	} else {
		const $by = $m(draining_turns.v);
		let $bz = null;
		if ($by[0] === 0) {
			const draining = $by[1];
			$bz = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bz = undefined;
		}
		$bx = $bz;
	}
	return $bx;
}
function $bu(self, value, $g) {
	self[0].v = value;
	$bv(self, $g);
}
function $bC(self) {
	return self[0].v;
}
function $bB(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($bC(self));
		return;
	} ]);
	observer($bC(self));
	return [ self[1], id ];
}
function $bA(self, observer, $ae, $af) {
	$ai(get_owner($af), $bB(self, observer), $ae);
}
function $aH(self, source, render, $aI, $aJ) {
	const gated = $x($aG(source));
	const wired2 = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value) => {
		$y(gated, value, $aI);
		if (!(wired2.v)) {
			wired2.v = true;
			$aK(self, gated, render, $aI, $aJ);
		}
		return;
	};
	$bA(source, (value) => {
		const mine = generation.v + 1;
		generation.v = mine;
		clear_chunk_error($aI);
		const arm = __chunk_arm(value);
		if (__chunk_ready(arm)) {
			set_chunk_pending(false, $aI);
			advance(value);
		} else {
			set_chunk_pending(true, $aI);
			__chunk_load(arm, () => {
				return $q([ 1 ], ($bs) => {
					if (generation.v === mine) {
						set_chunk_pending(false, [ 0, $bs ]);
						advance(value);
					}
					return;
				});
			}, (reason) => {
				return $q([ 1 ], ($bt) => {
					if (generation.v === mine) {
						set_chunk_pending(false, [ 0, $bt ]);
						$bu(chunk_error_signal, [ 0, reason ], [ 0, $bt ]);
					}
					return;
				});
			});
		}
		return;
	}, $aI, $aJ);
	return __clone(self);
}
function $bE(body) {
	const scope = new3();
	const result = body(scope);
	return [ result, __clone(scope) ];
}
function $bF(policy, body) {
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
__vilan_chunks.fn.$K = $K;
__vilan_chunks.fn.$V = $V;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $u(current_path([ 1 ]), parse, [ 1 ]);
mount_root("app", ($E) => {
	return app(route, [ 1 ], $E);
});
