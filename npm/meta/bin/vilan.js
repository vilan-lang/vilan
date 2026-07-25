#!/usr/bin/env node
// npm's `bin` entry for the compiler. The whole stub is one hand-off: see
// ../lib/launch.js.
//
// `process.exitCode` rather than `process.exit()`: an error message written to
// a piped stderr may still be in flight, and exiting outright would truncate
// it. Nothing else is pending — `spawnSync` has already returned.
"use strict";

process.exitCode = require("../lib/launch.js").launch("vilan");
