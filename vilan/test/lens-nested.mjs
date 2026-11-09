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
	let $bA = null;
	if (!(condition)) {
		$bA = (() => {
			throw __panic(message, caller);
		})();
	}
	return $bA;
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
	const $bG = self;
	let $bJ = null;
	if ($bG[0] === 0) {
		const $bH = b;
		let $bI = null;
		if ($bH[0] === 0) {
			$bI = $bG[1] === $bH[1];
		} else {
			$bI = false;
		}
		$bJ = $bI;
	} else {
		const $bK = b;
		$bJ = $bK[0] === 1;
	}
	return $bJ;
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
		let $ac = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$ac = undefined;
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
		$ac;
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
	let $p = null;
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
		$p = undefined;
	}
	return $p;
}
function defer_to_owner(cleanup, $m) {
	const $n = $m;
	let $o = null;
	if ($n[0] === 0) {
		const owner = $n[1];
		$o = defer(owner, cleanup);
	} else {
		$o = undefined;
	}
	return $o;
}
function place_subscription(subscription, site) {
	if (tracing.v) {
		trace_hooks.v[8](subscription[1], site);
	}
	return subscription;
}
function settle_together(body, $aq) {
	const $ar = $aq;
	let $as = null;
	if ($ar[0] === 0) {
		const current = $ar[1];
		$as = body(current);
	} else {
		const $at = last(draining_turns.v);
		let $au = null;
		if ($at[0] === 0) {
			const draining = $at[1];
			$au = body(draining);
		} else {
			$au = batch(body, $aq);
		}
		$as = $au;
	}
	return $as;
}
function new3() {
	return [ [  ] ];
}
function follow(self, at2) {
	if (self[0].length === 0) {
		self[0].push(new4());
	}
	return follow2(__at(self[0], 0, "std/src/reactive/lens.vl:145:3"), at2);
}
function apply(self, change3, wakes) {
	if (self[0].length > 0) {
		apply2(__at(self[0], 0, "std/src/reactive/lens.vl:150:4"), change3, wakes);
	}
}
function locate(self, slot3) {
	if (self[0].length === 0) {
		return [ 1 ];
	}
	return locate2(__at(self[0], 0, "std/src/reactive/lens.vl:158:3"), slot3);
}
function follows_items(self) {
	return true;
}
function release(self, slot3) {
	if (self[0].length > 0) {
		release2(__at(self[0], 0, "std/src/reactive/lens.vl:167:4"), slot3);
	}
}
function new4() {
	return [ [  ], [  ], [  ], [  ], [  ], [  ], 0 ];
}
function seat(self, at2) {
	let low = 0;
	let high = self[3].length;
	while (high - low > 1) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[3], middle, "std/src/reactive/lens.vl:213:7") <= at2) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at2 - __at(self[3], low, "std/src/reactive/lens.vl:219:14") ];
}
function restart(self) {
	let starts = [  ];
	let running = 0;
	let place = 0;
	for (const block of self[1]) {
		starts.push(running);
		__at_put(self[2], as_usize(block), place, "std/src/reactive/lens.vl:229:4");
		running = running + __at(self[0], as_usize(block), "std/src/reactive/lens.vl:230:15").length;
		place = place + 1;
	}
	self[3] = starts;
}
function renumber(self, block, from) {
	let index = from;
	const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:239:14").length;
	while (index < held) {
		const slot3 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:241:15"), index, "std/src/reactive/lens.vl:241:15");
		if (slot3 !== -(1)) {
			__at_put(self[4], as_usize(slot3), block, "std/src/reactive/lens.vl:243:5");
			__at_put(self[5], as_usize(slot3), index, "std/src/reactive/lens.vl:244:5");
		}
		index = index + 1;
	}
}
function open(self, place) {
	const block = as_i32(self[0].length);
	self[0].push([  ]);
	self[2].push(0);
	__insert_at(self[1], place, block, "std/src/reactive/lens.vl:255:14");
	return block;
}
function widen(self, at2, count2) {
	if (count2 === 0 || at2 >= self[6]) {
		return;
	}
	const $I = seat(self, at2);
	const place = $I[0];
	const index = $I[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:265:15");
	let $J = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:269:5"), index, -(1), "std/src/reactive/lens.vl:269:35");
			made = made + 1;
		}
		$J = undefined;
	} else {
		const before = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:273:17"));
		let grown = [  ];
		let copied = 0;
		while (copied < index) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:277:16"));
			copied = copied + 1;
		}
		let made2 = 0;
		while (made2 < count2) {
			grown.push(-(1));
			made2 = made2 + 1;
		}
		const held = before.length;
		while (copied < held) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:287:16"));
			copied = copied + 1;
		}
		__at_put(self[0], as_usize(block), grown, "std/src/reactive/lens.vl:290:4");
	}
	$J;
	self[6] = self[6] + count2;
	renumber(self, block, index + count2);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:294:6").length > 2 * block_size) {
		rebalance(self, place);
	}
	restart(self);
}
function close(self, at2, count2, wakes) {
	let left = count2;
	while (left > 0 && at2 < self[6]) {
		const $G = seat(self, at2);
		const place = $G[0];
		const index = $G[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:306:16");
		const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:307:15").length;
		let $H = null;
		if (left < held - index) {
			$H = left;
		} else {
			$H = held - index;
		}
		const take = $H;
		let taken = 0;
		while (taken < take) {
			const slot3 = __remove_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:311:16"), index, "std/src/reactive/lens.vl:311:46");
			if (slot3 !== -(1)) {
				__at_put(self[4], as_usize(slot3), -(1), "std/src/reactive/lens.vl:313:6");
				wakes.push([ 2, slot3 ]);
			}
			taken = taken + 1;
		}
		self[6] = self[6] - take;
		left = left - take;
		if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:320:7").length === 0) {
			__remove_at(self[1], place, "std/src/reactive/lens.vl:321:16");
		} else {
			renumber(self, block, index);
		}
		restart(self);
	}
}
function rebalance(self, place) {
	const block = __at(self[1], place, "std/src/reactive/lens.vl:331:15");
	const all = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:332:13"));
	let first = [  ];
	let index = 0;
	while (index < block_size) {
		first.push(__at(all, index, "std/src/reactive/lens.vl:336:15"));
		index = index + 1;
	}
	__at_put(self[0], as_usize(block), first, "std/src/reactive/lens.vl:339:3");
	let next = place + 1;
	const held = all.length;
	while (index < held) {
		const fresh = open(self, next);
		let filled = 0;
		while (filled < block_size && index < held) {
			__at(self[0], as_usize(fresh), "std/src/reactive/lens.vl:346:5").push(__at(all, index, "std/src/reactive/lens.vl:346:40"));
			index = index + 1;
			filled = filled + 1;
		}
		renumber(self, fresh, 0);
		next = next + 1;
	}
}
function follow2(self, at2) {
	while (self[6] <= at2) {
		const places = self[1].length;
		if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:361:34")), "std/src/reactive/lens.vl:361:22").length >= block_size) {
			const fresh = open(self, places);
			__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:363:5");
			self[3].push(self[6]);
		}
		const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:366:15");
		__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:367:4").push(-(1));
		self[6] = self[6] + 1;
	}
	const $j = seat(self, at2);
	const place = $j[0];
	const index = $j[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:371:15");
	const held = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:372:14"), index, "std/src/reactive/lens.vl:372:14");
	if (held !== -(1)) {
		return held;
	}
	const slot3 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:379:3"), index, slot3, "std/src/reactive/lens.vl:379:3");
	return slot3;
}
function apply2(self, change3, wakes) {
	const $E = change3;
	let $F = null;
	if ($E[0] === 0) {
		const at2 = $E[1];
		const removed = $E[2];
		const inserted = $E[3];
		close(self, at2, removed, wakes);
		widen(self, at2, inserted);
		$F = undefined;
	} else if ($E[0] === 1) {
		const at3 = $E[1];
		const count2 = $E[2];
		$F = widen(self, at3, count2);
	} else if ($E[0] === 3) {
		const at4 = $E[1];
		const count3 = $E[2];
		$F = close(self, at4, count3, wakes);
	} else if ($E[0] === 2) {
		const at5 = $E[1];
		let $L = null;
		if (at5 < self[6]) {
			const $K = seat(self, at5);
			const place = $K[0];
			const index = $K[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:396:29")), "std/src/reactive/lens.vl:396:17"), index, "std/src/reactive/lens.vl:396:17");
			if (slot3 !== -(1)) {
				wakes.push([ 0, slot3 ]);
			}
			$L = undefined;
		}
		$F = $L;
	} else if ($E[0] === 4) {
		const at6 = $E[1];
		const part3 = $E[2];
		let $N = null;
		if (at6 < self[6]) {
			const $M = seat(self, at6);
			const place2 = $M[0];
			const index2 = $M[1];
			const slot4 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:405:29")), "std/src/reactive/lens.vl:405:17"), index2, "std/src/reactive/lens.vl:405:17");
			if (slot4 !== -(1)) {
				wakes.push([ 1, slot4, part3 ]);
			}
			$N = undefined;
		}
		$F = $N;
	} else if ($E[0] === 5) {
		const from = $E[1];
		const to = $E[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $O = null;
		if (from > to) {
			$O = from;
		} else {
			$O = to;
		}
		const far = $O;
		while (self[6] <= far) {
			const places = self[1].length;
			if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:419:36")), "std/src/reactive/lens.vl:419:24").length >= block_size) {
				const fresh = open(self, places);
				__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:421:7");
				self[3].push(self[6]);
			}
			const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:424:17");
			__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:425:6").push(-(1));
			self[6] = self[6] + 1;
		}
		const $P = seat(self, from);
		const left = $P[0];
		const index3 = $P[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:429:18");
		const slot5 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:430:16"), index3, "std/src/reactive/lens.vl:430:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:432:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:433:17");
		} else {
			renumber(self, source, index3);
		}
		restart(self);
		let $Q = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$Q = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:441:25")), "std/src/reactive/lens.vl:441:13").length ];
		} else {
			$Q = seat(self, to);
		}
		const $R = $Q;
		const place3 = $R[0];
		const landing = $R[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:445:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:446:5"), landing, slot5, "std/src/reactive/lens.vl:446:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing);
		restart(self);
		$F = undefined;
	} else {
		let slot6 = 0;
		const slots = as_i32(self[4].length);
		while (slot6 < slots) {
			if (__at(self[4], as_usize(slot6), "std/src/reactive/lens.vl:455:9") !== -(1)) {
				__at_put(self[4], as_usize(slot6), -(1), "std/src/reactive/lens.vl:456:7");
				wakes.push([ 2, slot6 ]);
			}
			slot6 = slot6 + 1;
		}
		self[0] = [  ];
		self[1] = [  ];
		self[2] = [  ];
		self[3] = [  ];
		self[6] = 0;
		$F = undefined;
	}
	return $F;
}
function release2(self, slot3) {
	const block = __at(self[4], as_usize(slot3), "std/src/reactive/lens.vl:471:15");
	if (block === -(1)) {
		return;
	}
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:475:3"), __at(self[5], as_usize(slot3), "std/src/reactive/lens.vl:475:33"), -(1), "std/src/reactive/lens.vl:475:3");
	__at_put(self[4], as_usize(slot3), -(1), "std/src/reactive/lens.vl:476:3");
}
function locate2(self, slot3) {
	const block = __at(self[4], as_usize(slot3), "std/src/reactive/lens.vl:480:15");
	if (block === -(1)) {
		return [ 1 ];
	}
	return [ 0, __at(self[3], __at(self[2], as_usize(block), "std/src/reactive/lens.vl:484:20"), "std/src/reactive/lens.vl:484:8") + __at(self[5], as_usize(slot3), "std/src/reactive/lens.vl:484:51") ];
}
function count(self) {
	return self.length;
}
function item(self, at2) {
	return __list_get(self, at2);
}
function lend(self, at2, read) {
	if (at2 < self.length) {
		read(__at(self, at2, "std/src/reactive/lens.vl:2222:10"));
	}
}
function put(self, at2, value) {
	__at_put(self, at2, __clone(value), "std/src/reactive/lens.vl:2242:3");
}
function change(self, at2, with2) {
	if (at2 < self.length) {
		with2(__at(self, at2, "std/src/reactive/lens.vl:2247:14"));
	}
}
function new5(value, caller) {
	let subscribers = [  ];
	const cell = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell[0]), caller);
	}
	return cell;
}
function lens_over(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new5(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over(value, handler, access) {
	return lens_over([ 0, __shared_new(value) ], handler, access);
}
function sequence(value, $a) {
	return over(value, new3(), [ (held, at2) => {
		return at2 < count(held);
	}, (held, at2) => {
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
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$d = $f;
	}
	return $d;
}
function follow_id(storage, watch, holds, at2) {
	if (viewing(watch, (held) => {
		return follows_items(held[0]);
	}) && !(peek_storage(storage, (held) => {
		return holds(held, at2);
	}))) {
		return -(1);
	}
	const id = follow(watch.v[0], at2);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] + 1;
	return id;
}
function slot_of(self, id) {
	const $l = __clone(self);
	let $k = null;
	if (id < 0) {
		$k = -(1);
	} else {
		$k = id;
	}
	return [ $l, $k ];
}
function follow3(self, at2) {
	const core = self[0];
	return slot_of(self, follow_id(core.v[0], core.v[3], core.v[1][0], at2));
}
function release_id(watch, id) {
	if (id < 0) {
		return;
	}
	const entry = as_usize(id);
	const left = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1071:13")[2] - 1;
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1072:2")[2] = left;
	if (left <= 0) {
		release(watch.v[0], id);
	}
}
function release3(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot(self, at2, $b) {
	const handle = follow3(self, at2);
	defer_to_owner(() => {
		return release3(handle);
	}, $b);
	return handle;
}
function at(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][0].v[3], (held) => {
		return locate(held[0], self[1]);
	});
}
function peek_storage2(storage, read) {
	const $t = storage;
	let $u = null;
	if ($t[0] === 0) {
		const cell = $t[1];
		$u = viewing2(cell, read);
	} else {
		const port = $t[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $v = out;
		let $w = null;
		if ($v[0] === 0) {
			const value = __clone($v[1]);
			$w = value;
		} else {
			$w = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$u = $w;
	}
	return $u;
}
function coming_one(self, change3) {
	if (self[0].v[4].length === 0) {
		return;
	}
	const taps = __clone(self[0].v[4]);
	for (const tap of taps) {
		tap[1](change3);
	}
}
function mutate(self, change3) {
	const $y = self[0].v[0];
	let $z = null;
	if ($y[0] === 0) {
		const cell = $y[1];
		$z = change3(cell.v);
	} else {
		const port = __clone($y[1]);
		$z = port[1](change3);
	}
	return $z;
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $ad = null;
	if (is_empty(self)) {
		$ad = [ 1 ];
	} else {
		$ad = __list_get(self, self.length - 1);
	}
	return $ad;
}
function notify(self, $Y) {
	const traced = tracing.v;
	let $Z = null;
	if (traced) {
		$Z = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$Z = "";
	}
	const cause = $Z;
	const $aa = $Y;
	let $ab = null;
	if ($aa[0] === 0) {
		const turn = $aa[1];
		$ab = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $ae = last(draining_turns.v);
		let $af = null;
		if ($ae[0] === 0) {
			const draining = $ae[1];
			$af = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $ag = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$ag = undefined;
				}
				$ag;
			}
			$af = undefined;
		}
		$ab = $af;
	}
	return $ab;
}
function wake_all(watch, slot3, $ak) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot3), "std/src/reactive/lens.vl:1120:36"));
	});
	const $al = held[0];
	if ($al[0] === 0) {
		notify($al[1], $ak);
	}
	for (const part3 of held[1]) {
		notify(part3[1], $ak);
	}
}
function wake_part(watch, slot3, name, $am) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot3), "std/src/reactive/lens.vl:1130:36"));
	});
	const $an = held[0];
	if ($an[0] === 0) {
		notify($an[1], $am);
	}
	for (const part3 of held[1]) {
		if (part3[0] === name) {
			notify(part3[1], $am);
		}
	}
}
function batch(body, $av) {
	const $aw = $av;
	let $ax = null;
	if ($aw[0] === 0) {
		const current = $aw[1];
		$ax = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$ax = result;
	}
	return $ax;
}
function landed(self, changes, late, $B) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index = 0;
	for (const change3 of changes) {
		const $C = change3;
		let $D = null;
		if ($C[0] === 0) {
			const _at = $C[1];
			const removed = $C[2];
			const inserted = $C[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$D = undefined;
		} else if ($C[0] === 1) {
			const _at2 = $C[1];
			const _count = $C[2];
			$D = resized = true;
		} else if ($C[0] === 3) {
			const _at3 = $C[1];
			const _count2 = $C[2];
			$D = resized = true;
		} else if ($C[0] === 6) {
			$D = resized = true;
		} else if ($C[0] === 5) {
			const _from = $C[1];
			const _to = $C[2];
			$D = shaped = true;
		} else {
			$D = undefined;
		}
		$D;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change3, wakes);
		}
		let $T = null;
		if (self[0].v[4].length > 0) {
			let $S = null;
			if (!(late)) {
				$S = [ 0 ];
			} else if (!(several)) {
				$S = [ 1 ];
			} else if (index === 0) {
				$S = [ 2 ];
			} else {
				$S = [ 3 ];
			}
			const stage = $S;
			const taps = __clone(self[0].v[4]);
			for (const tap of taps) {
				tap[2](change3, stage);
			}
			$T = undefined;
		}
		$T;
		index = index + 1;
	}
	const published = __clone(self[0].v[2]);
	let $U = null;
	if (shaped || resized) {
		$U = watch.v[2];
	} else {
		$U = [ 1 ];
	}
	const shape = __clone($U);
	let $V = null;
	if (resized) {
		$V = watch.v[3];
	} else {
		$V = [ 1 ];
	}
	const sized = __clone($V);
	let $W = undefined;
	let $X = undefined;
	if (wakes.length === 0 && (($W = shape), $W[0] === 1) && (($X = sized), $X[0] === 1)) {
		notify(published, $B);
		return;
	}
	settle_together(($ah) => {
		for (const wake of wakes) {
			const $ai = wake;
			let $aj = null;
			if ($ai[0] === 0) {
				const slot3 = $ai[1];
				$aj = wake_all(watch, slot3, [ 0, $ah ]);
			} else if ($ai[0] === 2) {
				const slot4 = $ai[1];
				$aj = wake_all(watch, slot4, [ 0, $ah ]);
			} else {
				const slot5 = $ai[1];
				const part3 = $ai[2];
				$aj = wake_part(watch, slot5, part3, [ 0, $ah ]);
			}
			$aj;
		}
		const $ao = sized;
		if ($ao[0] === 0) {
			notify($ao[1], [ 0, $ah ]);
		}
		const $ap = shape;
		if ($ap[0] === 0) {
			notify($ap[1], [ 0, $ah ]);
		}
		notify(published, [ 0, $ah ]);
		return;
	}, $B);
}
function landed_one(self, change3, $A) {
	landed(self, [ __clone(change3) ], false, $A);
}
function lend3(self, at2, read) {
	if (at2 < self.length) {
		read(__at_view(self, at2, "std/src/reactive/lens.vl:2222:10"));
	}
}
function change2(self, at2, with2) {
	if (at2 < self.length) {
		with2(__at_view(self, at2, "std/src/reactive/lens.vl:2247:14"));
	}
}
function lens_over2(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new5(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function sequence2(self, part3, empty2, lend4, borrow, $q) {
	const slot3 = __clone(self);
	const outer = __clone(self[0]);
	const outer_storage = __clone(self[0][0].v[0]);
	const view = self[0][0].v[1][2];
	const modify = self[0][0].v[1][4];
	const port = [ (read) => {
		const $r = at(slot3);
		let $s = null;
		if ($r[0] === 0) {
			const at2 = $r[1];
			$s = peek_storage2(outer_storage, (held) => {
				return view(held, at2, (item2) => {
					return lend4(item2, read);
				});
			});
		} else {
			$s = read(empty2);
		}
		return $s;
	}, (change3) => {
		const $x = at(slot3);
		if ($x[0] === 0) {

		} else {
			return;
		}
		coming_one(outer, [ 4, __clone($x[1]), part3 ]);
		mutate(outer, (held) => {
			return modify(held, $x[1], (item2) => {
				return borrow(item2, change3);
			});
		});
		landed_one(outer, [ 4, __clone($x[1]), part3 ], $q);
		return;
	} ];
	return lens_over2([ 1, port ], new3(), [ (held, at2) => {
		return at2 < count(held);
	}, (held, at2) => {
		return item(held, at2);
	}, (held, at2, read) => {
		return lend3(held, at2, read);
	}, (held, at2, value) => {
		return put(held, at2, value, $q);
	}, (held, at2, with2) => {
		return change2(held, at2, with2, $q);
	} ]);
}
function follow_id2(storage, watch, holds, at2) {
	if (viewing(watch, (held) => {
		return follows_items(held[0]);
	}) && !(peek_storage2(storage, (held) => {
		return holds(held, at2);
	}))) {
		return -(1);
	}
	const id = follow(watch.v[0], at2);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] + 1;
	return id;
}
function follow4(self, at2) {
	const core = self[0];
	return slot_of(self, follow_id2(core.v[0], core.v[3], core.v[1][0], at2));
}
function release4(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot2(self, at2, $b) {
	const handle = follow4(self, at2);
	defer_to_owner(() => {
		return release4(handle);
	}, $b);
	return handle;
}
function peek(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function get(self) {
	const $aG = at(self);
	if ($aG[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held) => {
		return read(held, $aG[1]);
	});
}
function slot_cell(watch, slot3) {
	if (slot3 < 0) {
		return new5(0, "std/src/reactive/lens.vl:1082:19");
	}
	const entry = as_usize(slot3);
	const $aL = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1085:5")[0];
	const cell = __clone($aL[1]);
	if ($aL[0] === 0) {
		return cell;
	}
	const cell2 = new5(0, "std/src/reactive/lens.vl:1088:25");
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1089:2")[0] = [ 0, __clone(cell2) ];
	return cell2;
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
	return on_settle(slot_cell(self[0][0].v[3], self[1]), subscriber);
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
function part_cell(watch, slot3, name) {
	if (slot3 < 0) {
		return new5(0, "std/src/reactive/lens.vl:1097:19");
	}
	const found = viewing2(watch, (held) => {
		let out = [ 1 ];
		for (const part3 of __at(held[1], as_usize(slot3), "std/src/reactive/lens.vl:1101:15")[1]) {
			if (part3[0] === name) {
				out = [ 0, __clone(part3[1]) ];
			}
		}
		return out;
	});
	const $aM = found;
	if ($aM[0] === 0) {
		return $aM[1];
	}
	const woke = new5(0, "std/src/reactive/lens.vl:1111:25");
	const entry = as_usize(slot3);
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1113:2")[1].push([ name, __clone(woke) ]);
	return woke;
}
function part(self, name, read) {
	return [ __clone(self), part_cell(self[0][0].v[3], self[1], name), read ];
}
function get2(self) {
	const $aP = at(self[0]);
	if ($aP[0] === 0) {

	} else {
		return [ 1 ];
	}
	const view = self[0][0][0].v[1][2];
	const read = self[2];
	let out = [ 1 ];
	peek(self[0][0], (held) => {
		return view(held, $aP[1], (item2) => {
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
function part2(self, name, read) {
	return [ __clone(self), part_cell(self[0][0].v[3], self[1], name), read ];
}
function attach_observer3(self, observer, immediately) {
	const subscription = on_settle3(self, mint_subscriber(() => {
		return observer(get2(self));
	}));
	if (immediately) {
		observer(get2(self));
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
function get3(self) {
	const $aV = at(self);
	if ($aV[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held) => {
		return read(held, $aV[1]);
	});
}
function on_settle4(self, subscriber) {
	return on_settle(slot_cell(self[0][0].v[3], self[1]), subscriber);
}
function attach_observer4(self, observer, immediately) {
	const subscription = on_settle4(self, mint_subscriber(() => {
		return observer(get3(self));
	}));
	if (immediately) {
		observer(get3(self));
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
function size(self) {
	return peek(self, (held) => {
		return count(held);
	});
}
function splice(self, at2, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self, at2, "std/src/reactive/lens.vl:2231:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at2 + offset, __clone(value), "std/src/reactive/lens.vl:2236:9");
		offset = offset + 1;
	}
}
function landed2(self, changes, late, $B) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index = 0;
	for (const change3 of changes) {
		const $bk = change3;
		let $bl = null;
		if ($bk[0] === 0) {
			const _at = $bk[1];
			const removed = $bk[2];
			const inserted = $bk[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$bl = undefined;
		} else if ($bk[0] === 1) {
			const _at2 = $bk[1];
			const _count = $bk[2];
			$bl = resized = true;
		} else if ($bk[0] === 3) {
			const _at3 = $bk[1];
			const _count2 = $bk[2];
			$bl = resized = true;
		} else if ($bk[0] === 6) {
			$bl = resized = true;
		} else if ($bk[0] === 5) {
			const _from = $bk[1];
			const _to = $bk[2];
			$bl = shaped = true;
		} else {
			$bl = undefined;
		}
		$bl;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change3, wakes);
		}
		let $bn = null;
		if (self[0].v[4].length > 0) {
			let $bm = null;
			if (!(late)) {
				$bm = [ 0 ];
			} else if (!(several)) {
				$bm = [ 1 ];
			} else if (index === 0) {
				$bm = [ 2 ];
			} else {
				$bm = [ 3 ];
			}
			const stage = $bm;
			const taps = __clone(self[0].v[4]);
			for (const tap of taps) {
				tap[2](change3, stage);
			}
			$bn = undefined;
		}
		$bn;
		index = index + 1;
	}
	const published = __clone(self[0].v[2]);
	let $bo = null;
	if (shaped || resized) {
		$bo = watch.v[2];
	} else {
		$bo = [ 1 ];
	}
	const shape = __clone($bo);
	let $bp = null;
	if (resized) {
		$bp = watch.v[3];
	} else {
		$bp = [ 1 ];
	}
	const sized = __clone($bp);
	let $bq = undefined;
	let $br = undefined;
	if (wakes.length === 0 && (($bq = shape), $bq[0] === 1) && (($br = sized), $br[0] === 1)) {
		notify(published, $B);
		return;
	}
	settle_together(($ah) => {
		for (const wake of wakes) {
			const $bs = wake;
			let $bt = null;
			if ($bs[0] === 0) {
				const slot3 = $bs[1];
				$bt = wake_all(watch, slot3, [ 0, $ah ]);
			} else if ($bs[0] === 2) {
				const slot4 = $bs[1];
				$bt = wake_all(watch, slot4, [ 0, $ah ]);
			} else {
				const slot5 = $bs[1];
				const part3 = $bs[2];
				$bt = wake_part(watch, slot5, part3, [ 0, $ah ]);
			}
			$bt;
		}
		const $bu = sized;
		if ($bu[0] === 0) {
			notify($bu[1], [ 0, $ah ]);
		}
		const $bv = shape;
		if ($bv[0] === 0) {
			notify($bv[1], [ 0, $ah ]);
		}
		notify(published, [ 0, $ah ]);
		return;
	}, $B);
}
function landed_one2(self, change3, $A) {
	landed2(self, [ __clone(change3) ], false, $A);
}
function splice2(self, at2, removed, inserted, $bb) {
	const held = size(self);
	let $bg = null;
	if (at2 > held) {
		$bg = held;
	} else {
		$bg = at2;
	}
	const start = $bg;
	let $bh = null;
	if (start + removed > held) {
		$bh = held - start;
	} else {
		$bh = removed;
	}
	const taking = $bh;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change3 = [ 0, start, taking, arriving ];
	coming_one(self, change3);
	mutate(self, (held2) => {
		return splice(held2, start, taking, inserted);
	});
	landed_one2(self, change3, $bb);
}
function insert_at(self, at2, value, $ba) {
	return splice2(self, at2, 0, [ __clone(value) ], $ba);
}
function eq4(self, b) {
	const $bB = self;
	let $bE = null;
	if ($bB[0] === 0) {
		const $bC = b;
		let $bD = null;
		if ($bC[0] === 0) {
			$bD = eq2($bB[1], $bC[1]);
		} else {
			$bD = false;
		}
		$bE = $bD;
	} else {
		const $bF = b;
		$bE = $bF[0] === 1;
	}
	return $bE;
}
function splice3(self, at2, removed, inserted, $bb) {
	const held = size(self);
	let $bP = null;
	if (at2 > held) {
		$bP = held;
	} else {
		$bP = at2;
	}
	const start = $bP;
	let $bQ = null;
	if (start + removed > held) {
		$bQ = held - start;
	} else {
		$bQ = removed;
	}
	const taking = $bQ;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change3 = [ 0, start, taking, arriving ];
	coming_one(self, change3);
	mutate(self, (held2) => {
		return splice(held2, start, taking, inserted);
	});
	landed_one(self, change3, $bb);
}
function insert_at2(self, at2, value, $ba) {
	return splice3(self, at2, 0, [ __clone(value) ], $ba);
}
function has(self, at2) {
	const holds = self[0].v[1][0];
	return peek(self, (held) => {
		return holds(held, at2);
	});
}
function replace(self, at2, value, $bT) {
	const watch = self[0].v[3];
	let $bU = null;
	if (has(self, at2)) {
		$bU = [ 2, __clone(at2) ];
	} else if (viewing(watch, (held) => {
		return follows_items(held[0]);
	})) {
		return false;
	} else {
		$bU = [ 1, __clone(at2), 1 ];
	}
	const change3 = $bU;
	coming_one(self, change3);
	const write = self[0].v[1][3];
	mutate(self, (held) => {
		return write(held, at2, value);
	});
	landed_one2(self, change3, $bT);
	return true;
}
function set(self, value, $bR) {
	const $bS = at(self);
	if ($bS[0] === 0) {

	} else {
		return false;
	}
	return replace(self[0], $bS[1], value, $bR);
}
function remove_at(self, at2, $bW) {
	return splice3(self, at2, 1, [  ], $bW);
}
function push(self, value, $bX) {
	return splice2(self, size(self), 0, [ __clone(value) ], $bX);
}
function peek2(self, read) {
	return peek_storage(self[0].v[0], read);
}
function has2(self, at2) {
	const holds = self[0].v[1][0];
	return peek2(self, (held) => {
		return holds(held, at2);
	});
}
function edit(self, at2, part3, change3, $ca) {
	if (!(has2(self, at2))) {
		return false;
	}
	coming_one(self, [ 4, __clone(at2), part3 ]);
	const modify = self[0].v[1][4];
	mutate(self, (held) => {
		return modify(held, at2, change3);
	});
	landed_one(self, [ 4, __clone(at2), part3 ], $ca);
	return true;
}
function edit2(self, part3, change3, $bY) {
	const $bZ = at(self);
	if ($bZ[0] === 0) {

	} else {
		return false;
	}
	return edit(self[0], $bZ[1], part3, change3, $bY);
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
on_change(__clone(y), (_item, $aE) => {
	const $aF = [ y_runs, "v" ];
	return y_runs.v = $aF[0][$aF[1]] + 1;
}, "lens-nested.vl:33:13");
on_change2(part(doing, "title", (section) => {
	return section[0];
}), (_t, $aN) => {
	const $aO = [ title_runs, "v" ];
	return title_runs.v = $aO[0][$aO[1]] + 1;
}, "lens-nested.vl:34:56");
on_change3(part2(doing, "items", (section) => {
	return section[1].length;
}), (_n, $aQ) => {
	const $aR = [ items_runs, "v" ];
	return items_runs.v = $aR[0][$aR[1]] + 1;
}, "lens-nested.vl:37:4");
on_change4(slot(sections, 0, [ 1 ]), (_section, $aT) => {
	const $aU = [ todo_runs, "v" ];
	return todo_runs.v = $aU[0][$aU[1]] + 1;
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
batch(($bV) => {
	remove_at(sections, 0, [ 0, $bV ]);
	push(items, "z", [ 0, $bV ]);
	edit2(doing, "title", (section) => {
		return section[0] = "In progress";
	}, [ 0, $bV ]);
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
