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
	return [ tag, __shared_new([  ]), __shared_new([  ]), __shared_new("") ];
}
function set_attribute(attributes, name, value) {
	let updated = [  ];
	let found = false;
	for (const attribute of attributes.v) {
		if (attribute[0] === name) {
			updated.push([ name, value ]);
			found = true;
		} else {
			updated.push(attribute);
		}
	}
	if (!(found)) {
		updated.push([ name, value ]);
	}
	attributes.v = updated;
}
function text(self, content) {
	self[3].v = content;
	self[2].v = [  ];
	return self;
}
function attr(self, name, value) {
	set_attribute(self[1], name, value);
	return self;
}
function child(self, child2) {
	self[2].v.push(child2);
	return self;
}
function bind_text(self, source) {
	self[3].v = $d(source);
	self[2].v = [  ];
	return self;
}
function is_void_element(tag) {
	const $p = tag;
	let $q = null;
	if ($p === "area") {
		$q = true;
	} else if ($p === "base") {
		$q = true;
	} else if ($p === "br") {
		$q = true;
	} else if ($p === "col") {
		$q = true;
	} else if ($p === "embed") {
		$q = true;
	} else if ($p === "hr") {
		$q = true;
	} else if ($p === "img") {
		$q = true;
	} else if ($p === "input") {
		$q = true;
	} else if ($p === "link") {
		$q = true;
	} else if ($p === "meta") {
		$q = true;
	} else if ($p === "source") {
		$q = true;
	} else if ($p === "track") {
		$q = true;
	} else if ($p === "wbr") {
		$q = true;
	} else {
		$q = false;
	}
	return $q;
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
	for (const child2 of view2[2].v) {
		out = out + render(child2);
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
	const nav = $m(view("nav"), page2, (current, $j) => {
		const $k = current;
		let $l = null;
		if ($k[0] === 0) {
			$l = text(attr(view("a"), "href", "/"), "Home");
		} else {
			$l = text(attr(view("a"), "href", "/about"), "About & friends");
		}
		return $l;
	});
	return child(child(child(attr(view("main"), "id", "app"), heading), list), nav);
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
		self[2].v.push($i(owner, ($h) => {
			return build(item, $h);
		}));
	}
	return self;
}
function $n(self) {
	return self[0].v;
}
function $m(self, source, build) {
	const value = $n(source);
	const owner = new2();
	self[2].v.push($i(owner, ($o) => {
		return build(value, $o);
	}));
	return self;
}
const title = $a("Tasks <live>");
const todos = $b([ "alpha", "beta & gamma" ]);
const page = $c([ 1 ]);
console.log(render(app(title, todos, page)));
console.log(render(text(view("p"), "<script>alert(\"&\")</script>")));
console.log(render(attr(attr(view("img"), "src", "/logo.png"), "alt", "a & b")));
