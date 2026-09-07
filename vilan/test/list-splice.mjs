function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __insert_at(list, index, value) {
	if (index >= 0 && index < list.length) return void list.splice(index, 0, value);
	if (index === list.length) return void list.push(value);
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __remove_at(list, index) {
	if (index >= 0 && index < list.length) return list.splice(index, 1)[0];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
let xs = [ 1, 2, 4 ];
__insert_at(xs, 2, 3);
console.log(xs.length);
console.log(__at(xs, 2));
console.log(__at(xs, 3));
__insert_at(xs, 4, 5);
console.log(__at(xs, 4));
__insert_at(xs, 0, 0);
console.log(__at(xs, 0));
console.log(__at(xs, 1));
console.log(xs.length);
console.log(__remove_at(xs, 0));
console.log(__at(xs, 0));
console.log(__remove_at(xs, 4));
console.log(xs.length);
console.log(__remove_at(xs, 1));
console.log(__at(xs, 1));
console.log(__at(xs, 2));
console.log(xs.length);
