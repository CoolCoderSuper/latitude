import {
  initialDesktopViewerState,
  mergeDesktopViewerState,
  normalizeDesktopScreens,
  parseDesktopBridgeMessage,
} from './desktopBridge';

describe('desktopBridge', () => {
  it('accepts only the expected bridge event', () => {
    expect(parseDesktopBridgeMessage('not json')).toBeNull();
    expect(parseDesktopBridgeMessage('{"type":"unexpected"}')).toBeNull();
    expect(
      parseDesktopBridgeMessage(
        '{"type":"desktop-state","state":{"ready":true}}',
      ),
    ).toEqual({ type: 'desktop-state', state: { ready: true } });
  });

  it('normalizes screens and ignores invalid geometry', () => {
    expect(
      normalizeDesktopScreens([
        { id: 'bad', width: 0, height: 100 },
        { id: 'main', width: 1920, height: 1080, primary: true },
      ]),
    ).toEqual([
      {
        id: 'main',
        label: '2',
        title: 'Screen 2',
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        primary: true,
      },
    ]);
  });

  it('merges valid state fields while preserving invalid values', () => {
    const current = initialDesktopViewerState(false, []);
    const next = mergeDesktopViewerState(current, {
      ready: true,
      zoomLevel: 'bad',
      pointerMode: 'direct',
      activeMouseButton: 99,
      pressedModifiers: ['control', 42],
    });

    expect(next.ready).toBe(true);
    expect(next.zoomLevel).toBe(1);
    expect(next.pointerMode).toBe('direct');
    expect(next.activeMouseButton).toBe(1);
    expect(next.pressedModifiers).toEqual(['control']);
  });
});
