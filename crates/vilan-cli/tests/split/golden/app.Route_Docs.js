const __vilan_chunks = globalThis.__vilan_chunks;
const $bG = __vilan_chunks.fn.$bG;
const $bs = __vilan_chunks.fn.$bs;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $cE, $cF) {
	return $bG($bG(view("article"), panel("Docs", "page " + page, $cE, $cF), $cE, $cF), docs_nav(page, $cE, $cF), $cE, $cF);
}
function docs_nav(page, $cG, $cH) {
	return $bG(view("nav"), $bs("Next", [ 1, page + 1 ], $cG, $cH), $cG, $cH);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
