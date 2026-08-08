function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function a(){console.log("built");return 5;}const b=7;const c=14;const d=196;const e=[0,3,6,9];const f=107;const g=a();console.log(b+c+d+__at(e,2)+f+g);