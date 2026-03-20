<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { RangeKey } from '../types/contracts';

  export let activeRange: RangeKey = '1y';
  const ranges: RangeKey[] = ['1m', '3m', '6m', '1y', '3y', 'all'];
  const dispatch = createEventDispatcher<{ select: RangeKey }>();
</script>

<div class="segmented-row segmented-row--ranges">
  {#each ranges as range}
    <button
      type="button"
      class="segment-button"
      class:active={range === activeRange}
      on:mousedown|preventDefault
      on:click={() => dispatch('select', range)}
    >
      {range.toUpperCase()}
    </button>
  {/each}
</div>
