const __vilan_chunks = globalThis.__vilan_chunks;
const $aI = __vilan_chunks.fn.$aI;
const $aW = __vilan_chunks.fn.$aW;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $bQ, $bR) {
	return $aW($aW(view("article"), panel("Docs", "page " + page, $bQ, $bR), $bQ, $bR), docs_nav(page, $bQ, $bR), $bQ, $bR);
}
function docs_nav(page, $bS, $bT) {
	return $aW(view("nav"), $aI("Next", [ 1, page + 1 ], $bS, $bT), $bS, $bT);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
