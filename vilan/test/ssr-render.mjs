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
	const $C = tag;
	let $D = null;
	if ($C === "area") {
		$D = true;
	} else if ($C === "base") {
		$D = true;
	} else if ($C === "br") {
		$D = true;
	} else if ($C === "col") {
		$D = true;
	} else if ($C === "embed") {
		$D = true;
	} else if ($C === "hr") {
		$D = true;
	} else if ($C === "img") {
		$D = true;
	} else if ($C === "input") {
		$D = true;
	} else if ($C === "link") {
		$D = true;
	} else if ($C === "meta") {
		$D = true;
	} else if ($C === "source") {
		$D = true;
	} else if ($C === "track") {
		$D = true;
	} else if ($C === "wbr") {
		$D = true;
	} else {
		$D = false;
	}
	return $D;
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
		const $E = child;
		let $F = null;
		if ($E[0] === 0) {
			const element = $E[1];
			out = out + render(element);
			$F = undefined;
		} else {
			const content = $E[1];
			out = out + escape_text(content);
			$F = undefined;
		}
		$F;
	}
	return out + "</" + view2[0] + ">";
}
function app(title2, todos2, page2) {
	const heading = $g(view("h1"), title2);
	const list = $k(view("ul"), $j(todos2, (todo) => {
		return todo;
	}, (todo, $i) => {
		return text(view("li"), todo);
	}));
	const nav = $w(view("nav"), $v(page2, (current, $r) => {
		const $s = current;
		let $t = null;
		if ($s[0] === 0) {
			$t = text($u(view("a"), "href", "/"), "Home");
		} else {
			$t = text($u(view("a"), "href", "/about"), "About & friends");
		}
		return $t;
	}));
	return $B($B($B($u(view("main"), "id", "app"), heading), list), nav);
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
	return __clone(self[0].v);
}
function $g(self, source) {
	self[3].v = $h(source);
	self[2].v = [  ];
	return __clone(self);
}
function $j(source, key, render2) {
	return [ __clone(source), key, render2 ];
}
function $p(self, content) {
	place(content, self[0]);
	return [  ];
}
function $q(owner, body) {
	return body(owner);
}
function $m(parent, source, key, render2) {
	const region = open(parent);
	const items = $h(source);
	for (const item of items) {
		const owner = new2();
		$q(owner, ($o) => {
			return $p(region, render2(item, $o));
		});
	}
}
function $l(self, parent) {
	$m(parent, self[0], self[1], self[2]);
}
function $k(self, content) {
	$l(content, self);
	return __clone(self);
}
function $u(self, name2, value) {
	apply(value, self, name2);
	return __clone(self);
}
function $v(source, render2) {
	return [ __clone(source), render2 ];
}
function $y(parent, source, render2) {
	const region = open(parent);
	const value = $h(source);
	const owner = new2();
	$q(owner, ($A) => {
		return $p(region, render2(value, $A));
	});
}
function $x(self, parent) {
	$y(parent, self[0], self[1]);
}
function $w(self, content) {
	$x(content, self);
	return __clone(self);
}
function $B(self, content) {
	place(content, self);
	return __clone(self);
}
function $H(self, parent, name2) {
	set_attribute(parent[1], name2, $h(self));
}
function $G(self, name2, value) {
	$H(value, self, name2);
	return __clone(self);
}
function $I(self, content) {
	place2(content, self);
	return __clone(self);
}
function $K(self, parent) {
	parent[2].v.push([ 1, $h(self) ]);
}
function $J(self, content) {
	$K(content, self);
	return __clone(self);
}
function $L(self, content) {
	place3(content, self);
	return __clone(self);
}
const title = $a("Tasks <live>");
const todos = $c([ "alpha", "beta & gamma" ]);
const page = $c([ 1 ]);
console.log(render(app(title, todos, page)));
console.log(render(text(view("p"), "<script>alert(\"&\")</script>")));
console.log(render($u($u(view("img"), "src", "/logo.png"), "alt", "a & b")));
console.log(render($B($u(view("svg"), "viewBox", "0 0 24 24"), $u(view("path"), "d", "M5 12h14"))));
const name = $a("world & <you>");
const mixed = $J($I($B($I($G(view("p"), "data-live", $a("a \"quoted\" & value")), "Take "), text(view("code"), "vilan upgrade")), " & enjoy. "), name);
console.log(render(mixed));
const pair = [ text(view("i"), "a"), text(view("b"), "b") ];
console.log(render($L(view("p"), pair)));
console.log(render(text($I(view("p"), "gone"), "kept")));
