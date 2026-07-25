#!/usr/bin/env node
// npm's `bin` entry for the language server — the compiler's twin, and the
// reason resolution lives in a module both stubs share rather than in either
// of them. See ../lib/launch.js.
"use strict";

process.exitCode = require("../lib/launch.js").launch("vilan-lsp");
