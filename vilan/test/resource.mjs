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
	$a(a);
	const b = [ "b" ];
	$a(b);
	console.log("locals-body");
}
function early(stop) {
	const r = [ "early" ];
	$a(r);
	if (stop) {
		console.log("stopping");
		return;
	}
	console.log("continuing");
}
function overwrite() {
	let r = [ "old" ];
	try {
		$a(r);
		r = [ "new" ];
	} finally {
		$a(r);
	}
	console.log("overwrite-body");
}
function nested() {
	const pair = [ [ "first" ], [ "second" ] ];
	$c(pair);
	console.log("nested-body");
}
function containment() {
	const bag = [ [ "bagged" ] ];
	$e(bag);
	console.log("containment-body");
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
	} finally {
		$g(slot);
	}
	console.log("view-writes-body");
}
function loaned() {
	const s = [ [ "loaned" ] ];
	try {

	} finally {
		$g(s);
	}
	console.log("loaned-body");
}
function component_owned() {
	let slot = [ [ "component-old" ] ];
	try {
		$a(slot[0]);
		slot[0] = [ "component-new" ];
	} finally {
		$g(slot);
	}
	console.log("component-owned-body");
}
function component_view(slot) {
	$a(slot[0]);
	slot[0] = [ "through-view" ];
}
function component_data() {
	let counted = [ [ "counted" ], 1 ];
	try {
		counted[1] = 2;
	} finally {
		$i(counted);
	}
	console.log("component-data-body");
}
function component_writes() {
	component_owned();
	let slot = [ [ "viewed-old" ] ];
	try {
		component_view(slot);
	} finally {
		$g(slot);
	}
	component_data();
	console.log("component-writes-body");
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
