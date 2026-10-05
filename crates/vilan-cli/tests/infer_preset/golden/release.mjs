function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function a(){console.log("built");return 5;}const b=7;const c=14;const d=196;const e=[0,3,6,9];const f=107;const g=a();console.log(String(b+c+d+__at(e,2,"src/main.vl:53:26")+f+g));