const __vilan_chunks = globalThis.__vilan_chunks;
const child = __vilan_chunks.fn.child;
const link = __vilan_chunks.fn.link;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $bX, $bY) {
	return child(child(view("article"), panel("Docs", "page " + page, $bX, $bY), $bX, $bY, "app.vl:86:18"), docs_nav(page, $bX, $bY), $bX, $bY, "app.vl:86:55");
}
function docs_nav(page, $bZ, $ca) {
	return child(view("nav"), link("Next", [ 1, page + 1 ], $bZ, $ca), $bZ, $ca, "app.vl:91:14");
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
//# sourceMappingURL=app.Route_Docs.js.map
