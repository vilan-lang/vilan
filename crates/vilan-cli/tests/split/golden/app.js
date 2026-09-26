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
function home_page($be, $bf) {
	return __vilan_chunks.fn.home_page($be, $bf);
}
function docs_page(page, $bi, $bj) {
	return __vilan_chunks.fn.docs_page(page, $bi, $bj);
}
function not_found_page($bm, $bn) {
	return __vilan_chunks.fn.not_found_page($bm, $bn);
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
function dispose(self, $T) {
	const $U = $T;
	let $V = null;
	if ($U[0] === 0) {
		const established = $U[1];
		$V = [ 0, established ];
	} else {
		$V = $n(draining_turns.v);
	}
	const ambient = $V;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $W = [ 0, handle[0] ];
	let $X = null;
	if ($W[0] === 0) {
		const subscribers = $W[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$X = undefined;
	} else {
		$X = undefined;
	}
	$X;
	const $Y = ambient;
	let $Z = null;
	if ($Y[0] === 0) {
		const turn = $Y[1];
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
		$Z = undefined;
	} else {
		$Z = undefined;
	}
	$Z;
	const $aa = handle[3].v;
	let $ab = null;
	if ($aa[0] === 0) {
		const release = $aa[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ab = undefined;
	} else {
		$ab = undefined;
	}
	return $ab;
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
	let $cs = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $cm = __guarded(cleanup);
			let $cn = null;
			if ($cm[0] === 0) {
				const message = $cm[1];
				if ($co(failure)) {
					failure = [ 0, message ];
				}
				$cn = undefined;
			} else {
				$cn = undefined;
			}
			$cn;
		}
		self[0].v = [  ];
		const $cq = failure;
		let $cr = null;
		if ($cq[0] === 0) {
			const message2 = $cq[1];
			$cr = (() => {
				throw message2;
			})();
		} else {
			$cr = undefined;
		}
		$cs = $cr;
	}
	return $cs;
}
function get_owner($aQ) {
	return $aQ;
}
function register_with_owner(subscription, $N, $O) {
	const $P = $O;
	let $Q = null;
	if ($P[0] === 0) {
		const owner = $P[1];
		$Q = $R(owner, subscription, $N);
	} else {
		$Q = __clone(subscription);
	}
	return $Q;
}
function ensure_wired($e) {
	if (!(wired.v)) {
		wired.v = true;
		$f(path_signal, __router_path(), $e);
		__dom_window().addEventListener("popstate", () => {
			return $t([ 1 ], ($s) => {
				$f(path_signal, __router_path(), [ 0, $s ]);
				return;
			});
		});
	}
}
function current_path($d) {
	ensure_wired($d);
	return path_signal;
}
function navigate(path, $au) {
	ensure_wired($au);
	history.pushState("", "", path);
	$f(path_signal, path, $au);
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
	let $ah = null;
	if (is_svg_tag(tag)) {
		$ah = [ document.createElementNS("http://www.w3.org/2000/svg", tag) ];
	} else {
		$ah = [ document.createElement(tag) ];
	}
	return $ah;
}
function is_svg_tag(tag) {
	const $af = tag;
	let $ag = null;
	if ($af === "svg") {
		$ag = true;
	} else if ($af === "path") {
		$ag = true;
	} else if ($af === "circle") {
		$ag = true;
	} else if ($af === "ellipse") {
		$ag = true;
	} else if ($af === "rect") {
		$ag = true;
	} else if ($af === "line") {
		$ag = true;
	} else if ($af === "polyline") {
		$ag = true;
	} else if ($af === "polygon") {
		$ag = true;
	} else if ($af === "g") {
		$ag = true;
	} else if ($af === "defs") {
		$ag = true;
	} else if ($af === "use") {
		$ag = true;
	} else if ($af === "symbol") {
		$ag = true;
	} else if ($af === "marker") {
		$ag = true;
	} else if ($af === "pattern") {
		$ag = true;
	} else if ($af === "mask") {
		$ag = true;
	} else if ($af === "clipPath") {
		$ag = true;
	} else if ($af === "linearGradient") {
		$ag = true;
	} else if ($af === "radialGradient") {
		$ag = true;
	} else if ($af === "stop") {
		$ag = true;
	} else if ($af === "text") {
		$ag = true;
	} else if ($af === "tspan") {
		$ag = true;
	} else if ($af === "textPath") {
		$ag = true;
	} else if ($af === "filter") {
		$ag = true;
	} else if ($af === "foreignObject") {
		$ag = true;
	} else if ($af === "feGaussianBlur") {
		$ag = true;
	} else if ($af === "feColorMatrix") {
		$ag = true;
	} else if ($af === "feOffset") {
		$ag = true;
	} else if ($af === "feMerge") {
		$ag = true;
	} else if ($af === "feMergeNode") {
		$ag = true;
	} else if ($af === "feFlood") {
		$ag = true;
	} else if ($af === "feComposite") {
		$ag = true;
	} else if ($af === "feBlend") {
		$ag = true;
	} else if ($af === "feDropShadow") {
		$ag = true;
	} else {
		$ag = false;
	}
	return $ag;
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
		return $t([ 1 ], ($av) => {
			return handler(dispatched, $av);
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
function set_chunk_pending(busy, $bL) {
	if ($A(chunk_pending_signal) !== busy) {
		$bu(chunk_pending_signal, busy, $bL);
	}
}
function clear_chunk_error($bC) {
	const $bD = $A(chunk_error_signal);
	let $bE = null;
	if ($bD[0] === 0) {
		const _reason = $bD[1];
		$bE = $bu(chunk_error_signal, [ 1 ], $bC);
	} else {
		$bE = undefined;
	}
	return $bE;
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
		let $ct = null;
		if (at + 1 < rows.length) {
			$ct = __at(rows, at + 1)[0];
		} else {
			$ct = self[0];
		}
		const end = $ct;
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
	const $cN = $t([ 1 ], ($cK) => {
		return $cL(body);
	});
	const built = $cN[0];
	const root = $cN[1];
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
	let $w = null;
	if (__at(parts, 0) === "docs" && parts.length === 2) {
		const $u = __parse_i32(__at(parts, 1));
		let $v = null;
		if ($u[0] === 0) {
			const page = $u[1];
			return [ 1, page ];
		} else {
			$v = undefined;
		}
		$w = $v;
	}
	$w;
	return [ 2 ];
}
function href(route2) {
	const $ao = route2;
	let $ap = null;
	if ($ao[0] === 0) {
		$ap = "/";
	} else if ($ao[0] === 1) {
		const page = $ao[1];
		$ap = "/docs/" + page;
	} else {
		$ap = "/404";
	}
	return $ap;
}
function to_path(self) {
	return href(self);
}
function announce(name, value) {
	console.log("init " + name + "=" + value);
	return value;
}
function panel(title, body, $bg, $bh) {
	return $aw($aw(view("section"), text(view("h2"), title), $bg, $bh), text(view("p"), body), $bg, $bh);
}
function app(route2, $ad, $ae) {
	$bo(route2);
	return $cb($aw($aw($aw(view("main"), $aw($aw(view("nav"), $ai("Home", [ 0 ], $ad, $ae), $ad, $ae), $ai("Docs", [ 1, 1 ], $ad, $ae), $ad, $ae), $ad, $ae), $aH(class2(view("p"), "pending"), $aA(pending(), (busy) => {
		let $az = null;
		if (busy) {
			$az = "...";
		} else {
			$az = "";
		}
		return $az;
	}, $ad, [ 0, $ae ]), $ad, $ae), $ad, $ae), $aH(class2(view("p"), "failed"), $aU(chunk_error(), (reason) => {
		const $aR = reason;
		let $aS = null;
		if ($aR[0] === 0) {
			const text2 = $aR[1];
			let $aT = null;
			if (text2.length > 0) {
				$aT = "!";
			} else {
				$aT = "?";
			}
			$aS = $aT;
		} else {
			$aS = "";
		}
		return $aS;
	}, $ad, [ 0, $ae ]), $ad, $ae), $ad, $ae), $bq(route2, (current, $bb) => {
		const $bc = current;
		let $bd = null;
		if ($bc[0] === 0) {
			$bd = home_page($ad, $bb);
		} else if ($bc[0] === 1) {
			const page = $bc[1];
			$bd = docs_page(page, $ad, $bb);
		} else {
			$bd = not_found_page($ad, $bb);
		}
		return $bd;
	}), $ad, $ae);
}
function eq(self, other) {
	const $cw = [ self, other ];
	let $cx = null;
	if ($cw[0][0] === 0 && $cw[1][0] === 0) {
		$cx = true;
	} else if ($cw[0][0] === 1 && $cw[1][0] === 1) {
		const s0 = $cw[0][1];
		const o0 = $cw[1][1];
		$cx = s0 === o0;
	} else if ($cw[0][0] === 2 && $cw[1][0] === 2) {
		$cx = true;
	} else {
		$cx = false;
	}
	return $cx;
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $m(self) {
	return self.length === 0;
}
function $n(self) {
	let $p = null;
	if ($m(self)) {
		$p = [ 1 ];
	} else {
		$p = __list_get(self, self.length - 1);
	}
	return $p;
}
function $h(self, $i) {
	const $j = $i;
	let $k = null;
	if ($j[0] === 0) {
		const turn = $j[1];
		$k = enqueue(turn, self[1].v);
	} else {
		const $q = $n(draining_turns.v);
		let $r = null;
		if ($q[0] === 0) {
			const draining = $q[1];
			$r = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$r = undefined;
		}
		$k = $r;
	}
	return $k;
}
function $f(self, value, $g) {
	self[0].v = __clone(value);
	$h(self, $g);
}
function $t(policy, body) {
	const fresh = new2();
	const result = body(fresh);
	drain(fresh);
	fresh[5].v = true;
	return result;
}
function $A(self) {
	return __clone(self[0].v);
}
function $D(self, $i) {
	const $E = $i;
	let $F = null;
	if ($E[0] === 0) {
		const turn = $E[1];
		$F = enqueue(turn, self[1].v);
	} else {
		const $G = $n(draining_turns.v);
		let $H = null;
		if ($G[0] === 0) {
			const draining = $G[1];
			$H = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$H = undefined;
		}
		$F = $H;
	}
	return $F;
}
function $C(self, value, $g) {
	self[0].v = __clone(value);
	$D(self, $g);
}
function $M(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $J(signal, observer) {
	const cell = signal[0];
	return $M(signal, mint_subscriber(() => {
		const $K = [ 0, cell ];
		let $L = null;
		if ($K[0] === 0) {
			const live = $K[1];
			$L = observer(live.v);
		} else {
			$L = undefined;
		}
		return $L;
	}));
}
function $I(self, observer) {
	return $J(self, observer);
}
function $R(self, item, $S) {
	if (self[1].v) {
		dispose(item, $S);
	} else {
		self[0].v.push(() => {
			dispose(item, $S);
			return;
		});
	}
	return __clone(item);
}
function $x(self, transform, $y, $z) {
	const derived = $a(transform($A(self)));
	as_derivation();
	register_with_owner($I(self, (value) => {
		$C(derived, transform(value), $y);
		return;
	}), $y, $z);
	return derived;
}
function $aq(self, name, value, $ar, $as) {
	apply(value, self, name, $ar, $as);
	return __clone(self);
}
function $al(self, route2, $am, $an) {
	const path = to_path(route2);
	return on_event($aq($aq(self, "href", path, $am, $an), "draggable", "false", $am, $an), "click", (event, $at) => {
		if (plain_left_click(event)) {
			event.preventDefault();
			navigate(path, [ 0, $at ]);
		}
		return;
	});
}
function $ai(label, route2, $aj, $ak) {
	return text($al(view("a"), route2, $aj, $ak), label);
}
function $aw(self, content, $ax, $ay) {
	place(content, self, $ax, $ay);
	return __clone(self);
}
function $aD(signal, observer) {
	const cell = signal[0];
	return $M(signal, mint_subscriber(() => {
		const $aE = [ 0, cell ];
		let $aF = null;
		if ($aE[0] === 0) {
			const live = $aE[1];
			$aF = observer(live.v);
		} else {
			$aF = undefined;
		}
		return $aF;
	}));
}
function $aC(self, observer) {
	return $aD(self, observer);
}
function $aA(self, transform, $y, $z) {
	const derived = $a(transform($A(self)));
	as_derivation();
	register_with_owner($aC(self, (value) => {
		$C(derived, transform(value), $y);
		return;
	}), $y, $z);
	return derived;
}
function $aN(self, observer, $aO, $aP) {
	$R(get_owner($aP), $I(self, observer), $aO);
}
function $aK(self, observer, $aL, $aM) {
	$aN(self, observer, $aL, $aM);
	observer($A(self));
}
function $aH(self, source, $aI, $aJ) {
	const element = __clone(self[0]);
	$aK(source, (value) => {
		element.textContent = value;
		return;
	}, $aI, $aJ);
	return __clone(self);
}
function $aW(self, observer) {
	return $aD(self, observer);
}
function $aU(self, transform, $y, $z) {
	const derived = $a(transform($A(self)));
	as_derivation();
	register_with_owner($aW(self, (value) => {
		$C(derived, transform(value), $y);
		return;
	}), $y, $z);
	return derived;
}
function $bo(source) {
	__chunk_preload(__chunk_arm($A(source)));
}
function $bu(self, value, $g) {
	self[0].v = __clone(value);
	$D(self, $g);
}
function $bV(self, observer, $aO, $aP) {
	$R(get_owner($aP), $aW(self, observer), $aO);
}
function $bU(self, observer, $aL, $aM) {
	$bV(self, observer, $aL, $aM);
	observer($A(self));
}
function $bq(source, render, $br) {
	const gated = $a($A(source));
	const armed = __shared_new(false);
	const generation = __shared_new(0);
	const advance = (value, $bt) => {
		armed.v = true;
		$bu(gated, value, [ 0, $bt ]);
		return;
	};
	const wire = ($bA) => {
		$bU(source, (value) => {
			return $t([ 1 ], ($bB) => {
				const mine = generation.v + 1;
				generation.v = mine;
				clear_chunk_error([ 0, $bB ]);
				const arm = __chunk_arm(value);
				if (__chunk_ready(arm)) {
					set_chunk_pending(false, [ 0, $bB ]);
					advance(value, $bB);
				} else {
					set_chunk_pending(true, [ 0, $bB ]);
					__chunk_load(arm, () => {
						return $t([ 1 ], ($bS) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bS ]);
								advance(value, $bS);
							}
							return;
						});
					}, (reason) => {
						return $t([ 1 ], ($bT) => {
							if (generation.v === mine) {
								set_chunk_pending(false, [ 0, $bT ]);
								$bu(chunk_error_signal, [ 0, reason ], [ 0, $bT ]);
							}
							return;
						});
					});
				}
				return;
			});
		}, $br, $bA);
		return;
	};
	return [ __clone(source), render, [ 0, __clone(gated) ], armed, wire ];
}
function $co(self) {
	const $cp = self;
	return $cp[0] === 1;
}
function $cG(self, content, end, $cH, $cI) {
	const marker = document.createTextNode("");
	host(self).insertBefore(marker, end);
	const staging = document.createDocumentFragment();
	place(content, [ __clone(staging) ], $cH, $cI);
	host(self).insertBefore(staging, end);
	return [ marker ];
}
function $cD(self, content, $cE, $cF) {
	return $cG(self, content, self[0], $cE, $cF);
}
function $cJ(owner, body) {
	return body(owner);
}
function $ch(parent, source, render, armed, $ci, $cj) {
	const region = open(parent);
	const last_value = __shared_new([ 1 ]);
	const live_row = __shared_new([ 1 ]);
	const live_owner = __shared_new([ 1 ]);
	defer(get_owner($cj), () => {
		const $ck = live_owner.v;
		let $cl = null;
		if ($ck[0] === 1) {
			$cl = $ck;
		} else {
			$cl = [ 0, dispose2($ck[1]) ];
		}
		$cl;
		close(region);
		return;
	});
	$bU(source, (value) => {
		const $cu = last_value.v;
		let $cv = null;
		if ($cu[0] === 0) {
			const previous = $cu[1];
			$cv = eq(previous, value);
		} else {
			$cv = false;
		}
		const unchanged = $cv;
		if (armed.v && !(unchanged)) {
			const $cy = live_owner.v;
			let $cz = null;
			if ($cy[0] === 1) {
				$cz = $cy;
			} else {
				$cz = [ 0, dispose2($cy[1]) ];
			}
			$cz;
			const $cA = live_row.v;
			let $cB = null;
			if ($cA[0] === 0) {
				const row = $cA[1];
				cut_row(region, row, region[0]);
				drop_row(region, row);
				$cB = undefined;
			} else {
				$cB = undefined;
			}
			$cB;
			const owner = new3();
			const row2 = $cJ(owner, ($cC) => {
				return $cD(region, render(value, $cC), $ci, $cC);
			});
			hold_rows(region, [ __clone(row2) ]);
			last_value.v = [ 0, __clone(value) ];
			live_row.v = [ 0, row2 ];
			live_owner.v = [ 0, owner ];
		}
		return;
	}, $ci, $cj);
}
function $cc(self, parent, $cd, $ce) {
	self[4]($ce);
	const $cf = self[2];
	let $cg = null;
	if ($cf[0] === 0) {
		const gated = $cf[1];
		$cg = $ch(parent, gated, self[1], self[3], $cd, $ce);
	} else {
		$cg = $ch(parent, self[0], self[1], self[3], $cd, $ce);
	}
	return $cg;
}
function $cb(self, content, $ax, $ay) {
	$cc(content, self, $ax, $ay);
	return __clone(self);
}
function $cL(body) {
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
__vilan_chunks.fn.$ai = $ai;
__vilan_chunks.fn.$aw = $aw;
__vilan_chunks.fn.LABEL = LABEL;
__vilan_chunks.fn.panel = panel;
__vilan_chunks.fn.view = view;
const route = $x(current_path([ 1 ]), parse, [ 1 ], [ 1 ]);
mount_root("app", ($ac) => {
	return app(route, [ 1 ], $ac);
});
