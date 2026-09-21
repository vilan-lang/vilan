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
function $j(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live2 = __shared_new(true);
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	signal[1].v.push([ id, () => {
		const $k = [ 0, cell ];
		let $l = null;
		if ($k[0] === 0) {
			const live3 = $k[1];
			$l = observer(live3.v);
		} else {
			$l = undefined;
		}
		return $l;
	}, live2, derived ]);
	return [ signal[1], id, live2, __shared_new([ 1 ]) ];
}
function $m(self) {
	return __clone(self[0].v);
}
function $i(self, observer) {
	const subscription = $j(self, observer);
	observer($m(self));
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
function $u(self) {
	return self.length === 0;
}
function $v(self) {
	return __list_get(self, self.length - 1);
}
function $p(self, $q) {
	const $r = $q;
	let $s = null;
	if ($r[0] === 0) {
		const turn = $r[1];
		$s = enqueue(turn, self[1].v);
	} else {
		const $w = $v(draining_turns.v);
		let $x = null;
		if ($w[0] === 0) {
			const draining = $w[1];
			$x = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$x = undefined;
		}
		$s = $x;
	}
	return $s;
}
function $n(self, value, $o) {
	self[0].v = __clone(value);
	$p(self, $o);
}
function $y(slot, $z, $A) {
	$d(slot, (inner) => {
		return console.log("holder " + $m(inner));
	}, $z, $A);
}
function $D(self) {
	return "plain box";
}
function $C(box) {
	console.log($D(box));
}
function $F(self) {
	return "marked box";
}
function $E(box) {
	console.log($F(box));
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
$a("static", [ 1 ]);
const live = $e("first");
$g(live, [ 1 ]);
$n(live, "second", [ 1 ]);
$y(live, [ 1 ]);
$C([ [  ] ]);
$E([ [  ] ]);
