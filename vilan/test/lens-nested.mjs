function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_put(list, index, value, location) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_view(list, index, location) {
	if (index >= 0 && index < list.length) return [ list, index ];
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
function __remove_at(list, index, location) {
	if (index >= 0 && index < list.length) return list.splice(index, 1)[0];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
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
function eq(self, b) {
	if (self.length !== b.length) {
		return false;
	}
	let index = 0;
	for (const item2 of self) {
		if (!(item2 === __at(b, index, "std/src/compare.vl:147:17"))) {
			return false;
		}
		index = index + 1;
	}
	return true;
}
function hash(self) {
	return __hash(self);
}
function assert(condition, message, caller) {
	let $bd = null;
	if (!(condition)) {
		$bd = (() => {
			throw __panic(message, caller);
		})();
	}
	return $bd;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $h = null;
	if (wrapped < 0) {
		$h = wrapped + modulus;
	} else {
		$h = wrapped;
	}
	return $h;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $g = null;
	if (truncated > 0) {
		$g = truncated;
	} else {
		$g = 0;
	}
	return $g;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $i = null;
	if (wrapped >= half) {
		$i = wrapped - modulus;
	} else {
		$i = wrapped;
	}
	return $i;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function eq2(self, b) {
	return self === b;
}
function eq3(self, b) {
	const $bj = self;
	let $bm = null;
	if ($bj[0] === 0) {
		const $bk = b;
		let $bl = null;
		if ($bk[0] === 0) {
			$bl = $bj[1] === $bk[1];
		} else {
			$bl = false;
		}
		$bm = $bl;
	} else {
		const $bn = b;
		$bm = $bn[0] === 1;
	}
	return $bm;
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function mint_subscriber(notify2) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify2, derived);
}
function subscriber_of(notify2, derived) {
	return [ fresh_id(), notify2, __shared_new(true), derived ];
}
function new2() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return is_empty(self[0].v) && is_empty(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $W = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$W = undefined;
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
		$W;
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
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $o = null;
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
		$o = undefined;
	}
	return $o;
}
function defer_to_owner(cleanup, $l) {
	const $m = $l;
	let $n = null;
	if ($m[0] === 0) {
		const owner = $m[1];
		$n = defer(owner, cleanup);
	} else {
		$n = undefined;
	}
	return $n;
}
function place_subscription(subscription, site) {
	if (tracing.v) {
		trace_hooks.v[8](subscription[1], site);
	}
	return subscription;
}
function settle_together(body, $ac) {
	const $ad = $ac;
	let $ae = null;
	if ($ad[0] === 0) {
		const current = $ad[1];
		$ae = body(current);
	} else {
		const $af = last(draining_turns.v);
		let $ag = null;
		if ($af[0] === 0) {
			const draining = $af[1];
			$ag = body(draining);
		} else {
			$ag = batch(body, $ac);
		}
		$ae = $ag;
	}
	return $ae;
}
function new3() {
	return [ [  ], [  ], [  ], [  ], [  ], [  ], 0 ];
}
function seat(self, at2) {
	let low = 0;
	let high = self[3].length;
	while (high - low > 1) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[3], middle, "std/src/reactive/lens.vl:157:7") <= at2) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at2 - __at(self[3], low, "std/src/reactive/lens.vl:163:14") ];
}
function restart(self) {
	let starts = [  ];
	let running = 0;
	let place = 0;
	for (const block of self[1]) {
		starts.push(running);
		__at_put(self[2], as_usize(block), place, "std/src/reactive/lens.vl:173:4");
		running = running + __at(self[0], as_usize(block), "std/src/reactive/lens.vl:174:15").length;
		place = place + 1;
	}
	self[3] = starts;
}
function renumber(self, block, from) {
	let index = from;
	const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:183:14").length;
	while (index < held) {
		const slot3 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:185:15"), index, "std/src/reactive/lens.vl:185:15");
		if (slot3 !== -(1)) {
			__at_put(self[4], as_usize(slot3), block, "std/src/reactive/lens.vl:187:5");
			__at_put(self[5], as_usize(slot3), index, "std/src/reactive/lens.vl:188:5");
		}
		index = index + 1;
	}
}
function open(self, place) {
	const block = as_i32(self[0].length);
	self[0].push([  ]);
	self[2].push(0);
	__insert_at(self[1], place, block, "std/src/reactive/lens.vl:199:14");
	return block;
}
function widen(self, at2, count2) {
	if (count2 === 0 || at2 >= self[6]) {
		return;
	}
	const $C = seat(self, at2);
	const place = $C[0];
	const index = $C[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:209:15");
	let $D = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:213:5"), index, -(1), "std/src/reactive/lens.vl:213:35");
			made = made + 1;
		}
		$D = undefined;
	} else {
		const before = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:217:17"));
		let grown = [  ];
		let copied = 0;
		while (copied < index) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:221:16"));
			copied = copied + 1;
		}
		let made2 = 0;
		while (made2 < count2) {
			grown.push(-(1));
			made2 = made2 + 1;
		}
		const held = before.length;
		while (copied < held) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:231:16"));
			copied = copied + 1;
		}
		__at_put(self[0], as_usize(block), grown, "std/src/reactive/lens.vl:234:4");
	}
	$D;
	self[6] = self[6] + count2;
	renumber(self, block, index + count2);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:238:6").length > 2 * block_size) {
		rebalance(self, place);
	}
	restart(self);
}
function close(self, at2, count2, wakes) {
	let left = count2;
	while (left > 0 && at2 < self[6]) {
		const $A = seat(self, at2);
		const place = $A[0];
		const index = $A[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:250:16");
		const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:251:15").length;
		let $B = null;
		if (left < held - index) {
			$B = left;
		} else {
			$B = held - index;
		}
		const take = $B;
		let taken = 0;
		while (taken < take) {
			const slot3 = __remove_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:255:16"), index, "std/src/reactive/lens.vl:255:46");
			if (slot3 !== -(1)) {
				__at_put(self[4], as_usize(slot3), -(1), "std/src/reactive/lens.vl:257:6");
				wakes.push([ 2, slot3 ]);
			}
			taken = taken + 1;
		}
		self[6] = self[6] - take;
		left = left - take;
		if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:264:7").length === 0) {
			__remove_at(self[1], place, "std/src/reactive/lens.vl:265:16");
		} else {
			renumber(self, block, index);
		}
		restart(self);
	}
}
function rebalance(self, place) {
	const block = __at(self[1], place, "std/src/reactive/lens.vl:275:15");
	const all = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:276:13"));
	let first = [  ];
	let index = 0;
	while (index < block_size) {
		first.push(__at(all, index, "std/src/reactive/lens.vl:280:15"));
		index = index + 1;
	}
	__at_put(self[0], as_usize(block), first, "std/src/reactive/lens.vl:283:3");
	let next = place + 1;
	const held = all.length;
	while (index < held) {
		const fresh = open(self, next);
		let filled = 0;
		while (filled < block_size && index < held) {
			__at(self[0], as_usize(fresh), "std/src/reactive/lens.vl:290:5").push(__at(all, index, "std/src/reactive/lens.vl:290:40"));
			index = index + 1;
			filled = filled + 1;
		}
		renumber(self, fresh, 0);
		next = next + 1;
	}
}
function follow(self, at2) {
	while (self[6] <= at2) {
		const places = self[1].length;
		if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:304:34")), "std/src/reactive/lens.vl:304:22").length >= block_size) {
			const fresh = open(self, places);
			__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:306:5");
			self[3].push(self[6]);
		}
		const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:309:15");
		__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:310:4").push(-(1));
		self[6] = self[6] + 1;
	}
	const $j = seat(self, at2);
	const place = $j[0];
	const index = $j[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:314:15");
	const held = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:315:14"), index, "std/src/reactive/lens.vl:315:14");
	if (held !== -(1)) {
		return held;
	}
	const slot3 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:322:3"), index, slot3, "std/src/reactive/lens.vl:322:3");
	return slot3;
}
function apply(self, change3, wakes) {
	const $y = change3;
	let $z = null;
	if ($y[0] === 0) {
		const at2 = $y[1];
		const removed = $y[2];
		const inserted = $y[3];
		close(self, at2, removed, wakes);
		widen(self, at2, inserted);
		$z = undefined;
	} else if ($y[0] === 1) {
		const at3 = $y[1];
		const count2 = $y[2];
		$z = widen(self, at3, count2);
	} else if ($y[0] === 3) {
		const at4 = $y[1];
		const count3 = $y[2];
		$z = close(self, at4, count3, wakes);
	} else if ($y[0] === 2) {
		const at5 = $y[1];
		let $F = null;
		if (at5 < self[6]) {
			const $E = seat(self, at5);
			const place = $E[0];
			const index = $E[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:339:29")), "std/src/reactive/lens.vl:339:17"), index, "std/src/reactive/lens.vl:339:17");
			if (slot3 !== -(1)) {
				wakes.push([ 0, slot3 ]);
			}
			$F = undefined;
		}
		$z = $F;
	} else if ($y[0] === 4) {
		const at6 = $y[1];
		const part2 = $y[2];
		let $H = null;
		if (at6 < self[6]) {
			const $G = seat(self, at6);
			const place2 = $G[0];
			const index2 = $G[1];
			const slot4 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:348:29")), "std/src/reactive/lens.vl:348:17"), index2, "std/src/reactive/lens.vl:348:17");
			if (slot4 !== -(1)) {
				wakes.push([ 1, slot4, part2 ]);
			}
			$H = undefined;
		}
		$z = $H;
	} else if ($y[0] === 5) {
		const from = $y[1];
		const to = $y[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $I = null;
		if (from > to) {
			$I = from;
		} else {
			$I = to;
		}
		const far = $I;
		while (self[6] <= far) {
			const places = self[1].length;
			if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:362:36")), "std/src/reactive/lens.vl:362:24").length >= block_size) {
				const fresh = open(self, places);
				__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:364:7");
				self[3].push(self[6]);
			}
			const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:367:17");
			__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:368:6").push(-(1));
			self[6] = self[6] + 1;
		}
		const $J = seat(self, from);
		const left = $J[0];
		const index3 = $J[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:372:18");
		const slot5 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:373:16"), index3, "std/src/reactive/lens.vl:373:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:375:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:376:17");
		} else {
			renumber(self, source, index3);
		}
		restart(self);
		let $K = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$K = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:384:25")), "std/src/reactive/lens.vl:384:13").length ];
		} else {
			$K = seat(self, to);
		}
		const $L = $K;
		const place3 = $L[0];
		const landing = $L[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:388:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:389:5"), landing, slot5, "std/src/reactive/lens.vl:389:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing);
		restart(self);
		$z = undefined;
	} else {
		let slot6 = 0;
		const slots = as_i32(self[4].length);
		while (slot6 < slots) {
			if (__at(self[4], as_usize(slot6), "std/src/reactive/lens.vl:398:9") !== -(1)) {
				__at_put(self[4], as_usize(slot6), -(1), "std/src/reactive/lens.vl:399:7");
				wakes.push([ 2, slot6 ]);
			}
			slot6 = slot6 + 1;
		}
		self[0] = [  ];
		self[1] = [  ];
		self[2] = [  ];
		self[3] = [  ];
		self[6] = 0;
		$z = undefined;
	}
	return $z;
}
function follows_items(self) {
	return true;
}
function release(self, slot3) {
	const block = __at(self[4], as_usize(slot3), "std/src/reactive/lens.vl:418:15");
	if (block === -(1)) {
		return;
	}
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:422:3"), __at(self[5], as_usize(slot3), "std/src/reactive/lens.vl:422:33"), -(1), "std/src/reactive/lens.vl:422:3");
	__at_put(self[4], as_usize(slot3), -(1), "std/src/reactive/lens.vl:423:3");
}
function locate(self, slot3) {
	const block = __at(self[4], as_usize(slot3), "std/src/reactive/lens.vl:427:15");
	if (block === -(1)) {
		return [ 1 ];
	}
	return [ 0, __at(self[3], __at(self[2], as_usize(block), "std/src/reactive/lens.vl:431:20"), "std/src/reactive/lens.vl:431:8") + __at(self[5], as_usize(slot3), "std/src/reactive/lens.vl:431:51") ];
}
function fresh_cells() {
	return [ new4(0, "std/src/reactive/lens.vl:888:33"), __shared_new([  ]), __shared_new(0) ];
}
function wake_all(cells, slot3, $R) {
	const held = viewing2(cells, (list) => {
		return __clone(__at(list, as_usize(slot3), "std/src/reactive/lens.vl:928:35"));
	});
	notify(held[0], $R);
	for (const part2 of __clone(held[1].v)) {
		notify(part2[1], $R);
	}
}
function wake_part(cells, slot3, name, $ab) {
	const held = viewing2(cells, (list) => {
		return __clone(__at(list, as_usize(slot3), "std/src/reactive/lens.vl:936:35"));
	});
	notify(held[0], $ab);
	for (const part2 of __clone(held[1].v)) {
		if (part2[0] === name) {
			notify(part2[1], $ab);
		}
	}
}
function item(self, at2) {
	return __list_get(self, at2);
}
function lend(self, at2, read) {
	if (at2 < self.length) {
		read(__at(self, at2, "std/src/reactive/lens.vl:2008:10"));
	}
}
function put(self, at2, value) {
	__at_put(self, at2, __clone(value), "std/src/reactive/lens.vl:2003:3");
}
function change(self, at2, with2) {
	if (at2 < self.length) {
		with2(__at(self, at2, "std/src/reactive/lens.vl:2014:14"));
	}
}
function new4(value, caller) {
	let subscribers = [  ];
	const cell = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell[0]), caller);
	}
	return cell;
}
function over(value, handler, access) {
	return [ [ 0, __shared_new(value) ], __shared_new(handler), __clone(access), __shared_new([  ]), new4(0, "std/src/reactive/lens.vl:647:28"), new4(0, "std/src/reactive/lens.vl:648:24"), new4(0, "std/src/reactive/lens.vl:649:24"), __shared_new([  ]), __shared_new(0), __shared_new([ 1 ]) ];
}
function sequence(value, $a) {
	return over(value, new3(), [ (held, at2) => {
		return item(held, at2);
	}, (held, at2, read) => {
		return lend(held, at2, read);
	}, (held, at2, value2) => {
		return put(held, at2, value2, $a);
	}, (held, at2, with2) => {
		return change(held, at2, with2, $a);
	} ]);
}
function lend2(value, read) {
	return read(value);
}
function viewing(cell, read) {
	return lend2(cell.v, read);
}
function viewing2(cell, read) {
	return lend2(cell.v, read);
}
function peek_storage(storage, read) {
	const $c = storage;
	let $d = null;
	if ($c[0] === 0) {
		const cell = $c[1];
		$d = viewing2(cell, read);
	} else {
		const port = $c[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $e = out;
		let $f = null;
		if ($e[0] === 0) {
			const value = __clone($e[1]);
			$f = value;
		} else {
			$f = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:632:13");
			})();
		}
		$d = $f;
	}
	return $d;
}
function holds(storage, view, at2) {
	let found = false;
	peek_storage(storage, (held) => {
		return view(held, at2, (_item) => {
			return found = true;
		});
	});
	return found;
}
function follow_id(storage, handler, cells, view, at2) {
	if (viewing(handler, (held) => {
		return follows_items(held);
	}) && !(holds(storage, view, at2))) {
		return -(1);
	}
	const id = follow(handler.v, at2);
	while (cells.v.length <= as_usize(id)) {
		cells.v.push(fresh_cells());
	}
	const slot_cells = viewing2(cells, (held) => {
		return __clone(__at(held, as_usize(id), "std/src/reactive/lens.vl:909:41"));
	});
	const $k = [ slot_cells[2], "v" ];
	slot_cells[2].v = $k[0][$k[1]] + 1;
	return id;
}
function slot_of(self, id) {
	if (id < 0) {
		return [ __clone(self), -(1), fresh_cells() ];
	}
	return [ __clone(self), id, viewing2(self[3], (held) => {
		return __clone(__at(held, as_usize(id), "std/src/reactive/lens.vl:827:66"));
	}) ];
}
function follow2(self, at2) {
	return slot_of(self, follow_id(self[0], self[1], self[3], self[2][1], at2));
}
function release_id(handler, cells, id) {
	if (id < 0) {
		return;
	}
	const slot_cells = viewing2(cells, (held) => {
		return __clone(__at(held, as_usize(id), "std/src/reactive/lens.vl:919:41"));
	});
	const left = slot_cells[2].v - 1;
	slot_cells[2].v = left;
	if (left <= 0) {
		release(handler.v, id);
	}
}
function release2(self) {
	release_id(self[0][1], self[0][3], self[1]);
}
function slot(self, at2, $b) {
	const handle = follow2(self, at2);
	defer_to_owner(() => {
		return release2(handle);
	}, $b);
	return handle;
}
function at(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][1], (handler) => {
		return locate(handler, self[1]);
	});
}
function coming(self, changes) {
	if (self[7].v.length === 0) {
		return;
	}
	for (const tap of __clone(self[7].v)) {
		for (const change3 of changes) {
			tap[1](change3);
		}
	}
}
function mutate(self, change3) {
	const $t = self[0];
	let $u = null;
	if ($t[0] === 0) {
		const cell = $t[1];
		$u = change3(cell.v);
	} else {
		const port = $t[1];
		$u = port[1](change3);
	}
	return $u;
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $X = null;
	if (is_empty(self)) {
		$X = [ 1 ];
	} else {
		$X = __list_get(self, self.length - 1);
	}
	return $X;
}
function notify(self, $S) {
	const traced = tracing.v;
	let $T = null;
	if (traced) {
		$T = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$T = "";
	}
	const cause = $T;
	const $U = $S;
	let $V = null;
	if ($U[0] === 0) {
		const turn = $U[1];
		$V = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $Y = last(draining_turns.v);
		let $Z = null;
		if ($Y[0] === 0) {
			const draining = $Y[1];
			$Z = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $aa = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$aa = undefined;
				}
				$aa;
			}
			$Z = undefined;
		}
		$V = $Z;
	}
	return $V;
}
function batch(body, $ah) {
	const $ai = $ah;
	let $aj = null;
	if ($ai[0] === 0) {
		const current = $ai[1];
		$aj = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$aj = result;
	}
	return $aj;
}
function landed(self, changes, late, $v) {
	if (changes.length === 0) {
		return;
	}
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	const tapped = self[7].v.length > 0;
	let index = 0;
	for (const change3 of changes) {
		const $w = change3;
		let $x = null;
		if ($w[0] === 0) {
			const _at = $w[1];
			const removed = $w[2];
			const inserted = $w[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$x = undefined;
		} else if ($w[0] === 1) {
			const _at2 = $w[1];
			const _count = $w[2];
			$x = resized = true;
		} else if ($w[0] === 3) {
			const _at3 = $w[1];
			const _count2 = $w[2];
			$x = resized = true;
		} else if ($w[0] === 6) {
			$x = resized = true;
		} else if ($w[0] === 5) {
			const _from = $w[1];
			const _to = $w[2];
			$x = shaped = true;
		} else {
			$x = undefined;
		}
		$x;
		apply(self[1].v, change3, wakes);
		let $N = null;
		if (tapped) {
			let $M = null;
			if (!(late)) {
				$M = [ 0 ];
			} else if (!(several)) {
				$M = [ 1 ];
			} else if (index === 0) {
				$M = [ 2 ];
			} else {
				$M = [ 3 ];
			}
			const stage = $M;
			for (const tap of __clone(self[7].v)) {
				tap[2](change3, stage);
			}
			$N = undefined;
		}
		$N;
		index = index + 1;
	}
	const cells = self[3];
	const published = __clone(self[4]);
	const shape = __clone(self[5]);
	const sized = __clone(self[6]);
	settle_together(($O) => {
		for (const wake of wakes) {
			const $P = wake;
			let $Q = null;
			if ($P[0] === 0) {
				const slot3 = $P[1];
				$Q = wake_all(cells, slot3, [ 0, $O ]);
			} else if ($P[0] === 2) {
				const slot4 = $P[1];
				$Q = wake_all(cells, slot4, [ 0, $O ]);
			} else {
				const slot5 = $P[1];
				const part2 = $P[2];
				$Q = wake_part(cells, slot5, part2, [ 0, $O ]);
			}
			$Q;
		}
		if (resized) {
			notify(sized, [ 0, $O ]);
		}
		if (shaped || resized) {
			notify(shape, [ 0, $O ]);
		}
		notify(published, [ 0, $O ]);
		return;
	}, $v);
}
function lend3(self, at2, read) {
	if (at2 < self.length) {
		read(__at_view(self, at2, "std/src/reactive/lens.vl:2008:10"));
	}
}
function change2(self, at2, with2) {
	if (at2 < self.length) {
		with2(__at_view(self, at2, "std/src/reactive/lens.vl:2014:14"));
	}
}
function sequence2(self, part2, empty2, lend4, borrow, $p) {
	const slot3 = __clone(self);
	const outer = __clone(self[0]);
	const outer_storage = __clone(self[0][0]);
	const view = self[0][2][1];
	const modify = self[0][2][3];
	const port = [ (read) => {
		const $q = at(slot3);
		let $r = null;
		if ($q[0] === 0) {
			const at2 = $q[1];
			$r = peek_storage(outer_storage, (held) => {
				return view(held, at2, (item2) => {
					return lend4(item2, read);
				});
			});
		} else {
			$r = read(empty2);
		}
		return $r;
	}, (change3) => {
		const $s = at(slot3);
		if ($s[0] === 0) {

		} else {
			return;
		}
		coming(outer, [ [ 4, __clone($s[1]), part2 ] ]);
		mutate(outer, (held) => {
			return modify(held, $s[1], (item2) => {
				return borrow(item2, change3);
			});
		});
		landed(outer, [ [ 4, __clone($s[1]), part2 ] ], false, $p);
		return;
	} ];
	return [ [ 1, port ], __shared_new(new3()), [ (held, at2) => {
		return item(held, at2);
	}, (held, at2, read) => {
		return lend3(held, at2, read);
	}, (held, at2, value) => {
		return put(held, at2, value, $p);
	}, (held, at2, with2) => {
		return change2(held, at2, with2, $p);
	} ], __shared_new([  ]), new4(0, "std/src/reactive/lens.vl:1064:28"), new4(0, "std/src/reactive/lens.vl:1065:24"), new4(0, "std/src/reactive/lens.vl:1066:24"), __shared_new([  ]), __shared_new(0), __shared_new([ 1 ]) ];
}
function peek_storage2(storage, read) {
	const $ak = storage;
	let $al = null;
	if ($ak[0] === 0) {
		const cell = $ak[1];
		$al = viewing2(cell, read);
	} else {
		const port = $ak[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $am = out;
		let $an = null;
		if ($am[0] === 0) {
			const value = __clone($am[1]);
			$an = value;
		} else {
			$an = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:632:13");
			})();
		}
		$al = $an;
	}
	return $al;
}
function holds2(storage, view, at2) {
	let found = false;
	peek_storage2(storage, (held) => {
		return view(held, at2, (_item) => {
			return found = true;
		});
	});
	return found;
}
function follow_id2(storage, handler, cells, view, at2) {
	if (viewing(handler, (held) => {
		return follows_items(held);
	}) && !(holds2(storage, view, at2))) {
		return -(1);
	}
	const id = follow(handler.v, at2);
	while (cells.v.length <= as_usize(id)) {
		cells.v.push(fresh_cells());
	}
	const slot_cells = viewing2(cells, (held) => {
		return __clone(__at(held, as_usize(id), "std/src/reactive/lens.vl:909:41"));
	});
	const $ao = [ slot_cells[2], "v" ];
	slot_cells[2].v = $ao[0][$ao[1]] + 1;
	return id;
}
function follow3(self, at2) {
	return slot_of(self, follow_id2(self[0], self[1], self[3], self[2][1], at2));
}
function release3(self) {
	release_id(self[0][1], self[0][3], self[1]);
}
function slot2(self, at2, $b) {
	const handle = follow3(self, at2);
	defer_to_owner(() => {
		return release3(handle);
	}, $b);
	return handle;
}
function peek(self, read) {
	return peek_storage2(self[0], read);
}
function get(self) {
	const $ar = at(self);
	if ($ar[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][2][0];
	return peek(self[0], (held) => {
		return read(held, $ar[1]);
	});
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function on_settle(self, subscriber) {
	return attach(self, subscriber);
}
function on_settle2(self, subscriber) {
	return on_settle(self[2][0], subscriber);
}
function attach_observer(self, observer, immediately) {
	const subscription = on_settle2(self, mint_subscriber(() => {
		return observer(get(self));
	}));
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function on_change(self, observer, caller) {
	return place_subscription(attach_observer(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function part(self, name, read) {
	for (const held of self[2][1].v) {
		if (held[0] === name) {
			return [ __clone(self), __clone(held[1]), read ];
		}
	}
	const woke = new4(0, "std/src/reactive/lens.vl:1081:26");
	self[2][1].v.push([ name, __clone(woke) ]);
	return [ __clone(self), woke, read ];
}
function peek2(self, read) {
	return peek_storage(self[0], read);
}
function get2(self) {
	const $ay = at(self[0]);
	if ($ay[0] === 0) {

	} else {
		return [ 1 ];
	}
	let out = [ 1 ];
	const view = self[0][0][2][1];
	const read = self[2];
	peek2(self[0][0], (held) => {
		return view(held, $ay[1], (item2) => {
			return out = [ 0, read(item2) ];
		});
	});
	return out;
}
function on_settle3(self, subscriber) {
	return on_settle(self[1], subscriber);
}
function attach_observer2(self, observer, immediately) {
	const subscription = on_settle3(self, mint_subscriber(() => {
		return observer(get2(self));
	}));
	if (immediately) {
		observer(get2(self));
	}
	return subscription;
}
function on_change2(self, observer, caller) {
	return place_subscription(attach_observer2(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function get3(self) {
	const $aB = at(self[0]);
	if ($aB[0] === 0) {

	} else {
		return [ 1 ];
	}
	let out = [ 1 ];
	const view = self[0][0][2][1];
	const read = self[2];
	peek2(self[0][0], (held) => {
		return view(held, $aB[1], (item2) => {
			return out = [ 0, read(item2) ];
		});
	});
	return out;
}
function attach_observer3(self, observer, immediately) {
	const subscription = on_settle3(self, mint_subscriber(() => {
		return observer(get3(self));
	}));
	if (immediately) {
		observer(get3(self));
	}
	return subscription;
}
function on_change3(self, observer, caller) {
	return place_subscription(attach_observer3(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function get4(self) {
	const $aE = at(self);
	if ($aE[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][2][0];
	return peek(self[0], (held) => {
		return read(held, $aE[1]);
	});
}
function on_settle4(self, subscriber) {
	return on_settle(self[2][0], subscriber);
}
function attach_observer4(self, observer, immediately) {
	const subscription = on_settle4(self, mint_subscriber(() => {
		return observer(get4(self));
	}));
	if (immediately) {
		observer(get4(self));
	}
	return subscription;
}
function on_change4(self, observer, caller) {
	return place_subscription(attach_observer4(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function count(self) {
	return self.length;
}
function size(self) {
	return peek(self, (held) => {
		return count(held);
	});
}
function splice(self, at2, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self, at2, "std/src/reactive/lens.vl:1992:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at2 + offset, __clone(value), "std/src/reactive/lens.vl:1997:9");
		offset = offset + 1;
	}
}
function landed2(self, changes, late, $v) {
	if (changes.length === 0) {
		return;
	}
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	const tapped = self[7].v.length > 0;
	let index = 0;
	for (const change3 of changes) {
		const $aT = change3;
		let $aU = null;
		if ($aT[0] === 0) {
			const _at = $aT[1];
			const removed = $aT[2];
			const inserted = $aT[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$aU = undefined;
		} else if ($aT[0] === 1) {
			const _at2 = $aT[1];
			const _count = $aT[2];
			$aU = resized = true;
		} else if ($aT[0] === 3) {
			const _at3 = $aT[1];
			const _count2 = $aT[2];
			$aU = resized = true;
		} else if ($aT[0] === 6) {
			$aU = resized = true;
		} else if ($aT[0] === 5) {
			const _from = $aT[1];
			const _to = $aT[2];
			$aU = shaped = true;
		} else {
			$aU = undefined;
		}
		$aU;
		apply(self[1].v, change3, wakes);
		let $aW = null;
		if (tapped) {
			let $aV = null;
			if (!(late)) {
				$aV = [ 0 ];
			} else if (!(several)) {
				$aV = [ 1 ];
			} else if (index === 0) {
				$aV = [ 2 ];
			} else {
				$aV = [ 3 ];
			}
			const stage = $aV;
			for (const tap of __clone(self[7].v)) {
				tap[2](change3, stage);
			}
			$aW = undefined;
		}
		$aW;
		index = index + 1;
	}
	const cells = self[3];
	const published = __clone(self[4]);
	const shape = __clone(self[5]);
	const sized = __clone(self[6]);
	settle_together(($O) => {
		for (const wake of wakes) {
			const $aX = wake;
			let $aY = null;
			if ($aX[0] === 0) {
				const slot3 = $aX[1];
				$aY = wake_all(cells, slot3, [ 0, $O ]);
			} else if ($aX[0] === 2) {
				const slot4 = $aX[1];
				$aY = wake_all(cells, slot4, [ 0, $O ]);
			} else {
				const slot5 = $aX[1];
				const part2 = $aX[2];
				$aY = wake_part(cells, slot5, part2, [ 0, $O ]);
			}
			$aY;
		}
		if (resized) {
			notify(sized, [ 0, $O ]);
		}
		if (shaped || resized) {
			notify(shape, [ 0, $O ]);
		}
		notify(published, [ 0, $O ]);
		return;
	}, $v);
}
function splice2(self, at2, removed, inserted, $aK) {
	const held = size(self);
	let $aP = null;
	if (at2 > held) {
		$aP = held;
	} else {
		$aP = at2;
	}
	const start = $aP;
	let $aQ = null;
	if (start + removed > held) {
		$aQ = held - start;
	} else {
		$aQ = removed;
	}
	const taking = $aQ;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change3 = [ 0, start, taking, arriving ];
	coming(self, [ __clone(change3) ]);
	mutate(self, (held2) => {
		return splice(held2, start, taking, inserted);
	});
	landed2(self, [ change3 ], false, $aK);
}
function insert_at(self, at2, value, $aJ) {
	return splice2(self, at2, 0, [ __clone(value) ], $aJ);
}
function eq4(self, b) {
	const $be = self;
	let $bh = null;
	if ($be[0] === 0) {
		const $bf = b;
		let $bg = null;
		if ($bf[0] === 0) {
			$bg = eq2($be[1], $bf[1]);
		} else {
			$bg = false;
		}
		$bh = $bg;
	} else {
		const $bi = b;
		$bh = $bi[0] === 1;
	}
	return $bh;
}
function size2(self) {
	return peek(self, (held) => {
		return count(held);
	});
}
function splice3(self, at2, removed, inserted, $aK) {
	const held = size2(self);
	let $bs = null;
	if (at2 > held) {
		$bs = held;
	} else {
		$bs = at2;
	}
	const start = $bs;
	let $bt = null;
	if (start + removed > held) {
		$bt = held - start;
	} else {
		$bt = removed;
	}
	const taking = $bt;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change3 = [ 0, start, taking, arriving ];
	coming(self, [ __clone(change3) ]);
	mutate(self, (held2) => {
		return splice(held2, start, taking, inserted);
	});
	landed(self, [ change3 ], false, $aK);
}
function insert_at2(self, at2, value, $aJ) {
	return splice3(self, at2, 0, [ __clone(value) ], $aJ);
}
function has(self, at2) {
	return holds2(self[0], self[2][1], at2);
}
function replace(self, at2, value, $bw) {
	let $bx = null;
	if (has(self, at2)) {
		$bx = [ 2, __clone(at2) ];
	} else if (viewing(self[1], (handler) => {
		return follows_items(handler);
	})) {
		return false;
	} else {
		$bx = [ 1, __clone(at2), 1 ];
	}
	const change3 = $bx;
	coming(self, [ __clone(change3) ]);
	const write = self[2][2];
	mutate(self, (held) => {
		return write(held, at2, value);
	});
	landed2(self, [ change3 ], false, $bw);
	return true;
}
function set(self, value, $bu) {
	const $bv = at(self);
	if ($bv[0] === 0) {

	} else {
		return false;
	}
	return replace(self[0], $bv[1], value, $bu);
}
function remove_at(self, at2, $bz) {
	return splice3(self, at2, 1, [  ], $bz);
}
function push(self, value, $bA) {
	return splice2(self, size(self), 0, [ __clone(value) ], $bA);
}
function has2(self, at2) {
	return holds(self[0], self[2][1], at2);
}
function edit(self, at2, part2, change3, $bD) {
	if (!(has2(self, at2))) {
		return false;
	}
	coming(self, [ [ 4, __clone(at2), part2 ] ]);
	const modify = self[2][3];
	mutate(self, (held) => {
		return modify(held, at2, change3);
	});
	landed(self, [ [ 4, __clone(at2), part2 ] ], false, $bD);
	return true;
}
function edit2(self, part2, change3, $bB) {
	const $bC = at(self);
	if ($bC[0] === 0) {

	} else {
		return false;
	}
	return edit(self[0], $bC[1], part2, change3, $bB);
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
}, (_id, _site) => {
	return;
} ]);
const waking = __shared_new([ 1 ]);
const owner_lists_allocated_count = __shared_new(0);
const block_size = 128;
const sections = sequence([ [ "Todo", [ "a", "b", "c" ] ], [ "Doing", [ "x", "y" ] ] ], [ 1 ]);
const doing = slot(sections, 1, [ 1 ]);
const empty = [  ];
const items = sequence2(doing, "items", empty, (section, read) => {
	return read(section[1]);
}, (section, change3) => {
	return change3(section[1]);
}, [ 1 ]);
const y = slot2(items, 1, [ 1 ]);
const y_runs = __shared_new(0);
const title_runs = __shared_new(0);
const items_runs = __shared_new(0);
const todo_runs = __shared_new(0);
on_change(__clone(y), (_item, $ap) => {
	const $aq = [ y_runs, "v" ];
	return y_runs.v = $aq[0][$aq[1]] + 1;
}, "lens-nested.vl:33:13");
on_change2(part(doing, "title", (section) => {
	return section[0];
}), (_t, $aw) => {
	const $ax = [ title_runs, "v" ];
	return title_runs.v = $ax[0][$ax[1]] + 1;
}, "lens-nested.vl:34:56");
on_change3(part(doing, "items", (section) => {
	return section[1].length;
}), (_n, $az) => {
	const $aA = [ items_runs, "v" ];
	return items_runs.v = $aA[0][$aA[1]] + 1;
}, "lens-nested.vl:37:4");
on_change4(slot(sections, 0, [ 1 ]), (_section, $aC) => {
	const $aD = [ todo_runs, "v" ];
	return todo_runs.v = $aD[0][$aD[1]] + 1;
}, "lens-nested.vl:38:28");
insert_at(items, 0, "w", [ 1 ]);
assert(eq(peek(sections, (held) => {
	return __clone(__at(held, 1, "lens-nested.vl:44:24")[1]);
}), [ "w", "x", "y" ]), "the write landed inside the outer item", "lens-nested.vl:43:2");
assert(eq4(at(y), [ 0, 2 ]) && eq3(get(y), [ 0, "y" ]) && y_runs.v === 0, "the inner slot followed, silently", "lens-nested.vl:47:2");
assert(items_runs.v === 1 && title_runs.v === 0 && todo_runs.v === 0, "only the items part of that section woke", "lens-nested.vl:51:2");
insert_at2(sections, 0, [ "Inbox", [  ] ], [ 1 ]);
set(y, "Y", [ 1 ]);
assert(eq(peek(sections, (held) => {
	return __clone(__at(held, 2, "lens-nested.vl:60:24")[1]);
}), [ "w", "x", "Y" ]), "an inner write lands in the moved section", "lens-nested.vl:59:2");
assert(y_runs.v === 1, "the inner slot woke for its own write", "lens-nested.vl:63:2");
batch(($by) => {
	remove_at(sections, 0, [ 0, $by ]);
	push(items, "z", [ 0, $by ]);
	edit2(doing, "title", (section) => {
		return section[0] = "In progress";
	}, [ 0, $by ]);
	return;
}, [ 1 ]);
assert(eq(peek(sections, (held) => {
	return __clone(__at(held, 1, "lens-nested.vl:72:24")[1]);
}), [ "w", "x", "Y", "z" ]), "the turn\'s inner write landed", "lens-nested.vl:71:2");
assert(title_runs.v === 1, "the title woke once", "lens-nested.vl:75:2");
remove_at(sections, 1, [ 1 ]);
assert(size(items) === 0 && eq3(get(y), [ 1 ]), "an inner lens whose item is gone reads empty", "lens-nested.vl:79:2");
console.log("nested: all passed");
//# sourceMappingURL=lens-nested.mjs.map
