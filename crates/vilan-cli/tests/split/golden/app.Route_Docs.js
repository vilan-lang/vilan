const __vilan_chunks = globalThis.__vilan_chunks;
const $ac = __vilan_chunks.fn.$ac;
const $aq = __vilan_chunks.fn.$aq;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $ba, $bb) {
	return $aq($aq(view("article"), panel("Docs", "page " + page, $ba, $bb), $ba, $bb), docs_nav(page, $ba, $bb), $ba, $bb);
}
function docs_nav(page, $bc, $bd) {
	return $aq(view("nav"), $ac("Next", [ 1, page + 1 ], $bc, $bd), $bc, $bd);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
