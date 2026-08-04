import { Plus, Terminal as TerminalIcon, X } from 'lucide-react-native';
import { useEffect, useMemo, useRef } from 'react';
import { AppState, Pressable, ScrollView, Text, View } from 'react-native';
import WebView from 'react-native-webview';

import type { LatitudePublicApi } from '../../api';
import { normalizeBaseUrl } from '../../api';
import { EmptyState, InlineNotice, LoadingBlock } from '../../components/ui';
import { useTheme } from '../../theme';
import type {
  ProjectDetail,
  RootTerminalLink,
  SessionRecord,
  TerminalSessionSummary,
} from '../../types';
import {
  terminalDocument,
  terminalDocumentTheme,
  terminalThemeInjectionScript,
} from './terminalDocument';
import { authenticatedWebSocketUrl } from '../../webview/urls';
import {
  useTerminalSessions,
  type TerminalTarget,
} from './useTerminalSessions';
import { createTerminalStyles } from './terminalStyles';

export function TerminalPanel({
  api,
  project,
  session,
}: {
  api: LatitudePublicApi;
  project: ProjectDetail;
  session: SessionRecord;
}) {
  const target = useMemo(
    () => ({
      title: project.name,
      terminalHref: project.terminal.href,
      listSessions: () => api.terminalSessions(project.name),
      createSession: () => api.createTerminalSession(project.name),
      closeSession: (sessionId: string) =>
        api.closeTerminalSession(project.name, sessionId),
    }),
    [api, project.name, project.terminal.href],
  );

  return <TerminalWorkspace session={session} target={target} />;
}

export function RootTerminalPanel({
  api,
  rootTerminal,
  session,
}: {
  api: LatitudePublicApi;
  rootTerminal: RootTerminalLink;
  session: SessionRecord;
}) {
  const target = useMemo(
    () => ({
      title: rootTerminal.label,
      terminalHref: rootTerminal.href,
      listSessions: () => api.rootTerminalSessions(),
      createSession: () => api.createRootTerminalSession(),
      closeSession: (sessionId: string) =>
        api.closeRootTerminalSession(sessionId),
    }),
    [api, rootTerminal.href, rootTerminal.label],
  );

  return <TerminalWorkspace session={session} target={target} />;
}

function TerminalWorkspace({
  session,
  target,
}: {
  session: SessionRecord;
  target: TerminalTarget;
}) {
  const { colors, styles } = useTheme();
  const terminalStyles = useMemo(() => createTerminalStyles(colors), [colors]);
  const {
    activeSessionId,
    closeSession,
    closingSessionId,
    createSession,
    creating,
    loading,
    notice,
    sessions,
    setActiveSessionId,
  } = useTerminalSessions(target);

  return (
    <View style={terminalStyles.panel}>
      <View style={terminalStyles.sessionBar}>
        <ScrollView
          horizontal
          contentContainerStyle={terminalStyles.sessionList}
          showsHorizontalScrollIndicator={false}
        >
          {sessions.map((terminalSession) => {
            const active = terminalSession.id === activeSessionId;
            return (
              <View key={terminalSession.id} style={terminalStyles.sessionItem}>
                <Pressable
                  onPress={() => setActiveSessionId(terminalSession.id)}
                  style={({ pressed }) => [
                    terminalStyles.sessionChip,
                    active && terminalStyles.sessionChipActive,
                    pressed && styles.pressed,
                  ]}
                >
                  <TerminalIcon
                    color={active ? colors.onAccent : colors.text}
                    size={15}
                  />
                  <Text
                    numberOfLines={1}
                    style={[
                      terminalStyles.sessionText,
                      active && terminalStyles.sessionTextActive,
                    ]}
                  >
                    {terminalSession.title}
                  </Text>
                </Pressable>
                <Pressable
                  accessibilityLabel={`Close ${terminalSession.title}`}
                  disabled={closingSessionId === terminalSession.id}
                  onPress={() => {
                    void closeSession(terminalSession.id);
                  }}
                  style={({ pressed }) => [
                    terminalStyles.sessionClose,
                    pressed && styles.pressed,
                  ]}
                >
                  <X color={colors.muted} size={14} />
                </Pressable>
              </View>
            );
          })}
        </ScrollView>
        <Pressable
          accessibilityLabel="New terminal"
          disabled={creating}
          onPress={() => {
            void createSession();
          }}
          style={({ pressed }) => [
            terminalStyles.newButton,
            pressed && styles.pressed,
          ]}
        >
          <Plus color={colors.onAccent} size={18} />
        </Pressable>
      </View>

      {notice && <InlineNotice tone="error" text={notice} />}

      <View style={terminalStyles.stack}>
        {loading ? (
          <LoadingBlock label="Loading terminals" />
        ) : sessions.length === 0 ? (
          <EmptyState title="No terminals" />
        ) : (
          sessions.map((terminalSession) => (
            <TerminalSessionView
              key={terminalSession.id}
              active={terminalSession.id === activeSessionId}
              baseUrl={session.baseUrl}
              session={terminalSession}
              terminalHref={target.terminalHref}
              terminalTitle={target.title}
              token={session.token}
            />
          ))
        )}
      </View>
    </View>
  );
}

