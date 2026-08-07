# The Vilan Organization's stance on AI

_This document was written exclusively by Reed Syllas without assistance from AI tooling._

In the modern age, AI-assisted programming has become quite common. With it, two opposing sides have formed: those who love it and those who hate it. I'm not going to engage with the topic as a whole, but I will set guidelines for the Vilan project.

AI has been instrumental in the development of Vilan. Much of what goes into a language: the compiler, regex grammar, spec documents, extensions for multiple editors, and so-on are tedious to build and maintain. As the founder and sole developer (as of 2026-08-06), I would not be this far without AI-assistance. In fact, I worked for many years on a predecessor to Vilan. I didn't get half as far as I have with Claude Code in the past few months (but I learned everything I needed to build a language and manage a project of this caliber, so it wasn't a waste!).

All this said, AI makes mistakes. A lot of mistakes. To keep it in check, a comprehensive test suite exists. As new bugs are discovered, they are fixed and pinned in the test suite so that they can't break again. Not _silently_, anyway.

Yet, a test suite only goes so far. Thus, AI-isms like frequent em-dashes, wordy or confusing sentences, and bugs have made their way into the codebase. These issues are periodically being removed. Contributions that improve this are encouraged.

Overall, AI has allowed this project to pseudo-operate with the resources of a full dev team.

---

AI contributions to this project are allowed, with caveats.

1. Human oversight must be used to ensure code is of sufficient quality.
2. Use of AI tooling must not be hidden. A generic "AI-assisted" attribution is acceptable.
3. The language's grammar, APIs, and visuals are designed by humans for humans and will always continue to be.

These requirements may become more or less restrictive over time.
