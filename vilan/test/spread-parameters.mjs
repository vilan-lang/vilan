function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
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
function __insert_at(list, index, value, location) {
	if (index >= 0 && index < list.length) return void list.splice(index, 0, value);
	if (index === list.length) return void list.push(value);
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __list_get(list, index) {
	return index >= 0 && index < list.length ? [ 0, __clone(list[index]) ] : [ 1 ];
}
function __list_pop(list) {
	return list.length === 0 ? [ 1 ] : [ 0, list.pop() ];
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
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
	return $v(self[0].v) && $v(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $u = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$u = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:414:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:417:25");
		}
		$u;
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
				while (!($v(turn[1].v)) && budget > 0) {
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
function middle(items) {
	return items[1];
}
function swap_pack(items) {
	items = __clone(items);
	items = [ 8, 9 ];
	return items[0];
}
function forward(items) {
	return items[0];
}
function outer(items) {
	return forward(items);
}
function $a(items) {
	return 1;
}
function $c(head, rest) {
	return head;
}
function $f(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $e(value) {
	return $f(value);
}
function $g(value) {
	return $f(value);
}
function $k(self) {
	return __clone(self[0].v);
}
function $v(self) {
	return self.length === 0;
}
function $w(self) {
	let $y = null;
	if ($v(self)) {
		$y = [ 1 ];
	} else {
		$y = __list_get(self, self.length - 1);
	}
	return $y;
}
function $q(self, $r) {
	const $s = $r;
	let $t = null;
	if ($s[0] === 0) {
		const turn = $s[1];
		$t = enqueue(turn, __clone(self[1].v));
	} else {
		const $z = $w(draining_turns.v);
		let $A = null;
		if ($z[0] === 0) {
			const draining = $z[1];
			$A = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$A = undefined;
		}
		$t = $A;
	}
	return $t;
}
function $o(self, value, $p) {
	self[0].v = __clone(value);
	$q(self, $p);
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
function $C(self, observer, immediately) {
	const subscription = $D(self, observer);
	if (immediately) {
		observer($k(self));
	}
	return subscription;
}
function $B(self, observer) {
	return $C(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function $i(sources, $j) {
	const snapshot = () => {
		return sources.map((source) => {
			return $k(source);
		});
	};
	const derived = $g(snapshot());
	sources.map((source) => {
		return $B(__clone(source), (_, $n) => {
			$o(derived, snapshot(), $j);
			return;
		});
	});
	return derived;
}
function $K(self, $r) {
	const $L = $r;
	let $M = null;
	if ($L[0] === 0) {
		const turn = $L[1];
		$M = enqueue(turn, __clone(self[1].v));
	} else {
		const $N = $w(draining_turns.v);
		let $O = null;
		if ($N[0] === 0) {
			const draining = $N[1];
			$O = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$O = undefined;
		}
		$M = $O;
	}
	return $M;
}
function $J(self, value, $p) {
	self[0].v = __clone(value);
	$K(self, $p);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
console.log(middle([ 4, 5, 6 ]));
console.log($a([  ]));
console.log($a([ 1 ]));
console.log($c(2, [ 3, 4 ]));
console.log(swap_pack([ 1, 2 ]));
console.log(outer([ 3, 4 ]));
const inner = [ 10, 11 ];
console.log($a([ ...inner, 12 ]));
const count = $e(20);
const name = $g("hi");
const both = $i([ __clone(count), name ], [ 1 ]);
console.log($k(both)[0]);
$J(count, 21, [ 1 ]);
console.log($k(both)[0]);
console.log($k(both)[1]);
