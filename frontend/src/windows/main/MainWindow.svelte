<script lang="ts">
  import Toolbar from '../../components/Toolbar.svelte';
  import IndexSwitcher from '../../components/IndexSwitcher.svelte';
  import BoardList from '../../components/BoardList.svelte';
  import MemberList from '../../components/MemberList.svelte';
  import ChartHeader from '../../components/ChartHeader.svelte';
  import ChartCanvas from '../../components/ChartCanvas.svelte';
  import TargetNoteEditor from '../../components/TargetNoteEditor.svelte';
  import StatusBar from '../../components/StatusBar.svelte';
  import type { BoardBuildStatusPayload } from '../../types/contracts';
  import {
    changeBoardAlgorithmFlow,
    changeGranularityFlow,
    openSettingsWindowFlow,
    runManualSyncFlow,
    saveCurrentTargetNoteFlow,
    selectBoardFlow,
    selectIndexFlow,
    selectSymbolFlow,
  } from '../../services/mainFlow';
  import { formatChartSourceHint, formatSourceStatusLabel, sanitizeUserFacingMessage } from '../../services/displayCopy';
  import { appStore, getBoardMembersByBoardAndAlgorithm } from '../../stores/appStore';
  import { chartStore } from '../../stores/chartStore';
  import type { ChartState, ChartViewStatus } from '../../stores/chartStore';
  import { noteStore } from '../../stores/noteStore';
  import { selectionStore } from '../../stores/selectionStore';
  import { syncStore } from '../../stores/syncStore';
  import { watchStore, type WatchState } from '../../stores/watchStore';
  import {
    boardBuildStore,
    buildBoardListItems,
    formatBoardBuildStatus,
    resolveBoardBuild,
    summarizeBoardBuilds,
  } from '../../stores/boardBuildStore';

  $: activeBoardId = $selectionStore.activeBoardId;
  $: memberItems = activeBoardId
    ? getBoardMembersByBoardAndAlgorithm($appStore.membersByBoard, activeBoardId, $selectionStore.boardAlgorithm)
    : [];
  $: boardItems = buildBoardListItems($appStore.boards, $boardBuildStore);
  $: currentBoardBuild =
    $selectionStore.targetType === 'board'
      ? resolveBoardBuild($appStore.boards, $boardBuildStore, $selectionStore.targetId)
      : null;
  $: chartTitle = $chartStore.meta.title || $appStore.activeTargetSummary.title || '目标图表';
  $: chartSourceHint = formatChartSourceHint({
    targetType: $selectionStore.targetType,
    granularity: $selectionStore.granularity,
    sourceStatus: $chartStore.sourceStatus,
    providerKind: $chartStore.meta.providerKind,
    providerSymbol: $chartStore.meta.providerSymbol,
    valueMode: $chartStore.meta.valueMode,
  });
  $: chartViewportKey = buildChartViewportKey(
    $selectionStore.targetType,
    $selectionStore.targetId,
    $selectionStore.granularity,
    $selectionStore.boardAlgorithm,
  );
  $: chartSubtitle = buildChartSubtitle(
    $chartStore.status,
    $chartStore.sourceStatus,
    currentBoardBuild,
    $watchStore,
    $selectionStore.granularity,
  );
  $: chartNotice = buildChartNotice($chartStore.status, $chartStore.errorMessage, currentBoardBuild, $watchStore);
  $: chartCanvasState = buildChartCanvasState($chartStore.status, $chartStore.errorMessage, currentBoardBuild);
  $: statusBarText = buildStatusBarText(
    $syncStore.message,
    $syncStore.latestTradeDate,
    summarizeBoardBuilds($appStore.boards, $boardBuildStore),
    $watchStore,
  );

  function buildStatusBarText(
    syncMessage: string,
    latestTradeDate: string | null,
    buildStats: { buildingCount: number; failedCount: number },
    watch: WatchState,
  ): string {
    const buildSummary =
      buildStats.buildingCount > 0
        ? `板块构建中：${buildStats.buildingCount}`
        : buildStats.failedCount > 0
          ? `板块构建失败：${buildStats.failedCount}`
          : '板块状态正常';

    return `同步：${sanitizeUserFacingMessage(syncMessage) || '状态已更新'} · 最新交易日：${latestTradeDate ?? '--'} · ${buildSummary} · ${buildWatchSummary(watch)}`;
  }

  function buildChartViewportKey(
    targetType: 'index' | 'board' | 'symbol',
    targetId: string,
    granularity: 'day' | 'week',
    boardAlgorithm: 'equal_weight_v1' | 'market_cap_weight_v1',
  ): string {
    return targetType === 'board'
      ? `${targetType}:${targetId}:${granularity}:${boardAlgorithm}`
      : `${targetType}:${targetId}:${granularity}`;
  }

  function buildChartSubtitle(
    chartStatus: ChartViewStatus,
    sourceStatus: string,
    boardBuild: BoardBuildStatusPayload | null,
    watch: WatchState,
    granularity: 'day' | 'week',
  ): string {
    if (boardBuild?.buildStatus === 'queued') {
      return '板块已入队，等待后台构建';
    }

    if (boardBuild?.buildStatus === 'running') {
      return '板块后台构建中';
    }

    if (boardBuild?.buildStatus === 'failed') {
      return '板块构建失败';
    }

    if (chartStatus === 'loading') {
      return '正在加载图表';
    }

    if (chartStatus === 'idle') {
      return '等待数据加载';
    }

    if (chartStatus === 'empty') {
      return '暂无图表数据';
    }

    if (chartStatus === 'failed') {
      return '图表状态异常';
    }

    if (granularity === 'day') {
      if (watch.phase === 'active') {
        const sourceLabel = formatSourceStatusLabel(watch.sourceStatus);
        return sourceLabel ? `盘中更新中 · ${sourceLabel}` : '盘中更新中';
      }

      if (watch.phase === 'starting') {
        return '正在建立盘中更新连接';
      }

      if (watch.marketState === 'closed') {
        return '当前非盘中时段';
      }
    }

    return formatSourceStatusLabel(sourceStatus) || '历史价格已就绪';
  }

  function buildChartNotice(
    chartStatus: ChartViewStatus,
    errorMessage: string,
    boardBuild: BoardBuildStatusPayload | null,
    watch: WatchState,
  ): { text: string; tone: 'neutral' | 'warning' | 'danger' } {
    if (boardBuild?.buildStatus === 'queued' || boardBuild?.buildStatus === 'running') {
      return {
        text: boardBuild.buildMessage?.trim() || formatBoardBuildStatus(boardBuild),
        tone: 'warning',
      };
    }

    if (boardBuild?.buildStatus === 'failed') {
      return {
        text: boardBuild.buildMessage?.trim() || '后台构建失败，请检查成分股或稍后重试',
        tone: 'danger',
      };
    }

    if (chartStatus === 'failed' && errorMessage) {
      return {
        text: sanitizeUserFacingMessage(errorMessage),
        tone: 'danger',
      };
    }

    if (watch.phase === 'error' && watch.lastMessage) {
      return {
        text: sanitizeUserFacingMessage(watch.lastMessage),
        tone: 'warning',
      };
    }

    if (watch.marketState === 'closed' && watch.lastMessage) {
      return {
        text: sanitizeUserFacingMessage(watch.lastMessage),
        tone: 'neutral',
      };
    }

    if (chartStatus === 'empty') {
      return {
        text: '当前目标暂无可展示的历史数据',
        tone: 'neutral',
      };
    }

    return {
      text: '',
      tone: 'neutral',
    };
  }

  function buildChartCanvasState(
    chartStatus: ChartViewStatus,
    errorMessage: string,
    boardBuild: BoardBuildStatusPayload | null,
  ): { headline: string; detail: string } {
    if (boardBuild?.buildStatus === 'queued' || boardBuild?.buildStatus === 'running') {
      return {
        headline: '板块后台构建中',
        detail: boardBuild.buildMessage?.trim() || `当前进度：${formatBoardBuildStatus(boardBuild)}`,
      };
    }

    if (boardBuild?.buildStatus === 'failed') {
      return {
        headline: '板块构建失败',
        detail: boardBuild.buildMessage?.trim() || '请调整成分股后重新保存',
      };
    }

    if (chartStatus === 'loading') {
      return {
        headline: '正在加载图表',
        detail: '正在准备所选目标的历史价格与来源说明',
      };
    }

    if (chartStatus === 'idle') {
      return {
        headline: '等待数据加载',
        detail: '正在准备图表数据与同步状态',
      };
    }

    if (chartStatus === 'empty') {
      return {
        headline: '暂无图表数据',
        detail: '等待后端返回历史 bars 或板块构建完成',
      };
    }

    if (chartStatus === 'failed') {
      return {
        headline: '图表状态异常',
        detail: sanitizeUserFacingMessage(errorMessage) || '请稍后重试或检查同步状态',
      };
    }

    return {
      headline: '历史价格走势',
      detail: '支持指数、板块与个股的统一展示',
    };
  }

  function buildWatchSummary(watch: WatchState): string {
    if (watch.phase === 'active') {
      const latestLabel = formatWatchTimestamp(watch.lastEventUpdatedAt ?? watch.updatedAt);
      const sourceLabel = formatSourceStatusLabel(watch.sourceStatus);
      return latestLabel
        ? `盘中更新：${latestLabel}${sourceLabel ? `（${sourceLabel}）` : ''}`
        : '盘中更新：已连接';
    }

    if (watch.phase === 'starting') {
      return '盘中更新：连接中';
    }

    if (watch.phase === 'stopping') {
      return '盘中更新：停止中';
    }

    if (watch.phase === 'error') {
      return `盘中更新：${sanitizeUserFacingMessage(watch.lastMessage) || '暂不可用'}`;
    }

    if (watch.marketState === 'closed') {
      return `盘中更新：${sanitizeUserFacingMessage(watch.lastMessage) || '当前非盘中时段'}`;
    }

    return '盘中更新：未开启';
  }

  function formatWatchTimestamp(updatedAt: string | null): string {
    if (!updatedAt) {
      return '';
    }

    const timestamp = new Date(updatedAt);

    if (Number.isNaN(timestamp.getTime())) {
      return updatedAt;
    }

    return new Intl.DateTimeFormat('zh-CN', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    }).format(timestamp);
  }

  function formatToolbarSyncLabel(updatedAt: string | null): string {
    if (!updatedAt) {
      return '最近同步 --';
    }

    const timestamp = new Date(updatedAt);

    if (Number.isNaN(timestamp.getTime())) {
      return `最近同步 ${updatedAt}`;
    }

    return `最近同步 ${new Intl.DateTimeFormat('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    }).format(timestamp)}`;
  }
