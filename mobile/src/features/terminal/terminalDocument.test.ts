import { lightColors } from '../../ui/theme/tokens';
import { terminalDocument, terminalDocumentTheme } from './terminalDocument';

describe('terminalDocument', () => {
  it('loads terminal assets from the connected Latitude server', () => {
    const html = terminalDocument(
      'Demo terminal',
      'ws://latitude.local/project/terminal/ws',
      terminalDocumentTheme('light', lightColors),
      'http://latitude.local:8080',
    );

    expect(html).toContain(
      'http://latitude.local:8080/__latitude/assets/terminal-viewer.bundle.css',
    );
    expect(html).toContain(
      'http://latitude.local:8080/__latitude/assets/terminal-viewer.bundle.js',
    );
    expect(html).toContain("nextSocket.binaryType = 'arraybuffer'");
    expect(html).toContain('window.LatitudeTerminalStream');
    expect(html).toContain('outputWriter.acceptFrame(event.data, nextSocket)');
    expect(html).toContain("type: 'hello'");
    expect(html).toContain("type: 'ack'");
    expect(html).toContain(
      '<div id="terminal-frame"><div id="terminal"></div></div>',
    );
    expect(html).not.toContain("querySelector('.xterm-screen')");
    expect(html).toContain('new ResizeObserver(queueResize)');
    expect(html).toContain('new window.WebglAddon.WebglAddon()');
    expect(html).toContain(
      'scheduleReconnect(event.code === 1013 || event.code === 4001)',
    );
    expect(html).not.toContain('event.data.text()');
    expect(html).not.toContain('cdn.jsdelivr.net');
  });
});
