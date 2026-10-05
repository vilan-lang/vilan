const __vilan_chunks = globalThis.__vilan_chunks;
const child = __vilan_chunks.fn.child;
const link = __vilan_chunks.fn.link;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $bK, $bL) {
	return child(child(view("article"), panel("Docs", "page " + page, $bK, $bL), $bK, $bL), docs_nav(page, $bK, $bL), $bK, $bL);
}
function docs_nav(page, $bM, $bN) {
	return child(view("nav"), link("Next", [ 1, page + 1 ], $bM, $bN), $bM, $bN);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
