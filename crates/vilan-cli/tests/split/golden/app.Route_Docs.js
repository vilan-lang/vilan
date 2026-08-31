const __vilan_chunks = globalThis.__vilan_chunks;
const $X = __vilan_chunks.fn.$X;
const $ai = __vilan_chunks.fn.$ai;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $aK, $aL) {
	return $ai($ai(view("article"), panel("Docs", "page " + page, $aK, $aL), $aK, $aL), docs_nav(page, $aK, $aL), $aK, $aL);
}
function docs_nav(page, $aM, $aN) {
	return $ai(view("nav"), $X("Next", [ 1, page + 1 ], $aM, $aN), $aM, $aN);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
