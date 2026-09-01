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
		while (!($t(turn[0].v)) && budget > 0) {
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
function $b(items) {
	return 1;
}
function $c(head, rest) {
	return head;
}
function $d(items) {
	return 1;
}
function $f(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $e(value) {
	return $f(value);
}
function $h(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $g(value) {
	return $h(value);
}
function $k(self) {
	return self[0].v;
}
function $m(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $l(value) {
	return $m(value);
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
				subscriber[1]();
			}
			$w = undefined;
		}
		$s = $w;
	}
	return $s;
}
function $n(self, value, $o) {
	self[0].v = value;
	$p(self, $o);
}
function $y(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $x(self, observer) {
	const subscription = $y(self, observer);
	observer($k(self));
	return subscription;
}
function $i(sources, $j) {
	const snapshot = () => {
		return sources.map((source) => {
			return $k(source);
		});
	};
	const derived = $l(snapshot());
	sources.map((source) => {
		return $x(source, (_) => {
			$n(derived, snapshot(), $j);
			return;
		});
	});
	return derived;
}
function $z(self) {
	return self[0].v;
}
function $B(self, $q) {
	const $C = $q;
	let $D = null;
	if ($C[0] === 0) {
		const turn = $C[1];
		$D = enqueue(turn, self[1].v);
	} else {
		const $E = $u(draining_turns.v);
		let $F = null;
		if ($E[0] === 0) {
			const draining = $E[1];
			$F = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$F = undefined;
		}
		$D = $F;
	}
	return $D;
}
function $A(self, value, $o) {
	self[0].v = value;
	$B(self, $o);
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
console.log(middle([ 4, 5, 6 ]));
console.log($a([  ]));
console.log($b([ 1 ]));
console.log($c(2, [ 3, 4 ]));
console.log(swap_pack([ 1, 2 ]));
console.log(outer([ 3, 4 ]));
const inner = [ 10, 11 ];
console.log($d([ ...inner, 12 ]));
const count = $e(20);
const name = $g("hi");
const both = $i([ __clone(count), name ], [ 1 ]);
console.log($z(both)[0]);
$A(count, 21, [ 1 ]);
console.log($z(both)[0]);
console.log($z(both)[1]);
