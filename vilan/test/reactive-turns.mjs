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
function __shared_new(value) {
	return { v: value };
}
class __Task {
	constructor(run, origin, nursery) {
		this.origin = origin;
		this.observed = false;
		this.nursery = nursery;
		this.owned = !!nursery;
		this.rejected = false;
		this.error = undefined;
		this.promise = run();
		this.promise.then(null, (error) => {
			this.rejected = true;
			this.error = error;
			if (this.owned && !__nursery_is_cancel(error)) this.nursery.__fail(this);
			if (!this.observed && !this.owned) {
				globalThis.setTimeout(() => {
					if (!this.observed) console.error("unhandled task error (spawned in " + this.origin + "): " + String(error));
				}, 0);
			}
		});
		if (nursery) nursery.children.push(this);
	}
	then(onFulfilled, onRejected) {
		this.observed = true;
		return this.promise.then(onFulfilled, onRejected);
	}
}
function __task(run, origin, nursery) {
	return new __Task(run, origin, nursery);
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
	return $q(self[0].v) && $q(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $p = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$p = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$p;
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
				while (!($q(turn[1].v)) && budget > 0) {
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
function flush($v) {
	const $w = $v;
	let $x = null;
	if ($w[0] === 0) {
		const turn = $w[1];
		$x = drain(turn);
	} else {
		$x = undefined;
	}
	return $x;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
async function tick() {

}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $g(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $d(signal, observer) {
	const cell = signal[0];
	return $g(signal, mint_subscriber(() => {
		const $e = [ 0, cell ];
		let $f = null;
		if ($e[0] === 0) {
			const live = $e[1];
			$f = observer(live.v);
		} else {
			$f = undefined;
		}
		return $f;
	}));
}
function $h(self) {
	return __clone(self[0].v);
}
function $c(self, observer) {
	const subscription = $d(self, observer);
	observer($h(self));
	return subscription;
}
function $q(self) {
	return self.length === 0;
}
function $r(self) {
	return __list_get(self, self.length - 1);
}
function $l(self, $m) {
	const $n = $m;
	let $o = null;
	if ($n[0] === 0) {
		const turn = $n[1];
		$o = enqueue(turn, self[1].v);
	} else {
		const $s = $r(draining_turns.v);
		let $t = null;
		if ($s[0] === 0) {
			const draining = $s[1];
			$t = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$t = undefined;
		}
		$o = $t;
	}
	return $o;
}
function $j(self, value, $k) {
	self[0].v = __clone(value);
	$l(self, $k);
}
function $A(policy, body) {
	const fresh = new2();
	const result = body(fresh);
	drain(fresh);
	fresh[5].v = true;
	return result;
}
function $C(body, $D) {
	const $E = $D;
	let $F = null;
	if ($E[0] === 0) {
		const current = $E[1];
		$F = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$F = result;
	}
	return $F;
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const a = $a(0);
const b = $a(0);
$c(a, (value) => {
	return console.log("a -> " + value);
});
$c(b, (value) => {
	return console.log("b -> " + value);
});
const turn_a = new2();
const turn_b = new2();
(($i) => {
	$j(a, 1, [ 0, $i ]);
	return;
})(turn_a);
(($u) => {
	$j(b, 1, [ 0, $u ]);
	flush([ 0, $u ]);
	return;
})(turn_b);
console.log("mid");
(($y) => {
	return flush([ 0, $y ]);
})(turn_a);
$A([ 0 ], ($z) => {
	$j(a, 2, [ 0, $z ]);
	$j(b, 2, [ 0, $z ]);
	console.log("inside");
	return;
});
$C(($B) => {
	$j(a, 3, [ 0, $B ]);
	console.log("batched");
	return;
}, [ 1 ]);
$A([ 0 ], ($G) => {
	$C(($H) => {
		$j(a, 4, [ 0, $H ]);
		return;
	}, [ 0, $G ]);
	console.log("joined");
	return;
});
const turn_c = new2();
(($I) => {
	__task(async () => {
		await (await (tick()));
		$j(a, 5, [ 0, $I ]);
		flush([ 0, $I ]);
		return;
	}, "main");
	return;
})(turn_c);
console.log("end-sync");
