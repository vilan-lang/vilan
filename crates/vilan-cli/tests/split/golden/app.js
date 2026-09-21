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
function home_page($aZ, $ba) {
	return __vilan_chunks.fn.home_page($aZ, $ba);
}
function docs_page(page, $bd, $be) {
	return __vilan_chunks.fn.docs_page(page, $bd, $be);
}
function not_found_page($bh, $bi) {
	return __vilan_chunks.fn.not_found_page($bh, $bi);
}
function hash(self) {
	return __hash(self);
}
function as_derivation() {
	minting_derivation.v = true;
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
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
function dispose(self, $Q) {
	self[2].v = false;
	const $R = [ 0, self[0] ];
	let $S = null;
	if ($R[0] === 0) {
		const subscribers = $R[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$S = undefined;
	} else {
		$S = undefined;
	}
	$S;
	const $T = $Q;
	let $U = null;
	if ($T[0] === 0) {
		const established = $T[1];
		$U = [ 0, established ];
	} else {
		$U = $n(draining_turns.v);
	}
	const ambient = $U;
	const $V = ambient;
	let $W = null;
	if ($V[0] === 0) {
		const turn = $V[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[2].v.delete(hash(self[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn[1].v) {
			if (subscriber3[0] !== self[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn[1].v = kept_derived;
		turn[3].v.delete(hash(self[1]));
		$W = undefined;
	} else {
		$W = undefined;
	}
	$W;
	const $X = self[3].v;
	let $Y = null;
	if ($X[0] === 0) {
		const release = $X[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$Y = undefined;
	} else {
		$Y = undefined;
	}
	return $Y;
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
	let $cm = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $cg = __guarded(cleanup);
			let $ch = null;
			if ($cg[0] === 0) {
				const message = $cg[1];
				if ($ci(failure)) {
					failure = [ 0, message ];
				}
				$ch = undefined;
			} else {
				$ch = undefined;
			}
			$ch;
		}
		self[0].v = [  ];
		const $ck = failure;
		let $cl = null;
		if ($ck[0] === 0) {
			const message2 = $ck[1];
			$cl = (() => {
				throw message2;
			})();
		} else {
			$cl = undefined;
		}
		$cm = $cl;
	}
	return $cm;
}
function get_owner($aM) {
	return $aM;
}
function register_with_owner(subscription, $K, $L) {
	const $M = $L;
	let $N = null;
	if ($M[0] === 0) {
		const owner = $M[1];
		$N = $O(owner, subscription, $K);
	} else {
		$N = __clone(subscription);
	}
	return $N;
}
function ensure_wired($e) {
	if (!(wired.v)) {
		wired.v = true;
		$f(path_signal, __router_path(), $e);
		__dom_window().addEventListener("popstate", () => {
			return $r([ 1 ], ($q) => {
				$f(path_signal, __router_path(), [ 0, $q ]);
				return;
			});
		});
	}
}
function current_path($d) {
	ensure_wired($d);
	return path_signal;
}
function navigate(path, $ar) {
	ensure_wired($ar);
	history.pushState("", "", path);
	$f(path_signal, path, $ar);
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
	let $ae = null;
	if (is_svg_tag(tag)) {
		$ae = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$ae = [ document.createElement(tag) ];
	}
	return $ae;
}
function is_svg_tag(tag) {
	const $ac = tag;
	let $ad = null;
	if ($ac === "svg") {
		$ad = true;
	} else if ($ac === "path") {
		$ad = true;
	} else if ($ac === "circle") {
		$ad = true;
	} else if ($ac === "ellipse") {
		$ad = true;
	} else if ($ac === "rect") {
		$ad = true;
	} else if ($ac === "line") {
		$ad = true;
	} else if ($ac === "polyline") {
		$ad = true;
	} else if ($ac === "polygon") {
		$ad = true;
	} else if ($ac === "g") {
		$ad = true;
	} else if ($ac === "defs") {
		$ad = true;
	} else if ($ac === "use") {
		$ad = true;
	} else if ($ac === "symbol") {
		$ad = true;
	} else if ($ac === "marker") {
		$ad = true;
	} else if ($ac === "pattern") {
		$ad = true;
	} else if ($ac === "mask") {
		$ad = true;
	} else if ($ac === "clipPath") {
		$ad = true;
	} else if ($ac === "linearGradient") {
		$ad = true;
	} else if ($ac === "radialGradient") {
		$ad = true;
	} else if ($ac === "stop") {
		$ad = true;
	} else if ($ac === "text") {
		$ad = true;
	} else if ($ac === "tspan") {
		$ad = true;
	} else if ($ac === "textPath") {
		$ad = true;
	} else if ($ac === "filter") {
		$ad = true;
	} else if ($ac === "foreignObject") {
		$ad = true;
	} else if ($ac === "feGaussianBlur") {
		$ad = true;
	} else if ($ac === "feColorMatrix") {
		$ad = true;
	} else if ($ac === "feOffset") {
		$ad = true;
	} else if ($ac === "feMerge") {
		$ad = true;
	} else if ($ac === "feMergeNode") {
		$ad = true;
	} else if ($ac === "feFlood") {
		$ad = true;
	} else if ($ac === "feComposite") {
		$ad = true;
	} else if ($ac === "feBlend") {
		$ad = true;
	} else if ($ac === "feDropShadow") {
		$ad = true;
	} else {
		$ad = false;
	}
	return $ad;
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
		return $r([ 1 ], ($as) => {
			return handler(dispatched, $as);
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
function set_chunk_pending(busy, $bG) {
	if ($y(chunk_pending_signal) !== busy) {
		$bp(chunk_pending_signal, busy, $bG);
	}
}
function clear_chunk_error($bx) {
	const $by = $y(chunk_error_signal);
	let $bz = null;
	if ($by[0] === 0) {
		const _reason = $by[1];
		$bz = $bp(chunk_error_signal, [ 1 ], $bx);
	} else {
		$bz = undefined;
	}
	return $bz;
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
		let $cn = null;
		if (at + 1 < rows.length) {
			$cn = __at(rows, at + 1)[0];
		} else {
			$cn = self[0];
		}
		const end = $cn;
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
	const $cH = $r([ 1 ], ($cE) => {
		return $cF(body);
	});
	const built = $cH[0];
	const root = $cH[1];
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
	let $u = null;
	if (__at(parts, 0) === "docs" && parts.length === 2) {
		const $s = __parse_i32(__at(parts, 1));
		let $t = null;
		if ($s[0] === 0) {
			const page = $s[1];
			return [ 1, page ];
		} else {
			$t = undefined;
		}
		$u = $t;
	}
	$u;
	return [ 2 ];
}
function href(route2) {
	const $al = route2;
	let $am = null;
	if ($al[0] === 0) {
		$am = "/";
	} else if ($al[0] === 1) {
		const page = $al[1];
		$am = "/docs/" + page;
	} else {
		$am = "/404";
	}
	return $am;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $bb, $bc) {
	return $at($at(view("section"), text(view("h2"), title), $bb, $bc), text(view("p"), body), $bb, $bc);
}
function app(route2, $aa, $ab) {
	$bj(route2);
	return $bV($at($at($at(view("main"), $at($at(view("nav"), $af("Home", [ 0 ], $aa, $ab), $aa, $ab), $af("Docs", [ 1, 1 ], $aa, $ab), $aa, $ab), $aa, $ab), $aD(class2(view("p"), "pending"), $ax(pending(), (busy) => {
		let $aw = null;
		if (busy) {
			$aw = "...";
		} else {
			$aw = "";
		}
		return $aw;
	}, $aa, [ 0, $ab ]), $aa, $ab), $aa, $ab), $aD(class2(view("p"), "failed"), $aQ(chunk_error(), (reason) => {
		const $aN = reason;
		let $aO = null;
		if ($aN[0] === 0) {
			const text2 = $aN[1];
			let $aP = null;
			if (text2.length > 0) {
				$aP = "!";
			} else {
				$aP = "?";
			}
			$aO = $aP;
		} else {
			$aO = "";
		}
		return $aO;
	}, $aa, [ 0, $ab ]), $aa, $ab), $aa, $ab), $bl(route2, (current, $aW) => {
		const $aX = current;
		let $aY = null;
		if ($aX[0] === 0) {
			$aY = home_page($aa, $aW);
		} else if ($aX[0] === 1) {
			const page = $aX[1];
			$aY = docs_page(page, $aa, $aW);
		} else {
			$aY = not_found_page($aa, $aW);
		}
		return $aY;
	}), $aa, $ab);
}
function eq(self, other) {
	const $cq = [ self, other ];
	let $cr = null;
	if ($cq[0][0] === 0 && $cq[1][0] === 0) {
		$cr = true;
	} else if ($cq[0][0] === 1 && $cq[1][0] === 1) {
		const s0 = $cq[0][1];
		const o0 = $cq[1][1];
		$cr = s0 === o0;
	} else if ($cq[0][0] === 2 && $cq[1][0] === 2) {
		$cr = true;
	} else {
		$cr = false;
	}
	return $cr;
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $m(self) {
	return self.length === 0;
}
function $n(self) {
	return __list_get(self, self.length - 1);
}
function $h(self, $i) {
	const $j = $i;
	let $k = null;
	if ($j[0] === 0) {
		const turn = $j[1];
		$k = enqueue(turn, self[1].v);
	} else {
		const $o = $n(draining_turns.v);
		let $p = null;
		if ($o[0] === 0) {
			const draining = $o[1];
			$p = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$p = undefined;
		}
		$k = $p;
	}
	return $k;
}
function $f(self, value, $g) {
	self[0].v = __clone(value);
	$h(self, $g);
}
function $r(policy, body) {
	const fresh = new2();
	const result = body(fresh);
	drain(fresh);
	fresh[5].v = true;
	return result;
}
function $y(self) {
	return __clone(self[0].v);
}
function $B(self, $i) {
	const $C = $i;
	let $D = null;
	if ($C[0] === 0) {
		const turn = $C[1];
		$D = enqueue(turn, self[1].v);
	} else {
		const $E = $n(draining_turns.v);
		let $F = null;
		if ($E[0] === 0) {
			const draining = $E[1];
			$F = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$F = undefined;
		}
		$D = $F;
	}
	return $D;
}
function $A(self, value, $g) {
	self[0].v = __clone(value);
	$B(self, $g);
}
function $H(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	signal[1].v.push([ id, () => {
		const $I = [ 0, cell ];
		let $J = null;
		if ($I[0] === 0) {
			const live2 = $I[1];
			$J = observer(live2.v);
		} else {
			$J = undefined;
		}
		return $J;
	}, live, derived ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $G(self, observer) {
	return $H(self, observer);
}
function $O(self, item, $P) {
	if (self[1].v) {
		dispose(item, $P);
	} else {
		self[0].v.push(() => {
			dispose(item, $P);
			return;
		});
	}
	return __clone(item);
}
function $v(self, transform, $w, $x) {
	const derived = $a(transform($y(self)));
	as_derivation();
	register_with_owner($G(self, (value) => {
		$A(derived, transform(value), $w);
		return;
	}), $w, $x);
	return derived;
}
function $an(self, name, value, $ao, $ap) {
	apply(value, self, name, $ao, $ap);
	return __clone(self);
}
function $ai(self, route2, $aj, $ak) {
	const path = to_path(route2);
	return on_event($an($an(self, "href", path, $aj, $ak), "draggable", "false", $aj, $ak), "click", (event, $aq) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $aq ]);
		}
		return;
	});
}
function $af(label, route2, $ag, $ah) {
	return text($ai(view("a"), route2, $ag, $ah), label);
}
function $at(self, content, $au, $av) {
	place(content, self, $au, $av);
	return __clone(self);
}
function $az(self, observer) {
	return $H(self, observer);
}
function $ax(self, transform, $w, $x) {
	const derived = $a(transform($y(self)));
	as_derivation();
	register_with_owner($az(self, (value) => {
		$A(derived, transform(value), $w);
		return;
	}), $w, $x);
	return derived;
}
function $aJ(self, observer, $aK, $aL) {
	$O(get_owner($aL), $G(self, observer), $aK);
}
function $aG(self, observer, $aH, $aI) {
	$aJ(self, observer, $aH, $aI);
	observer($y(self));
}
function $aD(self, source, $aE, $aF) {
	const element = __clone(self[0]);
	$aG(source, (value) => {
		element.textContent = value;
		return;
	}, $aE, $aF);
	return __clone(self);
}
function $aQ(self, transform, $w, $x) {
	const derived = $a(transform($y(self)));
	as_derivation();
	register_with_owner($az(self, (value) => {
		$A(derived, transform(value), $w);
		return;
	}), $w, $x);
	return derived;
}
function $bj(source) {
	__chunk_preload(__chunk_arm($y(source)));
}
function $bp(self, value, $g) {
	self[0].v = __clone(value);
	$B(self, $g);
}
function $bQ(self, observer, $aK, $aL) {
	$O(get_owner($aL), $az(self, observer), $aK);
}
function $bP(self, observer, $aH, $aI) {
	$bQ(self, observer, $aH, $aI);
	observer($y(self));
}
function $bl(source, render, $bm) {
	const gated = $a($y(source));
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value, $bo) => {
		armed.v = true;
		$bp(gated, value, [ 0, $bo ]);
		return;
	};
	const wire = ($bv) => {
		$bP(source, (value) => {
			return $r([ 1 ], ($bw) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $bw ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $bw ]);
					advance(value, $bw);
				} else {
					set_chunk_pending(true, [ 0, $bw ]);
					__chunk_load(arm, () => {
						return $r([ 1 ], ($bN) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bN ]);
								advance(value, $bN);
							}
							return;
						});
					}, (reason) => {
						return $r([ 1 ], ($bO) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bO ]);
								$bp(chunk_error_signal, [ 0, reason ], [ 0, $bO ]);
							}
							return;
						});
					});
				}
				return;
			});
		}, $bm, $bv);
		return;
	};
	return [ __clone(source), render, [ 0, __clone(gated) ], armed, wire ];
}
function $ci(self) {
	const $cj = self;
	return $cj[0] === 1;
}
function $cA(self, content, end, $cB, $cC) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(content, [ __clone(staging) ], $cB, $cC);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function $cx(self, content, $cy, $cz) {
	return $cA(self, content, self[0], $cy, $cz);
}
function $cD(owner, body) {
	return body(owner);
}
function $cb(parent, source, render, armed, $cc, $cd) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($cd), () => {
		const $ce = live_owner.v;
		let $cf = null;
		if ($ce[0] === 1) {
			$cf = $ce;
		} else {
			$cf = [ 0, dispose2($ce[1]) ];
		}
		$cf;
		close(region);
		return;
	});
	$bP(source, (value) => {
		const $co = last_value.v;
		let $cp = null;
		if ($co[0] === 0) {
			const previous = $co[1];
			$cp = eq(previous, value);
		} else {
			$cp = false;
		}
		const unchanged = $cp;
		if (armed.v && !(unchanged)) {
			const $cs = live_owner.v;
			let $ct = null;
			if ($cs[0] === 1) {
				$ct = $cs;
			} else {
				$ct = [ 0, dispose2($cs[1]) ];
			}
			$ct;
			const $cu = live_row.v;
			let $cv = null;
			if ($cu[0] === 0) {
				const row = $cu[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$cv = undefined;
			} else {
				$cv = undefined;
			}
			$cv;
			const owner = new3();
			const row2 = $cD(owner, ($cw) => {
				return $cx(region, render(value, $cw), $cc, $cw);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $cc, $cd);
}
function $bW(self, parent, $bX, $bY) {
	self[4]($bY);
	const $bZ = self[2];
	let $ca = null;
	if ($bZ[0] === 0) {
		const gated = $bZ[1];
		$ca = $cb(parent, gated, self[1], self[3], $bX, $bY);
	} else {
		$ca = $cb(parent, self[0], self[1], self[3], $bX, $bY);
	}
	return $ca;
}
function $bV(self, content, $au, $av) {
	$bW(content, self, $au, $av);
	return __clone(self);
}
function $cF(body) {
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
__vilan_chunks.fn.$af = $af;
__vilan_chunks.fn.$at = $at;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $v(current_path([ 1 ]), parse, [ 1 ], [ 1 ]);
mount_root("app", ($Z) => {
	return app(route, [ 1 ], $Z);
});
