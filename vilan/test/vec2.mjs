function scale(self, factor) {
	return [ self[0] * factor, self[1] * factor ];
}
function length(self) {
	return Math.sqrt(length_squared(self));
}
function length_squared(self) {
	return dot(self, self);
}
function distance(self, other) {
	return length(sub(self, other));
}
function dot(self, other) {
	return self[0] * other[0] + self[1] * other[1];
}
function add(self, b) {
	return [ self[0] + b[0], self[1] + b[1] ];
}
function sub(self, b) {
	return [ self[0] - b[0], self[1] - b[1] ];
}
function eq(self, b) {
	return self[0] === b[0] && self[1] === b[1];
}
const origin = [ 3.0, 4.0 ];
const pointer = [ 6.0, 8.0 ];
const same_as_origin = [ 3.0, 4.0 ];
console.log(length(origin));
console.log(distance(origin, pointer));
console.log(dot(origin, pointer));
const offset = sub(pointer, origin);
console.log("offset " + offset[0] + " " + offset[1]);
const summed = add(origin, pointer);
console.log("sum " + summed[0] + " " + summed[1]);
const doubled = scale(origin, 2.0);
console.log("scaled " + doubled[0] + " " + doubled[1]);
console.log(eq(origin, same_as_origin));
console.log(!(eq(origin, pointer)));
const start = [ 10.0, 10.0 ];
const now = [ 13.0, 14.0 ];
console.log(distance(now, start) > 3.0);
const travelled = sub(now, start);
console.log(length(travelled));
const threshold = 3.0;
console.log(length_squared(travelled));
console.log(length_squared(travelled) > threshold * threshold);
console.log(length_squared(travelled) === length(travelled) * length(travelled));
