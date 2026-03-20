<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let value = '';
  const dispatch = createEventDispatcher<{ save: string }>();
</script>

<section class="note-editor">
  <div class="note-editor__header">
    <div class="note-editor__title">观点笔记</div>
    <button type="button" class="note-editor__button" on:click={() => dispatch('save', value)}>保存</button>
  </div>
  <textarea bind:value class="note-editor__input" placeholder="记录当前目标的观点、假设和观察。" spellcheck={false}></textarea>
</section>
