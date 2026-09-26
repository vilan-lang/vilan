const __vilan_chunks = globalThis.__vilan_chunks;
const $ai = __vilan_chunks.fn.$ai;
const $aw = __vilan_chunks.fn.$aw;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $bi, $bj) {
	return $aw($aw(view("article"), panel("Docs", "page " + page, $bi, $bj), $bi, $bj), docs_nav(page, $bi, $bj), $bi, $bj);
}
function docs_nav(page, $bk, $bl) {
	return $aw(view("nav"), $ai("Next", [ 1, page + 1 ], $bk, $bl), $bk, $bl);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
