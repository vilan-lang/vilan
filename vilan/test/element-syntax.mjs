function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __shared_new(value) {
	return { v: value };
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
	parent[2].v.push([ 0, __clone(self) ]);
}
function place2(self, parent) {
	parent[2].v.push([ 1, self ]);
}
function apply(self, parent, name2) {
	set_attribute(parent[1], name2, self);
}
function is_void_element(tag) {
	const $k = tag;
	let $l = null;
	if ($k === "area") {
		$l = true;
	} else if ($k === "base") {
		$l = true;
	} else if ($k === "br") {
		$l = true;
	} else if ($k === "col") {
		$l = true;
	} else if ($k === "embed") {
		$l = true;
	} else if ($k === "hr") {
		$l = true;
	} else if ($k === "img") {
		$l = true;
	} else if ($k === "input") {
		$l = true;
	} else if ($k === "link") {
		$l = true;
	} else if ($k === "meta") {
		$l = true;
	} else if ($k === "source") {
		$l = true;
	} else if ($k === "track") {
		$l = true;
	} else if ($k === "wbr") {
		$l = true;
	} else {
		$l = false;
	}
	return $l;
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
		const $m = child;
		let $n = null;
		if ($m[0] === 0) {
			const element = $m[1];
			out = out + render(element);
			$n = undefined;
		} else {
			const content = $m[1];
			out = out + escape_text(content);
			$n = undefined;
		}
		$n;
	}
	return out + "</" + view2[0] + ">";
}
function row(label) {
	return $g($f(view("li"), "class", "item"), label);
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $e(self) {
	return self[0].v;
}
function $d(self, parent, name2) {
	set_attribute(parent[1], name2, $e(self));
}
function $c(self, name2, value) {
	$d(value, self, name2);
	return __clone(self);
}
function $f(self, name2, value) {
	apply(value, self, name2);
	return __clone(self);
}
function $g(self, content) {
	place2(content, self);
	return __clone(self);
}
function $h(self, content) {
	place(content, self);
	return __clone(self);
}
function $j(self, parent) {
	parent[2].v.push([ 1, $e(self) ]);
}
function $i(self, content) {
	$j(content, self);
	return __clone(self);
}
const name = $a("world & <you>");
console.log(render($i($g($h($g($f($c(view("p"), "data-live", name), "title", "hi"), "Take "), $g(view("code"), "vilan upgrade")), " & enjoy. "), name)));
console.log(render($f($f($f(view("input"), "type", "checkbox"), "disabled", ""), "aria-label", "Done")));
console.log(render($h($h(view("ul"), row("alpha")), row("beta"))));
console.log(render($h($f(view("svg"), "viewBox", "0 0 24 24"), $f(view("path"), "d", "M5 12h14"))));
console.log(render($h(view("div"), $g(view("span"), "chained"))));
