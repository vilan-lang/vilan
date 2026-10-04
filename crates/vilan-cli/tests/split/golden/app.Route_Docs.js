const __vilan_chunks = globalThis.__vilan_chunks;
const $bA = __vilan_chunks.fn.$bA;
const $bP = __vilan_chunks.fn.$bP;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $cO, $cP) {
	return $bP($bP(view("article"), panel("Docs", "page " + page, $cO, $cP), $cO, $cP), docs_nav(page, $cO, $cP), $cO, $cP);
}
function docs_nav(page, $cQ, $cR) {
	return $bP(view("nav"), $bA("Next", [ 1, page + 1 ], $cQ, $cR), $cQ, $cR);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
