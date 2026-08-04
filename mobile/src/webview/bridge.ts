export function commandInjectionScript(
  handlerName: string,
  payload: unknown,
): string {
  if (!/^[A-Za-z_$][A-Za-z0-9_$]*$/.test(handlerName)) {
    throw new Error('Invalid WebView bridge handler name.');
  }
  return `window.${handlerName} && window.${handlerName}(${JSON.stringify(payload)}); true;`;
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === 'object' && !Array.isArray(value));
}
