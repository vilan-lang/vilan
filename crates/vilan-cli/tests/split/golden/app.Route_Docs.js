const __vilan_chunks = globalThis.__vilan_chunks;
const $ai = __vilan_chunks.fn.$ai;
const $aw = __vilan_chunks.fn.$aw;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $bp, $bq) {
	return $aw($aw(view("article"), panel("Docs", "page " + page, $bp, $bq), $bp, $bq), docs_nav(page, $bp, $bq), $bp, $bq);
}
function docs_nav(page, $br, $bs) {
	return $aw(view("nav"), $ai("Next", [ 1, page + 1 ], $br, $bs), $br, $bs);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
