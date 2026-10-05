function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __shared_new(value) {
	return { v: value };
}
function new2(value) {
	return [ __shared_new(__clone(value)) ];
}
function get(self) {
	return __clone(self[0].v);
}
function set(self, value) {
	self[0].v = __clone(value);
}
function update(self, transform) {
	set(self, transform(get(self)));
}
const counter = new2(0);
update(counter, (n) => {
	return n + 1;
});
update(counter, (n) => {
	return n * 10;
});
console.log(String(get(counter)));
const label = new2("a");
set(label, "hello");
console.log(get(label));
