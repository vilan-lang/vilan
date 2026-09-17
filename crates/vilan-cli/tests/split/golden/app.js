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
function home_page($aW, $aX) {
	return __vilan_chunks.fn.home_page($aW, $aX);
}
function docs_page(page, $ba, $bb) {
	return __vilan_chunks.fn.docs_page(page, $ba, $bb);
}
function not_found_page($be, $bf) {
	return __vilan_chunks.fn.not_found_page($be, $bf);
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
function dispose(self, $P) {
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
	const ambient = $P;
	const $S = ambient;
	let $T = null;
	if ($S[0] === 0) {
		const turn = $S[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$T = undefined;
	} else {
		$T = undefined;
	}
	$T;
	const $U = self[2].v;
	let $V = null;
	if ($U[0] === 0) {
		const release = $U[1];
		self[2].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$V = undefined;
	} else {
		$V = undefined;
	}
	return $V;
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
	let $cj = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $cd = __guarded(cleanup);
			let $ce = null;
			if ($cd[0] === 0) {
				const message = $cd[1];
				if ($cf(failure)) {
					failure = [ 0, message ];
				}
				$ce = undefined;
			} else {
				$ce = undefined;
			}
			$ce;
		}
		self[0].v = [  ];
		const $ch = failure;
		let $ci = null;
		if ($ch[0] === 0) {
			const message2 = $ch[1];
			$ci = (() => {
				throw message2;
			})();
		} else {
			$ci = undefined;
		}
		$cj = $ci;
	}
	return $cj;
}
function get_owner($aJ) {
	return $aJ;
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
function navigate(path, $ao) {
	ensure_wired($ao);
	history.pushState("", "", path);
	$f(path_signal, path, $ao);
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
	let $ab = null;
	if (is_svg_tag(tag)) {
		$ab = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$ab = [ document.createElement(tag) ];
	}
	return $ab;
}
function is_svg_tag(tag) {
	const $Z = tag;
	let $aa = null;
	if ($Z === "svg") {
		$aa = true;
	} else if ($Z === "path") {
		$aa = true;
	} else if ($Z === "circle") {
		$aa = true;
	} else if ($Z === "ellipse") {
		$aa = true;
	} else if ($Z === "rect") {
		$aa = true;
	} else if ($Z === "line") {
		$aa = true;
	} else if ($Z === "polyline") {
		$aa = true;
	} else if ($Z === "polygon") {
		$aa = true;
	} else if ($Z === "g") {
		$aa = true;
	} else if ($Z === "defs") {
		$aa = true;
	} else if ($Z === "use") {
		$aa = true;
	} else if ($Z === "symbol") {
		$aa = true;
	} else if ($Z === "marker") {
		$aa = true;
	} else if ($Z === "pattern") {
		$aa = true;
	} else if ($Z === "mask") {
		$aa = true;
	} else if ($Z === "clipPath") {
		$aa = true;
	} else if ($Z === "linearGradient") {
		$aa = true;
	} else if ($Z === "radialGradient") {
		$aa = true;
	} else if ($Z === "stop") {
		$aa = true;
	} else if ($Z === "text") {
		$aa = true;
	} else if ($Z === "tspan") {
		$aa = true;
	} else if ($Z === "textPath") {
		$aa = true;
	} else if ($Z === "filter") {
		$aa = true;
	} else if ($Z === "foreignObject") {
		$aa = true;
	} else if ($Z === "feGaussianBlur") {
		$aa = true;
	} else if ($Z === "feColorMatrix") {
		$aa = true;
	} else if ($Z === "feOffset") {
		$aa = true;
	} else if ($Z === "feMerge") {
		$aa = true;
	} else if ($Z === "feMergeNode") {
		$aa = true;
	} else if ($Z === "feFlood") {
		$aa = true;
	} else if ($Z === "feComposite") {
		$aa = true;
	} else if ($Z === "feBlend") {
		$aa = true;
	} else if ($Z === "feDropShadow") {
		$aa = true;
	} else {
		$aa = false;
	}
	return $aa;
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
		return $q([ 1 ], ($ap) => {
			return handler(dispatched, $ap);
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
function set_chunk_pending(busy, $bD) {
	if ($x(chunk_pending_signal) !== busy) {
		$bE(chunk_pending_signal, busy, $bD);
	}
}
function clear_chunk_error($bu) {
	const $bv = $x(chunk_error_signal);
	let $bw = null;
	if ($bv[0] === 0) {
		const _reason = $bv[1];
		$bw = $bx(chunk_error_signal, [ 1 ], $bu);
	} else {
		$bw = undefined;
	}
	return $bw;
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
		let $ck = null;
		if (at + 1 < rows.length) {
			$ck = __at(rows, at + 1)[0];
		} else {
			$ck = self[0];
		}
		const end = $ck;
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
	const $cE = $q([ 1 ], ($cB) => {
		return $cC(body);
	});
	const built = $cE[0];
	const root = $cE[1];
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
	const $ai = route2;
	let $aj = null;
	if ($ai[0] === 0) {
		$aj = "/";
	} else if ($ai[0] === 1) {
		const page = $ai[1];
		$aj = "/docs/" + page;
	} else {
		$aj = "/404";
	}
	return $aj;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $aY, $aZ) {
	return $aq($aq(view("section"), text(view("h2"), title), $aY, $aZ), text(view("p"), body), $aY, $aZ);
}
function app(route2, $X, $Y) {
	$bg(route2);
	return $bS($aq($aq($aq(view("main"), $aq($aq(view("nav"), $ac("Home", [ 0 ], $X, $Y), $X, $Y), $ac("Docs", [ 1, 1 ], $X, $Y), $X, $Y), $X, $Y), $aA(class2(view("p"), "pending"), $au(pending(), (busy) => {
		let $at = null;
		if (busy) {
			$at = "...";
		} else {
			$at = "";
		}
		return $at;
	}, $X, [ 0, $Y ]), $X, $Y), $X, $Y), $aA(class2(view("p"), "failed"), $aN(chunk_error(), (reason) => {
		const $aK = reason;
		let $aL = null;
		if ($aK[0] === 0) {
			const text2 = $aK[1];
			let $aM = null;
			if (text2.length > 0) {
				$aM = "!";
			} else {
				$aM = "?";
			}
			$aL = $aM;
		} else {
			$aL = "";
		}
		return $aL;
	}, $X, [ 0, $Y ]), $X, $Y), $X, $Y), $bi(route2, (current, $aT) => {
		const $aU = current;
		let $aV = null;
		if ($aU[0] === 0) {
			$aV = home_page($X, $aT);
		} else if ($aU[0] === 1) {
			const page = $aU[1];
			$aV = docs_page(page, $X, $aT);
		} else {
			$aV = not_found_page($X, $aT);
		}
		return $aV;
	}), $X, $Y);
}
function eq(self, other) {
	const $cn = [ self, other ];
	let $co = null;
	if ($cn[0][0] === 0 && $cn[1][0] === 0) {
		$co = true;
	} else if ($cn[0][0] === 1 && $cn[1][0] === 1) {
		const s0 = $cn[0][1];
		const o0 = $cn[1][1];
		$co = s0 === o0;
	} else if ($cn[0][0] === 2 && $cn[1][0] === 2) {
		$co = true;
	} else {
		$co = false;
	}
	return $co;
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
		const $H = [ 0, cell ];
		let $I = null;
		if ($H[0] === 0) {
			const live = $H[1];
			$I = observer(live.v);
		} else {
			$I = undefined;
		}
		return $I;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
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
function $ak(self, name, value, $al, $am) {
	apply(value, self, name, $al, $am);
	return __clone(self);
}
function $af(self, route2, $ag, $ah) {
	const path = to_path(route2);
	return on_event($ak($ak(self, "href", path, $ag, $ah), "draggable", "false", $ag, $ah), "click", (event, $an) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $an ]);
		}
		return;
	});
}
function $ac(label, route2, $ad, $ae) {
	return text($af(view("a"), route2, $ad, $ae), label);
}
function $aq(self, content, $ar, $as) {
	place(content, self, $ar, $as);
	return __clone(self);
}
function $ax(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		const $ay = [ 0, cell ];
		let $az = null;
		if ($ay[0] === 0) {
			const live = $ay[1];
			$az = observer(live.v);
		} else {
			$az = undefined;
		}
		return $az;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $aw(self, observer) {
	return $ax(self, observer);
}
function $au(self, transform, $v, $w) {
	const derived = $a(transform($x(self)));
	register_with_owner($aw(self, (value) => {
		$z(derived, transform(value), $v);
		return;
	}), $v, $w);
	return derived;
}
function $aG(self, observer, $aH, $aI) {
	$N(get_owner($aI), $F(self, observer), $aH);
}
function $aD(self, observer, $aE, $aF) {
	$aG(self, observer, $aE, $aF);
	observer($x(self));
}
function $aA(self, source, $aB, $aC) {
	const element = __clone(self[0]);
	$aD(source, (value) => {
		element.textContent = value;
		return;
	}, $aB, $aC);
	return __clone(self);
}
function $aQ(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		const $aR = [ 0, cell ];
		let $aS = null;
		if ($aR[0] === 0) {
			const live = $aR[1];
			$aS = observer(live.v);
		} else {
			$aS = undefined;
		}
		return $aS;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $aP(self, observer) {
	return $aQ(self, observer);
}
function $aN(self, transform, $v, $w) {
	const derived = $a(transform($x(self)));
	register_with_owner($aP(self, (value) => {
		$z(derived, transform(value), $v);
		return;
	}), $v, $w);
	return derived;
}
function $bg(source) {
	__chunk_preload(__chunk_arm($x(source)));
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
	self[0].v = __clone(value);
	$bn(self, $g);
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
	self[0].v = __clone(value);
	$by(self, $g);
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
function $bP(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		const $bQ = [ 0, cell ];
		let $bR = null;
		if ($bQ[0] === 0) {
			const live = $bQ[1];
			$bR = observer(live.v);
		} else {
			$bR = undefined;
		}
		return $bR;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $bO(self, observer) {
	return $bP(self, observer);
}
function $bN(self, observer, $aH, $aI) {
	$N(get_owner($aI), $bO(self, observer), $aH);
}
function $bM(self, observer, $aE, $aF) {
	$bN(self, observer, $aE, $aF);
	observer($x(self));
}
function $bi(source, render, $bj) {
	const gated = $a($x(source));
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value, $bl) => {
		armed.v = true;
		$bm(gated, value, [ 0, $bl ]);
		return;
	};
	const wire = ($bs) => {
		$bM(source, (value) => {
			return $q([ 1 ], ($bt) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $bt ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $bt ]);
					advance(value, $bt);
				} else {
					set_chunk_pending(true, [ 0, $bt ]);
					__chunk_load(arm, () => {
						return $q([ 1 ], ($bK) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bK ]);
								advance(value, $bK);
							}
							return;
						});
					}, (reason) => {
						return $q([ 1 ], ($bL) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bL ]);
								$bx(chunk_error_signal, [ 0, reason ], [ 0, $bL ]);
							}
							return;
						});
					});
				}
				return;
			});
		}, $bj, $bs);
		return;
	};
	return [ __clone(source), render, [ 0, __clone(gated) ], armed, wire ];
}
function $cf(self) {
	const $cg = self;
	return $cg[0] === 1;
}
function $cx(self, content, end, $cy, $cz) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(content, [ __clone(staging) ], $cy, $cz);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function $cu(self, content, $cv, $cw) {
	return $cx(self, content, self[0], $cv, $cw);
}
function $cA(owner, body) {
	return body(owner);
}
function $bY(parent, source, render, armed, $bZ, $ca) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($ca), () => {
		const $cb = live_owner.v;
		let $cc = null;
		if ($cb[0] === 1) {
			$cc = $cb;
		} else {
			$cc = [ 0, dispose2($cb[1]) ];
		}
		$cc;
		close(region);
		return;
	});
	$bM(source, (value) => {
		const $cl = last_value.v;
		let $cm = null;
		if ($cl[0] === 0) {
			const previous = $cl[1];
			$cm = eq(previous, value);
		} else {
			$cm = false;
		}
		const unchanged = $cm;
		if (armed.v && !(unchanged)) {
			const $cp = live_owner.v;
			let $cq = null;
			if ($cp[0] === 1) {
				$cq = $cp;
			} else {
				$cq = [ 0, dispose2($cp[1]) ];
			}
			$cq;
			const $cr = live_row.v;
			let $cs = null;
			if ($cr[0] === 0) {
				const row = $cr[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$cs = undefined;
			} else {
				$cs = undefined;
			}
			$cs;
			const owner = new3();
			const row2 = $cA(owner, ($ct) => {
				return $cu(region, render(value, $ct), $bZ, $ct);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $bZ, $ca);
}
function $bT(self, parent, $bU, $bV) {
	self[4]($bV);
	const $bW = self[2];
	let $bX = null;
	if ($bW[0] === 0) {
		const gated = $bW[1];
		$bX = $bY(parent, gated, self[1], self[3], $bU, $bV);
	} else {
		$bX = $bY(parent, self[0], self[1], self[3], $bU, $bV);
	}
	return $bX;
}
function $bS(self, content, $ar, $as) {
	$bT(content, self, $ar, $as);
	return __clone(self);
}
function $cC(body) {
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
__vilan_chunks.fn.$ac = $ac;
__vilan_chunks.fn.$aq = $aq;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $u(current_path([ 1 ]), parse, [ 1 ], [ 1 ]);
mount_root("app", ($W) => {
	return app(route, [ 1 ], $W);
});
