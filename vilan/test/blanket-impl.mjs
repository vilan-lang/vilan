function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
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
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		let seen = false;
		for (const queued of turn[0].v) {
			if (queued[0] === subscriber[0]) {
				seen = true;
			}
		}
		if (!(seen)) {
			turn[0].v.push(__clone(subscriber));
		}
	}
	if (turn[2].v && !(turn[3].v) && !(turn[1].v)) {
		turn[3].v = true;
		queueMicrotask(() => {
			turn[3].v = false;
			drain(turn);
			return;
		});
	}
}
function drain(turn) {
	if (!(turn[1].v)) {
		turn[1].v = true;
		draining_turns.v.push(__clone(turn));
		let budget = 100000;
		while (!($r(turn[0].v)) && budget > 0) {
			const wave = turn[0].v;
			turn[0].v = [  ];
			for (const subscriber of wave) {
				subscriber[1]();
				budget = budget - 1;
			}
		}
		__list_pop(draining_turns.v);
		turn[1].v = false;
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
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $k(self) {
	return self[0].v;
}
function $i(self, observer) {
	const subscription = $j(self, observer);
	observer($k(self));
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
function $r(self) {
	return self.length === 0;
}
function $s(self) {
	return __list_get(self, self.length - 1);
}
function $n(self, $o) {
	const $p = $o;
	let $q = null;
	if ($p[0] === 0) {
		const turn = $p[1];
		$q = enqueue(turn, self[1].v);
	} else {
		const $t = $s(draining_turns.v);
		let $u = null;
		if ($t[0] === 0) {
			const draining = $t[1];
			$u = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$u = undefined;
		}
		$q = $u;
	}
	return $q;
}
function $l(self, value, $m) {
	self[0].v = value;
	$n(self, $m);
}
function $y(self, react) {
	react(self);
}
function $v(slot, $w, $x) {
	$y(slot, (inner) => {
		return console.log("holder " + $k(inner));
	}, $w, $x);
}
function $A(self) {
	return "plain box";
}
function $z(box) {
	console.log($A(box));
}
function $C(self) {
	return "marked box";
}
function $B(box) {
	console.log($C(box));
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
$a("static", [ 1 ]);
const live = $e("first");
$g(live, [ 1 ]);
$l(live, "second", [ 1 ]);
$v(live, [ 1 ]);
$z([ [  ] ]);
$B([ [  ] ]);
