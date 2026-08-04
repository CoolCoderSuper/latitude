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
    expect(html).not.toContain('cdn.jsdelivr.net');
  });
});
