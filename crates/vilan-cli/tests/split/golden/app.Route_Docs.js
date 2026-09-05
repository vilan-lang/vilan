const __vilan_chunks = globalThis.__vilan_chunks;
const $Y = __vilan_chunks.fn.$Y;
const $aj = __vilan_chunks.fn.$aj;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $aP, $aQ) {
	return $aj($aj(view("article"), panel("Docs", "page " + page, $aP, $aQ), $aP, $aQ), docs_nav(page, $aP, $aQ), $aP, $aQ);
}
function docs_nav(page, $aR, $aS) {
	return $aj(view("nav"), $Y("Next", [ 1, page + 1 ], $aR, $aS), $aR, $aS);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