function TerminalSessionView({
  active,
  baseUrl,
  session,
  terminalHref,
  terminalTitle,
  token,
}: {
  active: boolean;
  baseUrl: string;
  session: TerminalSessionSummary;
  terminalHref: string;
  terminalTitle: string;
  token: string;
}) {
  const { colors, mode, styles } = useTheme();
  const terminalStyles = useMemo(() => createTerminalStyles(colors), [colors]);
  const webViewRef = useRef<WebView>(null);
  const terminalTheme = useMemo(
    () => terminalDocumentTheme(mode, colors),
    [colors, mode],
  );
  const initialThemeRef = useRef(terminalTheme);
  const terminalUrl = useMemo(
    () =>
      authenticatedWebSocketUrl({
        baseUrl,
        href: terminalHref,
        parameters: { session: session.id },
        token,
      }),
    [baseUrl, terminalHref, session.id, token],
  );
  const terminalHtml = useMemo(
    () =>
      terminalDocument(
        `${terminalTitle} - ${session.title}`,
        terminalUrl,
        initialThemeRef.current,
        normalizeBaseUrl(baseUrl),
      ),
    [baseUrl, session.title, terminalTitle, terminalUrl],
  );
  const terminalThemeScript = useMemo(
    () => terminalThemeInjectionScript(terminalTheme),
    [terminalTheme],
  );
  const terminalSource = useMemo(
    () => ({ html: terminalHtml, baseUrl: normalizeBaseUrl(baseUrl) }),
    [baseUrl, terminalHtml],
  );

  useEffect(() => {
    webViewRef.current?.injectJavaScript(terminalThemeScript);
  }, [terminalThemeScript]);

  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => {
      if (state === 'active') {
        webViewRef.current?.injectJavaScript(
          'window.latitudeReconnect && window.latitudeReconnect(true); true;',
        );
      }
    });
    return () => subscription.remove();
  }, []);

  return (
    <View
      pointerEvents={active ? 'auto' : 'none'}
      style={[terminalStyles.frame, active && terminalStyles.frameActive]}
    >
      <WebView
        ref={webViewRef}
        domStorageEnabled
        injectedJavaScript={terminalThemeScript}
        injectedJavaScriptBeforeContentLoaded={terminalThemeScript}
        javaScriptEnabled
        keyboardDisplayRequiresUserAction={false}
        mixedContentMode="always"
        originWhitelist={['*']}
        setSupportMultipleWindows={false}
        source={terminalSource}
        startInLoadingState
        style={styles.webView}
      />
    </View>
  );
}
