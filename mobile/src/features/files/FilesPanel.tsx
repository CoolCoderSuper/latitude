import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from 'react';
import { Search } from 'lucide-react-native';
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
import {
  EmptyState,
  IconButton,
  InlineNotice,
  LoadingBlock,
} from '../../components/ui';
import { useTheme } from '../../theme';
import type { SessionRecord } from '../../types';
import { editorOnlyScript } from './editorBridge';
import { FileSearchPanel } from './FileSearchPanel';
import { useFileBrowser } from './useFileBrowser';

export type FilesPanelHandle = { goBack: () => void };

export const FilesPanel = forwardRef<
  FilesPanelHandle,
  {
    active: boolean;
    api: LatitudePublicApi;
    onFolderNavigationChange: (canGoBack: boolean) => void;
    projectName: string;
    session: SessionRecord;
  }
>(function FilesPanel(
  { active, api, onFolderNavigationChange, projectName, session },
  ref,
) {
  const { colors, mode, styles } = useTheme();
  const webViewRef = useRef<WebView>(null);
  const {
    canGoBack,
    entries,
    error,
    goBack,
    loading,
    openFile,
    path,
    selectEntry,
    selectedFile,
    selectedLine,
    selectedColumn,
  } = useFileBrowser({ api, projectName });
  const [searchOpen, setSearchOpen] = useState(false);
  const nativeStyles = useMemo(() => createNativeStyles(colors), [colors]);
  const editorScript = useMemo(
    () => editorOnlyScript(mode, session.token),
    [mode, session.token],
  );

  useEffect(() => {
    webViewRef.current?.injectJavaScript(editorScript);
  }, [editorScript]);

  const canNavigateBack = searchOpen || canGoBack;
  const navigateBack = useCallback(() => {
    if (searchOpen) {
      setSearchOpen(false);
      return;
    }
    goBack();
  }, [goBack, searchOpen]);

  useImperativeHandle(ref, () => ({ goBack: navigateBack }), [navigateBack]);

  useEffect(() => {
    onFolderNavigationChange(canNavigateBack);
  }, [canNavigateBack, onFolderNavigationChange]);

  useEffect(() => {
    if (!active || !canNavigateBack) return;
    const subscription = BackHandler.addEventListener(
      'hardwareBackPress',
      () => {
        navigateBack();
        return true;
      },
    );
    return () => subscription.remove();
  }, [active, canNavigateBack, navigateBack]);

  if (searchOpen) {
    return (
      <FileSearchPanel
        api={api}
        onClose={() => setSearchOpen(false)}
        onOpenFile={(result) => {
          setSearchOpen(false);
          openFile(result.path, result.line, result.column);
        }}
        projectName={projectName}
      />
    );
  }

  if (selectedFile) {
    const locationQuery = new URLSearchParams({ path: selectedFile });
    if (selectedLine) locationQuery.set('line', String(selectedLine));
    if (selectedColumn) locationQuery.set('column', String(selectedColumn));
    const uri = absoluteUrl(
      session.baseUrl,
      `/${encodeURIComponent(projectName)}/_files?${locationQuery.toString()}`,
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
    return (
      <View style={styles.screenContent}>
        <LoadingBlock label="Loading files" />
      </View>
    );
  }

  return (
    <View style={nativeStyles.container}>
      <View style={nativeStyles.locationBar}>
        <Text numberOfLines={1} style={nativeStyles.locationText}>
          {path || 'Project files'}
        </Text>
        <IconButton
          accessibilityLabel="Search project files"
          icon={<Search color={colors.accent} size={20} />}
          onPress={() => setSearchOpen(true)}
        />
      </View>
      {error ? (
        <View style={nativeStyles.notice}>
          <InlineNotice text={error} tone="error" />
        </View>
      ) : null}
      <FlatList
        contentContainerStyle={
          entries.length === 0 ? nativeStyles.emptyList : nativeStyles.list
        }
        data={entries}
        keyExtractor={(item) => item.path}
        renderItem={({ item }) => (
          <Pressable
            onPress={() => {
              selectEntry(item);
            }}
            style={({ pressed }) => [
              nativeStyles.row,
              pressed && styles.pressed,
            ]}
          >
            <Text numberOfLines={1} style={nativeStyles.rowText}>
              {item.name}
            </Text>
          </Pressable>
        )}
        ListEmptyComponent={<EmptyState title="This folder is empty" />}
      />
    </View>
  );
});

function createNativeStyles(colors: ReturnType<typeof useTheme>['colors']) {
  return StyleSheet.create({
    container: { flex: 1, backgroundColor: colors.background },
    locationBar: {
      minHeight: 44,
      flexDirection: 'row',
      alignItems: 'center',
      justifyContent: 'center',
      borderBottomWidth: 1,
      borderBottomColor: colors.border,
      paddingHorizontal: 14,
      backgroundColor: colors.panel,
    },
    locationText: {
      flex: 1,
      color: colors.text,
      fontSize: 13,
      fontWeight: '900',
    },
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
