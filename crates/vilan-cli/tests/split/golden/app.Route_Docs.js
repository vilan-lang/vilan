const __vilan_chunks = globalThis.__vilan_chunks;
const $Y = __vilan_chunks.fn.$Y;
const $am = __vilan_chunks.fn.$am;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $aS, $aT) {
	return $am($am(view("article"), panel("Docs", "page " + page, $aS, $aT), $aS, $aT), docs_nav(page, $aS, $aT), $aS, $aT);
}
function docs_nav(page, $aU, $aV) {
	return $am(view("nav"), $Y("Next", [ 1, page + 1 ], $aU, $aV), $aU, $aV);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
