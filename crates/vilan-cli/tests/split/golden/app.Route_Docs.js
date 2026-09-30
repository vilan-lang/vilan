const __vilan_chunks = globalThis.__vilan_chunks;
const $aF = __vilan_chunks.fn.$aF;
const $aT = __vilan_chunks.fn.$aT;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $bN, $bO) {
	return $aT($aT(view("article"), panel("Docs", "page " + page, $bN, $bO), $bN, $bO), docs_nav(page, $bN, $bO), $bN, $bO);
}
function docs_nav(page, $bP, $bQ) {
	return $aT(view("nav"), $aF("Next", [ 1, page + 1 ], $bP, $bQ), $bP, $bQ);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
