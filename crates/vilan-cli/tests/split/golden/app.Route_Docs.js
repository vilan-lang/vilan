const __vilan_chunks = globalThis.__vilan_chunks;
const $bI = __vilan_chunks.fn.$bI;
const $bu = __vilan_chunks.fn.$bu;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $cG, $cH) {
	return $bI($bI(view("article"), panel("Docs", "page " + page, $cG, $cH), $cG, $cH), docs_nav(page, $cG, $cH), $cG, $cH);
}
function docs_nav(page, $cI, $cJ) {
	return $bI(view("nav"), $bu("Next", [ 1, page + 1 ], $cI, $cJ), $cI, $cJ);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
