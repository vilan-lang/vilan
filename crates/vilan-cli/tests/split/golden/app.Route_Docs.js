const __vilan_chunks = globalThis.__vilan_chunks;
const $ae = __vilan_chunks.fn.$ae;
const $as = __vilan_chunks.fn.$as;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $bc, $bd) {
	return $as($as(view("article"), panel("Docs", "page " + page, $bc, $bd), $bc, $bd), docs_nav(page, $bc, $bd), $bc, $bd);
}
function docs_nav(page, $be, $bf) {
	return $as(view("nav"), $ae("Next", [ 1, page + 1 ], $be, $bf), $be, $bf);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
