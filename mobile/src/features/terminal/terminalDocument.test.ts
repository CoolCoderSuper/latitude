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
    expect(html).toContain('terminal.write(new Uint8Array(event.data))');
    expect(html).toContain('new ResizeObserver(queueResize)');
    expect(html).toContain('new window.WebglAddon.WebglAddon()');
    expect(html).toContain('scheduleReconnect(event.code === 1013)');
    expect(html).not.toContain('event.data.text()');
    expect(html).not.toContain('cdn.jsdelivr.net');
  });
});
