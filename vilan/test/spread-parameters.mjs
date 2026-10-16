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
let __shared_identity_next = 1;
function __shared_identity(cell) {
	return cell.__id ??= __shared_identity_next++;
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
function mint_subscriber(notify3) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify3, derived);
}
function subscriber_of(notify3, derived) {
	return [ fresh_id(), notify3, __shared_new(true), derived ];
}
function is_quiescent(self) {
	return is_empty(self[0].v) && is_empty(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $h = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$h = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			if (tracing.v) {
				trace_hooks.v[3](turn, subscriber[0]);
			}
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:421:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:424:25");
		}
		$h;
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
						if (tracing.v) {
							trace_hooks.v[4](turn, subscriber);
						} else if (subscriber[2].v) {
							subscriber[1]();
						}
						budget = budget - 1;
					}
				}
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[2].v = new Map();
				for (const subscriber2 of wave) {
					if (tracing.v) {
						trace_hooks.v[4](turn, subscriber2);
					} else if (subscriber2[2].v) {
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
function queue_wakes(turn, subscribers, traced, cause) {
	if (traced) {
		waking.v = [ 0, cause ];
		enqueue(turn, subscribers);
		waking.v = [ 1 ];
	} else {
		enqueue(turn, subscribers);
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
function width(items) {
	return 1;
}
function after(head, rest) {
	return head;
}
function new2(value, caller) {
	let subscribers = [  ];
	const cell = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell[0]), caller);
	}
	return cell;
}
function new3(value) {
	return new2(value, "std/src/reactive.vl:2448:15");
}
function new4(value) {
	return new2(value, "std/src/reactive.vl:2448:15");
}
function get(self) {
	return __clone(self[0].v);
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $i = null;
	if (is_empty(self)) {
		$i = [ 1 ];
	} else {
		$i = __list_get(self, self.length - 1);
	}
	return $i;
}
function notify(self, $d) {
	const traced = tracing.v;
	let $e = null;
	if (traced) {
		$e = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$e = "";
	}
	const cause = $e;
	const $f = $d;
	let $g = null;
	if ($f[0] === 0) {
		const turn = $f[1];
		$g = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $j = last(draining_turns.v);
		let $k = null;
		if ($j[0] === 0) {
			const draining = $j[1];
			$k = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $l = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$l = undefined;
				}
				$l;
			}
			$k = undefined;
		}
		$g = $k;
	}
	return $g;
}
function set(self, value, $c, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $c);
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function observe(signal, observer) {
	const cell = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $m = [ 0, cell ];
		let $n = null;
		if ($m[0] === 0) {
			const live = $m[1];
			$n = observer(live.v);
		} else {
			$n = undefined;
		}
		return $n;
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
function gather(sources, $a) {
	const snapshot = () => {
		return sources.map((source) => {
			return get(source);
		});
	};
	const derived = new4(snapshot());
	sources.map((source) => {
		return sub(__clone(source), (_, $b) => {
			set(derived, snapshot(), $a, "spread-parameters.vl:40:11");
			return;
		});
	});
	return derived;
}
function notify2(self, $d) {
	const traced = tracing.v;
	let $o = null;
	if (traced) {
		$o = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$o = "";
	}
	const cause = $o;
	const $p = $d;
	let $q = null;
	if ($p[0] === 0) {
		const turn = $p[1];
		$q = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $r = last(draining_turns.v);
		let $s = null;
		if ($r[0] === 0) {
			const draining = $r[1];
			$s = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $t = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$t = undefined;
				}
				$t;
			}
			$s = undefined;
		}
		$q = $s;
	}
	return $q;
}
function set2(self, value, $c, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify2(self, $c);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const tracing = __shared_new(false);
const trace_hooks = __shared_new([ (_identity, _site) => {
	return;
}, (_site) => {
	return;
}, (_identity) => {
	return "";
}, (_turn, _id) => {
	return;
}, (_turn, _subscriber) => {
	return;
}, (_subscriber, _cause) => {
	return;
}, (_subscriber, _draining) => {
	return false;
}, (_turn, _id) => {
	return;
} ]);
const waking = __shared_new([ 1 ]);
console.log(String(middle([ 4, 5, 6 ])));
console.log(String(width([  ])));
console.log(String(width([ 1 ])));
console.log(String(after(2, [ 3, 4 ])));
console.log(String(swap_pack([ 1, 2 ])));
console.log(String(outer([ 3, 4 ])));
const inner = [ 10, 11 ];
console.log(String(width([ ...inner, 12 ])));
const count = new3(20);
const name = new4("hi");
const both = gather([ __clone(count), name ], [ 1 ]);
console.log(String(get(both)[0]));
set2(count, 21, [ 1 ], "spread-parameters.vl:59:8");
console.log(String(get(both)[0]));
console.log(get(both)[1]);
//# sourceMappingURL=spread-parameters.mjs.map
