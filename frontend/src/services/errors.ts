export function formatErrorMessage(error: unknown, fallback = '操作失败'): string {
  const message = extractErrorMessage(error);
  return message?.trim() ? message : fallback;
}

export function createCommandError(command: string, error: unknown): Error {
  return new Error(`命令 ${command} 调用失败：${formatErrorMessage(error, '未知错误')}`);
}

function extractErrorMessage(error: unknown, depth = 0): string | null {
  if (depth > 3) {
    return null;
  }

  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }

  if (typeof error === 'string' && error.trim()) {
    return error;
  }

  if (!error || typeof error !== 'object') {
    return null;
  }

  const record = error as Record<string, unknown>;

  for (const key of ['message', 'Message', 'error', 'Error', 'reason', 'Reason', 'cause']) {
    const value = record[key];
    const message = extractErrorMessage(value, depth + 1);

    if (message?.trim()) {
      return message;
    }
  }

  return null;
}
