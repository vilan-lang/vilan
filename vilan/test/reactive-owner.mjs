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
function is_quiescent(self) {
	return $R(self[0].v) && $R(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $ai = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$ai = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$ai;
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
				while (!($R(turn[1].v)) && budget > 0) {
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
function dispose(self, $N) {
	const $O = $N;
	let $P = null;
	if ($O[0] === 0) {
		const established = $O[1];
		$P = [ 0, established ];
	} else {
		$P = $Q(draining_turns.v);
	}
	const ambient = $P;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $T = [ 0, handle[0] ];
	let $U = null;
	if ($T[0] === 0) {
		const subscribers = $T[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$U = undefined;
	} else {
		$U = undefined;
	}
	$U;
	const $V = ambient;
	let $W = null;
	if ($V[0] === 0) {
		const turn = $V[1];
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
		$W = undefined;
	} else {
		$W = undefined;
	}
	$W;
	const $X = handle[3].v;
	let $Y = null;
	if ($X[0] === 0) {
		const release = $X[1];
		handle[3].v = [ 1 ];
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
function new2() {
	return [ __shared_new([ 0, [ 1 ], [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $ab = null;
	if (is_disposed(self)) {
		cleanup();
	} else {
		const $Z = self[0].v[1];
		let $aa = null;
		if ($Z[0] === 0) {
			const list = $Z[1];
			$aa = list.v.push(cleanup);
		} else {
			owner_lists_allocated_count.v = owner_lists_allocated_count.v + 1;
			self[0].v[1] = [ 0, __shared_new([ cleanup ]) ];
			$aa = undefined;
		}
		$ab = $aa;
	}
	return $ab;
}
function renew(self, with_nursery) {
	const live = [ self[0], self[0].v[0] ];
	dispose2(live);
	const next = self[0].v[0];
	if (with_nursery) {
		self[0].v[2] = [ 0, detached_nursery() ];
	}
	return [ self[0], next ];
}
function nursery(self) {
	let $w = null;
	if (is_disposed(self)) {
		$w = [ 1 ];
	} else {
		$w = self[0].v[2];
	}
	return $w;
}
function dispose2(self) {
	let $v = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, [ 1 ], [ 1 ] ];
		const $l = held[2];
		let $m = null;
		if ($l[0] === 0) {
			const nursery2 = $l[1];
			$m = nursery2.cancel();
		} else {
			$m = undefined;
		}
		$m;
		const $n = held[1];
		let $o = null;
		if ($n[0] === 0) {
			const cleanups = $n[1];
			let failure = [ 1 ];
			for (const cleanup of cleanups.v) {
				const $p = __guarded(cleanup);
				let $q = null;
				if ($p[0] === 0) {
					const message = $p[1];
					if ($r(failure)) {
						failure = [ 0, message ];
					}
					$q = undefined;
				} else {
					$q = undefined;
				}
				$q;
			}
			const $t = failure;
			let $u = null;
			if ($t[0] === 0) {
				const message2 = $t[1];
				$u = (() => {
					throw message2;
				})();
			} else {
				$u = undefined;
			}
			$o = $u;
		} else {
			$o = undefined;
		}
		$v = $o;
	}
	return $v;
}
function get_owner($K) {
	return $K;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $I = previous;
		let $J = null;
		if ($I[0] === 0) {
			const earlier = $I[1];
			$J = earlier();
		} else {
			$J = undefined;
		}
		return $J;
	} ];
}
function detached_nursery() {
	return __nursery_new_detached();
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $r(self) {
	const $s = self;
	return $s[0] === 1;
}
function $k(runs, body) {
	const run = renew(runs, true);
	const $x = nursery(run);
	let $y = null;
	if ($x[0] === 0) {
		const nursery2 = $x[1];
		$y = (($z) => {
			return (($A) => {
				return body($z, $A);
			})(nursery2);
		})(run);
	} else {
		$y = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $y;
}
function $G(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $D(signal, observer) {
	const cell = signal[0];
	return $G(signal, mint_subscriber(() => {
		const $E = [ 0, cell ];
		let $F = null;
		if ($E[0] === 0) {
			const live = $E[1];
			$F = observer(live.v);
		} else {
			$F = undefined;
		}
		return $F;
	}));
}
function $H(self) {
	return __clone(self[0].v);
}
function $C(self, observer, immediately) {
	const subscription = $D(self, observer);
	if (immediately) {
		observer($H(self));
	}
	return subscription;
}
function $B(self, observer) {
	return $C(self, observer, true);
}
function $R(self) {
	return self.length === 0;
}
function $Q(self) {
	let $S = null;
	if ($R(self)) {
		$S = [ 1 ];
	} else {
		$S = __list_get(self, self.length - 1);
	}
	return $S;
}
function $L(self, item, $M) {
	defer(self, () => {
		dispose(item, $M);
		return;
	});
	return __clone(item);
}
function $f(self, body, $g, $h) {
	const runs = new2();
	const subscription = $B(self, (value) => {
		return $k(runs, ($i, $j) => {
			return body(value, $i, $j);
		});
	});
	also_releasing(subscription, () => {
		return release_runs(runs);
	});
	$L(get_owner($h), subscription, $g);
}
function $ae(self, $af) {
	const $ag = $af;
	let $ah = null;
	if ($ag[0] === 0) {
		const turn = $ag[1];
		$ah = enqueue(turn, self[1].v);
	} else {
		const $ak = $Q(draining_turns.v);
		let $al = null;
		if ($ak[0] === 0) {
			const draining = $ak[1];
			$al = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$al = undefined;
		}
		$ah = $al;
	}
	return $ah;
}
function $ac(self, value, $ad) {
	self[0].v = __clone(value);
	$ae(self, $ad);
}
function $av(owner2, body) {
	return body(owner2);
}
function $az(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const count = $a(1);
const owner = new2();
(($c) => {
	$f(__clone(count), (value, $d, $e) => {
		return console.log("seen " + value);
	}, [ 1 ], $c);
	return;
})(owner);
$ac(count, 2, [ 1 ]);
dispose2(owner);
$ac(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($am) => {
	(($an) => {
		$f(__clone(count), (value, $ao, $ap) => {
			return console.log("inner " + value);
		}, [ 1 ], $an);
		return;
	})(inner);
	$f(__clone(count), (value, $aq, $ar) => {
		return console.log("outer " + value);
	}, [ 1 ], $am);
	return;
})(outer);
$ac(count, 4, [ 1 ]);
dispose2(inner);
$ac(count, 5, [ 1 ]);
dispose2(outer);
$ac(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$av(wrapped, ($as) => {
	$f(__clone(count), (value, $at, $au) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $as);
	return;
});
$ac(count, 7, [ 1 ]);
dispose2(wrapped);
$ac(count, 8, [ 1 ]);
console.log("fin");
const $aA = $az(($aw) => {
	$f(__clone(count), (value, $ax, $ay) => {
		return console.log("comp " + value);
	}, [ 1 ], $aw);
	return "built";
});
const label = $aA[0];
const scope = $aA[1];
console.log(label);
$ac(count, 9, [ 1 ]);
dispose2(scope);
$ac(count, 10, [ 1 ]);
console.log("post");
