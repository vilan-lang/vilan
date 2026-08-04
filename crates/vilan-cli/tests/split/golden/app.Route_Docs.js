const __vilan_chunks = globalThis.__vilan_chunks;
const $K = __vilan_chunks.fn.$K;
const $V = __vilan_chunks.fn.$V;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $az, $aA) {
	return $V($V(view("article"), panel("Docs", "page " + page, $az, $aA), $az, $aA), docs_nav(page, $az, $aA), $az, $aA);
}
function docs_nav(page, $aB, $aC) {
	return $V(view("nav"), $K("Next", [ 1, page + 1 ], $aB, $aC), $aB, $aC);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
