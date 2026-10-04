function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __at_put(list, index, value) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
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
function __insert_at(list, index, value) {
	if (index >= 0 && index < list.length) return void list.splice(index, 0, value);
	if (index === list.length) return void list.push(value);
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __list_get(list, index) {
	return index >= 0 && index < list.length ? [ 0, __clone(list[index]) ] : [ 1 ];
}
function __list_pop(list) {
	return list.length === 0 ? [ 1 ] : [ 0, list.pop() ];
}
function __nursery_has_spawned(n) {
	return n.children.length > 0;
}
class __Nursery {
	constructor(parent) {
		this.children = [];
		this.failedTask = undefined;
		this.failWake = undefined;
		this.controller = new AbortController();
		if (parent) {
			const signal = parent.controller.signal;
			if (signal.aborted) this.controller.abort(signal.reason);
			else signal.addEventListener("abort", () => this.controller.abort(signal.reason), { once: true });
		}
	}
	cancel() {
		this.controller.abort();
	}
	__fail(task) {
		if (this.failedTask === undefined) {
			this.failedTask = task;
			this.controller.abort();
			if (this.failWake) this.failWake();
		}
	}
	is_cancelled() {
		return this.controller.signal.aborted;
	}
	signal_of() {
		return this.controller.signal;
	}
}
function __nursery_new(parent) {
	return new __Nursery(parent && parent[0] === 0 ? parent[1] : undefined);
}
function __nursery_new_detached() {
	const n = __nursery_new(undefined);
	n.detached = true;
	n.__fail = function (task) {
		if (!task.observed) {
			globalThis.setTimeout(() => {
				if (!task.observed) console.error("unhandled task error (spawned in " + task.origin + "): " + String(task.error));
			}, 0);
		}
	};
	return n;
}
function __nursery_is_cancel(error) {
	return !!error && error.name === "AbortError";
}
async function __nursery_run(n, body) {
	let result;
	let bodyError;
	let bodyFailed = false;
	try {
		result = await body();
	} catch (error) {
		bodyFailed = true;
		bodyError = error;
	}
	if (bodyFailed) n.controller.abort();
	const failed = new Promise((resolve) => {
		n.failWake = resolve;
		if (n.failedTask !== undefined) resolve();
	});
	let index = 0;
	while (!bodyFailed && n.failedTask === undefined && index < n.children.length) {
		try {
			await Promise.race([n.children[index], failed]);
		} catch (error) {}
		if (n.failedTask === undefined) index += 1;
	}
	if (!bodyFailed && n.failedTask === undefined) return result;
	for (const task of n.children) task.then(null, () => {});
	if (bodyFailed) throw bodyError;
	const winner = n.failedTask;
	throw typeof winner.error === "string" ? winner.error + " (in task spawned in " + winner.origin + ")" : winner.error;
}
let __shared_identity_next = 1;
function __shared_identity(cell) {
	return cell.__id ??= __shared_identity_next++;
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
function hash(self) {
	return __hash(self);
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
function is_quiescent(self) {
	return $H(self[0].v) && $H(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $at = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$at = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$at;
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
				while (!($H(turn[1].v)) && budget > 0) {
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
function defer_subscriber(turn, subscriber) {
	const $ar = turn;
	let $as = null;
	if ($ar[0] === 0) {
		const ambient = $ar[1];
		$as = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $ay = $av(draining_turns.v);
		let $az = null;
		if ($ay[0] === 0) {
			const draining = $ay[1];
			$az = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$az = undefined;
		}
		$as = $az;
	}
	return $as;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $bj) {
	const $bk = $bj;
	let $bl = null;
	if ($bk[0] === 0) {
		const established = $bk[1];
		$bl = [ 0, established ];
	} else {
		$bl = $av(draining_turns.v);
	}
	const ambient = $bl;
	release_under(self, ambient);
}
function detach(handle) {
	const $aD = $av(releasing_turns.v);
	let $aE = null;
	if ($aD[0] === 0) {
		const at_release = $aD[1];
		$aE = at_release;
	} else {
		$aE = $av(draining_turns.v);
	}
	const turn = $aE;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $aF = [ 0, handle[0] ];
	let $aG = null;
	if ($aF[0] === 0) {
		const subscribers = $aF[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aG = undefined;
	} else {
		$aG = undefined;
	}
	$aG;
	const $aH = ambient;
	let $aI = null;
	if ($aH[0] === 0) {
		const turn = $aH[1];
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
		$aI = undefined;
	} else {
		$aI = undefined;
	}
	$aI;
	const $aJ = handle[3].v;
	let $aK = null;
	if ($aJ[0] === 0) {
		const release = $aJ[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aK = undefined;
	} else {
		$aK = undefined;
	}
	return $aK;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $bm = null;
	if (is_disposed(self)) {
		cleanup();
	} else {
		if (self[0].v[2]) {
			self[0].v[1].v.push(cleanup);
		} else {
			owner_lists_allocated_count.v = owner_lists_allocated_count.v + 1;
			self[0].v[1] = __shared_new([ cleanup ]);
			self[0].v[2] = true;
		}
		$bm = undefined;
	}
	return $bm;
}
function renew(self) {
	const $M = self[0].v[3];
	let $N = null;
	if ($M[0] === 0) {
		const nursery2 = __clone($M[1]);
		let $O = null;
		if (has_spawned(nursery2)) {
			$O = [ 1 ];
		} else {
			$O = [ 0, nursery2 ];
		}
		$N = $O;
	} else {
		$N = [ 1 ];
	}
	const carried = $N;
	advance([ self[0], self[0].v[0] ], carried);
	if ($R(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $ab = null;
	if (is_disposed(self)) {
		$ab = [ 1 ];
	} else {
		$ab = self[0].v[3];
	}
	return $ab;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $aa = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $P = held[3];
		let $Q = null;
		if ($P[0] === 0) {
			const nursery2 = $P[1];
			if ($R(carried)) {
				nursery2.cancel();
			}
			$Q = undefined;
		} else {
			$Q = undefined;
		}
		$Q;
		let $Z = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $T = __guarded(cleanup);
				let $U = null;
				if ($T[0] === 0) {
					const message = $T[1];
					if ($R(failure)) {
						failure = [ 0, message ];
					}
					$U = undefined;
				} else {
					$U = undefined;
				}
				$U;
			}
			const $X = failure;
			let $Y = null;
			if ($X[0] === 0) {
				const message2 = $X[1];
				$Y = (() => {
					throw message2;
				})();
			} else {
				$Y = undefined;
			}
			$Z = $Y;
		}
		$aa = $Z;
	}
	return $aa;
}
function get_owner($bg) {
	return $bg;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker() {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ] ]) ];
}
function has_run(tracker) {
	return tracker[0].v[0] > 0;
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $F = tracker[0].v[6];
	let $G = null;
	if ($F[0] === 0) {
		const lists = $F[1];
		if (!($H(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$G = undefined;
	} else {
		$G = undefined;
	}
	$G;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $ag = tracker[0].v[6];
	let $ah = null;
	if ($ag[0] === 0) {
		const lists = $ag[1];
		$ah = lists;
	} else {
		return;
		$ah = undefined;
	}
	const lists2 = $ah;
	const $ai = tracker[0].v[5];
	let $aj = null;
	if ($ai[0] === 0) {
		const target = __clone($ai[1]);
		$aj = reconnect(tracker, lists2, target);
	} else {
		if (!($H(lists2.v[0])) || !($H(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$aj = undefined;
	}
	$aj;
	if (!($H(lists2.v[0]))) {
		lists2.v[0] = [  ];
	}
}
function reconnect(tracker, lists, target) {
	if ($H(lists.v[0]) && $H(lists.v[2])) {
		return;
	}
	const held = __clone(lists.v[2]);
	const reading = __clone(lists.v[0]);
	let kept = [  ];
	for (const _edge of held) {
		kept.push(false);
	}
	let next = [  ];
	tracker[0].v[3] = true;
	let position = 0;
	for (const dependency of reading) {
		const $ap = reusable(held, kept, dependency[0], position);
		let $aq = null;
		if ($ap[0] === 0) {
			const index = $ap[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$aq = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$aq = undefined;
		}
		$aq;
		position = position + 1;
	}
	lists.v[2] = next;
	let index2 = 0;
	for (const edge of held) {
		if (!(__at(kept, index2))) {
			detach(edge[1]);
		}
		index2 = index2 + 1;
	}
	tracker[0].v[3] = false;
}
function reusable(held, kept, identity, position) {
	const $al = identity;
	let $am = null;
	if ($al[0] === 0) {
		const wanted = $al[1];
		if (position < held.length && !(__at(kept, position)) && same_identity(__at(held, position)[0], wanted)) {
			return [ 0, position ];
		}
		let index = 0;
		while (index < held.length) {
			if (!(__at(kept, index)) && same_identity(__at(held, index)[0], wanted)) {
				return [ 0, index ];
			}
			index = index + 1;
		}
		$am = [ 1 ];
	} else {
		$am = [ 1 ];
	}
	return $am;
}
function same_identity(identity, wanted) {
	const $an = identity;
	let $ao = null;
	if ($an[0] === 0) {
		const held = $an[1];
		$ao = held === wanted;
	} else {
		$ao = false;
	}
	return $ao;
}
function relay_for(tracker, target) {
	return subscriber_of(() => {
		const connecting = tracker[0].v[3];
		tracker[0].v[2] = true;
		if (connecting) {
			tracker[0].v[4] = true;
		} else {
			wake(target);
		}
		return;
	}, true);
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $aV = tracker[0].v[6];
	let $aW = null;
	if ($aV[0] === 0) {
		const lists = $aV[1];
		$aW = lists;
	} else {
		return;
		$aW = undefined;
	}
	const lists2 = $aW;
	let $aX = null;
	if (!($H(lists2.v[1]))) {
		const read = __clone(lists2.v[1]);
		lists2.v[1] = [  ];
		tracker[0].v[3] = true;
		let edges = [  ];
		for (const dependency of read) {
			edges.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
		}
		lists2.v[2] = edges;
		tracker[0].v[3] = false;
		if (tracker[0].v[4]) {
			tracker[0].v[4] = false;
			wake(target);
		}
		$aX = undefined;
	}
	return $aX;
}
function forget_reads(tracker) {
	const $bd = tracker[0].v[6];
	let $be = null;
	if ($bd[0] === 0) {
		const lists = $bd[1];
		$be = lists;
	} else {
		return;
		$be = undefined;
	}
	const lists2 = $be;
	let $bf = null;
	if (!($H(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$bf = undefined;
	}
	$bf;
	if (!($H(lists2.v[1]))) {
		lists2.v[1] = [  ];
	}
}
function detach_tracker(tracker) {
	tracker[0].v[5] = [ 1 ];
	forget_reads(tracker);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $r = previous;
		let $s = null;
		if ($r[0] === 0) {
			const earlier = $r[1];
			$s = earlier();
		} else {
			$s = undefined;
		}
		return $s;
	} ];
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function name(self) {
	return "square";
}
function area(self) {
	return self[0] * self[0];
}
function describe(self, prefix) {
	return "" + prefix + name(self) + " " + self[0];
}
function name2(self) {
	return "rect";
}
function area2(self) {
	return self[0] * self[1];
}
function describe2(self, prefix) {
	return "" + prefix + name2(self) + " " + self[0] + "x" + self[1];
}
function get(self) {
	return self[0][1].get(self[0][0]) + self[1];
}
function on_settle(self, subscriber) {
	return self[0][1].on_settle(self[0][0], subscriber);
}
function total(shapes2) {
	let sum = 0;
	for (const shape of shapes2) {
		sum = sum + shape[1].area(shape[0]);
	}
	return sum;
}
const $a = Object.create({area: area, describe: describe, name: name});
const $b = Object.create({area: area2, describe: describe2, name: name2});
function $c(self) {
	return self[1].area(self[0]) * 2;
}
function $d(shapes2) {
	let best = 0;
	for (const shape of shapes2) {
		if (shape[1].area(shape[0]) > best) {
			best = shape[1].area(shape[0]);
		}
	}
	return best;
}
function $e(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $g(self) {
	return __clone(self[0].v);
}
function $i(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $h(self, subscriber) {
	return $i(self, subscriber);
}
function $k(signal, observer) {
	const cell = signal[0];
	return $i(signal, mint_subscriber(() => {
		const $l = [ 0, cell ];
		let $m = null;
		if ($l[0] === 0) {
			const live = $l[1];
			$m = observer(live.v);
		} else {
			$m = undefined;
		}
		return $m;
	}));
}
function $j(self, observer, immediately) {
	const subscription = $k(self, observer);
	if (immediately) {
		observer($g(self));
	}
	return subscription;
}
function $n(self) {
	return [ 0, __shared_identity(self[0]) ];
}
function $o(self) {
	return [ () => {
		return $g(self);
	}, (subscriber) => {
		return $h(self, subscriber);
	}, () => {
		return;
	} ];
}
function $q(flow, observer, immediately) {
	const instance = $o(flow);
	const pull = instance[0];
	if (!(immediately)) {
		pull();
	}
	const subscription = instance[1](mint_subscriber(() => {
		return observer(pull());
	}));
	also_releasing(subscription, instance[2]);
	if (immediately) {
		observer(pull());
	}
	return subscription;
}
function $p(self, observer) {
	return $q(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false);
}
function $H(self) {
	return self.length === 0;
}
function $R(self) {
	const $S = self;
	return $S[0] === 1;
}
function $L(runs, body) {
	const run = renew(runs);
	const $ac = nursery(run);
	let $ad = null;
	if ($ac[0] === 0) {
		const nursery2 = $ac[1];
		$ad = (($ae) => {
			return (($af) => {
				return body($ae, $af);
			})(nursery2);
		})(run);
	} else {
		$ad = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $ad;
}
function $av(self) {
	let $ax = null;
	if ($H(self)) {
		$ax = [ 1 ];
	} else {
		$ax = __list_get(self, self.length - 1);
	}
	return $ax;
}
function $E(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = $L(runs, ($I, $J) => {
		return (($K) => {
			return body($I, $K, $J);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function $A(runs, tracker, body) {
	let value = $E(runs, tracker, ($B, $C, $D) => {
		return body($B, $C, $D);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = $E(runs, tracker, ($aL, $aM, $aN) => {
			return body($aL, $aM, $aN);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function $aO(self, observer) {
	return $j(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function $bh(self, item, $bi) {
	defer(self, () => {
		dispose(item, $bi);
		return;
	});
	return __clone(item);
}
function $t(self, body, $u, $v) {
	const runs = new2();
	const tracker = new_tracker();
	let latest = [ 1 ];
	const subscription = $aO(self, (value2, $w) => {
		latest = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			$A(runs, tracker, ($x, $y, $z) => {
				return body(value2, $x, $y, $z);
			});
		}
		return;
	});
	const rerun = subscriber_of(() => {
		let $aU = null;
		if (tracker[0].v[2]) {
			const $aP = latest;
			let $aQ = null;
			if ($aP[0] === 0) {
				const value2 = __clone($aP[1]);
				$aQ = $A(runs, tracker, ($aR, $aS, $aT) => {
					return body(value2, $aR, $aS, $aT);
				});
			} else {
				$aQ = undefined;
			}
			$aU = $aQ;
		}
		return $aU;
	}, false);
	attach_tracker(tracker, rerun);
	const $aY = latest;
	let $aZ = null;
	if ($aY[0] === 0) {
		const value = __clone($aY[1]);
		$aZ = $A(runs, tracker, ($ba, $bb, $bc) => {
			return body(value, $ba, $bb, $bc);
		});
	} else {
		$aZ = undefined;
	}
	$aZ;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	$bh(get_owner($v), subscription, $u);
}
const $f = Object.create({get: $g, on_settle: $h, attach_observer: $j, identity: $n, start: $o, on_change: $p, effect: $t});
function $bo(self, observer, immediately) {
	const subscription = on_settle(self, mint_subscriber(() => {
		return observer(get(self));
	}));
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function $bp(self) {
	return [ 1 ];
}
function $bq(self) {
	return [ () => {
		return get(self);
	}, (subscriber) => {
		return on_settle(self, subscriber);
	}, () => {
		return;
	} ];
}
function $bs(flow, observer, immediately) {
	const instance = $bq(flow);
	const pull = instance[0];
	if (!(immediately)) {
		pull();
	}
	const subscription = instance[1](mint_subscriber(() => {
		return observer(pull());
	}));
	also_releasing(subscription, instance[2]);
	if (immediately) {
		observer(pull());
	}
	return subscription;
}
function $br(self, observer) {
	return $bs(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false);
}
function $bu(self, observer) {
	return $bo(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function $bt(self, body, $u, $v) {
	const runs = new2();
	const tracker = new_tracker();
	let latest = [ 1 ];
	const subscription = $bu(self, (value2, $w) => {
		latest = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			$A(runs, tracker, ($x, $y, $z) => {
				return body(value2, $x, $y, $z);
			});
		}
		return;
	});
	const rerun = subscriber_of(() => {
		let $bx = null;
		if (tracker[0].v[2]) {
			const $bv = latest;
			let $bw = null;
			if ($bv[0] === 0) {
				const value2 = __clone($bv[1]);
				$bw = $A(runs, tracker, ($aR, $aS, $aT) => {
					return body(value2, $aR, $aS, $aT);
				});
			} else {
				$bw = undefined;
			}
			$bx = $bw;
		}
		return $bx;
	}, false);
	attach_tracker(tracker, rerun);
	const $by = latest;
	let $bz = null;
	if ($by[0] === 0) {
		const value = __clone($by[1]);
		$bz = $A(runs, tracker, ($ba, $bb, $bc) => {
			return body(value, $ba, $bb, $bc);
		});
	} else {
		$bz = undefined;
	}
	$bz;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	$bh(get_owner($v), subscription, $u);
}
const $bn = Object.create({get: get, on_settle: on_settle, attach_observer: $bo, identity: $bp, start: $bq, on_change: $br, effect: $bt});
function $bE(self, $bF) {
	const $bG = $bF;
	let $bH = null;
	if ($bG[0] === 0) {
		const turn = $bG[1];
		$bH = enqueue(turn, __clone(self[1].v));
	} else {
		const $bI = $av(draining_turns.v);
		let $bJ = null;
		if ($bI[0] === 0) {
			const draining = $bI[1];
			$bJ = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bJ = undefined;
		}
		$bH = $bJ;
	}
	return $bH;
}
function $bC(self, value, $bD) {
	self[0].v = __clone(value);
	$bE(self, $bD);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const run_nurseries_allocated_count = __shared_new(0);
const slots = [ [ [ [ 3 ], $a ] ], [ [ [ 2, 5 ], $b ] ] ];
for (const slot of slots) {
	console.log(slot[0][1].describe(slot[0][0], "- "));
	console.log("" + slot[0][1].name(slot[0][0]) + " " + slot[0][1].area(slot[0][0]) + " " + $c(slot[0]));
}
const shapes = [ [ [ 4, 4 ], $b ], [ [ 2 ], $a ] ];
console.log(total(shapes));
console.log($d(shapes));
console.log(__at(slots, 0));
const root = $e(1);
const sources = [ __clone([ root, $f ]), [ [ __clone([ root, $f ]), 100 ], $bn ] ];
const watch = (($bB) => {
	return $bB[1].on_change($bB[0], (n, $bA) => {
		return console.log("offset saw " + n);
	});
})(__clone(__at(sources, 1)));
$bC(root, 5, [ 1 ]);
for (const source of sources) {
	console.log(source[1].get(source[0]));
}
dispose(watch, [ 1 ]);
