function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_put(list, index, value, location) {
	if (index >= 0 && index < list.length) return list[index] = value;
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
function __map_get(map, key) {
	return map.has(key) ? [ 0, __clone(map.get(key)) ] : [ 1 ];
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
function hash(self) {
	return __hash(self);
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $l = null;
	if (wrapped < 0) {
		$l = wrapped + modulus;
	} else {
		$l = wrapped;
	}
	return $l;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $m = null;
	if (wrapped >= half) {
		$m = wrapped - modulus;
	} else {
		$m = wrapped;
	}
	return $m;
}
function as_i53(self) {
	const widened = Number(self);
	return Number(Math.trunc(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function next_random(bound) {
	const state = seed.v * 16807 % 2147483647;
	seed.v = state;
	return as_i32(state % as_i53(bound));
}
function render_step(step) {
	const $j = step;
	let $k = null;
	if ($j[0] === 0) {
		const index = $j[1];
		$k = "K" + index + " ";
	} else if ($j[0] === 1) {
		const index2 = $j[1];
		$k = "R" + index2 + " ";
	} else {
		$k = "F ";
	}
	return $k;
}
function render(plan) {
	let out = "";
	for (const step of plan[0]) {
		out = out + render_step(step);
	}
	out = out + "| ";
	for (const index of plan[1]) {
		out = out + ("" + index + " ");
	}
	return out;
}
function compare_int(label, old4, new_items) {
	const indexed = reconcile(old4, old4, new_items, (item) => {
		return item;
	}, (before, after) => {
		return before === after;
	});
	const scanned = scan_reconcile(old4, old4, new_items, (item) => {
		return item;
	}, (before, after) => {
		return before === after;
	});
	cases.v = cases.v + 1;
	if (render(indexed) !== render(scanned)) {
		(() => {
			throw __panic("" + label + ": indexed=[" + render(indexed) + "] scanned=[" + render(scanned) + "]", "reconcile-index.vl:107:3");
		})();
	}
}
function eq(self, other) {
	return self[0] === other[0];
}
function hash2(self) {
	return hash(self[0]);
}
function eq2(self, other) {
	return self[0] === other[0] && self[1] === other[1];
}
function hash3(self) {
	return hash(self[0]);
}
function compare_tags(label, old4, new_items) {
	const indexed = reconcile2(old4, old4, new_items, (item) => {
		return __clone(item);
	}, (_before, _after) => {
		return true;
	});
	const scanned = scan_reconcile2(old4, old4, new_items, (item) => {
		return __clone(item);
	}, (_before, _after) => {
		return true;
	});
	cases.v = cases.v + 1;
	if (render(indexed) !== render(scanned)) {
		(() => {
			throw __panic("" + label + ": indexed=[" + render(indexed) + "] scanned=[" + render(scanned) + "]", "reconcile-index.vl:158:3");
		})();
	}
}
function compare_pairs(label, old4, new_items) {
	const indexed = reconcile3(old4, old4, new_items, (item) => {
		return __clone(item);
	}, (_before, _after) => {
		return true;
	});
	const scanned = scan_reconcile3(old4, old4, new_items, (item) => {
		return __clone(item);
	}, (_before, _after) => {
		return true;
	});
	cases.v = cases.v + 1;
	if (render(indexed) !== render(scanned)) {
		(() => {
			throw __panic("" + label + ": indexed=[" + render(indexed) + "] scanned=[" + render(scanned) + "]", "reconcile-index.vl:167:3");
		})();
	}
}
function reconcile(old_keys, old_items, items, key_of, same) {
	let claimed = [  ];
	for (const _ of old_keys) {
		claimed.push(false);
	}
	const held = old_keys.length;
	let first = new Map();
	let next_same = [  ];
	for (const _ of old_keys) {
		next_same.push([ 1 ]);
	}
	let build = held;
	while (build > 0) {
		build = build - 1;
		const canonical = hash(__at(old_keys, build, "std/src/reactive.vl:3677:19"));
		__at_put(next_same, build, __map_get(first, canonical), "std/src/reactive.vl:3678:3");
		first.set(canonical, build);
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		const canonical2 = hash(item_key);
		let head = __map_get(first, canonical2);
		let advancing = true;
		while (advancing) {
			const $a = head;
			let $b = null;
			if ($a[0] === 0) {
				const at = $a[1];
				if (__at(claimed, at, "std/src/reactive.vl:3694:9")) {
					head = __at(next_same, at, "std/src/reactive.vl:3695:14");
				} else {
					advancing = false;
				}
				$b = undefined;
			} else {
				$b = advancing = false;
			}
			$b;
		}
		const $c = head;
		let $d = null;
		if ($c[0] === 0) {
			const at2 = $c[1];
			$d = first.set(canonical2, at2);
		} else {
			$d = first.delete(canonical2);
		}
		$d;
		let found = [ 1 ];
		let walk = head;
		let walking = true;
		while (walking) {
			const $e = walk;
			let $f = null;
			if ($e[0] === 0) {
				const at3 = $e[1];
				if (!(__at(claimed, at3, "std/src/reactive.vl:3713:10")) && __at(old_keys, at3, "std/src/reactive.vl:3713:25") === item_key) {
					found = [ 0, at3 ];
					walking = false;
				} else {
					walk = __at(next_same, at3, "std/src/reactive.vl:3720:14");
				}
				$f = undefined;
			} else {
				$f = walking = false;
			}
			$f;
		}
		let step = [ 2 ];
		const $g = found;
		if ($g[0] === 0) {
			__at_put(claimed, $g[1], true, "std/src/reactive.vl:3728:4");
			let $h = null;
			if (same(__at(old_items, $g[1], "std/src/reactive.vl:3729:19"), item)) {
				$h = [ 0, $g[1] ];
			} else {
				$h = [ 1, $g[1] ];
			}
			step = $h;
		}
		steps.push(step);
	}
	let removed = [  ];
	let index = 0;
	while (index < held) {
		if (!(__at(claimed, index, "std/src/reactive.vl:3736:7"))) {
			removed.push(index);
		}
		index = index + 1;
	}
	return [ steps, removed ];
}
function scan_reconcile(old_keys, old_items, items, key_of, same) {
	let claimed = [  ];
	for (const _ of old_keys) {
		claimed.push(false);
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		let step = [ 2 ];
		let index = 0;
		while (index < old_keys.length) {
			if (!(__at(claimed, index, "reconcile-index.vl:54:8")) && __at(old_keys, index, "reconcile-index.vl:54:26") === item_key) {
				__at_put(claimed, index, true, "reconcile-index.vl:55:5");
				let $i = null;
				if (same(__at(old_items, index, "reconcile-index.vl:56:20"), item)) {
					$i = [ 0, index ];
				} else {
					$i = [ 1, index ];
				}
				step = $i;
				break;
			}
			index = index + 1;
		}
		steps.push(step);
	}
	let removed = [  ];
	let index2 = 0;
	while (index2 < old_keys.length) {
		if (!(__at(claimed, index2, "reconcile-index.vl:70:7"))) {
			removed.push(index2);
		}
		index2 = index2 + 1;
	}
	return [ steps, removed ];
}
function reconcile2(old_keys, old_items, items, key_of, same) {
	let claimed = [  ];
	for (const _ of old_keys) {
		claimed.push(false);
	}
	const held = old_keys.length;
	let first = new Map();
	let next_same = [  ];
	for (const _ of old_keys) {
		next_same.push([ 1 ]);
	}
	let build = held;
	while (build > 0) {
		build = build - 1;
		const canonical = hash2(__at(old_keys, build, "std/src/reactive.vl:3677:19"));
		__at_put(next_same, build, __map_get(first, canonical), "std/src/reactive.vl:3678:3");
		first.set(canonical, build);
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		const canonical2 = hash2(item_key);
		let head = __map_get(first, canonical2);
		let advancing = true;
		while (advancing) {
			const $n = head;
			let $o = null;
			if ($n[0] === 0) {
				const at = $n[1];
				if (__at(claimed, at, "std/src/reactive.vl:3694:9")) {
					head = __at(next_same, at, "std/src/reactive.vl:3695:14");
				} else {
					advancing = false;
				}
				$o = undefined;
			} else {
				$o = advancing = false;
			}
			$o;
		}
		const $p = head;
		let $q = null;
		if ($p[0] === 0) {
			const at2 = $p[1];
			$q = first.set(canonical2, at2);
		} else {
			$q = first.delete(canonical2);
		}
		$q;
		let found = [ 1 ];
		let walk = head;
		let walking = true;
		while (walking) {
			const $r = walk;
			let $s = null;
			if ($r[0] === 0) {
				const at3 = $r[1];
				if (!(__at(claimed, at3, "std/src/reactive.vl:3713:10")) && eq(__at(old_keys, at3, "std/src/reactive.vl:3713:25"), item_key)) {
					found = [ 0, at3 ];
					walking = false;
				} else {
					walk = __at(next_same, at3, "std/src/reactive.vl:3720:14");
				}
				$s = undefined;
			} else {
				$s = walking = false;
			}
			$s;
		}
		let step = [ 2 ];
		const $t = found;
		if ($t[0] === 0) {
			__at_put(claimed, $t[1], true, "std/src/reactive.vl:3728:4");
			let $u = null;
			if (same(__at(old_items, $t[1], "std/src/reactive.vl:3729:19"), item)) {
				$u = [ 0, $t[1] ];
			} else {
				$u = [ 1, $t[1] ];
			}
			step = $u;
		}
		steps.push(step);
	}
	let removed = [  ];
	let index = 0;
	while (index < held) {
		if (!(__at(claimed, index, "std/src/reactive.vl:3736:7"))) {
			removed.push(index);
		}
		index = index + 1;
	}
	return [ steps, removed ];
}
function scan_reconcile2(old_keys, old_items, items, key_of, same) {
	let claimed = [  ];
	for (const _ of old_keys) {
		claimed.push(false);
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		let step = [ 2 ];
		let index = 0;
		while (index < old_keys.length) {
			if (!(__at(claimed, index, "reconcile-index.vl:54:8")) && eq(__at(old_keys, index, "reconcile-index.vl:54:26"), item_key)) {
				__at_put(claimed, index, true, "reconcile-index.vl:55:5");
				let $v = null;
				if (same(__at(old_items, index, "reconcile-index.vl:56:20"), item)) {
					$v = [ 0, index ];
				} else {
					$v = [ 1, index ];
				}
				step = $v;
				break;
			}
			index = index + 1;
		}
		steps.push(step);
	}
	let removed = [  ];
	let index2 = 0;
	while (index2 < old_keys.length) {
		if (!(__at(claimed, index2, "reconcile-index.vl:70:7"))) {
			removed.push(index2);
		}
		index2 = index2 + 1;
	}
	return [ steps, removed ];
}
function reconcile3(old_keys, old_items, items, key_of, same) {
	let claimed = [  ];
	for (const _ of old_keys) {
		claimed.push(false);
	}
	const held = old_keys.length;
	let first = new Map();
	let next_same = [  ];
	for (const _ of old_keys) {
		next_same.push([ 1 ]);
	}
	let build = held;
	while (build > 0) {
		build = build - 1;
		const canonical = hash3(__at(old_keys, build, "std/src/reactive.vl:3677:19"));
		__at_put(next_same, build, __map_get(first, canonical), "std/src/reactive.vl:3678:3");
		first.set(canonical, build);
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		const canonical2 = hash3(item_key);
		let head = __map_get(first, canonical2);
		let advancing = true;
		while (advancing) {
			const $w = head;
			let $x = null;
			if ($w[0] === 0) {
				const at = $w[1];
				if (__at(claimed, at, "std/src/reactive.vl:3694:9")) {
					head = __at(next_same, at, "std/src/reactive.vl:3695:14");
				} else {
					advancing = false;
				}
				$x = undefined;
			} else {
				$x = advancing = false;
			}
			$x;
		}
		const $y = head;
		let $z = null;
		if ($y[0] === 0) {
			const at2 = $y[1];
			$z = first.set(canonical2, at2);
		} else {
			$z = first.delete(canonical2);
		}
		$z;
		let found = [ 1 ];
		let walk = head;
		let walking = true;
		while (walking) {
			const $A = walk;
			let $B = null;
			if ($A[0] === 0) {
				const at3 = $A[1];
				if (!(__at(claimed, at3, "std/src/reactive.vl:3713:10")) && eq2(__at(old_keys, at3, "std/src/reactive.vl:3713:25"), item_key)) {
					found = [ 0, at3 ];
					walking = false;
				} else {
					walk = __at(next_same, at3, "std/src/reactive.vl:3720:14");
				}
				$B = undefined;
			} else {
				$B = walking = false;
			}
			$B;
		}
		let step = [ 2 ];
		const $C = found;
		if ($C[0] === 0) {
			__at_put(claimed, $C[1], true, "std/src/reactive.vl:3728:4");
			let $D = null;
			if (same(__at(old_items, $C[1], "std/src/reactive.vl:3729:19"), item)) {
				$D = [ 0, $C[1] ];
			} else {
				$D = [ 1, $C[1] ];
			}
			step = $D;
		}
		steps.push(step);
	}
	let removed = [  ];
	let index = 0;
	while (index < held) {
		if (!(__at(claimed, index, "std/src/reactive.vl:3736:7"))) {
			removed.push(index);
		}
		index = index + 1;
	}
	return [ steps, removed ];
}
function scan_reconcile3(old_keys, old_items, items, key_of, same) {
	let claimed = [  ];
	for (const _ of old_keys) {
		claimed.push(false);
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		let step = [ 2 ];
		let index = 0;
		while (index < old_keys.length) {
			if (!(__at(claimed, index, "reconcile-index.vl:54:8")) && eq2(__at(old_keys, index, "reconcile-index.vl:54:26"), item_key)) {
				__at_put(claimed, index, true, "reconcile-index.vl:55:5");
				let $E = null;
				if (same(__at(old_items, index, "reconcile-index.vl:56:20"), item)) {
					$E = [ 0, index ];
				} else {
					$E = [ 1, index ];
				}
				step = $E;
				break;
			}
			index = index + 1;
		}
		steps.push(step);
	}
	let removed = [  ];
	let index2 = 0;
	while (index2 < old_keys.length) {
		if (!(__at(claimed, index2, "reconcile-index.vl:70:7"))) {
			removed.push(index2);
		}
		index2 = index2 + 1;
	}
	return [ steps, removed ];
}
const seed = __shared_new(7);
const cases = __shared_new(0);
compare_int("empty->empty", [  ], [  ]);
compare_int("empty->three", [  ], [ 1, 2, 3 ]);
compare_int("three->empty", [ 1, 2, 3 ], [  ]);
compare_int("append", [ 1, 2, 3 ], [ 1, 2, 3, 4 ]);
compare_int("prepend", [ 1, 2, 3 ], [ 0, 1, 2, 3 ]);
compare_int("remove-front", [ 1, 2, 3 ], [ 2, 3 ]);
compare_int("remove-back", [ 1, 2, 3 ], [ 1, 2 ]);
compare_int("remove-middle", [ 1, 2, 3 ], [ 1, 3 ]);
compare_int("reverse", [ 1, 2, 3, 4 ], [ 4, 3, 2, 1 ]);
compare_int("swap", [ 1, 2, 3 ], [ 1, 3, 2 ]);
compare_int("dup-old", [ 1, 1, 2 ], [ 1, 2 ]);
compare_int("dup-new", [ 1, 2 ], [ 1, 1, 2 ]);
compare_int("dup-both", [ 1, 1, 2, 2 ], [ 2, 1, 2, 1 ]);
compare_int("all-fresh", [ 1, 2, 3 ], [ 7, 8, 9 ]);
compare_int("dup-only", [ 5, 5, 5 ], [ 5, 5, 5, 5 ]);
let round = 0;
while (round < 600) {
	const old_length = next_random(9);
	const new_length = next_random(9);
	const alphabet = 1 + next_random(5);
	let old = [  ];
	let fill = 0;
	while (fill < old_length) {
		old.push(next_random(alphabet));
		fill = fill + 1;
	}
	let fresh = [  ];
	fill = 0;
	while (fill < new_length) {
		fresh.push(next_random(alphabet));
		fill = fill + 1;
	}
	compare_int("random " + round, old, fresh);
	round = round + 1;
}
round = 0;
while (round < 400) {
	const old_length2 = next_random(7);
	const new_length2 = next_random(7);
	let old2 = [  ];
	let fill2 = 0;
	while (fill2 < old_length2) {
		old2.push([ next_random(3), next_random(50) ]);
		fill2 = fill2 + 1;
	}
	let fresh2 = [  ];
	fill2 = 0;
	while (fill2 < new_length2) {
		fresh2.push([ next_random(3), next_random(50) ]);
		fill2 = fill2 + 1;
	}
	compare_tags("tags " + round, old2, fresh2);
	round = round + 1;
}
round = 0;
while (round < 400) {
	const old_length3 = next_random(7);
	const new_length3 = next_random(7);
	let old3 = [  ];
	let fill3 = 0;
	while (fill3 < old_length3) {
		old3.push([ next_random(3), next_random(4) ]);
		fill3 = fill3 + 1;
	}
	let fresh3 = [  ];
	fill3 = 0;
	while (fill3 < new_length3) {
		fresh3.push([ next_random(3), next_random(4) ]);
		fill3 = fill3 + 1;
	}
	compare_pairs("pairs " + round, old3, fresh3);
	round = round + 1;
}
console.log("cases=" + cases.v + " differences=0");
