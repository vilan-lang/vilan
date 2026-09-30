const __vilan_chunks = globalThis.__vilan_chunks;
const $bB = __vilan_chunks.fn.$bB;
const $bn = __vilan_chunks.fn.$bn;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $cz, $cA) {
	return $bB($bB(view("article"), panel("Docs", "page " + page, $cz, $cA), $cz, $cA), docs_nav(page, $cz, $cA), $cz, $cA);
}
function docs_nav(page, $cB, $cC) {
	return $bB(view("nav"), $bn("Next", [ 1, page + 1 ], $cB, $cC), $cB, $cC);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
