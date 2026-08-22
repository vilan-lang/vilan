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
function home_page($ax, $ay) {
	return __vilan_chunks.fn.home_page($ax, $ay);
}
function docs_page(page, $aB, $aC) {
	return __vilan_chunks.fn.docs_page(page, $aB, $aC);
}
function not_found_page($aF, $aG) {
	return __vilan_chunks.fn.not_found_page($aF, $aG);
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
	$am;
	const $an = self[2].v;
	let $ao = null;
	if ($an[0] === 0) {
		const release = $an[1];
		self[2].v = [ 1 ];
		release();
		$ao = undefined;
	} else {
		$ao = undefined;
	}
	return $ao;
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
function set_chunk_pending(busy, $bn) {
	if ($aa(chunk_pending_signal) !== busy) {
		$bo(chunk_pending_signal, busy, $bn);
	}
}
function clear_chunk_error($be) {
	const $bf = $at(chunk_error_signal);
	let $bg = null;
	if ($bf[0] === 0) {
		const _reason = $bf[1];
		$bg = $bh(chunk_error_signal, [ 1 ], $be);
	} else {
		$bg = undefined;
	}
	return $bg;
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
	const $bI = $bH([ 1 ], ($bF) => {
		return $bG(body);
	});
	const built = $bI[0];
	const root = $bI[1];
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
function panel(title, body, $az, $aA) {
	return $V($V(view("section"), text(view("h2"), title), $az, $aA), text(view("p"), body), $az, $aA);
}
function app(route2, $F, $G) {
	$aH(route2);
	return $aJ($V($V($V(view("main"), $V($V(view("nav"), $K("Home", [ 0 ], $F, $G), $F, $G), $K("Docs", [ 1, 1 ], $F, $G), $F, $G), $F, $G), bind_text(class2(view("p"), "pending"), $Z(pending(), (busy) => {
		let $Y = null;
		if (busy) {
			$Y = "...";
		} else {
			$Y = "";
		}
		return $Y;
	}, $F), $F, $G), $F, $G), bind_text(class2(view("p"), "failed"), $as(chunk_error(), (reason) => {
		const $ap = reason;
		let $aq = null;
		if ($ap[0] === 0) {
			const text2 = $ap[1];
			let $ar = null;
			if (text2.length > 0) {
				$ar = "!";
			} else {
				$ar = "?";
			}
			$aq = $ar;
		} else {
			$aq = "";
		}
		return $aq;
	}, $F), $F, $G), $F, $G), route2, (current, $au) => {
		const $av = current;
		let $aw = null;
		if ($av[0] === 0) {
			$aw = home_page($F, $au);
		} else if ($av[0] === 1) {
			const page = $av[1];
			$aw = docs_page(page, $F, $au);
		} else {
			$aw = not_found_page($F, $au);
		}
		return $aw;
	}, $F, $G);
}
function eq(self, other) {
	const $aT = [ self, other ];
	let $aU = null;
	if ($aT[0][0] === 0 && $aT[1][0] === 0) {
		$aU = true;
	} else if ($aT[0][0] === 1 && $aT[1][0] === 1) {
		const s0 = $aT[0][1];
		const o0 = $aT[1][1];
		$aU = s0 === o0;
	} else if ($aT[0][0] === 2 && $aT[1][0] === 2) {
		$aU = true;
	} else {
		$aU = false;
	}
	return $aU;
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
	return [ self[1], id, __shared_new([ 1 ]) ];
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
function $at(self) {
	return self[0].v;
}
function $as(self, transform, $v) {
	const derived = $a(transform($at(self)));
	self[1].v.push([ fresh_id(), () => {
		$f(derived, transform($at(self)), $v);
		return;
	} ]);
	return derived;
}
function $aI(self) {
	return self[0].v;
}
function $aH(source) {
	__chunk_preload(__chunk_arm($aI(source)));
}
function $ba(owner, body) {
	return body(owner);
}
function $bd(self) {
	return self[0].v;
}
function $bc(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($bd(self));
		return;
	} ]);
	observer($bd(self));
	return [ self[1], id, __shared_new([ 1 ]) ];
}
function $bb(self, observer, $ae, $af) {
	$ai(get_owner($af), $bc(self, observer), $ae);
}
function $aM(self, source, render, $aN, $aO) {
	const element = __clone(self[0]);
	const last_value = __shared_new([ 1 ]);
	const live_view = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($aO), () => {
		const $aP = live_owner.v;
		let $aQ = null;
		if ($aP[0] === 1) {
			$aQ = $aP;
		} else {
			$aQ = [ 0, dispose2($aP[1]) ];
		}
		$aQ;
		return;
	});
	$bb(source, (value) => {
		const $aR = last_value.v;
		let $aS = null;
		if ($aR[0] === 0) {
			const previous = $aR[1];
			$aS = eq(previous, value);
		} else {
			$aS = false;
		}
		const unchanged = $aS;
		if (!(unchanged)) {
			const $aV = live_owner.v;
			let $aW = null;
			if ($aV[0] === 1) {
				$aW = $aV;
			} else {
				$aW = [ 0, dispose2($aV[1]) ];
			}
			$aW;
			const $aX = live_view.v;
			let $aY = null;
			if ($aX[0] === 0) {
				const built = $aX[1];
				$aY = built[0].remove();
			} else {
				$aY = undefined;
			}
			$aY;
			const owner = new3();
			const built2 = $ba(owner, ($aZ) => {
				return render(value, $aZ);
			});
			element.appendChild(built2[0]);
			last_value.v = [ 0, __clone(value) ];
			live_view.v = [ 0, __clone(built2) ];
			live_owner.v = [ 0, __clone(owner) ];
		}
		return;
	}, $aN, $aO);
	return __clone(self);
}
function $bi(self, $i) {
	const $bj = $i;
	let $bk = null;
	if ($bj[0] === 0) {
		const turn = $bj[1];
		$bk = enqueue(turn, self[1].v);
	} else {
		const $bl = $m(draining_turns.v);
		let $bm = null;
		if ($bl[0] === 0) {
			const draining = $bl[1];
			$bm = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bm = undefined;
		}
		$bk = $bm;
	}
	return $bk;
}
function $bh(self, value, $g) {
	self[0].v = value;
	$bi(self, $g);
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
				subscriber[1]();
			}
			$bt = undefined;
		}
		$br = $bt;
	}
	return $br;
}
function $bo(self, value, $g) {
	self[0].v = value;
	$bp(self, $g);
}
function $bx(self, $i) {
	const $by = $i;
	let $bz = null;
	if ($by[0] === 0) {
		const turn = $by[1];
		$bz = enqueue(turn, self[1].v);
	} else {
		const $bA = $m(draining_turns.v);
		let $bB = null;
		if ($bA[0] === 0) {
			const draining = $bA[1];
			$bB = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bB = undefined;
		}
		$bz = $bB;
	}
	return $bz;
}
function $bw(self, value, $g) {
	self[0].v = value;
	$bx(self, $g);
}
function $bE(self) {
	return self[0].v;
}
function $bD(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($bE(self));
		return;
	} ]);
	observer($bE(self));
	return [ self[1], id, __shared_new([ 1 ]) ];
}
function $bC(self, observer, $ae, $af) {
	$ai(get_owner($af), $bD(self, observer), $ae);
}
function $aJ(self, source, render, $aK, $aL) {
	const gated = $x($aI(source));
	const wired2 = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value) => {
		$y(gated, value, $aK);
		if (!(wired2.v)) {
			wired2.v = true;
			$aM(self, gated, render, $aK, $aL);
		}
		return;
	};
	$bC(source, (value) => {
		const mine = generation.v + 1;
		generation.v = mine;
		clear_chunk_error($aK);
		const arm = __chunk_arm(value);
		if (__chunk_ready(arm)) {
			set_chunk_pending(false, $aK);
			advance(value);
		} else {
			set_chunk_pending(true, $aK);
			__chunk_load(arm, () => {
				return $q([ 1 ], ($bu) => {
					if (generation.v === mine) {
						set_chunk_pending(false, [ 0, $bu ]);
						advance(value);
					}
					return;
				});
			}, (reason) => {
				return $q([ 1 ], ($bv) => {
					if (generation.v === mine) {
						set_chunk_pending(false, [ 0, $bv ]);
						$bw(chunk_error_signal, [ 0, reason ], [ 0, $bv ]);
					}
					return;
				});
			});
		}
		return;
	}, $aK, $aL);
	return __clone(self);
}
function $bG(body) {
	const scope = new3();
	const result = body(scope);
	return [ result, __clone(scope) ];
}
function $bH(policy, body) {
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
