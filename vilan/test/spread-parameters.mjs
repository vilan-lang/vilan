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
	return $u(self[0].v) && $u(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $t = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$t = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$t;
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
				while (!($u(turn[1].v)) && budget > 0) {
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
function $u(self) {
	return self.length === 0;
}
function $v(self) {
	let $x = null;
	if ($u(self)) {
		$x = [ 1 ];
	} else {
		$x = __list_get(self, self.length - 1);
	}
	return $x;
}
function $p(self, $q) {
	const $r = $q;
	let $s = null;
	if ($r[0] === 0) {
		const turn = $r[1];
		$s = enqueue(turn, self[1].v);
	} else {
		const $y = $v(draining_turns.v);
		let $z = null;
		if ($y[0] === 0) {
			const draining = $y[1];
			$z = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$z = undefined;
		}
		$s = $z;
	}
	return $s;
}
function $n(self, value, $o) {
	self[0].v = __clone(value);
	$p(self, $o);
}
function $E(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $B(signal, observer) {
	const cell = signal[0];
	return $E(signal, mint_subscriber(() => {
		const $C = [ 0, cell ];
		let $D = null;
		if ($C[0] === 0) {
			const live = $C[1];
			$D = observer(live.v);
		} else {
			$D = undefined;
		}
		return $D;
	}));
}
function $A(self, observer) {
	const subscription = $B(self, observer);
	observer($k(self));
	return subscription;
}
function $i(sources, $j) {
	const snapshot = () => {
		return sources.map((source) => {
			return $k(source);
		});
	};
	const derived = $g(snapshot());
	sources.map((source) => {
		return $A(source, (_) => {
			$n(derived, snapshot(), $j);
			return;
		});
	});
	return derived;
}
function $H(self, $q) {
	const $I = $q;
	let $J = null;
	if ($I[0] === 0) {
		const turn = $I[1];
		$J = enqueue(turn, self[1].v);
	} else {
		const $K = $v(draining_turns.v);
		let $L = null;
		if ($K[0] === 0) {
			const draining = $K[1];
			$L = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$L = undefined;
		}
		$J = $L;
	}
	return $J;
}
function $G(self, value, $o) {
	self[0].v = __clone(value);
	$H(self, $o);
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
$G(count, 21, [ 1 ]);
console.log($k(both)[0]);
console.log($k(both)[1]);
