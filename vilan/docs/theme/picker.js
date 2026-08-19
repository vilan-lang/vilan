// The theme picker, reduced to the two vilan themes (docs-port.md §3.1 S3).
//
// mdBook's picker lists five stock themes, hardcoded in its `index.hbs`
// (Auto / Light / Rust / Coal / Navy / Ayu); the book ships two — `light`
// and `navy`, reskinned onto the vilan role tokens in `css/variables.css`.
// `css/vilan.css` hides the other three entries so nothing flashes, and
// this file REMOVES them so book.js's arrow-key navigation, which walks the
// list's <li> siblings, never lands on a hidden item. It also names the
// dark theme for what it is: mdBook's class stays `navy` (book.toml's
// `preferred-dark-theme`, the `html.navy` selector the tokens hang on), the
// label the reader sees says Dark.
//
// Not a fork of `index.hbs` (docs-port.md §4 Q5), and not `vilan.js` (the
// highlighter and the playground links, another lane's file): a small
// `additional-js` file of its own, loaded after book.js like vilan.js is.
//
// A preference left in localStorage from before the reskin (rust / coal /
// ayu) is migrated through mdBook's own click path — the hidden entries are
// aliased onto the two vilan themes in variables.css, so the page already
// looks right; this just makes the stored value name a theme that exists,
// before the entry it would point at is removed.
(function () {
	var alias = { rust: "light", coal: "navy", ayu: "navy" };
	var saved = null;
	try {
		saved = localStorage.getItem("mdbook-theme");
	} catch (e) {}
	if (saved && alias[saved]) {
		var target = document.getElementById("mdbook-theme-" + alias[saved]);
		if (target) {
			target.click();
		}
	}
	Object.keys(alias).forEach(function (name) {
		var entry = document.getElementById("mdbook-theme-" + name);
		if (entry && entry.parentElement && entry.parentElement.tagName === "LI") {
			entry.parentElement.remove();
		}
	});
	var dark = document.getElementById("mdbook-theme-navy");
	if (dark) {
		dark.textContent = "Dark";
	}
})();
