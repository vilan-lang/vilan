function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __at_put(list, index, value) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __list_pop(list) {
	return list.length === 0 ? [ 1 ] : [ 0, list.pop() ];
}
function $a(self, index, value) {
	const length = self.length;
	if (index < 0 || index > length) {
		(() => {
			throw "index out of bounds: the length is " + length + " but the index is " + index;
		})();
	}
	if (index === length) {
		self.push(value);
	} else {
		self.push(__at(self, length - 1));
		let cursor = length - 1;
		while (cursor > index) {
			__at_put(self, cursor, __at(self, cursor - 1));
			cursor = cursor - 1;
		}
		__at_put(self, index, value);
	}
}
function $b(self, index) {
	const length = self.length;
	if (index < 0 || index >= length) {
		(() => {
			throw "index out of bounds: the length is " + length + " but the index is " + index;
		})();
	}
	const removed = __at(self, index);
	let cursor = index;
	while (cursor < length - 1) {
		__at_put(self, cursor, __at(self, cursor + 1));
		cursor = cursor + 1;
	}
	__list_pop(self);
	return removed;
}
let xs = [ 1, 2, 4 ];
$a(xs, 2, 3);
console.log(xs.length);
console.log(__at(xs, 2));
console.log(__at(xs, 3));
$a(xs, 4, 5);
console.log(__at(xs, 4));
$a(xs, 0, 0);
console.log(__at(xs, 0));
console.log(__at(xs, 1));
console.log(xs.length);
console.log($b(xs, 0));
console.log(__at(xs, 0));
console.log($b(xs, 4));
console.log(xs.length);
