const __vilan_chunks = globalThis.__vilan_chunks;
const $X = __vilan_chunks.fn.$X;
const $ai = __vilan_chunks.fn.$ai;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $aJ, $aK) {
	return $ai($ai(view("article"), panel("Docs", "page " + page, $aJ, $aK), $aJ, $aK), docs_nav(page, $aJ, $aK), $aJ, $aK);
}
function docs_nav(page, $aL, $aM) {
	return $ai(view("nav"), $X("Next", [ 1, page + 1 ], $aL, $aM), $aL, $aM);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
