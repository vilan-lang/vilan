function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
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
function new2() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return $ab(self[0].v) && $ab(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $av = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$av = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$av;
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
				while (!($ab(turn[1].v)) && budget > 0) {
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
function dispose(self, $X) {
	const $Y = $X;
	let $Z = null;
	if ($Y[0] === 0) {
		const established = $Y[1];
		$Z = [ 0, established ];
	} else {
		$Z = $aa(draining_turns.v);
	}
	const ambient = $Z;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ad = [ 0, handle[0] ];
	let $ae = null;
	if ($ad[0] === 0) {
		const subscribers = $ad[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$ae = undefined;
	} else {
		$ae = undefined;
	}
	$ae;
	const $af = ambient;
	let $ag = null;
	if ($af[0] === 0) {
		const turn = $af[1];
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
		$ag = undefined;
	} else {
		$ag = undefined;
	}
	$ag;
	const $ah = handle[3].v;
	let $ai = null;
	if ($ah[0] === 0) {
		const release = $ah[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ai = undefined;
	} else {
		$ai = undefined;
	}
	return $ai;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aj = null;
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
		$aj = undefined;
	}
	return $aj;
}
function renew(self) {
	const $r = self[0].v[3];
	let $s = null;
	if ($r[0] === 0) {
		const nursery2 = __clone($r[1]);
		let $t = null;
		if (has_spawned(nursery2)) {
			$t = [ 1 ];
		} else {
			$t = [ 0, nursery2 ];
		}
		$s = $t;
	} else {
		$s = [ 1 ];
	}
	const carried = $s;
	advance([ self[0], self[0].v[0] ], carried);
	if ($w(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $G = null;
	if (is_disposed(self)) {
		$G = [ 1 ];
	} else {
		$G = self[0].v[3];
	}
	return $G;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $F = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $u = held[3];
		let $v = null;
		if ($u[0] === 0) {
			const nursery2 = $u[1];
			if ($w(carried)) {
				nursery2.cancel();
			}
			$v = undefined;
		} else {
			$v = undefined;
		}
		$v;
		let $E = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $y = __guarded(cleanup);
				let $z = null;
				if ($y[0] === 0) {
					const message = $y[1];
					if ($w(failure)) {
						failure = [ 0, message ];
					}
					$z = undefined;
				} else {
					$z = undefined;
				}
				$z;
			}
			const $C = failure;
			let $D = null;
			if ($C[0] === 0) {
				const message2 = $C[1];
				$D = (() => {
					throw message2;
				})();
			} else {
				$D = undefined;
			}
			$E = $D;
		}
		$F = $E;
	}
	return $F;
}
function get_owner($U) {
	return $U;
}
function release_runs(runs2) {
	dispose2([ runs2[0], runs2[0].v[0] ]);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $S = previous;
		let $T = null;
		if ($S[0] === 0) {
			const earlier = $S[1];
			$T = earlier();
		} else {
			$T = undefined;
		}
		return $T;
	} ];
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function tick() {
	ticks = ticks + 2;
	ticks = ticks * 3;
}
function $e(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $w(self) {
	const $x = self;
	return $x[0] === 1;
}
function $q(runs2, body) {
	const run = renew(runs2);
	const $H = nursery(run);
	let $I = null;
	if ($H[0] === 0) {
		const nursery2 = $H[1];
		$I = (($J) => {
			return (($K) => {
				return body($J, $K);
			})(nursery2);
		})(run);
	} else {
		$I = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $I;
}
function $Q(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $N(signal, observer) {
	const cell = signal[0];
	return $Q(signal, mint_subscriber(() => {
		const $O = [ 0, cell ];
		let $P = null;
		if ($O[0] === 0) {
			const live = $O[1];
			$P = observer(live.v);
		} else {
			$P = undefined;
		}
		return $P;
	}));
}
function $R(self) {
	return __clone(self[0].v);
}
function $M(self, observer, immediately) {
	const subscription = $N(self, observer);
	if (immediately) {
		observer($R(self));
	}
	return subscription;
}
function $L(self, observer) {
	return $M(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false);
}
function $ab(self) {
	return self.length === 0;
}
function $aa(self) {
	let $ac = null;
	if ($ab(self)) {
		$ac = [ 1 ];
	} else {
		$ac = __list_get(self, self.length - 1);
	}
	return $ac;
}
function $V(self, item, $W) {
	defer(self, () => {
		dispose(item, $W);
		return;
	});
	return __clone(item);
}
function $k(self, body, $l, $m) {
	const runs2 = new3();
	const subscription = $L(self, (value, $n) => {
		return $q(runs2, ($o, $p) => {
			return (() => {
				return body(value, $o, [ 1 ], $p);
			})();
		});
	});
	also_releasing(subscription, () => {
		return release_runs(runs2);
	});
	$V(get_owner($m), subscription, $l);
}
function $ao(owner, body) {
	return body(owner);
}
function $ar(self, $as) {
	const $at = $as;
	let $au = null;
	if ($at[0] === 0) {
		const turn = $at[1];
		$au = enqueue(turn, __clone(self[1].v));
	} else {
		const $ax = $aa(draining_turns.v);
		let $ay = null;
		if ($ax[0] === 0) {
			const draining = $ax[1];
			$ay = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$ay = undefined;
		}
		$au = $ay;
	}
	return $au;
}
function $ap(self, value, $aq) {
	self[0].v = __clone(value);
	$ar(self, $aq);
}
function $aA(body, $aB) {
	const $aC = $aB;
	let $aD = null;
	if ($aC[0] === 0) {
		const current = $aC[1];
		$aD = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$aD = result;
	}
	return $aD;
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const run_nurseries_allocated_count = __shared_new(0);
let ticks = 0;
const fired = __shared_new(0);
const $a = [ fired, "v" ];
fired.v = $a[0][$a[1]] + 1;
const $b = [ fired, "v" ];
fired.v = $b[0][$b[1]] - 3;
const $c = [ fired, "v" ];
fired.v = $c[0][$c[1]] * -(5);
console.log("statement: " + fired.v);
const bump = () => {
	const $d = [ fired, "v" ];
	return fired.v = $d[0][$d[1]] + 1;
};
bump();
bump();
console.log("closure: " + fired.v);
const tally = __shared_new([ 1 ]);
tally.v[0] = tally.v[0] + 41;
console.log("field: " + tally.v[0]);
const a = $e(0);
const b = $e(0);
const runs = __shared_new(0);
const watching = new3();
$ao(watching, ($f) => {
	$k(__clone(a), (_x, $g, $h, $i) => {
		const $j = [ runs, "v" ];
		return runs.v = $j[0][$j[1]] + 1;
	}, [ 1 ], $f);
	$k(__clone(b), (_x, $ak, $al, $am) => {
		const $an = [ runs, "v" ];
		return runs.v = $an[0][$an[1]] + 10;
	}, [ 1 ], $f);
	return;
});
$ap(a, 1, [ 1 ]);
console.log("inline: " + runs.v);
$aA(($az) => {
	$ap(a, 2, [ 0, $az ]);
	$ap(b, 2, [ 0, $az ]);
	return;
}, [ 1 ]);
console.log("drained: " + runs.v);
dispose2(watching);
let captured = 1;
const double = () => {
	return captured = captured * 2;
};
double();
double();
console.log("captured: " + captured);
tick();
tick();
console.log("module: " + ticks);
