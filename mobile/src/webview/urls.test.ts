import { authenticatedWebSocketUrl, withRawMedia } from './urls';

describe('WebView URLs', () => {
  it('builds secure authenticated WebSocket URLs with encoded parameters', () => {
    expect(
      authenticatedWebSocketUrl({
        baseUrl: 'https://latitude.example/base',
        href: '/_terminal/',
        parameters: { session: 'session with spaces' },
        token: 'secret/token',
      }),
    ).toBe(
      'wss://latitude.example/_terminal/ws?token=secret%2Ftoken&session=session+with+spaces',
    );
  });

  it('adds the raw media parameter without dropping existing parameters', () => {
    expect(withRawMedia('https://latitude.example/video?download=1')).toBe(
      'https://latitude.example/video?download=1&raw=1',
    );
  });
});
