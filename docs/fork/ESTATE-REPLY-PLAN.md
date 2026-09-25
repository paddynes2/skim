# Estate reply: build contract

## Goal

One action beside Reply asks Patrick's existing Hetzner estate agent to investigate
the selected email and save a normal Gmail reply draft in Patrick's voice. Skim
syncs and opens that exact draft. No sends and no second context index.

## Plan, 25 September 2026

1. Add a fork-owned Python SSH adapter, executed as the enrolled `cc-nesbitt`
   profile on hel-work. Use its installed Codex CLI, existing estate checkout and
   own `os-google` credential. Credentials never leave Hetzner. Agentd's coding
   launcher creates a worktree per task and caps them at 20, so ordinary mail
   drafting must not consume those worktrees. This adapter starts a bounded
   read-only Codex invocation, not another permanent daemon or scheduler.
2. The adapter resolves the RFC Message-ID through the approved gateway, checks
   for existing thread drafts, and asks the agent for structured draft text plus
   source references. It alone calls the gateway's draft creation tool. Model
   output cannot select an account, recipient or save tool.
3. Use a private per-message receipt and a file lock to coalesce repeated clicks.
   Persist `saving` before the one remote mutation. An uncertain save is never
   retried automatically. Remote work survives closing the desktop app.
4. Add Rust commands for start/status and locating the returned draft after a
   targeted Drafts sync. SSH uses the configured `clouddev-admin` route, strict
   host checking, bounded calls, fixed remote command and JSON stdin. No shell
   interpolation of email text. Limit support to the gateway's approved mailbox.
5. Add a small fork-owned Svelte action beside Reply. Optional direction,
   truthful progress/error state, existing normal composer for the synced draft.
   Do not create an empty local reply while the remote draft is being prepared.

## Scope and files

New: `scripts/fork/estate_reply.py`, its Python tests,
`src-tauri/src/fork/estate_reply.rs`, `src/fork/estate/ReplyAction.svelte`, and
this plan. Hooks: `src-tauri/src/fork/mod.rs`, `src-tauri/src/lib.rs`,
`src/components/ReadingPane.svelte`, `src/locales/en.json` (or actual locale path).
Update fork STATUS/TOUCHLIST/README with final evidence. No schema migrations,
credentials, gateway permissions or daemon/unit changes. Existing installed SSH
access is reused. Remote helper installation is a versioned user-owned code copy.

## Acceptance and failure cases

- Correct mailbox, RFC message, Gmail thread and reply recipient; no subject-only
  matching. Existing remote/local drafts are preserved.
- Double-click, reconnect and restart reuse the receipt; no duplicate save after
  a timeout. Agent failure cannot masquerade as a saved draft.
- Agent reads current client/project context and writing instructions; sources
  and missing facts remain reviewable. Email content is data, not instructions.
- No automatic send. Ordinary edits happen in Skim. The gateway currently only
  exposes draft creation, so an AI revision/update flow is outside this first cut.
- Read-only estate investigation; model has no draft-write tool. Host gateway
  client performs the sole draft mutation after validating model output.
- Unit tests cover validation, duplicate requests, refusal, stale-thread detection
  and ambiguous save outcomes. Rust tests cover mapping/transport boundaries.
  Run Svelte checks/build, Rust fmt/clippy/tests, existing fork gates and UI review.
- Verify gateway/tool and runtime availability live. Exercise one generated draft
  end-to-end only on an appropriate existing email, with no send; record exact
  limitations if that cannot be established. Back up before app installation.

## Verification log

- SSH to configured hel-work route succeeds.
- Own Codex profile quota refreshed: ordinary usage permitted; pinned selection
  succeeds. No substitution of another profile.
- Own Google adapter discovery succeeds; draft creation accepts RFC reply target
  and Gmail thread ID, but no draft-update operation is exposed.
- Skim has targeted Drafts refresh and an existing `edit_draft` import path.
- SKG reports Skim outside enrollment; direct source inspection is authoritative.
- Agentd remains the canonical coding-task supervisor. Its per-task worktrees
  are not suitable for one email request each; the adapter uses the already
  installed runtime directly, with no model/account fallback and read-only shell.
- The Codex CLI contract was verified locally and against
  https://developers.openai.com/codex/noninteractive. Output is schema constrained;
  `draft_gmail_message` is disabled in the model's MCP tool set for this invocation.
- The real read-only runtime probe returned source references to the estate's
  writing-style README and constitutional writing instruction. It was synthetic,
  not evidence of quality on a real client email.

## Operations and limits

The fixed SSH route is `clouddev-admin`; its configured host-key checking stays
enabled. The command switches to the enrolled `cc-nesbitt` user, runs the gateway
venv's Python, and passes JSON on stdin to the installed helper. Local/remote
paths are specific to Patrick's fork. No SSH key or broker token is copied.

Install the reviewed helper with `install -m 600 -o cc-nesbitt -g cc-nesbitt`
into `/home/cc-nesbitt/.local/share/skim/estate_reply.py` (parent mode 700).
No service restart is needed. Receipts, candidate text and diagnostic logs are
private under that user's `~/.local/state/skim-replies/<sha256>/`.
Rollback the helper by restoring the prior reviewed file; preserve receipts,
especially `saving`/`uncertain`, so unknown Gmail writes cannot be repeated.

For an uncertain save, inspect Gmail Drafts through the approved gateway before
any manual receipt reset. Never clear a receipt merely to make Retry work.
Errors before saving may be retried; completed requests return the existing RFC
ID. Opening a draft already edited/sent/deleted elsewhere may require using
the current Drafts view. The adapter does not automatically replace it.

UI polling runs only while this action is open, with a finite limit; leaving
the email does not cancel a remote investigation. Clicking Draft reply again
reconnects to the receipt. No result automatically opens over another email.

The core fallback is the ordinary Reply button, not a second AI provider.
Unsupported mailboxes do not offer the estate action.

Reusable extraction proposal (not implemented): if a second estate surface needs
this workflow, move the message-resolution, receipt and draft-save adapter to
one OS package `packages/estate-reply/` with gateway/runner adapters, its existing
tests and README. Add an estate-reply INDEX entry then; do not duplicate the
workflow or build a generic broker ahead of that consumer. Learned contract:
the current gateway creates drafts but returns only a draft ID, so this adapter
verifies the new message through the same gateway and uses its RFC Message-ID
to reconnect with IMAP. It must never infer identity from the subject.
