function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __chunk_arm(value) {
	return Array.isArray(value) ? value[0] : -1;
}
function __chunk_load(arm, then) {
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
		}, (error) => {
			delete chunks.pending[arm];
			console.error("[vilan] route chunk " + url + " failed to load", error);
			throw error;
		});
		chunks.pending[arm] = inflight;
	}
	inflight.then(then, () => {});
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
function home_page($ap, $aq) {
	return __vilan_chunks.fn.home_page($ap, $aq);
}
function docs_page(page, $at, $au) {
	return __vilan_chunks.fn.docs_page(page, $at, $au);
}
function not_found_page($ax, $ay) {
	return __vilan_chunks.fn.not_found_page($ax, $ay);
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
		while (!($k(turn[0].v)) && budget > 0) {
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
function dispose(self, $aj) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $ak = $aj;
	let $al = null;
	if ($ak[0] === 0) {
		const turn = $ak[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		$al = undefined;
	} else {
		$al = undefined;
	}
	return $al;
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
function get_owner($af) {
	return $af;
}
function ensure_wired($d) {
	if (!(wired.v)) {
		wired.v = true;
		$e(path_signal, __router_path(), $d);
		window.addEventListener("popstate", () => {
			return $p([ 1 ], ($o) => {
				$e(path_signal, __router_path(), [ 0, $o ]);
				return;
			});
		});
	}
}
function current_path($c) {
	ensure_wired($c);
	return path_signal;
}
function navigate(path, $S) {
	ensure_wired($S);
	history.pushState("", "", path);
	$e(path_signal, path, $S);
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
function view(tag) {
	let $I = null;
	if (is_svg_tag(tag)) {
		$I = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$I = [ document.createElement(tag) ];
	}
	return $I;
}
function is_svg_tag(tag) {
	const $G = tag;
	let $H = null;
	if ($G === "svg") {
		$H = true;
	} else if ($G === "path") {
		$H = true;
	} else if ($G === "circle") {
		$H = true;
	} else if ($G === "ellipse") {
		$H = true;
	} else if ($G === "rect") {
		$H = true;
	} else if ($G === "line") {
		$H = true;
	} else if ($G === "polyline") {
		$H = true;
	} else if ($G === "polygon") {
		$H = true;
	} else if ($G === "g") {
		$H = true;
	} else if ($G === "defs") {
		$H = true;
	} else if ($G === "use") {
		$H = true;
	} else if ($G === "symbol") {
		$H = true;
	} else if ($G === "marker") {
		$H = true;
	} else if ($G === "pattern") {
		$H = true;
	} else if ($G === "mask") {
		$H = true;
	} else if ($G === "clipPath") {
		$H = true;
	} else if ($G === "linearGradient") {
		$H = true;
	} else if ($G === "radialGradient") {
		$H = true;
	} else if ($G === "stop") {
		$H = true;
	} else if ($G === "text") {
		$H = true;
	} else if ($G === "tspan") {
		$H = true;
	} else if ($G === "textPath") {
		$H = true;
	} else if ($G === "filter") {
		$H = true;
	} else if ($G === "foreignObject") {
		$H = true;
	} else if ($G === "feGaussianBlur") {
		$H = true;
	} else if ($G === "feColorMatrix") {
		$H = true;
	} else if ($G === "feOffset") {
		$H = true;
	} else if ($G === "feMerge") {
		$H = true;
	} else if ($G === "feMergeNode") {
		$H = true;
	} else if ($G === "feFlood") {
		$H = true;
	} else if ($G === "feComposite") {
		$H = true;
	} else if ($G === "feBlend") {
		$H = true;
	} else if ($G === "feDropShadow") {
		$H = true;
	} else {
		$H = false;
	}
	return $H;
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
		return $p([ 1 ], ($T) => {
			return handler(dispatched, $T);
		});
	});
	return __clone(self);
}
function bind_text(self, source, $aa, $ab) {
	const element = __clone(self[0]);
	$ac(source, (value) => {
		element.textContent = value;
		return;
	}, $aa, $ab);
	return __clone(self);
}
function chunk_pending() {
	return chunk_pending_signal;
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
	const $bi = $bh([ 1 ], ($bf) => {
		return $bg(body);
	});
	const built = $bi[0];
	const root = $bi[1];
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
	let $s = null;
	if (__at(parts, 0) === "docs" && parts.length === 2) {
		const $q = __parse_i32(__at(parts, 1));
		let $r = null;
		if ($q[0] === 0) {
			const page = $q[1];
			return [ 1, page ];
		} else {
			$r = undefined;
		}
		$s = $r;
	}
	$s;
	return [ 2 ];
}
function href(route2) {
	const $M = route2;
	let $N = null;
	if ($M[0] === 0) {
		$N = "/";
	} else if ($M[0] === 1) {
		const page = $M[1];
		$N = "/docs/" + page;
	} else {
		$N = "/404";
	}
	return $N;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $ar, $as) {
	return $U($U(view("section"), text(view("h2"), title), $ar, $as), text(view("p"), body), $ar, $as);
}
function app(route2, $E, $F) {
	return $az($U($U(view("main"), $U($U(view("nav"), $J("Home", [ 0 ], $E, $F), $E, $F), $J("Docs", [ 1, 1 ], $E, $F), $E, $F), $E, $F), bind_text(class2(view("p"), "pending"), $Y(pending(), (busy) => {
		let $X = null;
		if (busy) {
			$X = "...";
		} else {
			$X = "";
		}
		return $X;
	}, $E), $E, $F), $E, $F), route2, (current, $am) => {
		const $an = current;
		let $ao = null;
		if ($an[0] === 0) {
			$ao = home_page($E, $am);
		} else if ($an[0] === 1) {
			const page = $an[1];
			$ao = docs_page(page, $E, $am);
		} else {
			$ao = not_found_page($E, $am);
		}
		return $ao;
	}, $E, $F);
}
function eq(self, other) {
	const $aK = [ self, other ];
	let $aL = null;
	if ($aK[0][0] === 0 && $aK[1][0] === 0) {
		$aL = true;
	} else if ($aK[0][0] === 1 && $aK[1][0] === 1) {
		const s0 = $aK[0][1];
		const o0 = $aK[1][1];
		$aL = s0 === o0;
	} else if ($aK[0][0] === 2 && $aK[1][0] === 2) {
		$aL = true;
	} else {
		$aL = false;
	}
	return $aL;
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $k(self) {
	return self.length === 0;
}
function $l(self) {
	return __list_get(self, self.length - 1);
}
function $g(self, $h) {
	const $i = $h;
	let $j = null;
	if ($i[0] === 0) {
		const turn = $i[1];
		$j = enqueue(turn, self[1].v);
	} else {
		const $m = $l(draining_turns.v);
		let $n = null;
		if ($m[0] === 0) {
			const draining = $m[1];
			$n = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$n = undefined;
		}
		$j = $n;
	}
	return $j;
}
function $e(self, value, $f) {
	self[0].v = value;
	$g(self, $f);
}
function $p(policy, body) {
	const fresh = new2();
	const result = body(fresh);
	drain(fresh);
	fresh[2].v = true;
	return result;
}
function $v(self) {
	return self[0].v;
}
function $w(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $y(self, $h) {
	const $z = $h;
	let $A = null;
	if ($z[0] === 0) {
		const turn = $z[1];
		$A = enqueue(turn, self[1].v);
	} else {
		const $B = $l(draining_turns.v);
		let $C = null;
		if ($B[0] === 0) {
			const draining = $B[1];
			$C = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$C = undefined;
		}
		$A = $C;
	}
	return $A;
}
function $x(self, value, $f) {
	self[0].v = value;
	$y(self, $f);
}
function $t(self, transform, $u) {
	const derived = $w(transform($v(self)));
	self[1].v.push([ fresh_id(), () => {
		$x(derived, transform($v(self)), $u);
		return;
	} ]);
	return derived;
}
function $O(self, name, value, $P, $Q) {
	apply(value, self, name, $P, $Q);
	return __clone(self);
}
function $J(label, route2, $K, $L) {
	const path = to_path(route2);
	return on_event(text($O(view("a"), "href", path, $K, $L), label), "click", (event, $R) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $R ]);
		}
		return;
	});
}
function $U(self, content, $V, $W) {
	place(content, self, $V, $W);
	return __clone(self);
}
function $Z(self) {
	return self[0].v;
}
function $Y(self, transform, $u) {
	const derived = $a(transform($Z(self)));
	self[1].v.push([ fresh_id(), () => {
		$e(derived, transform($Z(self)), $u);
		return;
	} ]);
	return derived;
}
function $ag(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($v(self));
		return;
	} ]);
	observer($v(self));
	return [ self[1], id ];
}
function $ah(self, item, $ai) {
	self[0].v.push(() => {
		dispose(item, $ai);
		return;
	});
	return __clone(item);
}
function $ac(self, observer, $ad, $ae) {
	$ah(get_owner($ae), $ag(self, observer), $ad);
}
function $aC(self) {
	return self[0].v;
}
function $aR(owner, body) {
	return body(owner);
}
function $aU(self) {
	return self[0].v;
}
function $aT(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($aU(self));
		return;
	} ]);
	observer($aU(self));
	return [ self[1], id ];
}
function $aS(self, observer, $ad, $ae) {
	$ah(get_owner($ae), $aT(self, observer), $ad);
}
function $aD(self, source, render, $aE, $aF) {
	const element = __clone(self[0]);
	const last_value = __shared_new([ 1 ]);
	const live_view = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($aF), () => {
		const $aG = live_owner.v;
		let $aH = null;
		if ($aG[0] === 1) {
			$aH = $aG;
		} else {
			$aH = [ 0, dispose2($aG[1]) ];
		}
		$aH;
		return;
	});
	$aS(source, (value) => {
		const $aI = last_value.v;
		let $aJ = null;
		if ($aI[0] === 0) {
			const previous = $aI[1];
			$aJ = eq(previous, value);
		} else {
			$aJ = false;
		}
		const unchanged = $aJ;
		if (!(unchanged)) {
			const $aM = live_owner.v;
			let $aN = null;
			if ($aM[0] === 1) {
				$aN = $aM;
			} else {
				$aN = [ 0, dispose2($aM[1]) ];
			}
			$aN;
			const $aO = live_view.v;
			let $aP = null;
			if ($aO[0] === 0) {
				const built = $aO[1];
				$aP = built[0].remove();
			} else {
				$aP = undefined;
			}
			$aP;
			const owner = new3();
			const built2 = $aR(owner, ($aQ) => {
				return render(value, $aQ);
			});
			element.appendChild(built2[0]);
			last_value.v = [ 0, __clone(value) ];
			live_view.v = [ 0, __clone(built2) ];
			live_owner.v = [ 0, __clone(owner) ];
		}
		return;
	}, $aE, $aF);
	return __clone(self);
}
function $aW(self, $h) {
	const $aX = $h;
	let $aY = null;
	if ($aX[0] === 0) {
		const turn = $aX[1];
		$aY = enqueue(turn, self[1].v);
	} else {
		const $aZ = $l(draining_turns.v);
		let $ba = null;
		if ($aZ[0] === 0) {
			const draining = $aZ[1];
			$ba = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$ba = undefined;
		}
		$aY = $ba;
	}
	return $aY;
}
function $aV(self, value, $f) {
	self[0].v = value;
	$aW(self, $f);
}
function $be(self) {
	return self[0].v;
}
function $bd(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($be(self));
		return;
	} ]);
	observer($be(self));
	return [ self[1], id ];
}
function $bc(self, observer, $ad, $ae) {
	$ah(get_owner($ae), $bd(self, observer), $ad);
}
function $az(self, source, render, $aA, $aB) {
	const gated = $w($aC(source));
	const wired2 = __shared_new(false);
	const advance = (value) => {
		$x(gated, value, $aA);
		if (!(wired2.v)) {
			wired2.v = true;
			$aD(self, gated, render, $aA, $aB);
		}
		return;
	};
	$bc(source, (value) => {
		const arm = __chunk_arm(value);
		if (__chunk_ready(arm)) {
			advance(value);
		} else {
			$aV(chunk_pending_signal, true, $aA);
			__chunk_load(arm, () => {
				return $p([ 1 ], ($bb) => {
					$aV(chunk_pending_signal, false, [ 0, $bb ]);
					advance(value);
					return;
				});
			});
		}
		return;
	}, $aA, $aB);
	return __clone(self);
}
function $bg(body) {
	const scope = new3();
	const result = body(scope);
	return [ result, __clone(scope) ];
}
function $bh(policy, body) {
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
const BASE = announce("BASE", 2);
const SCALED = announce("SCALED", BASE * 3);
const LABEL = "scale " + SCALED;
__vilan_chunks.url[0] = "app.Route_Home.js";
__vilan_chunks.url[1] = "app.Route_Docs.js";
__vilan_chunks.url[2] = "app.Route_NotFound.js";
__vilan_chunks.fn.$J = $J;
__vilan_chunks.fn.$U = $U;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $t(current_path([ 1 ]), parse, [ 1 ]);
mount_root("app", ($D) => {
	return app(route, [ 1 ], $D);
});
