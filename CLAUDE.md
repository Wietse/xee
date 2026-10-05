# xee — Claude Code entry point

@AGENTS.md

`AGENTS.md` is the instruction file for every agent harness and the only copy of the workflow contract; the import above loads it. What follows applies to Claude Code alone.

- Do **not** use the Agent tool's `isolation: "worktree"` option for a review probe. Create the detached worktree yourself, as `AGENTS.md` *Independent review* shows, and pass its *path* in the subagent's prompt.
- Agent memory notes under `~/.claude/` are a cache (pointers, machine state, working preferences), never the record.
