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
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
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
function $d(self, react) {
	react(self);
}
function $a(label, $b, $c) {
	$d(label, (text) => {
		return console.log("[" + text + "]");
	}, $b, $c);
}
function $f(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $e(value) {
	return $f(value);
}
function $m(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $j(signal, observer) {
	const cell = signal[0];
	return $m(signal, mint_subscriber(() => {
		const $k = [ 0, cell ];
		let $l = null;
		if ($k[0] === 0) {
			const live2 = $k[1];
			$l = observer(live2.v);
		} else {
			$l = undefined;
		}
		return $l;
	}));
}
function $n(self) {
	return __clone(self[0].v);
}
function $i(self, observer) {
	const subscription = $j(self, observer);
	observer($n(self));
	return subscription;
}
function $h(self, react) {
	$i(self, react);
}
function $g(label, $b, $c) {
	$h(label, (text) => {
		return console.log("[" + text + "]");
	}, $b, $c);
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
		$t = enqueue(turn, self[1].v);
	} else {
		const $z = $w(draining_turns.v);
		let $A = null;
		if ($z[0] === 0) {
			const draining = $z[1];
			$A = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
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
function $B(slot, $C, $D) {
	$d(slot, (inner) => {
		return console.log("holder " + $n(inner));
	}, $C, $D);
}
function $G(self) {
	return "plain box";
}
function $F(box) {
	console.log($G(box));
}
function $I(self) {
	return "marked box";
}
function $H(box) {
	console.log($I(box));
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
$a("static", [ 1 ]);
const live = $e("first");
$g(live, [ 1 ]);
$o(live, "second", [ 1 ]);
$B(live, [ 1 ]);
$F([ [  ] ]);
$H([ [  ] ]);
