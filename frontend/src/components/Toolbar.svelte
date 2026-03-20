<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let title = '市场';
  export let syncLabel = '最近同步 09:30';
  export let syncDisabled = false;

  const dispatch = createEventDispatcher<{ sync: void; settings: void }>();
</script>

<header class="toolbar">
  <div class="traffic-lights" aria-hidden="true">
    <span></span><span></span><span></span>
  </div>
  <div class="toolbar__title">{title}</div>
  <div class="toolbar__actions">
    <span class="toolbar__meta">{syncLabel}</span>
    <button type="button" class="toolbar__button" disabled={syncDisabled} on:click={() => dispatch('sync')}>同步</button>
    <button type="button" class="toolbar__button" on:click={() => dispatch('settings')}>设置</button>
  </div>
</header>
