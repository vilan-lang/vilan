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
function place(self, parent) {
	parent[2].v.push([ 0, self ]);
}
function place2(self, parent) {
	parent[2].v.push([ 1, self ]);
}
function apply(self, parent, name2) {
	set_attribute(parent[1], name2, self);
}
function is_void_element(tag) {
	const $q = tag;
	let $r = null;
	if ($q === "area") {
		$r = true;
	} else if ($q === "base") {
		$r = true;
	} else if ($q === "br") {
		$r = true;
	} else if ($q === "col") {
		$r = true;
	} else if ($q === "embed") {
		$r = true;
	} else if ($q === "hr") {
		$r = true;
	} else if ($q === "img") {
		$r = true;
	} else if ($q === "input") {
		$r = true;
	} else if ($q === "link") {
		$r = true;
	} else if ($q === "meta") {
		$r = true;
	} else if ($q === "source") {
		$r = true;
	} else if ($q === "track") {
		$r = true;
	} else if ($q === "wbr") {
		$r = true;
	} else {
		$r = false;
	}
	return $r;
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
		const $s = child;
		let $t = null;
		if ($s[0] === 0) {
			const element = $s[1];
			out = out + render(element);
			$t = undefined;
		} else {
			const content = $s[1];
			out = out + escape_text(content);
			$t = undefined;
		}
		$t;
	}
	return out + "</" + view2[0] + ">";
}
function row(label) {
	return $l($k(view("li"), "class", "item"), label);
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(__clone(value)), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $h(self) {
	return __clone(self[0].v);
}
function $j(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $i(self, subscriber) {
	return $j(self, subscriber);
}
function $g(self) {
	return [ () => {
		return $h(self);
	}, (subscriber) => {
		return $i(self, subscriber);
	}, () => {
		return;
	} ];
}
function $f(flow) {
	const instance = $g(flow);
	const value = instance[0]();
	instance[2]();
	return value;
}
function $e(flow, parent, name2) {
	set_attribute(parent[1], name2, $f(flow));
}
function $d(self, parent, name2) {
	$e(self, parent, name2);
}
function $c(self, name2, value) {
	$d(value, self, name2);
	return __clone(self);
}
function $k(self, name2, value) {
	apply(value, self, name2);
	return __clone(self);
}
function $l(self, content) {
	place2(content, self);
	return __clone(self);
}
function $m(self, content) {
	place(content, self);
	return __clone(self);
}
function $p(flow, parent) {
	parent[2].v.push([ 1, $f(flow) ]);
}
function $o(self, parent) {
	$p(self, parent);
}
function $n(self, content) {
	$o(content, self);
	return __clone(self);
}
const name = $a("world & <you>");
console.log(render($n($l($m($l($k($c(view("p"), "data-live", __clone(name)), "title", "hi"), "Take "), $l(view("code"), "vilan upgrade")), " & enjoy. "), name)));
console.log(render($k($k($k(view("input"), "type", "checkbox"), "aria-label", "Done"), "disabled", "")));
console.log(render($m($m(view("ul"), row("alpha")), row("beta"))));
console.log(render($m($k(view("svg"), "viewBox", "0 0 24 24"), $k(view("path"), "d", "M5 12h14"))));
console.log(render($m(view("div"), $l(view("span"), "chained"))));
