const __vilan_chunks = globalThis.__vilan_chunks;
const child = __vilan_chunks.fn.child;
const link = __vilan_chunks.fn.link;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $bO, $bP) {
	return child(child(view("article"), panel("Docs", "page " + page, $bO, $bP), $bO, $bP), docs_nav(page, $bO, $bP), $bO, $bP);
}
function docs_nav(page, $bQ, $bR) {
	return child(view("nav"), link("Next", [ 1, page + 1 ], $bQ, $bR), $bQ, $bR);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
//# sourceMappingURL=app.Route_Docs.js.map
