<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { IndexItem } from '../types/contracts';

  export let items: IndexItem[] = [];
  export let activeId = 'IXIC';

  const dispatch = createEventDispatcher<{ select: string }>();
</script>

<nav class="index-strip" aria-label="Indexes">
  {#each items as item}
    <button
      type="button"
      class:active={item.id === activeId}
      class="index-strip__item"
      disabled={item.disabled}
      on:click={() => dispatch('select', item.id)}
    >
      <span class="index-strip__label">{item.label}</span>
    </button>
  {/each}
</nav>
