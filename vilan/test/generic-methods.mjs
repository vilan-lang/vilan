function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __shared_new(value) {
	return { v: value };
}
function $a(value) {
	return [ __shared_new(value) ];
}
function $c(self) {
	return __clone(self[0].v);
}
function $d(self, value) {
	self[0].v = __clone(value);
}
function $b(self, transform) {
	$d(self, transform($c(self)));
}
const counter = $a(0);
$b(counter, (n) => {
	return n + 1;
});
$b(counter, (n) => {
	return n * 10;
});
console.log($c(counter));
const label = $a("a");
$d(label, "hello");
console.log($c(label));
