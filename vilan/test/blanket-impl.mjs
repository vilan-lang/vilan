function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
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
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		if (!(turn[1].v.has(key))) {
			turn[1].v.set(key, true);
			turn[0].v.push(__clone(subscriber));
		}
	}
	if (turn[3].v && !(turn[4].v) && !(turn[2].v)) {
		turn[4].v = true;
		queueMicrotask(() => {
			turn[4].v = false;
			drain(turn);
			return;
		});
	}
}
function drain(turn) {
	if (!(turn[2].v)) {
		turn[2].v = true;
		draining_turns.v.push(__clone(turn));
		__with_finally(() => {
			let budget = 100000;
			while (!($t(turn[0].v)) && budget > 0) {
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[1].v = new Map();
				for (const subscriber of wave) {
					if (subscriber[2].v) {
						subscriber[1]();
					}
					budget = budget - 1;
				}
			}
			return;
		}, () => {
			__list_pop(draining_turns.v);
			turn[2].v = false;
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
	}, live2 ]);
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
function $t(self) {
	return self.length === 0;
}
function $u(self) {
	return __list_get(self, self.length - 1);
}
function $p(self, $q) {
	const $r = $q;
	let $s = null;
	if ($r[0] === 0) {
		const turn = $r[1];
		$s = enqueue(turn, self[1].v);
	} else {
		const $v = $u(draining_turns.v);
		let $w = null;
		if ($v[0] === 0) {
			const draining = $v[1];
			$w = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$w = undefined;
		}
		$s = $w;
	}
	return $s;
}
function $n(self, value, $o) {
	self[0].v = __clone(value);
	$p(self, $o);
}
function $x(slot, $y, $z) {
	$d(slot, (inner) => {
		return console.log("holder " + $m(inner));
	}, $y, $z);
}
function $C(self) {
	return "plain box";
}
function $B(box) {
	console.log($C(box));
}
function $E(self) {
	return "marked box";
}
function $D(box) {
	console.log($E(box));
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
$a("static", [ 1 ]);
const live = $e("first");
$g(live, [ 1 ]);
$n(live, "second", [ 1 ]);
$x(live, [ 1 ]);
$B([ [  ] ]);
$D([ [  ] ]);
