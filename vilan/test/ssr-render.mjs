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
	for (const item of self) {
		parent[2].v.push([ 0, __clone(item) ]);
	}
}
function apply(self, parent, name2) {
	set_attribute(parent[1], name2, self);
}
function is_void_element(tag) {
	const $v = tag;
	let $w = null;
	if ($v === "area") {
		$w = true;
	} else if ($v === "base") {
		$w = true;
	} else if ($v === "br") {
		$w = true;
	} else if ($v === "col") {
		$w = true;
	} else if ($v === "embed") {
		$w = true;
	} else if ($v === "hr") {
		$w = true;
	} else if ($v === "img") {
		$w = true;
	} else if ($v === "input") {
		$w = true;
	} else if ($v === "link") {
		$w = true;
	} else if ($v === "meta") {
		$w = true;
	} else if ($v === "source") {
		$w = true;
	} else if ($v === "track") {
		$w = true;
	} else if ($v === "wbr") {
		$w = true;
	} else {
		$w = false;
	}
	return $w;
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
		const $x = child;
		let $y = null;
		if ($x[0] === 0) {
			const element = $x[1];
			out = out + render(element);
			$y = undefined;
		} else {
			const content = $x[1];
			out = out + escape_text(content);
			$y = undefined;
		}
		$y;
	}
	return out + "</" + view2[0] + ">";
}
function app(title2, todos2, page2) {
	const heading = $g(view("h1"), title2);
	const list = $j(view("ul"), todos2, (todo) => {
		return todo;
	}, (todo, $i) => {
		return text(view("li"), todo);
	});
	const nav = $r(view("nav"), page2, (current, $n) => {
		const $o = current;
		let $p = null;
		if ($o[0] === 0) {
			$p = text($q(view("a"), "href", "/"), "Home");
		} else {
			$p = text($q(view("a"), "href", "/about"), "About & friends");
		}
		return $p;
	});
	return $u($u($u($q(view("main"), "id", "app"), heading), list), nav);
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $c(value) {
	return $b(value);
}
function $h(self) {
	return self[0].v;
}
function $g(self, source) {
	self[3].v = $h(source);
	self[2].v = [  ];
	return __clone(self);
}
function $m(owner, body) {
	return body(owner);
}
function $j(self, source, key, build) {
	const items = $h(source);
	for (const item of items) {
		const owner = new2();
		self[2].v.push([ 0, $m(owner, ($l) => {
			return build(item, $l);
		}) ]);
	}
	return __clone(self);
}
function $q(self, name2, value) {
	apply(value, self, name2);
	return __clone(self);
}
function $r(self, source, build) {
	const value = $h(source);
	const owner = new2();
	self[2].v.push([ 0, $m(owner, ($t) => {
		return build(value, $t);
	}) ]);
	return __clone(self);
}
function $u(self, content) {
	place(content, self);
	return __clone(self);
}
function $A(self, parent, name2) {
	set_attribute(parent[1], name2, $h(self));
}
function $z(self, name2, value) {
	$A(value, self, name2);
	return __clone(self);
}
function $B(self, content) {
	place2(content, self);
	return __clone(self);
}
function $D(self, parent) {
	parent[2].v.push([ 1, $h(self) ]);
}
function $C(self, content) {
	$D(content, self);
	return __clone(self);
}
function $E(self, content) {
	place3(content, self);
	return __clone(self);
}
const title = $a("Tasks <live>");
const todos = $c([ "alpha", "beta & gamma" ]);
const page = $c([ 1 ]);
console.log(render(app(title, todos, page)));
console.log(render(text(view("p"), "<script>alert(\"&\")</script>")));
console.log(render($q($q(view("img"), "src", "/logo.png"), "alt", "a & b")));
console.log(render($u($q(view("svg"), "viewBox", "0 0 24 24"), $q(view("path"), "d", "M5 12h14"))));
const name = $a("world & <you>");
const mixed = $C($B($u($B($z(view("p"), "data-live", $a("a \"quoted\" & value")), "Take "), text(view("code"), "vilan upgrade")), " & enjoy. "), name);
console.log(render(mixed));
const pair = [ text(view("i"), "a"), text(view("b"), "b") ];
console.log(render($E(view("p"), pair)));
console.log(render(text($B(view("p"), "gone"), "kept")));
