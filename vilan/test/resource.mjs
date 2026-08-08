function __replace(target, value) {
	if (Array.isArray(target) && Array.isArray(value)) target.length = value.length;
	return Object.assign(target, value);
}
function drop(self) {
	console.log(self[0]);
}
function drop2(self) {
	console.log("pair-body");
}
function locals() {
	const a = [ "a" ];
	try {
		const b = [ "b" ];
		try {
			console.log("locals-body");
		} finally {
			$a(b);
		}
	} finally {
		$a(a);
	}
}
function early(stop) {
	const r = [ "early" ];
	try {
		if (stop) {
			console.log("stopping");
			return;
		}
		console.log("continuing");
	} finally {
		$a(r);
	}
}
function overwrite() {
	let r = [ "old" ];
	try {
		$a(r);
		r = [ "new" ];
		console.log("overwrite-body");
	} finally {
		$a(r);
	}
}
function nested() {
	const pair = [ [ "first" ], [ "second" ] ];
	try {
		console.log("nested-body");
	} finally {
		$c(pair);
	}
}
function containment() {
	const bag = [ [ "bagged" ] ];
	try {
		console.log("containment-body");
	} finally {
		$e(bag);
	}
}
function refill(self) {
	$g(self);
	__replace(self, [ [ "refilled" ] ]);
}
function view_overwrite(slot) {
	$g(slot);
	__replace(slot, [ [ "replaced" ] ]);
}
function view_writes() {
	let slot = [ [ "original" ] ];
	try {
		view_overwrite(slot);
		refill(slot);
		console.log("view-writes-body");
	} finally {
		$g(slot);
	}
}
function loaned() {
	const s = [ [ "loaned" ] ];
	try {
		console.log("loaned-body");
	} finally {
		$g(s);
	}
}
function component_owned() {
	let slot = [ [ "component-old" ] ];
	try {
		$a(slot[0]);
		slot[0] = [ "component-new" ];
		console.log("component-owned-body");
	} finally {
		$g(slot);
	}
}
function component_view(slot) {
	$a(slot[0]);
	slot[0] = [ "through-view" ];
}
function component_data() {
	let counted = [ [ "counted" ], 1 ];
	try {
		counted[1] = 2;
		console.log("component-data-body");
	} finally {
		$i(counted);
	}
}
function component_writes() {
	component_owned();
	let slot = [ [ "viewed-old" ] ];
	try {
		component_view(slot);
		component_data();
		console.log("component-writes-body");
	} finally {
		$g(slot);
	}
}
function $a($b) {
	drop($b);
}
function $c($d) {
	drop2($d);
	$a($d[1]);
	$a($d[0]);
}
function $e($f) {
	$a($f[0]);
}
function $g($h) {
	$a($h[0]);
}
function $i($j) {
	$a($j[0]);
}
locals();
console.log("--");
early(true);
console.log("--");
early(false);
console.log("--");
overwrite();
console.log("--");
nested();
console.log("--");
containment();
console.log("--");
view_writes();
console.log("--");
loaned();
console.log("--");
component_writes();
