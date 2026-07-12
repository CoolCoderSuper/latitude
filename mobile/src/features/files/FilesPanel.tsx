import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from 'react';
import {
  BackHandler,
  FlatList,
  Pressable,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import WebView from 'react-native-webview';

import type { LatitudePublicApi } from '../../api';
import { absoluteUrl, authHeaders } from '../../api';
import { EmptyState, InlineNotice, LoadingBlock } from '../../components/ui';
import { useTheme } from '../../theme';
import type { ProjectFileEntry, SessionRecord } from '../../types';
import { errorMessage } from '../../utils/errors';

export type FilesPanelHandle = { goBack: () => void };

export const FilesPanel = forwardRef<FilesPanelHandle, {
  active: boolean;
  api: LatitudePublicApi;
  onFolderNavigationChange: (canGoBack: boolean) => void;
  projectName: string;
  session: SessionRecord;
}>(function FilesPanel(
  { active, api, onFolderNavigationChange, projectName, session },
  ref,
) {
  const { colors, mode, styles } = useTheme();
  const webViewRef = useRef<WebView>(null);
  const [path, setPath] = useState('');
  const [entries, setEntries] = useState<ProjectFileEntry[]>([]);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const nativeStyles = useMemo(
    () => createNativeStyles(colors),
    [colors],
  );
  const editorScript = useMemo(
    () => editorOnlyScript(mode, session.token),
    [mode, session.token],
  );

  useEffect(() => {
    webViewRef.current?.injectJavaScript(editorScript);
  }, [editorScript]);

  const loadFolder = useCallback(async (nextPath: string) => {
    setLoading(true);
    setError(null);
    try {
      const response = await api.files(projectName, nextPath);
      setPath(response.path);
      setEntries(response.entries);
    } catch (loadError) {
      setError(errorMessage(loadError));
    } finally {
      setLoading(false);
    }
  }, [api, projectName]);

  useEffect(() => { void loadFolder(''); }, [loadFolder]);

  const goBack = useCallback(() => {
    if (selectedFile) {
      setSelectedFile(null);
      return;
    }
    if (!path) return;
    const parts = path.split('/').filter(Boolean);
    parts.pop();
    void loadFolder(parts.join('/'));
  }, [loadFolder, path, selectedFile]);

  useImperativeHandle(ref, () => ({ goBack }), [goBack]);

  const canGoBack = Boolean(selectedFile || path);
  useEffect(() => {
    onFolderNavigationChange(canGoBack);
  }, [canGoBack, onFolderNavigationChange]);

  useEffect(() => {
    if (!active || !canGoBack) return;
    const subscription = BackHandler.addEventListener('hardwareBackPress', () => {
      goBack();
      return true;
    });
    return () => subscription.remove();
  }, [active, canGoBack, goBack]);

  if (selectedFile) {
    const uri = absoluteUrl(
      session.baseUrl,
      `/${encodeURIComponent(projectName)}/_files?path=${encodeURIComponent(selectedFile)}`,
    );
    return (
      <WebView
        key={selectedFile}
        ref={webViewRef}
        injectedJavaScript={editorScript}
        injectedJavaScriptBeforeContentLoaded={editorScript}
        javaScriptEnabled
        originWhitelist={['http://*', 'https://*']}
        sharedCookiesEnabled
        source={{
          uri,
          headers: {
            ...authHeaders(session.token),
            'X-Latitude-Theme': mode,
          },
        }}
        startInLoadingState
        style={[styles.webView, { backgroundColor: colors.background }]}
      />
    );
  }

  if (loading && entries.length === 0) {
    return <View style={styles.screenContent}><LoadingBlock label="Loading files" /></View>;
  }

  return (
    <View style={nativeStyles.container}>
      <View style={nativeStyles.locationBar}>
        <Text numberOfLines={1} style={nativeStyles.locationText}>
          {path || 'Project files'}
        </Text>
      </View>
      {error ? <View style={nativeStyles.notice}><InlineNotice text={error} tone="error" /></View> : null}
      <FlatList
        contentContainerStyle={entries.length === 0 ? nativeStyles.emptyList : nativeStyles.list}
        data={entries}
        keyExtractor={(item) => item.path}
        renderItem={({ item }) => (
          <Pressable
            onPress={() => {
              if (item.kind === 'directory') void loadFolder(item.path);
              else setSelectedFile(item.path);
            }}
            style={({ pressed }) => [nativeStyles.row, pressed && styles.pressed]}
          >
            <Text numberOfLines={1} style={nativeStyles.rowText}>{item.name}</Text>
          </Pressable>
        )}
        ListEmptyComponent={<EmptyState title="This folder is empty" />}
      />
    </View>
  );
});

function editorOnlyScript(mode: 'light' | 'dark', token: string) {
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
        .editor-host .cm-scroller {
          padding-top:0 !important;
          overscroll-behavior:contain;
          -webkit-overflow-scrolling:touch;
          touch-action:pan-x pan-y;
        }
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

function createNativeStyles(colors: ReturnType<typeof useTheme>['colors']) {
  return StyleSheet.create({
    container: { flex: 1, backgroundColor: colors.background },
    locationBar: {
      minHeight: 44,
      justifyContent: 'center',
      borderBottomWidth: 1,
      borderBottomColor: colors.border,
      paddingHorizontal: 14,
      backgroundColor: colors.panel,
    },
    locationText: { color: colors.text, fontSize: 13, fontWeight: '900' },
    notice: { padding: 10 },
    list: { padding: 8, gap: 4 },
    emptyList: { flexGrow: 1, justifyContent: 'center', padding: 14 },
    row: {
      minHeight: 46,
      justifyContent: 'center',
      borderBottomWidth: StyleSheet.hairlineWidth,
      borderBottomColor: colors.border,
      paddingHorizontal: 12,
      backgroundColor: colors.surface,
    },
    rowText: { color: colors.text, fontSize: 15, fontWeight: '700' },
  });
}
