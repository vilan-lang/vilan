// vilan dev runtime (HMR) — prepended to browser-leg bundles by an HMR-active
// `vilan run --watch` (hmr.md §2/§3). Plain ES2020, no dependencies. The port,
// this run's dev-channel token, this build's version, and this leg's bundle
// name are template-substituted at write time. It installs a
// `window.__VILAN_HMR__` singleton (a re-evaluated
// bundle reuses it), defines the instrumentation globals the compiled bundle
// calls (`__hmr_adopt*`/`__hmr_expose`, hmr.md §5) plus the `std::dev` host
// globals (`__hmr_register_teardown`/`__hmr_stash`/`__hmr_take`), and reacts to
// the dev channel: live-reload, CSS hot-swap, an error overlay, and the
// state-preserving `swap` (hmr.md §3/§4).
(function () {
    // Singleton guard — a re-evaluated bundle (the swap's `import()`) reuses the
    // live instance and must not open a second EventSource or reset the registry.
    if (window.__VILAN_HMR__) {
        return;
    }
    var PORT = __VILAN_HMR_PORT__;
    var TOKEN = "__VILAN_HMR_TOKEN__";
    var VERSION = __VILAN_HMR_VERSION__;
    var BUNDLE = "__VILAN_HMR_BUNDLE__";

    // Every dev-channel route requires this run's token (backlog E93), so every
    // request this file makes goes through here — there is no second way to
    // spell a channel URL. We can hold it because this bundle came from the
    // page's own origin; a page that merely knows the port cannot read it, which
    // is what keeps our compile diagnostics and our bundle to ourselves and
    // makes `POST /refresh` unforgeable. The token is hex, so it needs no
    // escaping.
    function channelUrl(route) {
        return "http://127.0.0.1:" + PORT + route + "?token=" + TOKEN;
    }

    // Swap state (hmr.md §3/§4). Held in this closure AND on the singleton, so
    // the globals below and the swap protocol share one view; `seed` and
    // `exposed` are mutated in place (never reassigned) to keep both in sync.
    var exposed = {}; // key -> { fp, getter } — the live bindings to capture.
    var seed = {}; // key -> { fp, value } — last capture, consulted on adopt.
    var teardowns = []; // cleanups run once, before the next bundle evaluates.
    var userStash = {}; // "user:"-prefixed app carryover (std::dev stash/take).

    var singleton = {
        port: PORT,
        version: VERSION,
        exposed: exposed,
        seed: seed,
        teardowns: teardowns,
        userStash: userStash,
        take: function (key) {
            var slot = "user:" + key;
            // The `Option` runtime encoding: `[0, value]` = Some, `[1]` = None.
            return Object.prototype.hasOwnProperty.call(userStash, slot)
                ? [0, userStash[slot]]
                : [1];
        },
        swap: swap,
    };
    window.__VILAN_HMR__ = singleton;

    // A binding whose fingerprint changed reinitializes fresh (§10(b)); noted
    // once per adopt call — a module binding's initializer runs once per bundle
    // evaluation, so that is once per key per swap.
    function note(key) {
        if (typeof console !== "undefined" && console.info) {
            console.info("[vilan] hmr: `" + key + "` changed shape, reinitialized");
        }
    }

    // --- Instrumentation globals (hmr.md §5), called by the emitted bundle. ---
    // Assigned to `globalThis` so the bundle's module-scoped top level resolves
    // them as free names. `__hmr_active` is a per-bundle transformer helper, not
    // one of these — the std hooks that guard on it work with no shim too.
    globalThis.__hmr_adopt = function (key, fp, thunk) {
        var entry = seed[key];
        if (entry) {
            if (entry.fp === fp) {
                return entry.value;
            }
            note(key);
        }
        return thunk();
    };
    // A signal/shared binding always builds a FRESH cell (old subscribers must
    // die); on a fingerprint-matching seed hit its payload is written in — the
    // value carries, the identity does not. `Signal` is `[{v},{v:subs}]`
    // (payload at `[0].v`), `Shared` is `{v}` (payload at `.v`).
    globalThis.__hmr_adopt_signal = function (key, fp, thunk) {
        var cell = thunk();
        var entry = seed[key];
        if (entry) {
            if (entry.fp === fp) {
                cell[0].v = entry.value;
            } else {
                note(key);
            }
        }
        return cell;
    };
    globalThis.__hmr_adopt_shared = function (key, fp, thunk) {
        var cell = thunk();
        var entry = seed[key];
        if (entry) {
            if (entry.fp === fp) {
                cell.v = entry.value;
            } else {
                note(key);
            }
        }
        return cell;
    };
    globalThis.__hmr_expose = function (key, fp, getter) {
        exposed[key] = { fp: fp, getter: getter };
    };
    // std::dev host globals — only reached behind an `hmr_active()` guard.
    globalThis.__hmr_register_teardown = function (cleanup) {
        teardowns.push(cleanup);
    };
    globalThis.__hmr_stash = function (key, value) {
        userStash["user:" + key] = value;
    };
    globalThis.__hmr_take = function (key) {
        return singleton.take(key);
    };

    // --- The swap protocol (hmr.md §3). ---
    // Swaps are serialized on a promise chain: a `swap` that arrives while a
    // prior `import()` is still pending would otherwise capture from the
    // already-cleared registry (empty seed) and mount over an un-torn-down
    // page. Chaining makes the second capture see the first bundle's
    // re-registered state.
    var swapChain = Promise.resolve();
    function swap(bundleText) {
        swapChain = swapChain.then(function () {
            return performSwap(bundleText);
        });
        return swapChain;
    }

    function performSwap(bundleText) {
        // (1) Capture — snapshot every exposed binding into the seed (a throwing
        // getter skips its key: fresh init), plus scroll and focus.
        var captured = {};
        for (var key in exposed) {
            if (!Object.prototype.hasOwnProperty.call(exposed, key)) {
                continue;
            }
            try {
                captured[key] = { fp: exposed[key].fp, value: exposed[key].getter() };
            } catch (error) {
                // A throwing getter leaves its binding unseeded — fresh init.
            }
        }
        // Refill `seed` in place so the globals and singleton keep their view.
        for (var stale in seed) {
            if (Object.prototype.hasOwnProperty.call(seed, stale)) {
                delete seed[stale];
            }
        }
        Object.assign(seed, captured);
        var scroll = captureScroll();
        var focus = captureFocus();

        // (2) Teardown — run and clear the list (each isolated), then clear the
        // registry so the next bundle re-registers into an empty one.
        var pending = teardowns.slice();
        teardowns.length = 0;
        for (var index = 0; index < pending.length; index++) {
            try {
                pending[index]();
            } catch (error) {
                // Isolate: one bad teardown must not strand the rest.
            }
        }
        for (var live in exposed) {
            if (Object.prototype.hasOwnProperty.call(exposed, live)) {
                delete exposed[live];
            }
        }

        // (3) Evaluate — import the new bundle as a module (top-level `let` is
        // module-scoped, so old and new bindings never collide).
        var url;
        try {
            url = URL.createObjectURL(new Blob([bundleText], { type: "text/javascript" }));
        } catch (error) {
            reload();
            return Promise.resolve();
        }
        return import(url)
            .then(function () {
                try {
                    URL.revokeObjectURL(url);
                } catch (error) {
                    // A stub URL may not revoke — harmless.
                }
                // (4) Restore scroll/focus best-effort — skip what no longer fits.
                restoreScroll(scroll);
                restoreFocus(focus);
            })
            .catch(function (error) {
                // (5) Teardown already ran — don't limp; reload to a clean boot.
                reload();
            });
    }

    // Host-continuity capture/restore — every host API guarded with `typeof` so
    // the node DOM stub (which lacks most of them) survives.
    function captureScroll() {
        if (typeof window === "undefined") {
            return null;
        }
        return { x: window.scrollX || 0, y: window.scrollY || 0 };
    }
    function restoreScroll(scroll) {
        if (scroll && typeof window !== "undefined" && typeof window.scrollTo === "function") {
            window.scrollTo(scroll.x, scroll.y);
        }
    }
    function captureFocus() {
        if (typeof document === "undefined") {
            return null;
        }
        var active = document.activeElement;
        if (!active || !active.id) {
            return null;
        }
        var info = { id: active.id };
        if (typeof active.selectionStart === "number") {
            info.selectionStart = active.selectionStart;
            info.selectionEnd = active.selectionEnd;
        }
        return info;
    }
    function restoreFocus(focus) {
        if (!focus || typeof document === "undefined") {
            return;
        }
        var element = document.getElementById(focus.id);
        if (!element) {
            return;
        }
        if (typeof element.focus === "function") {
            element.focus();
        }
        if (
            typeof focus.selectionStart === "number" &&
            typeof element.setSelectionRange === "function"
        ) {
            try {
                element.setSelectionRange(focus.selectionStart, focus.selectionEnd);
            } catch (error) {
                // A non-text element rejects a selection range — ignore.
            }
        }
    }

    function reload() {
        if (typeof location !== "undefined" && typeof location.reload === "function") {
            location.reload();
        }
    }

    var OVERLAY_ID = "__vilan_hmr_overlay__";
    // A source line that names a location (`app.vl:12:5`) — styled as a distinct
    // accent line in the overlay, and counted for the header badge.
    var LOCATION_LINE = /:\d+:\d+(\s|$)/;
    // A requirement-trace line (E80): indented `via app.vl:3:5 — the context
    // requirement flows through this call`, or the chain's indented elision
    // tail (`… 2 more uncovered calls on this path`). A `via` line names a
    // location too, so this class is tested BEFORE `LOCATION_LINE` wherever
    // that regex runs: a diagnostic with three hops counts ONCE in the badge.
    var TRACE_LINE = /^\s+(via |…)/;

    function removeOverlay() {
        var existing = document.getElementById(OVERLAY_ID);
        if (existing) {
            existing.remove();
        }
    }

    // The error overlay (hmr.md §2): a dark-translucent backdrop over a slim
    // panel — a header bar ("vilan — build failed" + an error count), the REAL
    // compiler diagnostics in a monospace block with each `file:line:col` on its
    // own accent line and a red left-rule, and a "clears on next save" hint. The
    // terminal stays authoritative; this is the copy for the eyes on the browser.
    // Dependency-free, ES2020, no fonts fetched. Every string is set via
    // `textContent`, so a diagnostic containing `<`/`>` can never inject markup.
    function showOverlay(message) {
        removeOverlay();
        message = message || "build failed; see the terminal";
        var lines = message.split("\n");
        var count = 0;
        for (var i = 0; i < lines.length; i++) {
            if (!TRACE_LINE.test(lines[i]) && LOCATION_LINE.test(lines[i])) {
                count += 1;
            }
        }

        var backdrop = document.createElement("div");
        backdrop.id = OVERLAY_ID;
        backdrop.style.cssText =
            "position:fixed;inset:0;z-index:2147483647;overflow:auto;margin:0;padding:32px;" +
            "background:rgba(12,12,16,0.86);color:#e6e6e6;box-sizing:border-box;" +
            "font:13px/1.55 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;";

        var panel = document.createElement("div");
        panel.style.cssText =
            "max-width:920px;margin:0 auto;background:#17171c;border:1px solid #33333c;" +
            "border-left:4px solid #e5484d;border-radius:6px;overflow:hidden;" +
            "box-shadow:0 12px 40px rgba(0,0,0,0.5);";

        var header = document.createElement("div");
        header.style.cssText =
            "display:flex;align-items:center;justify-content:space-between;gap:12px;" +
            "padding:12px 16px;background:#1e1e24;border-bottom:1px solid #2c2c34;";
        var title = document.createElement("span");
        title.style.cssText = "color:#ff6169;font-weight:600;letter-spacing:0.02em;";
        title.textContent = "vilan: build failed";
        header.appendChild(title);
        if (count > 0) {
            var badge = document.createElement("span");
            badge.style.cssText =
                "color:#f0b5b7;background:#3a1e20;border-radius:10px;padding:2px 10px;font-size:12px;";
            badge.textContent = count === 1 ? "1 error" : count + " errors";
            header.appendChild(badge);
        }
        panel.appendChild(header);

        var body = document.createElement("div");
        body.style.cssText = "padding:8px 16px 14px;white-space:pre-wrap;word-break:break-word;";
        for (var j = 0; j < lines.length; j++) {
            var line = lines[j];
            var row = document.createElement("div");
            if (TRACE_LINE.test(line)) {
                // The chain: a notch deeper than the message, in a muted
                // slate that reads as context rather than as a new location.
                row.style.cssText = "color:#9fb3c8;padding-left:1em;";
            } else if (LOCATION_LINE.test(line)) {
                row.style.cssText = "color:#7fd0ff;font-weight:600;margin-top:12px;";
            } else if (/^\s*note:/.test(line)) {
                row.style.cssText = "color:#d6a25f;";
            } else {
                row.style.cssText = "color:#e6e6e6;";
            }
            // A blank line renders as vertical space (a non-breaking space keeps
            // the empty div's height).
            row.textContent = line.length ? line : " ";
            body.appendChild(row);
        }
        panel.appendChild(body);

        var hint = document.createElement("div");
        hint.style.cssText =
            "padding:10px 16px;background:#1e1e24;border-top:1px solid #2c2c34;" +
            "color:#8a8a95;font-size:12px;";
        hint.textContent = "Fixed on next save; this clears on the next successful build.";
        panel.appendChild(hint);

        backdrop.appendChild(panel);
        (document.body || document.documentElement).appendChild(backdrop);
    }

    // A `css` event hot-swaps stylesheets without a reload. `asset` (when the
    // CLI names it) is the changed sidecar's filename (`client.css`) — touch
    // only the <link> whose href IS that file (hmr.md §2), so a multi-browser-
    // leg workspace refreshes exactly the stylesheet that changed; with no name
    // (an older CLI), touch every stylesheet <link>.
    //
    // The bytes come from the DEV CHANNEL's `/asset/<name>` route — never from
    // re-fetching the <link>'s own href. That href is the USER'S OWN server
    // route (e.g. `/client.css`), and the common serving idiom (the todo
    // example) reads that file ONCE at server boot and serves the same bytes
    // for the life of the process: exactly the hazard `fetchAndSwap` above
    // avoids for JS, and just as real for CSS — a css-only round never restarts
    // that server (hmr.md §6), so its route stays stale for the life of the
    // session. `dist/<leg>.css` is rewritten fresh every watch round, and the
    // dev channel always serves those current bytes with `Cache-Control:
    // no-cache` (`hmr.rs::serve_asset`), so there is no cache-busting query to
    // invent here either.
    //
    // Applied as an injected <style> that supersedes the original <link>
    // (disabled, its href left untouched) rather than a `blob:` URL: a <style>
    // updates the CSSOM synchronously with no second trip through the
    // browser's own stylesheet loader, updates in place on a later css event
    // (no object URL to revoke or leak), and — since the <link> is merely
    // disabled, never replaced — a plain page reload always starts clean (a
    // fresh `app.html` re-enables it) rather than carrying a dangling swap
    // artifact forward.
    //
    // A fetch that 404s or errors (the dev channel lacks the asset, or is
    // unreachable) warns and changes nothing, leaving the current stylesheet
    // exactly as it was — mirroring `fetchAndSwap`'s never-reload reasoning:
    // reloading would only re-request the user's own stale route. A 404 is
    // AMBIGUOUS (a missing asset, a hiccup, a route that never existed), so it
    // is never read as "this stylesheet was removed": only a `swap` event's
    // `sheets` set — the round's own statement — says that (`withdrawOwnSheet`).
    //
    // Keyed by SIDECAR NAME rather than by <link> element, because a sheet the
    // page has no <link> for is exactly the case that used to fall through the
    // floor: the document a boot-time-rendered server served before the
    // stylesheet existed carries no <link> for it, and a css-only round never
    // restarts that server, so it never gains one (kolt.local 007).
    var cssShadows = {}; // "client.css" -> the <style> carrying its current bytes.

    // This leg's OWN sidecar. The shim may create or withdraw the page's copy
    // of this one on its own authority — the page runs this bundle, so this
    // bundle's stylesheet is the page's stylesheet whether or not the markup
    // ever linked it. Any OTHER name is another page's business: the shim only
    // maintains what this document already links, and never invents a <style>
    // for it (in a multi-browser-leg workspace that would inject the admin
    // leg's sheet into the client leg's page).
    var OWN_SHEET = BUNDLE + ".css";

    function assetBasename(href) {
        var base = href.split("?")[0];
        var slash = base.lastIndexOf("/");
        return slash === -1 ? base : base.slice(slash + 1);
    }

    // The stylesheet <link>s whose href names `asset` (basename match, query
    // ignored) — the asset-matching semantics of hmr.md §2, unchanged.
    function linksFor(asset) {
        var links = document.querySelectorAll('link[rel="stylesheet"]');
        var found = [];
        for (var index = 0; index < links.length; index++) {
            var base = links[index].href.split("?")[0];
            if (base === asset || base.endsWith("/" + asset)) {
                found.push(links[index]);
            }
        }
        return found;
    }

    // The <style> carrying `name`'s current bytes, created on first use and
    // updated in place forever after — one element per sidecar, no stack.
    //
    // Placed immediately after the <link> it supersedes when there is one, so
    // the sheet keeps its position in the cascade (appending to <head> would
    // move it past every sheet that followed it, quietly re-deciding ties); a
    // sheet with no <link> is new to the document and joins <head> at the end,
    // where a <link> for it would have gone.
    function shadowFor(name, anchor) {
        var style = cssShadows[name];
        if (!style) {
            style = document.createElement("style");
            if (style.setAttribute) {
                style.setAttribute("data-vilan-hmr", name);
            }
            if (anchor && anchor.parentNode && anchor.parentNode.insertBefore) {
                anchor.parentNode.insertBefore(style, anchor.nextSibling || null);
            } else {
                (document.head || document.documentElement).appendChild(style);
            }
            cssShadows[name] = style;
        }
        return style;
    }

    // Whether this document is a place `name` can land: it links the sheet, or
    // the sheet is this leg's own. A name that is neither belongs to another
    // browser leg's page — every leg's page receives the same broadcast — and
    // is passed over in silence, not warned about.
    function appliesHere(name) {
        return linksFor(name).length > 0 || name === OWN_SHEET;
    }

    // Land `name`'s fresh bytes in this document. With <link>s to supersede,
    // they are disabled and shadowed (hrefs untouched); with none, the sheet is
    // this leg's own and simply joins <head> on its own. A name that is neither
    // linked here nor ours applies nowhere, and says so.
    function applyFreshCss(name, text) {
        var links = linksFor(name);
        if (!links.length && name !== OWN_SHEET) {
            return false;
        }
        for (var index = 0; index < links.length; index++) {
            links[index].disabled = true;
        }
        shadowFor(name, links[0]).textContent = text;
        return true;
    }

    function fetchAndApplyCss(name) {
        if (!appliesHere(name)) {
            return undefined;
        }
        return fetch(channelUrl("/asset/" + name))
            .then(function (response) {
                if (!response.ok) {
                    throw new Error("unexpected status " + response.status);
                }
                return response.text();
            })
            .then(function (text) {
                applyFreshCss(name, text);
            })
            .catch(function (error) {
                if (typeof console !== "undefined" && console.warn) {
                    console.warn(
                        "[vilan] hmr: could not fetch fresh css (" + name + "); keeping the current stylesheet",
                        error
                    );
                }
            });
    }

    // A `css` event: refresh the named sidecar, or — with no name — every
    // stylesheet <link> the page has, each by its own basename (hmr.md §2).
    function bumpStylesheets(asset) {
        if (asset) {
            return fetchAndApplyCss(asset);
        }
        // Nameless: every stylesheet the page links, each by its own basename.
        // Which is why a first-ever sidecar needs the NAMED event the CLI always
        // sends — a sheet with no <link> is in no list to walk.
        var links = document.querySelectorAll('link[rel="stylesheet"]');
        var seen = {};
        var pending = [];
        for (var index = 0; index < links.length; index++) {
            var name = assetBasename(links[index].href);
            if (seen[name]) {
                continue;
            }
            seen[name] = true;
            var applied = fetchAndApplyCss(name);
            if (applied) {
                pending.push(applied);
            }
        }
        return pending.length ? Promise.all(pending) : undefined;
    }

    // A `swap` event's `sheets` — the round's COMPLETE browser stylesheet set —
    // reconciled against this document. Present names are fetched and applied;
    // this leg's own sidecar being absent is the round's statement that it
    // emitted none, so the page's copy of it is withdrawn. A swap re-evaluates
    // the bundle without reloading the document, so this is the only thing that
    // refreshes stylesheets on a round that also changed a bundle, and the only
    // way a first-ever or a deleted sidecar reaches the page at all.
    //
    // An event with no `sheets` (nothing declared) reconciles nothing.
    function reconcileSheets(sheets) {
        if (!sheets || typeof sheets.length !== "number") {
            return undefined;
        }
        var declared = {};
        var pending = [];
        for (var index = 0; index < sheets.length; index++) {
            declared[sheets[index]] = true;
            var applied = fetchAndApplyCss(sheets[index]);
            if (applied) {
                pending.push(applied);
            }
        }
        if (!declared[OWN_SHEET]) {
            withdrawOwnSheet();
        }
        return pending.length ? Promise.all(pending) : undefined;
    }

    // This round emits no stylesheet for this leg: take back what the shim put
    // in, and disable the <link> that named it. Both are ours to undo — the
    // <style> is our own artifact, and `disabled` is the same non-destructive,
    // reload-clean toggle the supersede path already relies on. The <link> is
    // never re-enabled and never re-pointed: it addresses the user's own server
    // route, whose bytes are the boot-time snapshot of a file that no longer
    // exists.
    function withdrawOwnSheet() {
        var style = cssShadows[OWN_SHEET];
        if (style) {
            if (style.parentNode && style.parentNode.removeChild) {
                style.parentNode.removeChild(style);
            } else if (style.remove) {
                style.remove();
            }
            delete cssShadows[OWN_SHEET];
        }
        var links = linksFor(OWN_SHEET);
        for (var index = 0; index < links.length; index++) {
            links[index].disabled = true;
        }
    }

    // A staleness signal (a `swap` event, or a `connected` whose version is
    // ahead of ours): fetch this leg's current bundle from the dev channel —
    // which always serves the fresh dist bytes — and run the swap protocol. On
    // success the singleton's version advances so later `connected` checks
    // agree. A fetch failure warns and WAITS (the next event retries): it must
    // never reload, because the page's own server may serve a bundle it read
    // once at boot — reloading re-fetches that stale bundle, whose shim sees
    // the same version gap and reloads again, forever. The dev channel, not
    // the page reload, is the only sure route to current bytes.
    function fetchAndSwap(version) {
        return fetch(channelUrl("/bundle/" + BUNDLE + ".js"))
            .then(function (response) {
                return response.text();
            })
            .then(function (text) {
                var result = swap(text);
                if (result && typeof result.then === "function") {
                    return result.then(function () {
                        singleton.version = version;
                    });
                }
                singleton.version = version;
            })
            .catch(function (error) {
                if (typeof console !== "undefined" && console.warn) {
                    console.warn("[vilan] hmr: could not fetch the current bundle; waiting for the next event", error);
                }
            });
    }

    // One dev-channel event. Exposed on the singleton so the node-stub e2e can
    // drive the real event path (EventSource is absent under the stub). Returns
    // the action's promise where there is one, so a test can await completion.
    function handleEvent(data) {
        // Any non-error event clears a lingering overlay.
        if (data.kind !== "error") {
            removeOverlay();
        }
        switch (data.kind) {
            case "connected":
                // Sent on every (re)connect with the channel's current version.
                // A gap means this page runs a stale bundle (the common serving
                // idiom reads dist once at server boot) or missed swaps while
                // disconnected — either way, the heal is a swap from the dev
                // channel, NEVER a reload (hmr.md §2; a reload re-fetches the
                // stale bundle and loops).
                //
                // RESIDUE: the hello carries no `sheets`, so this heal
                // refreshes the bundle but reconciles no stylesheet. A tab
                // opened AFTER a sidecar appeared but before the next round
                // that names it — on a server that decided the page's <link>s
                // at boot and has not restarted since — therefore runs current
                // code with no styles until the next css or swap round. The
                // channel would have to carry the round's set on the hello for
                // this heal to close it too.
                if (data.version !== singleton.version) {
                    return fetchAndSwap(data.version);
                }
                break;
            case "swap":
                // Stylesheets first, so the re-evaluated bundle mounts into a
                // document that already carries this round's styles.
                return Promise.resolve(reconcileSheets(data.sheets)).then(function () {
                    return fetchAndSwap(data.version);
                });
            case "reload":
                reload();
                break;
            case "css":
                return bumpStylesheets(data.asset);
            case "error":
                showOverlay(data.message);
                break;
        }
    }
    singleton.handleEvent = handleEvent;

    function connect() {
        // Under the node DOM stub there is no EventSource; the e2e drives
        // `window.__VILAN_HMR__.handleEvent(...)` / `.swap(text)` directly.
        if (typeof EventSource === "undefined") {
            return;
        }
        // `EventSource` cannot set request headers, which is why the token
        // travels as a query parameter on this route — and, for one shape rather
        // than two, on all of them.
        var source = new EventSource(channelUrl("/events"));
        source.onmessage = function (event) {
            var data;
            try {
                data = JSON.parse(event.data);
            } catch (error) {
                return;
            }
            handleEvent(data);
        };
        // On error, EventSource reconnects natively — nothing clever to do.
    }

    connect();
})();
