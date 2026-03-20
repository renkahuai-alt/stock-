<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { MemberSummary } from '../types/contracts';

  export let items: MemberSummary[] = [];
  export let activeSymbol = '';

  const dispatch = createEventDispatcher<{ select: string }>();
</script>

<section class="rail rail--members">
  <div class="rail__title">成分股</div>
  {#each items as item}
    <button
      type="button"
      class="rail__row"
      class:selected={item.symbol === activeSymbol}
      on:click={() => dispatch('select', item.symbol)}
    >
      <span class="rail__primary">{item.symbol}</span>
      {#if item.weightPercent !== undefined}
        <span class="rail__meta">{item.weightPercent.toFixed(1)}%</span>
      {/if}
    </button>
  {/each}
</section>
