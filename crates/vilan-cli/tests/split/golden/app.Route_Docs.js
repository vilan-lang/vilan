const __vilan_chunks = globalThis.__vilan_chunks;
const $af = __vilan_chunks.fn.$af;
const $at = __vilan_chunks.fn.$at;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $bd, $be) {
	return $at($at(view("article"), panel("Docs", "page " + page, $bd, $be), $bd, $be), docs_nav(page, $bd, $be), $bd, $be);
}
function docs_nav(page, $bf, $bg) {
	return $at(view("nav"), $af("Next", [ 1, page + 1 ], $bf, $bg), $bf, $bg);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
