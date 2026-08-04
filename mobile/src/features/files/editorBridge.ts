import type { ThemeMode } from '../../theme';

export function editorOnlyScript(mode: ThemeMode, token: string): string {
  return `
    (() => {
      document.documentElement.dataset.latitudeTheme = ${JSON.stringify(mode)};
      document.documentElement.style.colorScheme = ${JSON.stringify(mode)};
      document.cookie = 'latitude_public_session=' + ${JSON.stringify(token)} + '; Path=/; SameSite=Lax';
      if (!window.__latitudeNativeFetch) {
        window.__latitudeNativeFetch = window.fetch.bind(window);
        window.fetch = (input, init = {}) => {
          const target = new URL(typeof input === 'string' ? input : input.url, location.href);
          if (target.origin !== location.origin) return window.__latitudeNativeFetch(input, init);
          const headers = new Headers(init.headers || {});
          headers.set('Authorization', 'Bearer ' + ${JSON.stringify(token)});
          return window.__latitudeNativeFetch(input, { ...init, headers });
        };
      }
      let style = document.getElementById('latitude-mobile-editor');
      if (!style) { style = document.createElement('style'); style.id = 'latitude-mobile-editor'; document.head.appendChild(style); }
      style.textContent = \`
        .files-header, .latitude-theme-toggle, .file-sidebar, .file-resizer { display:none !important; }
        html, body { height:var(--mobile-editor-height, 100dvh) !important; overflow:hidden !important; }
        .files-page { display:block !important; height:var(--mobile-editor-height, 100dvh) !important; padding:0 !important; }
        .file-workspace { display:block !important; height:var(--mobile-editor-height, 100dvh) !important; border:0 !important; border-radius:0 !important; }
        .file-main { display:flex !important; height:var(--mobile-editor-height, 100dvh) !important; }
        .file-preview, .editor-host { height:100% !important; }
        .file-actions { top:8px !important; right:8px !important; }
        .file-actions span { display:none !important; }
        .file-actions button { min-width:64px; min-height:40px !important; padding:0 12px !important; border-radius:8px !important; opacity:.94; }
        .file-actions button:disabled { display:none !important; }
        .editor-host .cm-editor { height:100% !important; font-size:16px !important; line-height:1.55 !important; }
        .editor-host .cm-scroller { padding-top:0 !important; overscroll-behavior:contain; -webkit-overflow-scrolling:touch; touch-action:pan-x pan-y; }
        .editor-host .cm-content { padding:8px 0 28px !important; caret-color:var(--files-accent); }
        .editor-host .cm-line { padding:0 10px !important; }
        .editor-host .cm-gutters { min-width:42px; font-size:12px; }
        .editor-host .cm-lineNumbers .cm-gutterElement { min-width:34px; padding:0 7px 0 4px; }
        .editor-host .cm-cursor { border-left-width:2px !important; border-left-color:var(--files-accent) !important; }
        .editor-host .cm-selectionBackground { border-radius:2px; }
        .media-preview { height:var(--mobile-editor-height, 100dvh) !important; padding:12px !important; }
      \`;
      const updateEditorViewport = () => {
        const viewport = window.visualViewport;
        const height = viewport ? viewport.height : window.innerHeight;
        document.documentElement.style.setProperty('--mobile-editor-height', height + 'px');
        requestAnimationFrame(() => {
          const selection = window.getSelection();
          if (!selection || selection.rangeCount === 0) return;
          const rect = selection.getRangeAt(0).getBoundingClientRect();
          const scroller = document.querySelector('.cm-scroller');
          if (scroller && rect.bottom > height - 20) scroller.scrollTop += rect.bottom - height + 36;
          else if (scroller && rect.top < 8) scroller.scrollTop += rect.top - 16;
        });
      };
      window.visualViewport?.addEventListener('resize', updateEditorViewport);
      window.visualViewport?.addEventListener('scroll', updateEditorViewport);
      document.addEventListener('selectionchange', () => requestAnimationFrame(updateEditorViewport));
      updateEditorViewport();
    })();
    true;
  `;
}
