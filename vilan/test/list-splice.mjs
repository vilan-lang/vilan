function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __insert_at(list, index, value, location) {
	if (index >= 0 && index < list.length) return void list.splice(index, 0, value);
	if (index === list.length) return void list.push(value);
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function __remove_at(list, index, location) {
	if (index >= 0 && index < list.length) return list.splice(index, 1)[0];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
let xs = [ 1, 2, 4 ];
__insert_at(xs, 2, 3, "list-splice.vl:6:5");
console.log(xs.length);
console.log(__at(xs, 2, "list-splice.vl:8:8"));
console.log(__at(xs, 3, "list-splice.vl:9:8"));
__insert_at(xs, 4, 5, "list-splice.vl:11:5");
console.log(__at(xs, 4, "list-splice.vl:12:8"));
__insert_at(xs, 0, 0, "list-splice.vl:14:5");
console.log(__at(xs, 0, "list-splice.vl:15:8"));
console.log(__at(xs, 1, "list-splice.vl:16:8"));
console.log(xs.length);
console.log(__remove_at(xs, 0, "list-splice.vl:19:11"));
console.log(__at(xs, 0, "list-splice.vl:20:8"));
console.log(__remove_at(xs, 4, "list-splice.vl:21:11"));
console.log(xs.length);
console.log(__remove_at(xs, 1, "list-splice.vl:24:11"));
console.log(__at(xs, 1, "list-splice.vl:25:8"));
console.log(__at(xs, 2, "list-splice.vl:26:8"));
console.log(xs.length);
