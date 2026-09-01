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
function place(self, parent) {
	parent[2].v.push([ 0, __clone(self) ]);
}
function place2(self, parent) {
	parent[2].v.push([ 1, self ]);
}
function place3(self, parent) {
	parent[2].v.push([ 1, $e(self) ]);
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
	set_attribute(parent[1], name2, $e(self));
}
function is_void_element(tag) {
	const $s = tag;
	let $t = null;
	if ($s === "area") {
		$t = true;
	} else if ($s === "base") {
		$t = true;
	} else if ($s === "br") {
		$t = true;
	} else if ($s === "col") {
		$t = true;
	} else if ($s === "embed") {
		$t = true;
	} else if ($s === "hr") {
		$t = true;
	} else if ($s === "img") {
		$t = true;
	} else if ($s === "input") {
		$t = true;
	} else if ($s === "link") {
		$t = true;
	} else if ($s === "meta") {
		$t = true;
	} else if ($s === "source") {
		$t = true;
	} else if ($s === "track") {
		$t = true;
	} else if ($s === "wbr") {
		$t = true;
	} else {
		$t = false;
	}
	return $t;
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
		const $u = child;
		let $v = null;
		if ($u[0] === 0) {
			const element = $u[1];
			out = out + render(element);
			$v = undefined;
		} else {
			const content = $u[1];
			out = out + escape_text(content);
			$v = undefined;
		}
		$v;
	}
	return out + "</" + view2[0] + ">";
}
function app(title2, todos2, page2) {
	const heading = $d(view("h1"), title2);
	const list = $g(view("ul"), todos2, (todo) => {
		return todo;
	}, (todo, $f) => {
		return text(view("li"), todo);
	});
	const nav = $o(view("nav"), page2, (current, $k) => {
		const $l = current;
		let $m = null;
		if ($l[0] === 0) {
			$m = text($n(view("a"), "href", "/"), "Home");
		} else {
			$m = text($n(view("a"), "href", "/about"), "About & friends");
		}
		return $m;
	});
	return $r($r($r($n(view("main"), "id", "app"), heading), list), nav);
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $e(self) {
	return self[0].v;
}
function $d(self, source) {
	self[3].v = $e(source);
	self[2].v = [  ];
	return __clone(self);
}
function $j(owner, body) {
	return body(owner);
}
function $g(self, source, key, build) {
	const items = $e(source);
	for (const item of items) {
		const owner = new2();
		self[2].v.push([ 0, $j(owner, ($i) => {
			return build(item, $i);
		}) ]);
	}
	return __clone(self);
}
function $n(self, name2, value) {
	apply(value, self, name2);
	return __clone(self);
}
function $o(self, source, build) {
	const value = $e(source);
	const owner = new2();
	self[2].v.push([ 0, $j(owner, ($q) => {
		return build(value, $q);
	}) ]);
	return __clone(self);
}
function $r(self, content) {
	place(content, self);
	return __clone(self);
}
function $w(self, name2, value) {
	apply2(value, self, name2);
	return __clone(self);
}
function $x(self, content) {
	place2(content, self);
	return __clone(self);
}
function $y(self, content) {
	place3(content, self);
	return __clone(self);
}
function $z(self, content) {
	place4(content, self);
	return __clone(self);
}
const title = $a("Tasks <live>");
const todos = $a([ "alpha", "beta & gamma" ]);
const page = $a([ 1 ]);
console.log(render(app(title, todos, page)));
console.log(render(text(view("p"), "<script>alert(\"&\")</script>")));
console.log(render($n($n(view("img"), "src", "/logo.png"), "alt", "a & b")));
console.log(render($r($n(view("svg"), "viewBox", "0 0 24 24"), $n(view("path"), "d", "M5 12h14"))));
const name = $a("world & <you>");
const mixed = $y($x($r($x($w(view("p"), "data-live", $a("a \"quoted\" & value")), "Take "), text(view("code"), "vilan upgrade")), " & enjoy. "), name);
console.log(render(mixed));
const pair = [ text(view("i"), "a"), text(view("b"), "b") ];
console.log(render($z(view("p"), pair)));
console.log(render(text($x(view("p"), "gone"), "kept")));
