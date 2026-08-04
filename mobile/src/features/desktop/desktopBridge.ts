import type { DesktopScreen } from '../../types';
import { isRecord } from '../../webview/bridge';

export type PointerMode = 'touchpad' | 'direct';

export type DesktopViewerScreen = DesktopScreen;

export type DesktopViewerState = {
  ready: boolean;
  connected: boolean;
  status: string;
  statusIsError: boolean;
  viewOnly: boolean;
  controlGranted: boolean;
  autoScale: boolean;
  zoomLevel: number;
  selectedScreenId: string;
  screens: DesktopViewerScreen[];
  pointerMode: PointerMode;
  activeMouseButton: number;
  dragLocked: boolean;
  pressedModifiers: string[];
};

export type DesktopCommand =
  | { type: 'requestState' }
  | { type: 'reconnect'; force: boolean }
  | { type: 'releaseInput' }
  | { type: 'sendText'; text: string }
  | { type: 'selectScreen'; screenId: string }
  | { type: 'toggleScale' }
  | { type: 'zoomOut' }
  | { type: 'zoomIn' }
  | { type: 'toggleModifier'; modifier: string }
  | { type: 'pressKey'; key: string }
  | { type: 'shortcut'; shortcut: string }
  | { type: 'setPointerMode'; mode: PointerMode }
  | { type: 'setMouseButton'; buttonMask: number }
  | { type: 'toggleDragLock' }
  | { type: 'refresh' };

export const DESKTOP_MOUSE_BUTTONS = [
  { label: 'L', mask: 0x1, title: 'Left click' },
  { label: 'M', mask: 0x2, title: 'Middle click' },
  { label: 'R', mask: 0x4, title: 'Right click' },
] as const;

export function initialDesktopViewerState(
  viewOnly: boolean,
  screens: DesktopScreen[],
): DesktopViewerState {
  const normalizedScreens = normalizeDesktopScreens(screens);
  return {
    ready: false,
    connected: false,
    status: 'Connecting',
    statusIsError: false,
    viewOnly,
    controlGranted: false,
    autoScale: true,
    zoomLevel: 1,
    selectedScreenId: preferredScreenId(normalizedScreens),
    screens: normalizedScreens,
    pointerMode: 'touchpad',
    activeMouseButton: 0x1,
    dragLocked: false,
    pressedModifiers: [],
  };
}

export function parseDesktopBridgeMessage(
  value: string,
): { type: 'desktop-state'; state: unknown } | null {
  try {
    const parsed: unknown = JSON.parse(value);
    if (isRecord(parsed) && parsed.type === 'desktop-state') {
      return { type: 'desktop-state', state: parsed.state };
    }
  } catch {
    // Malformed messages are ignored at the trust boundary.
  }
  return null;
}

export function mergeDesktopViewerState(
  current: DesktopViewerState,
  incoming: unknown,
): DesktopViewerState {
  if (!isRecord(incoming)) {
    return current;
  }

  return {
    ...current,
    ready: booleanValue(incoming.ready, current.ready),
    connected: booleanValue(incoming.connected, current.connected),
    status:
      typeof incoming.status === 'string' ? incoming.status : current.status,
    statusIsError: booleanValue(incoming.statusIsError, current.statusIsError),
    viewOnly: booleanValue(incoming.viewOnly, current.viewOnly),
    controlGranted: booleanValue(
      incoming.controlGranted,
      current.controlGranted,
    ),
    autoScale: booleanValue(incoming.autoScale, current.autoScale),
    zoomLevel: finiteNumber(incoming.zoomLevel, current.zoomLevel),
    selectedScreenId:
      typeof incoming.selectedScreenId === 'string'
        ? incoming.selectedScreenId
        : current.selectedScreenId,
    screens: Array.isArray(incoming.screens)
      ? normalizeDesktopScreens(incoming.screens)
      : current.screens,
    pointerMode: incoming.pointerMode === 'direct' ? 'direct' : 'touchpad',
    activeMouseButton: mouseButtonMask(
      incoming.activeMouseButton,
      current.activeMouseButton,
    ),
    dragLocked: booleanValue(incoming.dragLocked, current.dragLocked),
    pressedModifiers: Array.isArray(incoming.pressedModifiers)
      ? incoming.pressedModifiers.filter(
          (modifier): modifier is string => typeof modifier === 'string',
        )
      : current.pressedModifiers,
  };
}

export function normalizeDesktopScreens(value: unknown): DesktopViewerScreen[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((screen, index) => {
      if (!isRecord(screen)) {
        return null;
      }
      const width = finiteNumber(screen.width, 0);
      const height = finiteNumber(screen.height, 0);
      if (width <= 0 || height <= 0) {
        return null;
      }
      return {
        id: stringValue(screen.id, `screen-${index + 1}`),
        label: stringValue(screen.label, String(index + 1)),
        title: stringValue(screen.title, `Screen ${index + 1}`),
        x: finiteNumber(screen.x, 0),
        y: finiteNumber(screen.y, 0),
        width,
        height,
        primary: Boolean(screen.primary),
      };
    })
    .filter((screen): screen is DesktopViewerScreen => Boolean(screen));
}

function preferredScreenId(screens: DesktopViewerScreen[]): string {
  if (screens.length < 2) {
    return 'all';
  }
  return (
    screens.find((screen) => screen.primary)?.id ?? screens[0]?.id ?? 'all'
  );
}

function stringValue(value: unknown, fallback: string): string {
  return typeof value === 'string' && value ? value : fallback;
}

function booleanValue(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback;
}

function finiteNumber(value: unknown, fallback: number): number {
  const number = Number(value);
  return Number.isFinite(number) ? number : fallback;
}

function mouseButtonMask(value: unknown, fallback: number): number {
  const mask = Number(value);
  return DESKTOP_MOUSE_BUTTONS.some((button) => button.mask === mask)
    ? mask
    : fallback;
}
