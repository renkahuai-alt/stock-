<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { BoardAlgorithm, Granularity, TargetType } from '../types/contracts';

  export let title = 'AI半导体';
  export let subtitle = '等待数据加载';
  export let sourceHint = '';
  export let notice = '';
  export let noticeTone: 'neutral' | 'warning' | 'danger' = 'neutral';
  export let targetType: TargetType = 'board';
  export let activeBoardAlgorithm: BoardAlgorithm = 'equal_weight_v1';
  export let activeGranularity: Granularity = 'day';

  const dispatch = createEventDispatcher<{
    boardAlgorithmChange: BoardAlgorithm;
    granularityChange: Granularity;
  }>();

  $: boardAlgorithmDisabled = targetType !== 'board';
</script>

<div class="chart-header">
  <div class="chart-header__copy">
    <h1 class="chart-header__title">{title}</h1>
    <p class="chart-header__subtitle">{subtitle}</p>
    {#if sourceHint}
      <p class="chart-header__hint">{sourceHint}</p>
    {/if}
    {#if notice}
      <p class={`chart-header__notice chart-header__notice--${noticeTone}`}>{notice}</p>
    {/if}
  </div>
  <div class="chart-header__controls">
    <div class="chart-header__control-group">
      <button
        type="button"
        class="segment-button"
        class:active={activeBoardAlgorithm === 'equal_weight_v1'}
        disabled={boardAlgorithmDisabled}
        on:click={() => dispatch('boardAlgorithmChange', 'equal_weight_v1')}
      >
        等权
      </button>
      <button
        type="button"
        class="segment-button"
        class:active={activeBoardAlgorithm === 'market_cap_weight_v1'}
        disabled={boardAlgorithmDisabled}
        on:click={() => dispatch('boardAlgorithmChange', 'market_cap_weight_v1')}
      >
        市值
      </button>
    </div>
    <div class="chart-header__control-group">
      <button
        type="button"
        class="segment-button"
        class:active={activeGranularity === 'day'}
        on:click={() => dispatch('granularityChange', 'day')}
      >
        日K
      </button>
      <button
        type="button"
        class="segment-button"
        class:active={activeGranularity === 'week'}
        on:click={() => dispatch('granularityChange', 'week')}
      >
        周K
      </button>
    </div>
  </div>
</div>
