const __vilan_chunks = globalThis.__vilan_chunks;
const $bA = __vilan_chunks.fn.$bA;
const $bO = __vilan_chunks.fn.$bO;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $cM, $cN) {
	return $bO($bO(view("article"), panel("Docs", "page " + page, $cM, $cN), $cM, $cN), docs_nav(page, $cM, $cN), $cM, $cN);
}
function docs_nav(page, $cO, $cP) {
	return $bO(view("nav"), $bA("Next", [ 1, page + 1 ], $cO, $cP), $cO, $cP);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
