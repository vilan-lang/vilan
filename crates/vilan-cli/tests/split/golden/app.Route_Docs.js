const __vilan_chunks = globalThis.__vilan_chunks;
const $J = __vilan_chunks.fn.$J;
const $U = __vilan_chunks.fn.$U;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $at, $au) {
	return $U($U(view("article"), panel("Docs", "page " + page, $at, $au), $at, $au), docs_nav(page, $at, $au), $at, $au);
}
function docs_nav(page, $av, $aw) {
	return $U(view("nav"), $J("Next", [ 1, page + 1 ], $av, $aw), $av, $aw);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
