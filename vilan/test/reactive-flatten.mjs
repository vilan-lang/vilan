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
function subscriber_of(notify, derived) {
	return [ fresh_id(), notify, __shared_new(true), derived ];
}
function is_quiescent(self) {
	return $x(self[0].v) && $x(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $w = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$w = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$w;
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
				while (!($x(turn[1].v)) && budget > 0) {
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
	const $u = turn;
	let $v = null;
	if ($u[0] === 0) {
		const ambient = $u[1];
		$v = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $B = $y(draining_turns.v);
		let $C = null;
		if ($B[0] === 0) {
			const draining = $B[1];
			$C = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$C = undefined;
		}
		$v = $C;
	}
	return $v;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function retiring(subscriber, release) {
	return detached(subscriber[0], subscriber[2], release);
}
function detached(id, live, release) {
	let subscribers = [  ];
	const empty = __shared_new(subscribers);
	return [ empty, id, live, __shared_new([ 0, release ]) ];
}
function dispose(self, $ag) {
	const $ah = $ag;
	let $ai = null;
	if ($ah[0] === 0) {
		const established = $ah[1];
		$ai = [ 0, established ];
	} else {
		$ai = $y(draining_turns.v);
	}
	const ambient = $ai;
	release_under(self, ambient);
}
function detach(handle) {
	const $I = $y(releasing_turns.v);
	let $J = null;
	if ($I[0] === 0) {
		const at_release = $I[1];
		$J = at_release;
	} else {
		$J = $y(draining_turns.v);
	}
	const turn = $J;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $K = [ 0, handle[0] ];
	let $L = null;
	if ($K[0] === 0) {
		const subscribers = $K[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$L = undefined;
	} else {
		$L = undefined;
	}
	$L;
	const $M = ambient;
	let $N = null;
	if ($M[0] === 0) {
		const turn = $M[1];
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
		$N = undefined;
	} else {
		$N = undefined;
	}
	$N;
	const $O = handle[3].v;
	let $P = null;
	if ($O[0] === 0) {
		const release = $O[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$P = undefined;
	} else {
		$P = undefined;
	}
	return $P;
}
function register_with_owner(subscription, $aa, $ab) {
	const $ac = $ab;
	let $ad = null;
	if ($ac[0] === 0) {
		const owner = $ac[1];
		$ad = $ae(owner, subscription, $aa);
	} else {
		$ad = __clone(subscription);
	}
	return $ad;
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $Y = previous;
		let $Z = null;
		if ($Y[0] === 0) {
			const earlier = $Y[1];
			$Z = earlier();
		} else {
			$Z = undefined;
		}
		return $Z;
	} ];
}
function relay_to(subscriber) {
	return subscriber_of(() => {
		return wake(subscriber);
	}, true);
}
function detach_held(handle) {
	const $D = handle.v;
	let $E = null;
	if ($D[0] === 0) {
		const held = $D[1];
		$E = detach(held);
	} else {
		$E = undefined;
	}
	$E;
	handle.v = [ 1 ];
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $c(value) {
	return $b(value);
}
function $e(self, select) {
	return [ __clone(self), select ];
}
function $n(self) {
	return __clone(self[0].v);
}
function $p(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $o(self, subscriber) {
	return $p(self, subscriber);
}
function $m(self) {
	return [ () => {
		return $n(self);
	}, (subscriber) => {
		return $o(self, subscriber);
	}, () => {
		return;
	} ];
}
function $s(self, subscriber) {
	return $p(self, subscriber);
}
function $q(self) {
	return [ () => {
		return $n(self);
	}, (subscriber) => {
		return $s(self, subscriber);
	}, () => {
		return;
	} ];
}
function $x(self) {
	return self.length === 0;
}
function $y(self) {
	let $A = null;
	if ($x(self)) {
		$A = [ 1 ];
	} else {
		$A = __list_get(self, self.length - 1);
	}
	return $A;
}
function $l(self) {
	const select = self[1];
	const outer2 = $m(__clone(self[0]));
	const outer_pull = outer2[0];
	const followed = __shared_new($q(select(outer_pull())));
	const followed_handle = __shared_new([ 1 ]);
	return [ () => {
		return followed.v[0]();
	}, (subscriber) => {
		followed_handle.v = [ 0, followed.v[1](relay_to(subscriber)) ];
		const outer_handle = outer2[1](subscriber_of(() => {
			detach_held(followed_handle);
			followed.v[2]();
			const next = $q(select(outer_pull()));
			followed_handle.v = [ 0, next[1](relay_to(subscriber)) ];
			followed.v = next;
			wake(subscriber);
			return;
		}, true));
		return retiring(subscriber, () => {
			detach_held(followed_handle);
			detach(outer_handle);
			return;
		});
	}, () => {
		followed.v[2]();
		outer2[2]();
		return;
	} ];
}
function $S(self, $T) {
	const $U = $T;
	let $V = null;
	if ($U[0] === 0) {
		const turn = $U[1];
		$V = enqueue(turn, self[1].v);
	} else {
		const $W = $y(draining_turns.v);
		let $X = null;
		if ($W[0] === 0) {
			const draining = $W[1];
			$X = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$X = undefined;
		}
		$V = $X;
	}
	return $V;
}
function $Q(self, value, $R) {
	self[0].v = __clone(value);
	$S(self, $R);
}
function $ae(self, item, $af) {
	if (self[1].v) {
		dispose(item, $af);
	} else {
		self[0].v.push(() => {
			dispose(item, $af);
			return;
		});
	}
	return __clone(item);
}
function $i(self, $j, $k) {
	const instance = $l(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$Q(cached, pull(), $j);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $j, $k);
	return cached;
}
function $f(self, $g, $h) {
	return [ $i(self, $g, $h) ];
}
function $aj(self) {
	return $n(self[0]);
}
function $ak(self, value, $R) {
	self[0].v = __clone(value);
	$S(self, $R);
}
function $aq(self, transform) {
	return [ __clone(self), transform ];
}
function $av(self, subscriber) {
	return $p(self[0], subscriber);
}
function $au(self) {
	return [ () => {
		return $aj(self);
	}, (subscriber) => {
		return $av(self, subscriber);
	}, () => {
		return;
	} ];
}
function $at(self) {
	const transform = self[1];
	const upstream = $au(__clone(self[0]));
	const pull = upstream[0];
	return [ () => {
		return transform(pull());
	}, upstream[1], upstream[2] ];
}
function $as(self, $j, $k) {
	const instance = $at(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$Q(cached, pull(), $j);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $j, $k);
	return cached;
}
function $ar(self, $g, $h) {
	return [ $as(self, $g, $h) ];
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const first = $a(1);
const second = $a(10);
const outer = $c(first);
const joined = $f($e(__clone(outer), (inner) => {
	return __clone(inner);
}), [ 1 ], [ 1 ]);
console.log($aj(joined));
$Q(first, 2, [ 1 ]);
console.log($aj(joined));
$ak(outer, second, [ 1 ]);
console.log($aj(joined));
$Q(first, 99, [ 1 ]);
console.log($aj(joined));
$Q(second, 11, [ 1 ]);
console.log($aj(joined));
const doubled = $ar($aq(joined, (value) => {
	return value * 2;
}), [ 1 ], [ 1 ]);
$Q(second, 21, [ 1 ]);
console.log($aj(doubled));
