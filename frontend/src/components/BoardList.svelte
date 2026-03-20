<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { BoardListItemViewModel } from '../stores/boardBuildStore';

  export let items: BoardListItemViewModel[] = [];
  export let activeBoardId = '';

  const dispatch = createEventDispatcher<{ select: string }>();
</script>

<section class="rail rail--boards">
  <div class="rail__title">自定义板块</div>
  {#each items as item}
    <button
      type="button"
      class="rail__row"
      class:selected={item.boardId === activeBoardId}
      on:click={() => dispatch('select', item.boardId)}
    >
      <span class="rail__primary">{item.name}</span>
      {#if item.buildStatusVisible}
        <span class="rail__meta">{item.buildStatusLabel}</span>
      {/if}
    </button>
  {/each}
</section>
