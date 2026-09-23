<script lang="ts">
  // Settings section for the local MCP server (PLAN.md Phase 12). Plain, no
  // violet: it is a transport, not an AI feature.
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "../../lib/i18n/index.svelte";

  type McpStatus = {
    enabled: boolean;
    listening: boolean;
    port: number;
    endpoint: string;
    token: string | null;
    addCommand: string | null;
  };
  let mcp = $state<McpStatus | null>(null);
  let copied = $state(false);

  async function load() {
    try {
      mcp = await invoke<McpStatus>("fork_mcp_status");
    } catch {
      mcp = null;
    }
  }
  async function toggle(on: boolean) {
    try {
      mcp = await invoke<McpStatus>("fork_mcp_set_enabled", { on });
    } catch {}
  }
  async function copy() {
    if (!mcp?.addCommand) return;
    await navigator.clipboard.writeText(mcp.addCommand);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
  $effect(() => {
    void load();
  });
</script>

<section class="fork-mcp">
  <div class="microlabel section">{t("fork.mcp.title")}</div>
  <label class="row">
    <input
      type="checkbox"
      checked={mcp?.enabled ?? true}
      onchange={(e) => toggle((e.target as HTMLInputElement).checked)}
    />
    <span>{t("fork.mcp.enable")}</span>
  </label>
  <p class="hint">{t("fork.mcp.hint")}</p>
  {#if mcp}
    <p class="status" class:live={mcp.listening}>
      {mcp.listening
        ? t("fork.mcp.listening", { port: mcp.port })
        : mcp.enabled
          ? t("fork.mcp.port_busy", { port: mcp.port })
          : t("fork.mcp.stopped")}
    </p>
    {#if mcp.enabled && mcp.addCommand}
      <p class="hint">{t("fork.mcp.register")}</p>
      <pre class="cmd"><code>{mcp.addCommand}</code></pre>
      <button type="button" class="copy" onclick={copy}>
        {copied ? t("fork.mcp.copied") : t("fork.mcp.copy")}
      </button>
      <p class="hint">{t("fork.mcp.token_note")}</p>
    {/if}
  {/if}
</section>

<style>
  .fork-mcp {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px 0 16px;
  }
  .section {
    padding: 8px 0 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13.5px;
  }
  .hint {
    margin: 0;
    font-size: 12.5px;
    color: var(--text-dim);
    max-width: 62ch;
  }
  .status {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--text-faint);
  }
  .status.live {
    color: var(--text);
  }
  .cmd {
    margin: 0;
    padding: 10px 12px;
    background: var(--surface-sunken, var(--surface));
    border: 1px solid var(--hairline);
    border-radius: var(--radius-s);
    font-family: var(--font-mono);
    font-size: 11.5px;
    white-space: pre-wrap;
    word-break: break-all;
    max-width: 72ch;
  }
  .copy {
    align-self: flex-start;
    font: inherit;
    font-size: 12.5px;
    padding: 5px 10px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    background: var(--surface-raised);
    color: var(--text);
    cursor: pointer;
  }
</style>
