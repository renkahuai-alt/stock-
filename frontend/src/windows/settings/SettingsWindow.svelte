<script lang="ts">
  import {
    setActiveSettingsSection,
    setBoardAlgorithmDraft,
    settingsStore,
  } from '../../stores/settingsStore';
  import {
    closeSettingsWindowFlow,
    requestDeleteBoardFlow,
    saveBoardFlow,
    saveCredentialsFlow,
    selectBoardDraftFlow,
    startCreateBoardFlow,
  } from '../../services/settingsFlow';

  $: activeMembers = $settingsStore.membersByBoard[$settingsStore.activeBoardId] ?? [];
</script>

<div class="settings-shell">
  <aside class="settings-nav">
    <div class="settings-nav__title">Settings</div>
    <button
      type="button"
      class="settings-nav__item"
      class:active={$settingsStore.activeSection === 'credentials'}
      on:click={() => setActiveSettingsSection('credentials')}
    >
      API 鉴权
    </button>
    <button
      type="button"
      class="settings-nav__item"
      class:active={$settingsStore.activeSection === 'boards'}
      on:click={() => setActiveSettingsSection('boards')}
    >
      板块维护
    </button>
  </aside>

  <main class="settings-panel">
    {#if $settingsStore.activeSection === 'credentials'}
      <section class="settings-pane">
        <div class="settings-pane__header">
          <h1 class="chart-header__title">Longbridge 鉴权</h1>
          <p class="chart-header__subtitle">保存真实鉴权配置，供同步与图表查询使用。</p>
        </div>

        <label class="settings-field">
          <span>App Key</span>
          <input bind:value={$settingsStore.appKey} />
        </label>
        <label class="settings-field">
          <span>App Secret</span>
          <input bind:value={$settingsStore.appSecret} />
        </label>
        <label class="settings-field">
          <span>Access Token</span>
          <input bind:value={$settingsStore.accessToken} />
        </label>

        {#if $settingsStore.credentialsFeedback}
          <div class="settings-feedback">{$settingsStore.credentialsFeedback}</div>
        {/if}
      </section>
    {:else}
      <section class="settings-pane settings-pane--boards">
        <div class="settings-pane__header">
          <h1 class="chart-header__title">板块维护</h1>
          <p class="chart-header__subtitle">维护自定义板块与成分股配置，兼容快路径与后台构建。</p>
        </div>

        <div class="settings-board-shell">
          <div class="settings-board-list">
            <button
              type="button"
              class="toolbar__button"
              disabled={$settingsStore.isDeletingBoard}
              on:click={() => startCreateBoardFlow()}
            >
              新建板块
            </button>
            {#each $settingsStore.boardCatalog as board}
              <div class="settings-board-row">
                <button
                  type="button"
                  class="rail__row"
                  class:selected={board.boardId === $settingsStore.activeBoardId}
                  disabled={$settingsStore.isDeletingBoard}
                  on:click={() => selectBoardDraftFlow(board.boardId)}
                >
                  <span class="rail__primary">{board.name}</span>
                </button>
                <button
                  type="button"
                  class="settings-board-row__delete"
                  class:settings-board-row__delete--confirm={$settingsStore.pendingDeleteBoardId === board.boardId}
                  disabled={$settingsStore.isSavingBoard || $settingsStore.isDeletingBoard}
                  on:click={() => requestDeleteBoardFlow(board.boardId)}
                >
                  {$settingsStore.pendingDeleteBoardId === board.boardId ? '确认删除' : '删除'}
                </button>
              </div>
            {/each}
          </div>

          <div class="settings-board-editor">
            <label class="settings-field">
              <span>板块名称</span>
              <input bind:value={$settingsStore.boardName} />
            </label>

            <div class="settings-field">
              <span>板块口径</span>
              <div class="chart-header__controls">
                <button
                  type="button"
                  class="segment-button"
                  class:active={$settingsStore.boardAlgorithm === 'equal_weight_v1'}
                  on:click={() => setBoardAlgorithmDraft('equal_weight_v1')}
                >
                  等权
                </button>
                <button
                  type="button"
                  class="segment-button"
                  class:active={$settingsStore.boardAlgorithm === 'market_cap_weight_v1'}
                  on:click={() => setBoardAlgorithmDraft('market_cap_weight_v1')}
                >
                  市值
                </button>
              </div>
            </div>

            <label class="settings-field">
              <span>新增股票代码</span>
              <input
                bind:value={$settingsStore.boardMembersInput}
                spellcheck={false}
                autocapitalize="characters"
                autocorrect="off"
              />
            </label>

            <div class="settings-field">
              <span>当前成分股</span>
              <div class="settings-chip-row">
                {#each activeMembers as member}
                  <span class="settings-chip">{member.symbol}</span>
                {/each}
              </div>
            </div>

            {#if $settingsStore.boardFeedback}
              <div class="settings-feedback">{$settingsStore.boardFeedback}</div>
            {/if}
          </div>
        </div>
      </section>
    {/if}

    <div class="settings-actions">
      {#if $settingsStore.activeSection === 'credentials'}
        <button
          type="button"
          class="toolbar__button"
          disabled={$settingsStore.isSavingCredentials}
          on:click={() => void saveCredentialsFlow()}
        >
          {$settingsStore.isSavingCredentials ? '保存中…' : '保存设置'}
        </button>
      {:else}
        <button
          type="button"
          class="toolbar__button"
          disabled={$settingsStore.isSavingBoard || $settingsStore.isDeletingBoard}
          on:click={() => void saveBoardFlow()}
        >
          {$settingsStore.isSavingBoard
            ? '保存中…'
            : $settingsStore.boardEditorMode === 'create'
              ? '创建板块'
              : '保存板块'}
        </button>
      {/if}
      <button type="button" class="toolbar__button" on:click={() => void closeSettingsWindowFlow()}>关闭</button>
    </div>
  </main>
</div>
