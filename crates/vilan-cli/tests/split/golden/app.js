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
function home_page($bc, $bd) {
	return __vilan_chunks.fn.home_page($bc, $bd);
}
function docs_page(page, $bg, $bh) {
	return __vilan_chunks.fn.docs_page(page, $bg, $bh);
}
function not_found_page($bk, $bl) {
	return __vilan_chunks.fn.not_found_page($bk, $bl);
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
function mint_subscriber(notify) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify, derived);
}
function subscriber_of(notify, derived) {
	return [ fresh_id(), notify, __shared_new(true), derived ];
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
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function dispose(self, $R) {
	const $S = $R;
	let $T = null;
	if ($S[0] === 0) {
		const established = $S[1];
		$T = [ 0, established ];
	} else {
		$T = $n(draining_turns.v);
	}
	const ambient = $T;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $U = [ 0, handle[0] ];
	let $V = null;
	if ($U[0] === 0) {
		const subscribers = $U[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$V = undefined;
	} else {
		$V = undefined;
	}
	$V;
	const $W = ambient;
	let $X = null;
	if ($W[0] === 0) {
		const turn = $W[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== handle[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[2].v.delete(hash(handle[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn[1].v) {
			if (subscriber3[0] !== handle[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn[1].v = kept_derived;
		turn[3].v.delete(hash(handle[1]));
		$X = undefined;
	} else {
		$X = undefined;
	}
	$X;
	const $Y = handle[3].v;
	let $Z = null;
	if ($Y[0] === 0) {
		const release = $Y[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$Z = undefined;
	} else {
		$Z = undefined;
	}
	return $Z;
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
	let $cq = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $ck = __guarded(cleanup);
			let $cl = null;
			if ($ck[0] === 0) {
				const message = $ck[1];
				if ($cm(failure)) {
					failure = [ 0, message ];
				}
				$cl = undefined;
			} else {
				$cl = undefined;
			}
			$cl;
		}
		self[0].v = [  ];
		const $co = failure;
		let $cp = null;
		if ($co[0] === 0) {
			const message2 = $co[1];
			$cp = (() => {
				throw message2;
			})();
		} else {
			$cp = undefined;
		}
		$cq = $cp;
	}
	return $cq;
}
function get_owner($aO) {
	return $aO;
}
function register_with_owner(subscription, $L, $M) {
	const $N = $M;
	let $O = null;
	if ($N[0] === 0) {
		const owner = $N[1];
		$O = $P(owner, subscription, $L);
	} else {
		$O = __clone(subscription);
	}
	return $O;
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
function navigate(path, $as) {
	ensure_wired($as);
	history.pushState("", "", path);
	$f(path_signal, path, $as);
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
	let $af = null;
	if (is_svg_tag(tag)) {
		$af = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$af = [ document.createElement(tag) ];
	}
	return $af;
}
function is_svg_tag(tag) {
	const $ad = tag;
	let $ae = null;
	if ($ad === "svg") {
		$ae = true;
	} else if ($ad === "path") {
		$ae = true;
	} else if ($ad === "circle") {
		$ae = true;
	} else if ($ad === "ellipse") {
		$ae = true;
	} else if ($ad === "rect") {
		$ae = true;
	} else if ($ad === "line") {
		$ae = true;
	} else if ($ad === "polyline") {
		$ae = true;
	} else if ($ad === "polygon") {
		$ae = true;
	} else if ($ad === "g") {
		$ae = true;
	} else if ($ad === "defs") {
		$ae = true;
	} else if ($ad === "use") {
		$ae = true;
	} else if ($ad === "symbol") {
		$ae = true;
	} else if ($ad === "marker") {
		$ae = true;
	} else if ($ad === "pattern") {
		$ae = true;
	} else if ($ad === "mask") {
		$ae = true;
	} else if ($ad === "clipPath") {
		$ae = true;
	} else if ($ad === "linearGradient") {
		$ae = true;
	} else if ($ad === "radialGradient") {
		$ae = true;
	} else if ($ad === "stop") {
		$ae = true;
	} else if ($ad === "text") {
		$ae = true;
	} else if ($ad === "tspan") {
		$ae = true;
	} else if ($ad === "textPath") {
		$ae = true;
	} else if ($ad === "filter") {
		$ae = true;
	} else if ($ad === "foreignObject") {
		$ae = true;
	} else if ($ad === "feGaussianBlur") {
		$ae = true;
	} else if ($ad === "feColorMatrix") {
		$ae = true;
	} else if ($ad === "feOffset") {
		$ae = true;
	} else if ($ad === "feMerge") {
		$ae = true;
	} else if ($ad === "feMergeNode") {
		$ae = true;
	} else if ($ad === "feFlood") {
		$ae = true;
	} else if ($ad === "feComposite") {
		$ae = true;
	} else if ($ad === "feBlend") {
		$ae = true;
	} else if ($ad === "feDropShadow") {
		$ae = true;
	} else {
		$ae = false;
	}
	return $ae;
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
		return $r([ 1 ], ($at) => {
			return handler(dispatched, $at);
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
function set_chunk_pending(busy, $bJ) {
	if ($y(chunk_pending_signal) !== busy) {
		$bs(chunk_pending_signal, busy, $bJ);
	}
}
function clear_chunk_error($bA) {
	const $bB = $y(chunk_error_signal);
	let $bC = null;
	if ($bB[0] === 0) {
		const _reason = $bB[1];
		$bC = $bs(chunk_error_signal, [ 1 ], $bA);
	} else {
		$bC = undefined;
	}
	return $bC;
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
		let $cr = null;
		if (at + 1 < rows.length) {
			$cr = __at(rows, at + 1)[0];
		} else {
			$cr = self[0];
		}
		const end = $cr;
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
	const $cL = $r([ 1 ], ($cI) => {
		return $cJ(body);
	});
	const built = $cL[0];
	const root = $cL[1];
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
	const $am = route2;
	let $an = null;
	if ($am[0] === 0) {
		$an = "/";
	} else if ($am[0] === 1) {
		const page = $am[1];
		$an = "/docs/" + page;
	} else {
		$an = "/404";
	}
	return $an;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $be, $bf) {
	return $au($au(view("section"), text(view("h2"), title), $be, $bf), text(view("p"), body), $be, $bf);
}
function app(route2, $ab, $ac) {
	$bm(route2);
	return $bZ($au($au($au(view("main"), $au($au(view("nav"), $ag("Home", [ 0 ], $ab, $ac), $ab, $ac), $ag("Docs", [ 1, 1 ], $ab, $ac), $ab, $ac), $ab, $ac), $aF(class2(view("p"), "pending"), $ay(pending(), (busy) => {
		let $ax = null;
		if (busy) {
			$ax = "...";
		} else {
			$ax = "";
		}
		return $ax;
	}, $ab, [ 0, $ac ]), $ab, $ac), $ab, $ac), $aF(class2(view("p"), "failed"), $aS(chunk_error(), (reason) => {
		const $aP = reason;
		let $aQ = null;
		if ($aP[0] === 0) {
			const text2 = $aP[1];
			let $aR = null;
			if (text2.length > 0) {
				$aR = "!";
			} else {
				$aR = "?";
			}
			$aQ = $aR;
		} else {
			$aQ = "";
		}
		return $aQ;
	}, $ab, [ 0, $ac ]), $ab, $ac), $ab, $ac), $bo(route2, (current, $aZ) => {
		const $ba = current;
		let $bb = null;
		if ($ba[0] === 0) {
			$bb = home_page($ab, $aZ);
		} else if ($ba[0] === 1) {
			const page = $ba[1];
			$bb = docs_page(page, $ab, $aZ);
		} else {
			$bb = not_found_page($ab, $aZ);
		}
		return $bb;
	}), $ab, $ac);
}
function eq(self, other) {
	const $cu = [ self, other ];
	let $cv = null;
	if ($cu[0][0] === 0 && $cu[1][0] === 0) {
		$cv = true;
	} else if ($cu[0][0] === 1 && $cu[1][0] === 1) {
		const s0 = $cu[0][1];
		const o0 = $cu[1][1];
		$cv = s0 === o0;
	} else if ($cu[0][0] === 2 && $cu[1][0] === 2) {
		$cv = true;
	} else {
		$cv = false;
	}
	return $cv;
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
function $K(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $H(signal, observer) {
	const cell = signal[0];
	return $K(signal, mint_subscriber(() => {
		const $I = [ 0, cell ];
		let $J = null;
		if ($I[0] === 0) {
			const live = $I[1];
			$J = observer(live.v);
		} else {
			$J = undefined;
		}
		return $J;
	}));
}
function $G(self, observer) {
	return $H(self, observer);
}
function $P(self, item, $Q) {
	if (self[1].v) {
		dispose(item, $Q);
	} else {
		self[0].v.push(() => {
			dispose(item, $Q);
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
function $ao(self, name, value, $ap, $aq) {
	apply(value, self, name, $ap, $aq);
	return __clone(self);
}
function $aj(self, route2, $ak, $al) {
	const path = to_path(route2);
	return on_event($ao($ao(self, "href", path, $ak, $al), "draggable", "false", $ak, $al), "click", (event, $ar) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $ar ]);
		}
		return;
	});
}
function $ag(label, route2, $ah, $ai) {
	return text($aj(view("a"), route2, $ah, $ai), label);
}
function $au(self, content, $av, $aw) {
	place(content, self, $av, $aw);
	return __clone(self);
}
function $aB(signal, observer) {
	const cell = signal[0];
	return $K(signal, mint_subscriber(() => {
		const $aC = [ 0, cell ];
		let $aD = null;
		if ($aC[0] === 0) {
			const live = $aC[1];
			$aD = observer(live.v);
		} else {
			$aD = undefined;
		}
		return $aD;
	}));
}
function $aA(self, observer) {
	return $aB(self, observer);
}
function $ay(self, transform, $w, $x) {
	const derived = $a(transform($y(self)));
	as_derivation();
	register_with_owner($aA(self, (value) => {
		$A(derived, transform(value), $w);
		return;
	}), $w, $x);
	return derived;
}
function $aL(self, observer, $aM, $aN) {
	$P(get_owner($aN), $G(self, observer), $aM);
}
function $aI(self, observer, $aJ, $aK) {
	$aL(self, observer, $aJ, $aK);
	observer($y(self));
}
function $aF(self, source, $aG, $aH) {
	const element = __clone(self[0]);
	$aI(source, (value) => {
		element.textContent = value;
		return;
	}, $aG, $aH);
	return __clone(self);
}
function $aU(self, observer) {
	return $aB(self, observer);
}
function $aS(self, transform, $w, $x) {
	const derived = $a(transform($y(self)));
	as_derivation();
	register_with_owner($aU(self, (value) => {
		$A(derived, transform(value), $w);
		return;
	}), $w, $x);
	return derived;
}
function $bm(source) {
	__chunk_preload(__chunk_arm($y(source)));
}
function $bs(self, value, $g) {
	self[0].v = __clone(value);
	$B(self, $g);
}
function $bT(self, observer, $aM, $aN) {
	$P(get_owner($aN), $aU(self, observer), $aM);
}
function $bS(self, observer, $aJ, $aK) {
	$bT(self, observer, $aJ, $aK);
	observer($y(self));
}
function $bo(source, render, $bp) {
	const gated = $a($y(source));
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value, $br) => {
		armed.v = true;
		$bs(gated, value, [ 0, $br ]);
		return;
	};
	const wire = ($by) => {
		$bS(source, (value) => {
			return $r([ 1 ], ($bz) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $bz ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $bz ]);
					advance(value, $bz);
				} else {
					set_chunk_pending(true, [ 0, $bz ]);
					__chunk_load(arm, () => {
						return $r([ 1 ], ($bQ) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bQ ]);
								advance(value, $bQ);
							}
							return;
						});
					}, (reason) => {
						return $r([ 1 ], ($bR) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bR ]);
								$bs(chunk_error_signal, [ 0, reason ], [ 0, $bR ]);
							}
							return;
						});
					});
				}
				return;
			});
		}, $bp, $by);
		return;
	};
	return [ __clone(source), render, [ 0, __clone(gated) ], armed, wire ];
}
function $cm(self) {
	const $cn = self;
	return $cn[0] === 1;
}
function $cE(self, content, end, $cF, $cG) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(content, [ __clone(staging) ], $cF, $cG);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function $cB(self, content, $cC, $cD) {
	return $cE(self, content, self[0], $cC, $cD);
}
function $cH(owner, body) {
	return body(owner);
}
function $cf(parent, source, render, armed, $cg, $ch) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($ch), () => {
		const $ci = live_owner.v;
		let $cj = null;
		if ($ci[0] === 1) {
			$cj = $ci;
		} else {
			$cj = [ 0, dispose2($ci[1]) ];
		}
		$cj;
		close(region);
		return;
	});
	$bS(source, (value) => {
		const $cs = last_value.v;
		let $ct = null;
		if ($cs[0] === 0) {
			const previous = $cs[1];
			$ct = eq(previous, value);
		} else {
			$ct = false;
		}
		const unchanged = $ct;
		if (armed.v && !(unchanged)) {
			const $cw = live_owner.v;
			let $cx = null;
			if ($cw[0] === 1) {
				$cx = $cw;
			} else {
				$cx = [ 0, dispose2($cw[1]) ];
			}
			$cx;
			const $cy = live_row.v;
			let $cz = null;
			if ($cy[0] === 0) {
				const row = $cy[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$cz = undefined;
			} else {
				$cz = undefined;
			}
			$cz;
			const owner = new3();
			const row2 = $cH(owner, ($cA) => {
				return $cB(region, render(value, $cA), $cg, $cA);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $cg, $ch);
}
function $ca(self, parent, $cb, $cc) {
	self[4]($cc);
	const $cd = self[2];
	let $ce = null;
	if ($cd[0] === 0) {
		const gated = $cd[1];
		$ce = $cf(parent, gated, self[1], self[3], $cb, $cc);
	} else {
		$ce = $cf(parent, self[0], self[1], self[3], $cb, $cc);
	}
	return $ce;
}
function $bZ(self, content, $av, $aw) {
	$ca(content, self, $av, $aw);
	return __clone(self);
}
function $cJ(body) {
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
__vilan_chunks.fn.$ag = $ag;
__vilan_chunks.fn.$au = $au;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $v(current_path([ 1 ]), parse, [ 1 ], [ 1 ]);
mount_root("app", ($aa) => {
	return app(route, [ 1 ], $aa);
});
