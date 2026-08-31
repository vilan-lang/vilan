function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __list_get(list, index) {
	return index >= 0 && index < list.length ? [ 0, __clone(list[index]) ] : [ 1 ];
}
function print(message) {
	console.log("[mine] " + message);
}
function pick(values) {
	return __list_get(values, 1);
}
function parse(text) {
	let $e = null;
	if (text === "7") {
		$e = [ 0, 7 ];
	} else {
		$e = [ 1, "not seven" ];
	}
	return $e;
}
const $a = pick([ 10, 20, 30 ]);
let $b = null;
if ($a[0] === 0) {
	const found = $a[1];
	$b = print("some");
} else {
	$b = print("none");
}
$b;
const $c = pick([  ]);
let $d = null;
if ($c[0] === 0) {
	const found2 = $c[1];
	$d = print("some");
} else {
	$d = print("none");
}
$d;
const $f = parse("7");
let $g = null;
if ($f[0] === 0) {
	const value = $f[1];
	$g = print("ok");
} else {
	const reason = $f[1];
	$g = print(reason);
}
$g;
const $h = parse("8");
let $i = null;
if ($h[0] === 0) {
	const value2 = $h[1];
	$i = print("ok");
} else {
	const reason2 = $h[1];
	$i = print(reason2);
}
process.exit($i);
