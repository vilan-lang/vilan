function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __at_put(list, index, value) {
	if (index >= 0 && index < list.length) return list[index] = value;
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
function __map_get(map, key) {
	return map.has(key) ? [ 0, __clone(map.get(key)) ] : [ 1 ];
}
function __shared_new(value) {
	return { v: value };
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $m = null;
	if (wrapped < 0) {
		$m = wrapped + modulus;
	} else {
		$m = wrapped;
	}
	return $m;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $n = null;
	if (wrapped >= half) {
		$n = wrapped - modulus;
	} else {
		$n = wrapped;
	}
	return $n;
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
	const $k = step;
	let $l = null;
	if ($k[0] === 0) {
		const index = $k[1];
		$l = "K" + index + " ";
	} else if ($k[0] === 1) {
		const index2 = $k[1];
		$l = "R" + index2 + " ";
	} else {
		$l = "F ";
	}
	return $l;
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
function compare_int(label, old3, new_items) {
	const indexed = $a(old3, old3, new_items, (item) => {
		return item;
	}, (before, after) => {
		return before === after;
	});
	const scanned = $i(old3, old3, new_items, (item) => {
		return item;
	}, (before, after) => {
		return before === after;
	});
	cases.v = cases.v + 1;
	if (render(indexed) !== render(scanned)) {
		(() => {
			throw "" + label + ": indexed=[" + render(indexed) + "] scanned=[" + render(scanned) + "]";
		})();
	}
}
function eq(self, other) {
	return self[0] === other[0];
}
function compare_tags(label, old3, new_items) {
	const indexed = $o(old3, old3, new_items, (item) => {
		return __clone(item);
	}, (_before, _after) => {
		return true;
	});
	const scanned = $w(old3, old3, new_items, (item) => {
		return __clone(item);
	}, (_before, _after) => {
		return true;
	});
	cases.v = cases.v + 1;
	if (render(indexed) !== render(scanned)) {
		(() => {
			throw "" + label + ": indexed=[" + render(indexed) + "] scanned=[" + render(scanned) + "]";
		})();
	}
}
function $a(old_keys, old_items, items, key_of, same) {
	let claimed = [  ];
	for (const _ of old_keys) {
		claimed.push(false);
	}
	const held = old_keys.length;
	let first = new Map();
	let next_same = [  ];
	for (const _ of old_keys) {
		next_same.push(-(1));
	}
	let build = held - 1;
	while (build >= 0) {
		const canonical = __hash(__at(old_keys, build));
		const $b = __map_get(first, canonical);
		let $c = null;
		if ($b[0] === 0) {
			const after = $b[1];
			$c = __at_put(next_same, build, after);
		} else {
			$c = undefined;
		}
		$c;
		first.set(canonical, build);
		build = build - 1;
	}
	let low = 0;
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		const canonical2 = __hash(item_key);
		const $d = __map_get(first, canonical2);
		let $e = null;
		if ($d[0] === 0) {
			const at = $d[1];
			$e = at;
		} else {
			$e = -(1);
		}
		let head = $e;
		while (head >= 0) {
			if (!(__at(claimed, head))) {
				break;
			}
			head = __at(next_same, head);
		}
		first.set(canonical2, head);
		let candidate = -(1);
		let walk = head;
		while (walk >= 0) {
			if (!(__at(claimed, walk)) && __at(old_keys, walk) === item_key) {
				candidate = walk;
				break;
			}
			walk = __at(next_same, walk);
		}
		let $f = null;
		if (candidate < 0) {
			$f = held;
		} else {
			$f = candidate;
		}
		const ceiling = $f;
		let found = candidate;
		let index = low;
		while (index < ceiling) {
			if (!(__at(claimed, index)) && __at(old_keys, index) === item_key) {
				found = index;
				break;
			}
			index = index + 1;
		}
		let step = [ 2 ];
		let $h = null;
		if (found >= 0) {
			__at_put(claimed, found, true);
			let $g = null;
			if (same(__at(old_items, found), item)) {
				$g = [ 0, found ];
			} else {
				$g = [ 1, found ];
			}
			step = $g;
			while (low < held) {
				if (!(__at(claimed, low))) {
					break;
				}
				low = low + 1;
			}
			$h = undefined;
		}
		$h;
		steps.push(step);
	}
	let removed = [  ];
	let index2 = 0;
	while (index2 < held) {
		if (!(__at(claimed, index2))) {
			removed.push(index2);
		}
		index2 = index2 + 1;
	}
	return [ steps, removed ];
}
function $i(old_keys, old_items, items, key_of, same) {
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
			if (!(__at(claimed, index)) && __at(old_keys, index) === item_key) {
				__at_put(claimed, index, true);
				let $j = null;
				if (same(__at(old_items, index), item)) {
					$j = [ 0, index ];
				} else {
					$j = [ 1, index ];
				}
				step = $j;
				break;
			}
			index = index + 1;
		}
		steps.push(step);
	}
	let removed = [  ];
	let index2 = 0;
	while (index2 < old_keys.length) {
		if (!(__at(claimed, index2))) {
			removed.push(index2);
		}
		index2 = index2 + 1;
	}
	return [ steps, removed ];
}
function $o(old_keys, old_items, items, key_of, same) {
	let claimed = [  ];
	for (const _ of old_keys) {
		claimed.push(false);
	}
	const held = old_keys.length;
	let first = new Map();
	let next_same = [  ];
	for (const _ of old_keys) {
		next_same.push(-(1));
	}
	let build = held - 1;
	while (build >= 0) {
		const canonical = __hash(__at(old_keys, build));
		const $p = __map_get(first, canonical);
		let $q = null;
		if ($p[0] === 0) {
			const after = $p[1];
			$q = __at_put(next_same, build, after);
		} else {
			$q = undefined;
		}
		$q;
		first.set(canonical, build);
		build = build - 1;
	}
	let low = 0;
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		const canonical2 = __hash(item_key);
		const $r = __map_get(first, canonical2);
		let $s = null;
		if ($r[0] === 0) {
			const at = $r[1];
			$s = at;
		} else {
			$s = -(1);
		}
		let head = $s;
		while (head >= 0) {
			if (!(__at(claimed, head))) {
				break;
			}
			head = __at(next_same, head);
		}
		first.set(canonical2, head);
		let candidate = -(1);
		let walk = head;
		while (walk >= 0) {
			if (!(__at(claimed, walk)) && eq(__at(old_keys, walk), item_key)) {
				candidate = walk;
				break;
			}
			walk = __at(next_same, walk);
		}
		let $t = null;
		if (candidate < 0) {
			$t = held;
		} else {
			$t = candidate;
		}
		const ceiling = $t;
		let found = candidate;
		let index = low;
		while (index < ceiling) {
			if (!(__at(claimed, index)) && eq(__at(old_keys, index), item_key)) {
				found = index;
				break;
			}
			index = index + 1;
		}
		let step = [ 2 ];
		let $v = null;
		if (found >= 0) {
			__at_put(claimed, found, true);
			let $u = null;
			if (same(__at(old_items, found), item)) {
				$u = [ 0, found ];
			} else {
				$u = [ 1, found ];
			}
			step = $u;
			while (low < held) {
				if (!(__at(claimed, low))) {
					break;
				}
				low = low + 1;
			}
			$v = undefined;
		}
		$v;
		steps.push(step);
	}
	let removed = [  ];
	let index2 = 0;
	while (index2 < held) {
		if (!(__at(claimed, index2))) {
			removed.push(index2);
		}
		index2 = index2 + 1;
	}
	return [ steps, removed ];
}
function $w(old_keys, old_items, items, key_of, same) {
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
			if (!(__at(claimed, index)) && eq(__at(old_keys, index), item_key)) {
				__at_put(claimed, index, true);
				let $x = null;
				if (same(__at(old_items, index), item)) {
					$x = [ 0, index ];
				} else {
					$x = [ 1, index ];
				}
				step = $x;
				break;
			}
			index = index + 1;
		}
		steps.push(step);
	}
	let removed = [  ];
	let index2 = 0;
	while (index2 < old_keys.length) {
		if (!(__at(claimed, index2))) {
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
console.log("cases=" + cases.v + " differences=0");
