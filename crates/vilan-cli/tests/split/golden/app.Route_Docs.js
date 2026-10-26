const __vilan_chunks = globalThis.__vilan_chunks;
const child = __vilan_chunks.fn.child;
const link = __vilan_chunks.fn.link;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $bV, $bW) {
	return child(child(view("article"), panel("Docs", "page " + page, $bV, $bW), $bV, $bW), docs_nav(page, $bV, $bW), $bV, $bW);
}
function docs_nav(page, $bX, $bY) {
	return child(view("nav"), link("Next", [ 1, page + 1 ], $bX, $bY), $bX, $bY);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
//# sourceMappingURL=app.Route_Docs.js.map
