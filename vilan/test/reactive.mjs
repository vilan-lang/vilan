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
	return $s(self[0].v) && $s(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $G = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$G = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$G;
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
				while (!($s(turn[1].v)) && budget > 0) {
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
function dispose(self, $o) {
	const $p = $o;
	let $q = null;
	if ($p[0] === 0) {
		const established = $p[1];
		$q = [ 0, established ];
	} else {
		$q = $r(draining_turns.v);
	}
	const ambient = $q;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $u = [ 0, handle[0] ];
	let $v = null;
	if ($u[0] === 0) {
		const subscribers = $u[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$v = undefined;
	} else {
		$v = undefined;
	}
	$v;
	const $w = ambient;
	let $x = null;
	if ($w[0] === 0) {
		const turn = $w[1];
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
		$x = undefined;
	} else {
		$x = undefined;
	}
	$x;
	const $y = handle[3].v;
	let $z = null;
	if ($y[0] === 0) {
		const release = $y[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$z = undefined;
	} else {
		$z = undefined;
	}
	return $z;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $c(self, transform) {
	return [ __clone(self), transform ];
}
function $i(self) {
	return __clone(self[0].v);
}
function $h(self) {
	return self[1]($i(self[0]));
}
function $g(source, observer) {
	return mint_subscriber(() => {
		return observer($h(source));
	});
}
function $l(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $k(self, subscriber) {
	return $l(self, subscriber);
}
function $j(self, subscriber) {
	return $k(self[0], subscriber);
}
function $f(source, observer) {
	const subscriber = $g(source, observer);
	return $j(source, subscriber);
}
function $e(self, observer) {
	return $f(self, observer);
}
function $d(self, observer) {
	const subscription = $e(self, observer);
	observer($h(self));
	return subscription;
}
function $s(self) {
	return self.length === 0;
}
function $r(self) {
	let $t = null;
	if ($s(self)) {
		$t = [ 1 ];
	} else {
		$t = __list_get(self, self.length - 1);
	}
	return $t;
}
function $m(self, item, $n) {
	if (self[1].v) {
		dispose(item, $n);
	} else {
		self[0].v.push(() => {
			dispose(item, $n);
			return;
		});
	}
	return __clone(item);
}
function $C(self, $D) {
	const $E = $D;
	let $F = null;
	if ($E[0] === 0) {
		const turn = $E[1];
		$F = enqueue(turn, self[1].v);
	} else {
		const $I = $r(draining_turns.v);
		let $J = null;
		if ($I[0] === 0) {
			const draining = $I[1];
			$J = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$J = undefined;
		}
		$F = $J;
	}
	return $F;
}
function $A(self, value, $B) {
	self[0].v = __clone(value);
	$C(self, $B);
}
function $K(self, transform, $L) {
	$A(self, transform($i(self)), $L);
}
function $N(signal, observer) {
	const cell = signal[0];
	return $l(signal, mint_subscriber(() => {
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
function $M(self, observer) {
	const subscription = $N(self, observer);
	observer($i(self));
	return subscription;
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const owner = new2();
const count = $a(0);
const doubled = $c(count, (n) => {
	return n * 2;
});
$m(owner, $d(doubled, (n) => {
	return console.log(n);
}), [ 1 ]);
$A(count, 1, [ 1 ]);
$K(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($h(doubled));
$m(owner, $M(count, (n) => {
	return console.log(n);
}), [ 1 ]);
$A(count, 20, [ 1 ]);
