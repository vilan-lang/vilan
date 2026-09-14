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
function home_page($aO, $aP) {
	return __vilan_chunks.fn.home_page($aO, $aP);
}
function docs_page(page, $aS, $aT) {
	return __vilan_chunks.fn.docs_page(page, $aS, $aT);
}
function not_found_page($aW, $aX) {
	return __vilan_chunks.fn.not_found_page($aW, $aX);
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
					subscriber[1]();
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
function dispose(self, $N) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const ambient = $N;
	const $O = ambient;
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
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$R = undefined;
	} else {
		$R = undefined;
	}
	return $R;
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
	let $cf = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $bZ = __guarded(cleanup);
			let $ca = null;
			if ($bZ[0] === 0) {
				const message = $bZ[1];
				if ($cb(failure)) {
					failure = [ 0, message ];
				}
				$ca = undefined;
			} else {
				$ca = undefined;
			}
			$ca;
		}
		self[0].v = [  ];
		const $cd = failure;
		let $ce = null;
		if ($cd[0] === 0) {
			const message2 = $cd[1];
			$ce = (() => {
				throw message2;
			})();
		} else {
			$ce = undefined;
		}
		$cf = $ce;
	}
	return $cf;
}
function get_owner($aD) {
	return $aD;
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
function navigate(path, $ak) {
	ensure_wired($ak);
	history.pushState("", "", path);
	$f(path_signal, path, $ak);
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
		return $q([ 1 ], ($al) => {
			return handler(dispatched, $al);
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
function set_chunk_pending(busy, $bv) {
	if ($x(chunk_pending_signal) !== busy) {
		$bw(chunk_pending_signal, busy, $bv);
	}
}
function clear_chunk_error($bm) {
	const $bn = $x(chunk_error_signal);
	let $bo = null;
	if ($bn[0] === 0) {
		const _reason = $bn[1];
		$bo = $bp(chunk_error_signal, [ 1 ], $bm);
	} else {
		$bo = undefined;
	}
	return $bo;
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
		let $cg = null;
		if (at + 1 < rows.length) {
			$cg = __at(rows, at + 1)[0];
		} else {
			$cg = self[0];
		}
		const end = $cg;
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
	const $cA = $q([ 1 ], ($cx) => {
		return $cy(body);
	});
	const built = $cA[0];
	const root = $cA[1];
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
	const $ae = route2;
	let $af = null;
	if ($ae[0] === 0) {
		$af = "/";
	} else if ($ae[0] === 1) {
		const page = $ae[1];
		$af = "/docs/" + page;
	} else {
		$af = "/404";
	}
	return $af;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $aQ, $aR) {
	return $am($am(view("section"), text(view("h2"), title), $aQ, $aR), text(view("p"), body), $aQ, $aR);
}
function app(route2, $T, $U) {
	$aY(route2);
	return $bO($am($am($am(view("main"), $am($am(view("nav"), $Y("Home", [ 0 ], $T, $U), $T, $U), $Y("Docs", [ 1, 1 ], $T, $U), $T, $U), $T, $U), $au(class2(view("p"), "pending"), $aq(pending(), (busy) => {
		let $ap = null;
		if (busy) {
			$ap = "...";
		} else {
			$ap = "";
		}
		return $ap;
	}, $T, [ 0, $U ]), $T, $U), $T, $U), $au(class2(view("p"), "failed"), $aH(chunk_error(), (reason) => {
		const $aE = reason;
		let $aF = null;
		if ($aE[0] === 0) {
			const text2 = $aE[1];
			let $aG = null;
			if (text2.length > 0) {
				$aG = "!";
			} else {
				$aG = "?";
			}
			$aF = $aG;
		} else {
			$aF = "";
		}
		return $aF;
	}, $T, [ 0, $U ]), $T, $U), $T, $U), $ba(route2, (current, $aL) => {
		const $aM = current;
		let $aN = null;
		if ($aM[0] === 0) {
			$aN = home_page($T, $aL);
		} else if ($aM[0] === 1) {
			const page = $aM[1];
			$aN = docs_page(page, $T, $aL);
		} else {
			$aN = not_found_page($T, $aL);
		}
		return $aN;
	}), $T, $U);
}
function eq(self, other) {
	const $cj = [ self, other ];
	let $ck = null;
	if ($cj[0][0] === 0 && $cj[1][0] === 0) {
		$ck = true;
	} else if ($cj[0][0] === 1 && $cj[1][0] === 1) {
		const s0 = $cj[0][1];
		const o0 = $cj[1][1];
		$ck = s0 === o0;
	} else if ($cj[0][0] === 2 && $cj[1][0] === 2) {
		$ck = true;
	} else {
		$ck = false;
	}
	return $ck;
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
				subscriber[1]();
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
	if (self[1].v) {
		dispose(item, $M);
	} else {
		self[0].v.push(() => {
			dispose(item, $M);
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
function $ag(self, name, value, $ah, $ai) {
	apply(value, self, name, $ah, $ai);
	return __clone(self);
}
function $ab(self, route2, $ac, $ad) {
	const path = to_path(route2);
	return on_event($ag($ag(self, "href", path, $ac, $ad), "draggable", "false", $ac, $ad), "click", (event, $aj) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $aj ]);
		}
		return;
	});
}
function $Y(label, route2, $Z, $aa) {
	return text($ab(view("a"), route2, $Z, $aa), label);
}
function $am(self, content, $an, $ao) {
	place(content, self, $an, $ao);
	return __clone(self);
}
function $as(self, observer) {
	return $G(self, observer);
}
function $aq(self, transform, $v, $w) {
	const derived = $a(transform($x(self)));
	register_with_owner($as(self, (value) => {
		$z(derived, transform(value), $v);
		return;
	}), $v, $w);
	return derived;
}
function $aA(self, observer, $aB, $aC) {
	$L(get_owner($aC), $F(self, observer), $aB);
}
function $ax(self, observer, $ay, $az) {
	$aA(self, observer, $ay, $az);
	observer($x(self));
}
function $au(self, source, $av, $aw) {
	const element = __clone(self[0]);
	$ax(source, (value) => {
		element.textContent = value;
		return;
	}, $av, $aw);
	return __clone(self);
}
function $aH(self, transform, $v, $w) {
	const derived = $a(transform($x(self)));
	register_with_owner($as(self, (value) => {
		$z(derived, transform(value), $v);
		return;
	}), $v, $w);
	return derived;
}
function $aY(source) {
	__chunk_preload(__chunk_arm($x(source)));
}
function $bf(self, $i) {
	const $bg = $i;
	let $bh = null;
	if ($bg[0] === 0) {
		const turn = $bg[1];
		$bh = enqueue(turn, self[1].v);
	} else {
		const $bi = $m(draining_turns.v);
		let $bj = null;
		if ($bi[0] === 0) {
			const draining = $bi[1];
			$bj = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bj = undefined;
		}
		$bh = $bj;
	}
	return $bh;
}
function $be(self, value, $g) {
	self[0].v = __clone(value);
	$bf(self, $g);
}
function $bq(self, $i) {
	const $br = $i;
	let $bs = null;
	if ($br[0] === 0) {
		const turn = $br[1];
		$bs = enqueue(turn, self[1].v);
	} else {
		const $bt = $m(draining_turns.v);
		let $bu = null;
		if ($bt[0] === 0) {
			const draining = $bt[1];
			$bu = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bu = undefined;
		}
		$bs = $bu;
	}
	return $bs;
}
function $bp(self, value, $g) {
	self[0].v = __clone(value);
	$bq(self, $g);
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
	self[0].v = __clone(value);
	$bx(self, $g);
}
function $bF(self, $i) {
	const $bG = $i;
	let $bH = null;
	if ($bG[0] === 0) {
		const turn = $bG[1];
		$bH = enqueue(turn, self[1].v);
	} else {
		const $bI = $m(draining_turns.v);
		let $bJ = null;
		if ($bI[0] === 0) {
			const draining = $bI[1];
			$bJ = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$bJ = undefined;
		}
		$bH = $bJ;
	}
	return $bH;
}
function $bE(self, value, $g) {
	self[0].v = __clone(value);
	$bF(self, $g);
}
function $bL(self, observer, $aB, $aC) {
	$L(get_owner($aC), $as(self, observer), $aB);
}
function $bK(self, observer, $ay, $az) {
	$bL(self, observer, $ay, $az);
	observer($x(self));
}
function $ba(source, render, $bb) {
	const gated = $a($x(source));
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value, $bd) => {
		armed.v = true;
		$be(gated, value, [ 0, $bd ]);
		return;
	};
	const wire = ($bk) => {
		$bK(source, (value) => {
			return $q([ 1 ], ($bl) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $bl ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $bl ]);
					advance(value, $bl);
				} else {
					set_chunk_pending(true, [ 0, $bl ]);
					__chunk_load(arm, () => {
						return $q([ 1 ], ($bC) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bC ]);
								advance(value, $bC);
							}
							return;
						});
					}, (reason) => {
						return $q([ 1 ], ($bD) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bD ]);
								$bE(chunk_error_signal, [ 0, reason ], [ 0, $bD ]);
							}
							return;
						});
					});
				}
				return;
			});
		}, $bb, $bk);
		return;
	};
	return [ __clone(source), render, [ 0, __clone(gated) ], armed, wire ];
}
function $cb(self) {
	const $cc = self;
	return $cc[0] === 1;
}
function $ct(self, content, end, $cu, $cv) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(content, [ __clone(staging) ], $cu, $cv);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function $cq(self, content, $cr, $cs) {
	return $ct(self, content, self[0], $cr, $cs);
}
function $cw(owner, body) {
	return body(owner);
}
function $bU(parent, source, render, armed, $bV, $bW) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($bW), () => {
		const $bX = live_owner.v;
		let $bY = null;
		if ($bX[0] === 1) {
			$bY = $bX;
		} else {
			$bY = [ 0, dispose2($bX[1]) ];
		}
		$bY;
		close(region);
		return;
	});
	$bK(source, (value) => {
		const $ch = last_value.v;
		let $ci = null;
		if ($ch[0] === 0) {
			const previous = $ch[1];
			$ci = eq(previous, value);
		} else {
			$ci = false;
		}
		const unchanged = $ci;
		if (armed.v && !(unchanged)) {
			const $cl = live_owner.v;
			let $cm = null;
			if ($cl[0] === 1) {
				$cm = $cl;
			} else {
				$cm = [ 0, dispose2($cl[1]) ];
			}
			$cm;
			const $cn = live_row.v;
			let $co = null;
			if ($cn[0] === 0) {
				const row = $cn[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$co = undefined;
			} else {
				$co = undefined;
			}
			$co;
			const owner = new3();
			const row2 = $cw(owner, ($cp) => {
				return $cq(region, render(value, $cp), $bV, $cp);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $bV, $bW);
}
function $bP(self, parent, $bQ, $bR) {
	self[4]($bR);
	const $bS = self[2];
	let $bT = null;
	if ($bS[0] === 0) {
		const gated = $bS[1];
		$bT = $bU(parent, gated, self[1], self[3], $bQ, $bR);
	} else {
		$bT = $bU(parent, self[0], self[1], self[3], $bQ, $bR);
	}
	return $bT;
}
function $bO(self, content, $an, $ao) {
	$bP(content, self, $an, $ao);
	return __clone(self);
}
function $cy(body) {
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
__vilan_chunks.fn.$Y = $Y;
__vilan_chunks.fn.$am = $am;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $u(current_path([ 1 ]), parse, [ 1 ], [ 1 ]);
mount_root("app", ($S) => {
	return app(route, [ 1 ], $S);
});