</script>

<div class="window-shell">
  <Toolbar
    syncLabel={formatToolbarSyncLabel($syncStore.lastSyncAt)}
    syncDisabled={$syncStore.status === 'first_sync_running' || $syncStore.status === 'incremental_sync_running'}
    on:sync={() => void runManualSyncFlow()}
    on:settings={() => void openSettingsWindowFlow()}
  />
  <IndexSwitcher
    items={$appStore.indexes}
    activeId={$selectionStore.activeIndexId}
    on:select={(event) => void selectIndexFlow(event.detail)}
  />
  <div class="workspace-shell">
    <BoardList items={boardItems} activeBoardId={activeBoardId} on:select={(event) => void selectBoardFlow(event.detail)} />
    <MemberList
      items={memberItems}
      activeSymbol={$selectionStore.activeSymbol}
      on:select={(event) => void selectSymbolFlow(event.detail)}
    />
    <main class="main-panel">
      <ChartHeader
        title={chartTitle}
        subtitle={chartSubtitle}
        sourceHint={chartSourceHint}
        notice={chartNotice.text}
        noticeTone={chartNotice.tone}
        targetType={$selectionStore.targetType}
        activeBoardAlgorithm={$selectionStore.boardAlgorithm}
        activeGranularity={$selectionStore.granularity}
        on:boardAlgorithmChange={(event) => void changeBoardAlgorithmFlow(event.detail)}
        on:granularityChange={(event) => void changeGranularityFlow(event.detail)}
      />
      <ChartCanvas
        bars={$chartStore.bars}
        activeOverlay={$chartStore.activeOverlay}
        status={$chartStore.status}
        headline={chartCanvasState.headline}
        detail={chartCanvasState.detail}
        viewportKey={chartViewportKey}
        initialVisibleBars={$selectionStore.granularity === 'week' ? 26 : 132}
      />
      <TargetNoteEditor value={$noteStore.content} on:save={(event) => void saveCurrentTargetNoteFlow(event.detail)} />
    </main>
  </div>
  <StatusBar text={statusBarText} />
</div>
