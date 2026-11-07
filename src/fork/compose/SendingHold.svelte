<script lang="ts">
  // Fork (D95): the compose window's send bar. Replaces the footer once Send
  // is clicked: "Sending to <who> in N s" with Undo while the hold lasts,
  // "Sending to <who>…" after it, and "Sent to <who>" only once Skim confirms
  // the send (`confirmed`, set by the composer from `mail:sent` for this
  // draft). Then `ondone` closes the window.
  import { onDestroy, onMount } from "svelte";
  import { errorMessage } from "../../lib/api";
  import { t } from "../../lib/i18n/index.svelte";
  import { CONFIRM_TIMEOUT_MS, SENT_LINGER_MS, canUndo, secondsLeft } from "./send-hold";

  let {
    recipients,
    notBefore,
    confirmed = false,
    onundo,
    ondone,
  }: {
    /** Addresses the mail goes to (To, Cc, Bcc). */
    recipients: string[];
    /** Unix seconds the held send goes out; null = no hold (sent at once). */
    notBefore: number | null;
    /** Skim reported this draft sent. */
    confirmed?: boolean;
    /** Cancel the held send. Throws when it is too late to cancel. */
    onundo: () => Promise<void>;
    ondone: () => void;
  } = $props();

  let now = $state(Date.now());
  let phase = $state<"held" | "undoing" | "sending" | "sent">(initialPhase());
  let note = $state("");
  let undoButton = $state<HTMLButtonElement | null>(null);
  let tick: ReturnType<typeof setInterval> | null = null;
  let fallback: ReturnType<typeof setTimeout> | null = null;
  let linger: ReturnType<typeof setTimeout> | null = null;
  let gone = false;

  function initialPhase(): "held" | "sending" {
    return canUndo(notBefore, Date.now()) ? "held" : "sending";
  }

  const who = $derived(
    recipients.length <= 1
      ? (recipients[0] ?? "")
      : t("fork.compose.hold_more", { first: recipients[0], count: recipients.length - 1 }),
  );
  const secs = $derived(notBefore === null ? 0 : secondsLeft(notBefore, now));

  function finish() {
    if (gone) return;
    gone = true;
    ondone();
  }

  function onTick() {
    now = Date.now();
    if (phase === "held" && !canUndo(notBefore, now)) phase = "sending";
    // The confirmation clock starts when the send is due, not at the click.
    if (phase === "sending" && !fallback && (notBefore === null || now / 1000 >= notBefore)) {
      fallback = setTimeout(finish, CONFIRM_TIMEOUT_MS);
    }
  }

  async function undo() {
    if (phase !== "held") return;
    phase = "undoing";
    try {
      await onundo();
    } catch (e) {
      note = t("fork.compose.hold_too_late");
      console.warn("undo send refused", errorMessage(e));
      phase = "sending";
    }
  }

  // "Sent" only on Skim's word, never because the countdown ran out.
  $effect(() => {
    if (!confirmed || phase === "sent" || gone) return;
    phase = "sent";
    if (fallback) clearTimeout(fallback);
    linger = setTimeout(finish, SENT_LINGER_MS);
  });

  onMount(() => {
    tick = setInterval(onTick, 250);
    onTick();
    // Enter or Space now lands on Undo, never on a field.
    queueMicrotask(() => undoButton?.focus());
  });

  onDestroy(() => {
    gone = true;
    if (tick) clearInterval(tick);
    if (fallback) clearTimeout(fallback);
    if (linger) clearTimeout(linger);
  });
</script>

<div class="hold" role="status" aria-live="polite">
  <span class="line">
    {#if phase === "sent"}
      {t("fork.compose.hold_sent", { who })}
    {:else if phase === "held" || phase === "undoing"}
      {t("fork.compose.hold_sending_in", { who, secs })}
    {:else}
      {t("fork.compose.hold_sending_now", { who })}
    {/if}
  </span>
  {#if note}<span class="note">{note}</span>{/if}
  <div class="grow"></div>
  {#if phase === "held" || phase === "undoing"}
    <button class="undo" bind:this={undoButton} disabled={phase === "undoing"} onclick={undo}>
      {t("fork.compose.undo")}
    </button>
  {/if}
</div>

<style>
  .hold {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 20px;
    border-top: 1px solid var(--hairline);
    font-size: 13px;
    min-height: 58px;
  }
  .line {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .note {
    color: var(--text-dim);
    font-size: 12px;
  }
  .grow {
    flex: 1;
  }
  .undo {
    padding: 8px 22px;
    border-radius: var(--radius-m);
    background: var(--primary);
    color: var(--on-primary);
    font-weight: 700;
    font-size: 13.5px;
  }
  .undo:hover:not(:disabled) {
    opacity: 0.88;
  }
  .undo:disabled {
    opacity: 0.6;
  }
</style>
