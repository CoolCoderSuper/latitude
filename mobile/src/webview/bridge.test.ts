import { commandInjectionScript } from './bridge';

describe('commandInjectionScript', () => {
  it('serializes command data without making it executable source', () => {
    const script = commandInjectionScript('latitudeBridge', {
      type: 'sendText',
      text: `'); window.compromised = true; ('`,
    });

    expect(script).toContain('window.latitudeBridge');
    expect(script).toContain(
      JSON.stringify(`'); window.compromised = true; ('`),
    );
  });

  it('rejects invalid handler names', () => {
    expect(() => commandInjectionScript('bridge();evil', {})).toThrow(
      'Invalid WebView bridge handler name.',
    );
  });
});
