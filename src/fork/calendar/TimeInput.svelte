<script lang="ts">
  // Fork (v1.1.1): the calendar time field. Replaces the WebView's native
  // hour/minute spinner: type "2pm" / "1430" / "14:30", or pick a quarter hour
  // from the list. An end field passes `anchor` (the start) and the list shows
  // durations. Arrow keys move through the list, Enter takes it, Esc closes.
  import { tick } from "svelte";
  import { parseTime, timeOptions } from "./time";

  interface Props {
    value: string;
    anchor?: string | null;
    readonly?: boolean;
    label?: string;
    onchange?: (v: string) => void;
  }
  let { value = $bindable(), anchor = null, readonly = false, label, onchange }: Props = $props();

  let text = $state(value);
  let open = $state(false);
  let active = $state(-1);
  let invalid = $state(false);
  let listEl = $state<HTMLUListElement | undefined>();
  const listId = `time-list-${Math.random().toString(36).slice(2, 8)}`;

  // Follow outside changes (the start moving the end) while not being edited.
  $effect(() => {
    const v = value;
    if (!open) {
      text = v;
      invalid = false;
    }
  });

  const options = $derived(timeOptions(anchor));

  function commit(v: string) {
    value = v;
    text = v;
    invalid = false;
    onchange?.(v);
  }

  async function show() {
    if (readonly) return;
    open = true;
    const exact = options.findIndex((o) => o.value === value);
    // Nearest option at or after the current value, so the list opens in place.
    active = exact >= 0 ? exact : options.findIndex((o) => o.value > value);
    await tick();
    listEl?.querySelector<HTMLElement>(`[data-i="${Math.max(active, 0)}"]`)?.scrollIntoView({ block: "center" });
  }

  function close(apply: boolean) {
    if (apply) {
      const parsed = parseTime(text);
      if (parsed) commit(parsed);
      else if (text.trim() === "") text = value;
      else invalid = true;
    }
    open = false;
  }

  async function move(dir: 1 | -1) {
    if (!open) await show();
    active = Math.min(options.length - 1, Math.max(0, active + dir));
    text = options[active].value;
    await tick();
    listEl?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      void move(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      void move(-1);
    } else if (e.key === "Enter" && open) {
      e.preventDefault();
      close(true);
    } else if (e.key === "Escape" && open) {
      e.preventDefault();
      e.stopPropagation();
      text = value;
      open = false;
    } else if (e.key === "Tab") {
      close(true);
    }
  }
</script>

<div class="time-input">
  <input
    type="text"
    inputmode="numeric"
    autocomplete="off"
    spellcheck="false"
    aria-label={label}
    aria-invalid={invalid}
    aria-expanded={open}
    role="combobox"
    aria-controls={listId}
    class:invalid
    bind:value={text}
    {readonly}
    onfocus={() => void show()}
    onclick={() => void show()}
    onblur={() => close(true)}
    onkeydown={onKeydown}
  />
  {#if open}
    <ul class="list" id={listId} role="listbox" bind:this={listEl}>
      {#each options as o, i (o.value)}
        <!-- mousedown, not click: it must land before the input's blur. -->
        <li
          role="option"
          aria-selected={o.value === value}
          data-i={i}
          class:active={i === active}
          class:current={o.value === value}
          onmousedown={(e) => {
            e.preventDefault();
            commit(o.value);
            open = false;
          }}
        >
          <span class="t">{o.value}</span>
          {#if o.note}<span class="d">{o.note}</span>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .time-input {
    position: relative;
    display: inline-block;
  }
  input {
    width: 76px;
    padding: 5px 8px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-s);
    background: var(--surface);
    font-variant-numeric: tabular-nums;
    user-select: text;
  }
  input:focus {
    border-color: var(--focus);
  }
  input.invalid {
    border-color: var(--danger);
  }
  input[readonly] {
    border-color: transparent;
    background: none;
  }
  .list {
    position: absolute;
    z-index: 30;
    top: calc(100% + 4px);
    left: 0;
    min-width: 150px;
    max-height: 232px;
    overflow-y: auto;
    list-style: none;
    padding: 4px;
    background: var(--surface-raised);
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-pop);
  }
  li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 5px 8px;
    border-radius: var(--radius-s);
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }
  li:hover,
  li.active {
    background: var(--hover);
  }
  li.current {
    background: var(--selected);
    font-weight: 600;
  }
  .d {
    color: var(--text-faint);
    font-size: 12px;
  }
</style>
