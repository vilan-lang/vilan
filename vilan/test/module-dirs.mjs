function surface() {
	return "nested::surface";
}
function label() {
	return "nested::ui::widget::label";
}
function width() {
	return 42;
}
function hello() {
	return "nested::util::hello";
}
console.log(surface());
console.log(hello());
console.log(hello());
console.log(label());
console.log(label());
console.log("width is " + width());
