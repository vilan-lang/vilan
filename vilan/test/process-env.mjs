function __args() {
	return process.argv.slice(2);
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __env(key) {
	const value = process.env[key];
	return value === undefined ? [ 1 ] : [ 0, value ];
}
function $c(self, fallback) {
	const $d = self;
	let $e = null;
	if ($d[0] === 0) {
		const x = __clone($d[1]);
		$e = x;
	} else {
		$e = __clone(fallback);
	}
	return $e;
}
const arguments2 = __args();
console.log(arguments2.length);
const $a = __env("VILAN_TEST_VAR");
let $b = null;
if ($a[0] === 0) {
	const value = $a[1];
	$b = console.log(value);
} else {
	$b = console.log("unset");
}
$b;
console.log($c(__env("DEFINITELY_NOT_SET_XYZ"), "unset"));
