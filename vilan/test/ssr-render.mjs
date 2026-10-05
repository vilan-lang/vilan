function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __shared_new(value) {
	return { v: value };
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function view(tag) {
	const attributes = __shared_new([  ]);
	if (tag === "svg") {
		set_attribute(attributes, "xmlns", "http://www.w3.org/2000/svg");
	}
	return [ tag, attributes, __shared_new([  ]), __shared_new("") ];
}
function set_attribute(attributes, name2, value) {
	let updated = [  ];
	let found = false;
	for (const attribute of attributes.v) {
		if (attribute[0] === name2) {
			updated.push([ name2, value ]);
			found = true;
		} else {
			updated.push(__clone(attribute));
		}
	}
	if (!(found)) {
		updated.push([ name2, value ]);
	}
	attributes.v = updated;
}
function text(self, content) {
	self[3].v = content;
	self[2].v = [  ];
	return __clone(self);
}
function open(parent) {
	return [ __clone(parent) ];
}
function place(self, parent) {
	parent[2].v.push([ 0, self ]);
}
function place2(self, parent) {
	parent[2].v.push([ 1, self ]);
}
function place3(self, parent) {
	for (const item of self) {
		parent[2].v.push([ 0, __clone(item) ]);
	}
}
function apply(self, parent, name2) {
	set_attribute(parent[1], name2, self);
}
function is_void_element(tag) {
	const $i = tag;
	let $j = null;
	if ($i === "area") {
		$j = true;
	} else if ($i === "base") {
		$j = true;
	} else if ($i === "br") {
		$j = true;
	} else if ($i === "col") {
		$j = true;
	} else if ($i === "embed") {
		$j = true;
	} else if ($i === "hr") {
		$j = true;
	} else if ($i === "img") {
		$j = true;
	} else if ($i === "input") {
		$j = true;
	} else if ($i === "link") {
		$j = true;
	} else if ($i === "meta") {
		$j = true;
	} else if ($i === "source") {
		$j = true;
	} else if ($i === "track") {
		$j = true;
	} else if ($i === "wbr") {
		$j = true;
	} else {
		$j = false;
	}
	return $j;
}
function escape_text(value) {
	return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}
function escape_attribute(value) {
	return value.replaceAll("&", "&amp;").replaceAll("\"", "&quot;");
}
function render(view2) {
	let out = "<" + view2[0];
	for (const attribute of view2[1].v) {
		out = out + " " + attribute[0] + "=\"" + escape_attribute(attribute[1]) + "\"";
	}
	out = out + ">";
	if (is_void_element(view2[0])) {
		return out;
	}
	out = out + escape_text(view2[3].v);
	for (const child7 of view2[2].v) {
		const $k = child7;
		let $l = null;
		if ($k[0] === 0) {
			const element = $k[1];
			out = out + render(element);
			$l = undefined;
		} else {
			const content = $k[1];
			out = out + escape_text(content);
			$l = undefined;
		}
		$l;
	}
	return out + "</" + view2[0] + ">";
}
function app(title2, todos2, page2) {
	const heading = bind_text(view("h1"), __clone(title2));
	const list = child(view("ul"), each(__clone(todos2), (todo) => {
		return todo;
	}, (todo, $a) => {
		return text(view("li"), todo);
	}));
	const nav = child2(view("nav"), swap(__clone(page2), (current, $e) => {
		const $f = current;
		let $g = null;
		if ($f[0] === 0) {
			$g = text(attr(view("a"), "href", "/"), "Home");
		} else {
			$g = text(attr(view("a"), "href", "/about"), "About & friends");
		}
		return $g;
	}));
	return child3(child3(child3(attr(view("main"), "id", "app"), heading), list), nav);
}
function new3(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function new4(value) {
	return new3(value);
}
function new8(value) {
	return new3(value);
}
function get(self) {
	return __clone(self[0].v);
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function on_settle(self, subscriber) {
	return attach(self, subscriber);
}
function start(self) {
	return [ () => {
		return get(self);
	}, (subscriber) => {
		return on_settle(self, subscriber);
	}, () => {
		return;
	} ];
}
function read_once(flow) {
	const instance = start(flow);
	const value = instance[0]();
	instance[2]();
	return value;
}
function bind_text(self, source) {
	self[3].v = read_once(source);
	self[2].v = [  ];
	return __clone(self);
}
function each(source, key, render2) {
	return [ source, key, render2 ];
}
function delta_cursor(self) {
	return [ 1 ];
}
function on_settle2(self, subscriber) {
	return attach(self, subscriber);
}
function delta_since(self, cursor) {
	let none = [  ];
	return none;
}
function drop_delta_cursor(self, cursor) {

}
function open_rows(self) {
	const $b = delta_cursor(self);
	let $c = null;
	if ($b[0] === 0) {
		const cursor = $b[1];
		$c = [ () => {
			return get(self);
		}, (subscriber) => {
			return on_settle2(self, subscriber);
		}, () => {
			return delta_since(self, cursor);
		}, () => {
			return drop_delta_cursor(self, cursor);
		} ];
	} else {
		$c = [ () => {
			return get(self);
		}, (subscriber) => {
			return on_settle2(self, subscriber);
		}, () => {
			return [ [ 2, get(self) ] ];
		}, () => {
			return;
		} ];
	}
	return $c;
}
function read_rows(source) {
	const instance = open_rows(source);
	const items = instance[0]();
	instance[3]();
	return items;
}
function open_row(self, content) {
	place(content, self[0]);
	return [  ];
}
function run_with_owner(owner, body) {
	return body(owner);
}
function place_each(parent, source, key, render2) {
	const region = open(parent);
	const items = read_rows(source);
	for (const item of items) {
		const owner = new2();
		run_with_owner(owner, ($d) => {
			return open_row(region, render2(item, $d));
		});
	}
}
function place4(self, parent) {
	place_each(parent, __clone(self[0]), self[1], self[2]);
}
function child(self, content) {
	place4(content, self);
	return __clone(self);
}
function attr(self, name2, value) {
	apply(value, self, name2);
	return __clone(self);
}
function swap(source, render2) {
	return [ source, render2 ];
}
function start2(self) {
	return [ () => {
		return get(self);
	}, (subscriber) => {
		return on_settle2(self, subscriber);
	}, () => {
		return;
	} ];
}
function read_once2(flow) {
	const instance = start2(flow);
	const value = instance[0]();
	instance[2]();
	return value;
}
function place_swap(parent, source, render2) {
	const region = open(parent);
	const value = read_once2(source);
	const owner = new2();
	run_with_owner(owner, ($h) => {
		return open_row(region, render2(value, $h));
	});
}
function place5(self, parent) {
	place_swap(parent, __clone(self[0]), self[1]);
}
function child2(self, content) {
	place5(content, self);
	return __clone(self);
}
function child3(self, content) {
	place(content, self);
	return __clone(self);
}
function apply_text_flow(flow, parent, name2) {
	set_attribute(parent[1], name2, read_once(flow));
}
function apply2(self, parent, name2) {
	apply_text_flow(self, parent, name2);
}
function attr2(self, name2, value) {
	apply2(value, self, name2);
	return __clone(self);
}
function child4(self, content) {
	place2(content, self);
	return __clone(self);
}
function place_text_flow(flow, parent) {
	parent[2].v.push([ 1, read_once(flow) ]);
}
function place6(self, parent) {
	place_text_flow(self, parent);
}
function child5(self, content) {
	place6(content, self);
	return __clone(self);
}
function child6(self, content) {
	place3(content, self);
	return __clone(self);
}
const no_cleanups = __shared_new([  ]);
const title = new4("Tasks <live>");
const todos = new8([ "alpha", "beta & gamma" ]);
const page = new8([ 1 ]);
console.log(render(app(title, todos, page)));
console.log(render(text(view("p"), "<script>alert(\"&\")</script>")));
console.log(render(attr(attr(view("img"), "src", "/logo.png"), "alt", "a & b")));
console.log(render(child3(attr(view("svg"), "viewBox", "0 0 24 24"), attr(view("path"), "d", "M5 12h14"))));
const name = new4("world & <you>");
const mixed = child5(child4(child3(child4(attr2(view("p"), "data-live", new4("a \"quoted\" & value")), "Take "), text(view("code"), "vilan upgrade")), " & enjoy. "), name);
console.log(render(mixed));
const pair = [ text(view("i"), "a"), text(view("b"), "b") ];
console.log(render(child6(view("p"), pair)));
console.log(render(text(child4(view("p"), "gone"), "kept")));
