const __vilan_chunks = globalThis.__vilan_chunks;
const $bA = __vilan_chunks.fn.$bA;
const $bQ = __vilan_chunks.fn.$bQ;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $cP, $cQ) {
	return $bQ($bQ(view("article"), panel("Docs", "page " + page, $cP, $cQ), $cP, $cQ), docs_nav(page, $cP, $cQ), $cP, $cQ);
}
function docs_nav(page, $cR, $cS) {
	return $bQ(view("nav"), $bA("Next", [ 1, page + 1 ], $cR, $cS), $cR, $cS);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
