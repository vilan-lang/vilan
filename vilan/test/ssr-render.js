function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __shared_new(value) {
	return { v: value };
}
function new2() {
	return [ __shared_new([  ]) ];
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
function bind_text(self, source) {
	self[3].v = $d(source);
	self[2].v = [  ];
	return __clone(self);
}
function place(self, parent) {
	parent[2].v.push([ 0, __clone(self) ]);
}
function place2(self, parent) {
	parent[2].v.push([ 1, self ]);
}
function place3(self, parent) {
	parent[2].v.push([ 1, $d(self) ]);
}
function place4(self, parent) {
	for (const item of self) {
		parent[2].v.push([ 0, __clone(item) ]);
	}
}
function apply(self, parent, name2) {
	set_attribute(parent[1], name2, self);
}
function apply2(self, parent, name2) {
	set_attribute(parent[1], name2, $d(self));
}
function is_void_element(tag) {
	const $r = tag;
	let $s = null;
	if ($r === "area") {
		$s = true;
	} else if ($r === "base") {
		$s = true;
	} else if ($r === "br") {
		$s = true;
	} else if ($r === "col") {
		$s = true;
	} else if ($r === "embed") {
		$s = true;
	} else if ($r === "hr") {
		$s = true;
	} else if ($r === "img") {
		$s = true;
	} else if ($r === "input") {
		$s = true;
	} else if ($r === "link") {
		$s = true;
	} else if ($r === "meta") {
		$s = true;
	} else if ($r === "source") {
		$s = true;
	} else if ($r === "track") {
		$s = true;
	} else if ($r === "wbr") {
		$s = true;
	} else {
		$s = false;
	}
	return $s;
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
	for (const child of view2[2].v) {
		const $t = child;
		let $u = null;
		if ($t[0] === 0) {
			const element = $t[1];
			out = out + render(element);
			$u = undefined;
		} else {
			const content = $t[1];
			out = out + escape_text(content);
			$u = undefined;
		}
		$u;
	}
	return out + "</" + view2[0] + ">";
}
function app(title2, todos2, page2) {
	const heading = bind_text(view("h1"), title2);
	const list = $f(view("ul"), todos2, (todo) => {
		return todo;
	}, (todo, $e) => {
		return text(view("li"), todo);
	});
	const nav = $n(view("nav"), page2, (current, $j) => {
		const $k = current;
		let $l = null;
		if ($k[0] === 0) {
			$l = text($m(view("a"), "href", "/"), "Home");
		} else {
			$l = text($m(view("a"), "href", "/about"), "About & friends");
		}
		return $l;
	});
	return $q($q($q($m(view("main"), "id", "app"), heading), list), nav);
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $c(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $d(self) {
	return self[0].v;
}
function $g(self) {
	return self[0].v;
}
function $i(owner, body) {
	return body(owner);
}
function $f(self, source, key, build) {
	const items = $g(source);
	for (const item of items) {
		const owner = new2();
		self[2].v.push([ 0, $i(owner, ($h) => {
			return build(item, $h);
		}) ]);
	}
	return __clone(self);
}
function $m(self, name2, value) {
	apply(value, self, name2);
	return __clone(self);
}
function $o(self) {
	return self[0].v;
}
function $n(self, source, build) {
	const value = $o(source);
	const owner = new2();
	self[2].v.push([ 0, $i(owner, ($p) => {
		return build(value, $p);
	}) ]);
	return __clone(self);
}
function $q(self, content) {
	place(content, self);
	return __clone(self);
}
function $v(self, name2, value) {
	apply2(value, self, name2);
	return __clone(self);
}
function $w(self, content) {
	place2(content, self);
	return __clone(self);
}
function $x(self, content) {
	place3(content, self);
	return __clone(self);
}
function $y(self, content) {
	place4(content, self);
	return __clone(self);
}
const title = $a("Tasks <live>");
const todos = $b([ "alpha", "beta & gamma" ]);
const page = $c([ 1 ]);
console.log(render(app(title, todos, page)));
console.log(render(text(view("p"), "<script>alert(\"&\")</script>")));
console.log(render($m($m(view("img"), "src", "/logo.png"), "alt", "a & b")));
console.log(render($q($m(view("svg"), "viewBox", "0 0 24 24"), $m(view("path"), "d", "M5 12h14"))));
const name = $a("world & <you>");
const mixed = $x($w($q($w($v(view("p"), "data-live", $a("a \"quoted\" & value")), "Take "), text(view("code"), "vilan upgrade")), " & enjoy. "), name);
console.log(render(mixed));
const pair = [ text(view("i"), "a"), text(view("b"), "b") ];
console.log(render($y(view("p"), pair)));
console.log(render(text($w(view("p"), "gone"), "kept")));
