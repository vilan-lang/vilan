const __vilan_chunks = globalThis.__vilan_chunks;
const panel = __vilan_chunks.fn.panel;
function not_found_page($ax, $ay) {
	return panel("Nothing here", "try /docs/1", $ax, $ay);
}
__vilan_chunks.fn.not_found_page = not_found_page;
