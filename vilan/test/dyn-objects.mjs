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
function is_quiescent(self) {
	return $R(self[0].v) && $R(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $Q = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$Q = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$Q;
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
function dispose(self, $v) {
	self[2].v = false;
	const $w = [ 0, self[0] ];
	let $x = null;
	if ($w[0] === 0) {
		const subscribers = $w[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$x = undefined;
	} else {
		$x = undefined;
	}
	$x;
	const $y = $v;
	let $z = null;
	if ($y[0] === 0) {
		const established = $y[1];
		$z = [ 0, established ];
	} else {
		$z = $A(draining_turns.v);
	}
	const ambient = $z;
	const $B = ambient;
	let $C = null;
	if ($B[0] === 0) {
		const turn = $B[1];
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
		$C = undefined;
	} else {
		$C = undefined;
	}
	$C;
	const $D = self[3].v;
	let $E = null;
	if ($D[0] === 0) {
		const release = $D[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$E = undefined;
	} else {
		$E = undefined;
	}
	return $E;
}
function get_owner($s) {
	return $s;
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
function on_change(self, observer) {
	const by = self[1];
	return self[0][1].on_change(self[0][0], (n) => {
		return observer(n + by);
	});
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
function $i(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	signal[1].v.push([ id, () => {
		const $j = [ 0, cell ];
		let $k = null;
		if ($j[0] === 0) {
			const live2 = $j[1];
			$k = observer(live2.v);
		} else {
			$k = undefined;
		}
		return $k;
	}, live, derived ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $h(self, observer) {
	return $i(self, observer);
}
function $l(self, observer) {
	const subscription = $i(self, observer);
	observer($g(self));
	return subscription;
}
function $A(self) {
	return __list_get(self, self.length - 1);
}
function $t(self, item, $u) {
	if (self[1].v) {
		dispose(item, $u);
	} else {
		self[0].v.push(() => {
			dispose(item, $u);
			return;
		});
	}
	return __clone(item);
}
function $p(self, observer, $q, $r) {
	$t(get_owner($r), $h(self, observer), $q);
}
function $m(self, observer, $n, $o) {
	$p(self, observer, $n, $o);
	observer($g(self));
}
const $f = Object.create({get: $g, on_change: $h, sub: $l, effect: $m});
function $G(self, observer) {
	const subscription = on_change(self, observer);
	observer(get(self));
	return subscription;
}
function $I(self, observer, $q, $r) {
	$t(get_owner($r), on_change(self, observer), $q);
}
function $H(self, observer, $n, $o) {
	$I(self, observer, $n, $o);
	observer(get(self));
}
const $F = Object.create({get: get, on_change: on_change, sub: $G, effect: $H});
function $R(self) {
	return self.length === 0;
}
function $M(self, $N) {
	const $O = $N;
	let $P = null;
	if ($O[0] === 0) {
		const turn = $O[1];
		$P = enqueue(turn, self[1].v);
	} else {
		const $S = $A(draining_turns.v);
		let $T = null;
		if ($S[0] === 0) {
			const draining = $S[1];
			$T = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$T = undefined;
		}
		$P = $T;
	}
	return $P;
}
function $K(self, value, $L) {
	self[0].v = __clone(value);
	$M(self, $L);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
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
const sources = [ __clone([ root, $f ]), [ [ __clone([ root, $f ]), 100 ], $F ] ];
const watch = (($J) => {
	return $J[1].on_change($J[0], (n) => {
		return console.log("offset saw " + n);
	});
})(__at(sources, 1));
$K(root, 5, [ 1 ]);
for (const source of sources) {
	console.log(source[1].get(source[0]));
}
dispose(watch, [ 1 ]);
