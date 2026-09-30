const __vilan_chunks = globalThis.__vilan_chunks;
const $ak = __vilan_chunks.fn.$ak;
const $ay = __vilan_chunks.fn.$ay;
const panel = __vilan_chunks.fn.panel;
const view = __vilan_chunks.fn.view;
function docs_page(page, $bl, $bm) {
	return $ay($ay(view("article"), panel("Docs", "page " + page, $bl, $bm), $bl, $bm), docs_nav(page, $bl, $bm), $bl, $bm);
}
function docs_nav(page, $bn, $bo) {
	return $ay(view("nav"), $ak("Next", [ 1, page + 1 ], $bn, $bo), $bn, $bo);
}
__vilan_chunks.fn.docs_nav = docs_nav;
__vilan_chunks.fn.docs_page = docs_page;
