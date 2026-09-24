const __vilan_chunks = globalThis.__vilan_chunks;
const $ag = __vilan_chunks.fn.$ag;
const $au = __vilan_chunks.fn.$au;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $bg, $bh) {
	return $au($au(view("article"), panel("Docs", "page " + page, $bg, $bh), $bg, $bh), docs_nav(page, $bg, $bh), $bg, $bh);
}
function docs_nav(page, $bi, $bj) {
	return $au(view("nav"), $ag("Next", [ 1, page + 1 ], $bi, $bj), $bi, $bj);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
