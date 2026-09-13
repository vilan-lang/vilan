function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __shared_new(value) {
	return { v: value };
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(false) ];
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
	const $D = tag;
	let $E = null;
	if ($D === "area") {
		$E = true;
	} else if ($D === "base") {
		$E = true;
	} else if ($D === "br") {
		$E = true;
	} else if ($D === "col") {
		$E = true;
	} else if ($D === "embed") {
		$E = true;
	} else if ($D === "hr") {
		$E = true;
	} else if ($D === "img") {
		$E = true;
	} else if ($D === "input") {
		$E = true;
	} else if ($D === "link") {
		$E = true;
	} else if ($D === "meta") {
		$E = true;
	} else if ($D === "source") {
		$E = true;
	} else if ($D === "track") {
		$E = true;
	} else if ($D === "wbr") {
		$E = true;
	} else {
		$E = false;
	}
	return $E;
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
		const $F = child;
		let $G = null;
		if ($F[0] === 0) {
			const element = $F[1];
			out = out + render(element);
			$G = undefined;
		} else {
			const content = $F[1];
			out = out + escape_text(content);
			$G = undefined;
		}
		$G;
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
	const nav = $w(view("nav"), page2, (current, $s) => {
		const $t = current;
		let $u = null;
		if ($t[0] === 0) {
			$u = text($v(view("a"), "href", "/"), "Home");
		} else {
			$u = text($v(view("a"), "href", "/about"), "About & friends");
		}
		return $u;
	});
	return $C($C($C($v(view("main"), "id", "app"), heading), list), nav);
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers), fresh_id() ];
}
function $a(value) {
	return $b(value);
}
function $c(value) {
	return $b(value);
}
function $h(self) {
	return __clone(self[0].v);
}
function $g(self, source) {
	self[3].v = $h(source);
	self[2].v = [  ];
	return __clone(self);
}
function $k(source, key, render2) {
	return [ __clone(source), key, render2 ];
}
function $q(self, content) {
	place(content, self[0]);
	return [  ];
}
function $r(owner, body) {
	return body(owner);
}
function $n(parent, source, key, render2) {
	const region = open(parent);
	const items = $h(source);
	for (const item of items) {
		const owner = new2();
		$r(owner, ($p) => {
			return $q(region, render2(item, $p));
		});
	}
}
function $m(self, parent) {
	$n(parent, self[0], self[1], self[2]);
}
function $l(self, content) {
	$m(content, self);
	return __clone(self);
}
function $j(self, source, key, build) {
	return $l(self, $k(source, key, build));
}
function $v(self, name2, value) {
	apply(value, self, name2);
	return __clone(self);
}
function $z(parent, source, render2) {
	const region = open(parent);
	const value = $h(source);
	const owner = new2();
	$r(owner, ($B) => {
		return $q(region, render2(value, $B));
	});
}
function $y(self, parent) {
	$z(parent, self[0], self[1]);
}
function $x(self, content) {
	$y(content, self);
	return __clone(self);
}
function $w(self, source, build) {
	return $x(self, [ __clone(source), build ]);
}
function $C(self, content) {
	place(content, self);
	return __clone(self);
}
function $I(self, parent, name2) {
	set_attribute(parent[1], name2, $h(self));
}
function $H(self, name2, value) {
	$I(value, self, name2);
	return __clone(self);
}
function $J(self, content) {
	place2(content, self);
	return __clone(self);
}
function $L(self, parent) {
	parent[2].v.push([ 1, $h(self) ]);
}
function $K(self, content) {
	$L(content, self);
	return __clone(self);
}
function $M(self, content) {
	place3(content, self);
	return __clone(self);
}
const next_subscriber_id = __shared_new(0);
const title = $a("Tasks <live>");
const todos = $c([ "alpha", "beta & gamma" ]);
const page = $c([ 1 ]);
console.log(render(app(title, todos, page)));
console.log(render(text(view("p"), "<script>alert(\"&\")</script>")));
console.log(render($v($v(view("img"), "src", "/logo.png"), "alt", "a & b")));
console.log(render($C($v(view("svg"), "viewBox", "0 0 24 24"), $v(view("path"), "d", "M5 12h14"))));
const name = $a("world & <you>");
const mixed = $K($J($C($J($H(view("p"), "data-live", $a("a \"quoted\" & value")), "Take "), text(view("code"), "vilan upgrade")), " & enjoy. "), name);
console.log(render(mixed));
const pair = [ text(view("i"), "a"), text(view("b"), "b") ];
console.log(render($M(view("p"), pair)));
console.log(render(text($J(view("p"), "gone"), "kept")));
