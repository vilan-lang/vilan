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
function hash2(self) {
	return __hash(self);
}
function new2() {
	const table = new Map();
	return [ table ];
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function mint_subscriber(notify4) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify4, derived);
}
function subscriber_of(notify4, derived) {
	return [ fresh_id(), notify4, __shared_new(true), derived ];
}
function new3() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return is_empty(self[0].v) && is_empty(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash2(subscriber[0]);
		let $e = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$e = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:414:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:417:25");
		}
		$e;
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
				while (!(is_empty(turn[1].v)) && budget > 0) {
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
function dispose(self, $u) {
	const $v = $u;
	let $w = null;
	if ($v[0] === 0) {
		const established = $v[1];
		$w = [ 0, established ];
	} else {
		$w = last(draining_turns.v);
	}
	const ambient = $w;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $x = [ 0, handle[0] ];
	let $y = null;
	if ($x[0] === 0) {
		const subscribers = $x[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$y = undefined;
	} else {
		$y = undefined;
	}
	$y;
	const $z = ambient;
	let $A = null;
	if ($z[0] === 0) {
		const turn = $z[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== handle[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[2].v.delete(hash2(handle[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn[1].v) {
			if (subscriber3[0] !== handle[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn[1].v = kept_derived;
		turn[3].v.delete(hash2(handle[1]));
		$A = undefined;
	} else {
		$A = undefined;
	}
	$A;
	const $B = handle[3].v;
	let $C = null;
	if ($B[0] === 0) {
		const release = $B[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$C = undefined;
	} else {
		$C = undefined;
	}
	return $C;
}
function new4() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $D = null;
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
		$D = undefined;
	}
	return $D;
}
function new5(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function new6(value) {
	return new5(value);
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $f = null;
	if (is_empty(self)) {
		$f = [ 1 ];
	} else {
		$f = __list_get(self, self.length - 1);
	}
	return $f;
}
function notify(self, $b) {
	const $c = $b;
	let $d = null;
	if ($c[0] === 0) {
		const turn = $c[1];
		$d = enqueue(turn, __clone(self[1].v));
	} else {
		const $g = last(draining_turns.v);
		let $h = null;
		if ($g[0] === 0) {
			const draining = $g[1];
			$h = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$h = undefined;
		}
		$d = $h;
	}
	return $d;
}
function update(self, mutate, $a) {
	mutate(self[0].v);
	notify(self, $a);
}
function get(self) {
	return __clone(self[0].v);
}
function new10(value) {
	return new5(value);
}
function insert(self, key, value) {
	self[0].set(hash(key), [ __clone(key), __clone(value) ]);
}
function notify2(self, $b) {
	const $i = $b;
	let $j = null;
	if ($i[0] === 0) {
		const turn = $i[1];
		$j = enqueue(turn, __clone(self[1].v));
	} else {
		const $k = last(draining_turns.v);
		let $l = null;
		if ($k[0] === 0) {
			const draining = $k[1];
			$l = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$l = undefined;
		}
		$j = $l;
	}
	return $j;
}
function update2(self, mutate, $a) {
	mutate(self[0].v);
	notify2(self, $a);
}
function len(self) {
	return self[0].size;
}
function update3(self, mutate, $a) {
	mutate([ self[0], "v" ]);
	notify2(self, $a);
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function observe(signal, observer) {
	const cell = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $r = [ 0, cell ];
		let $s = null;
		if ($r[0] === 0) {
			const live = $r[1];
			$s = observer(live.v);
		} else {
			$s = undefined;
		}
		return $s;
	}));
}
function attach_observer(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function sub(self, observer) {
	return attach_observer(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function take(self, item, $t) {
	defer(self, () => {
		dispose(item, $t);
		return;
	});
	return __clone(item);
}
function batch(body, $F) {
	const $G = $F;
	let $H = null;
	if ($G[0] === 0) {
		const current = $G[1];
		$H = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$H = result;
	}
	return $H;
}
function set(self, value, $J) {
	self[0].v = __clone(value);
	notify2(self, $J);
}
function set_with(self, transform, $I) {
	set(self, transform(get(self)), $I);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const owner = new4();
const todos = new6([ 1, 2 ]);
update(todos, (list) => {
	list.push(5);
	return;
}, [ 1 ]);
console.log(get(todos).length);
const scores = new10(new2());
update2(scores, (entries) => {
	insert(entries, "a", 1);
	insert(entries, "b", 2);
	return;
}, [ 1 ]);
console.log(len(get(scores)));
const count = new10(1);
update3(count, (value) => {
	value[0][value[1]] = value[0][value[1]] + 10;
	return;
}, [ 1 ]);
console.log(get(count));
const watched = new6([ 0 ]);
take(owner, sub(__clone(watched), (list, $q) => {
	return console.log("len " + list.length);
}), [ 1 ]);
update(watched, (list) => {
	list.push(1);
	list.push(2);
	return;
}, [ 1 ]);
update(watched, (list) => {
	return;
}, [ 1 ]);
console.log("---");
batch(($E) => {
	update(watched, (list) => {
		list.push(3);
		return;
	}, [ 0, $E ]);
	update(watched, (list) => {
		list.push(4);
		return;
	}, [ 0, $E ]);
	console.log("inside");
	return;
}, [ 1 ]);
update(todos, (list) => {
	list.push(6);
	console.log("reentrant " + get(todos).length);
	return;
}, [ 1 ]);
set_with(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log(get(count));
