const __vilan_chunks = globalThis.__vilan_chunks;
const $Y = __vilan_chunks.fn.$Y;
const $aj = __vilan_chunks.fn.$aj;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $aN, $aO) {
	return $aj($aj(view("article"), panel("Docs", "page " + page, $aN, $aO), $aN, $aO), docs_nav(page, $aN, $aO), $aN, $aO);
}
function docs_nav(page, $aP, $aQ) {
	return $aj(view("nav"), $Y("Next", [ 1, page + 1 ], $aP, $aQ), $aP, $aQ);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
