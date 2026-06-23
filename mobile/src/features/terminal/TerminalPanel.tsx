import {
  Plus,
  Terminal as TerminalIcon,
  X,
} from 'lucide-react-native';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AppState, Pressable, ScrollView, Text, View } from 'react-native';
import WebView from 'react-native-webview';

import type { LatitudePublicApi } from '../../api';
import { normalizeBaseUrl } from '../../api';
import { EmptyState, InlineNotice, LoadingBlock } from '../../components/ui';
import { useTheme } from '../../theme';
import type {
  ProjectDetail,
  SessionRecord,
  TerminalSessionSummary,
} from '../../types';
import { errorMessage } from '../../utils/errors';
import { terminalDocument } from './terminalDocument';

export function TerminalPanel({
  api,
  project,
  session,
}: {
  api: LatitudePublicApi;
  project: ProjectDetail;
  session: SessionRecord;
}) {
  const { colors, styles } = useTheme();
  const [sessions, setSessions] = useState<TerminalSessionSummary[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [closingSessionId, setClosingSessionId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const loadSessions = useCallback(async () => {
    setLoading(true);
    setNotice(null);
    try {
      let nextSessions = (await api.terminalSessions(project.name)).sessions;
      if (nextSessions.length === 0) {
        nextSessions = [await api.createTerminalSession(project.name)];
      }
      setSessions(nextSessions);
      setActiveSessionId((current) =>
        nextSessions.some((item) => item.id === current)
          ? current
          : nextSessions[0]?.id ?? null,
      );
    } catch (sessionError) {
      setNotice(errorMessage(sessionError));
    } finally {
      setLoading(false);
    }
  }, [api, project.name]);

  useEffect(() => {
    void loadSessions();
  }, [loadSessions]);

  const createSession = useCallback(async () => {
    if (creating) {
      return;
    }

    setCreating(true);
    setNotice(null);
    try {
      const created = await api.createTerminalSession(project.name);
      setSessions((current) => [...current, created]);
      setActiveSessionId(created.id);
    } catch (sessionError) {
      setNotice(errorMessage(sessionError));
    } finally {
      setCreating(false);
    }
  }, [api, creating, project.name]);

  const closeSession = useCallback(
    async (sessionId: string) => {
      if (closingSessionId) {
        return;
      }

      setClosingSessionId(sessionId);
      setNotice(null);
      try {
        await api.closeTerminalSession(project.name, sessionId);
        setSessions((current) => {
          const next = current.filter((item) => item.id !== sessionId);
          setActiveSessionId((active) =>
            active === sessionId ? next[0]?.id ?? null : active,
          );
          return next;
        });
      } catch (sessionError) {
        setNotice(errorMessage(sessionError));
      } finally {
        setClosingSessionId(null);
      }
    },
    [api, closingSessionId, project.name],
  );

  return (
    <View style={styles.terminalPanel}>
      <View style={styles.terminalSessionBar}>
        <ScrollView
          horizontal
          contentContainerStyle={styles.terminalSessionList}
          showsHorizontalScrollIndicator={false}
        >
          {sessions.map((terminalSession) => {
            const active = terminalSession.id === activeSessionId;
            return (
              <View key={terminalSession.id} style={styles.terminalSessionItem}>
                <Pressable
                  onPress={() => setActiveSessionId(terminalSession.id)}
                  style={({ pressed }) => [
                    styles.terminalSessionChip,
                    active && styles.terminalSessionChipActive,
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
                      styles.terminalSessionText,
                      active && styles.terminalSessionTextActive,
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
                    styles.terminalSessionClose,
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
            styles.terminalNewButton,
            pressed && styles.pressed,
          ]}
        >
          <Plus color={colors.onAccent} size={18} />
        </Pressable>
      </View>

      {notice && <InlineNotice tone="error" text={notice} />}

      <View style={styles.terminalStack}>
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
              project={project}
              session={terminalSession}
              token={session.token}
            />
          ))
        )}
      </View>
    </View>
  );
}

function terminalWebSocketUrl(
  baseUrl: string,
  terminalHref: string,
  token: string,
  sessionId: string,
): string {
  const cleanHref = terminalHref.replace(/\/+$/, '');
  const url = new URL(`${cleanHref}/ws`, `${normalizeBaseUrl(baseUrl)}/`);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  url.searchParams.set('token', token);
  url.searchParams.set('session', sessionId);
  return url.toString();
}

function TerminalSessionView({
  active,
  baseUrl,
  project,
  session,
  token,
}: {
  active: boolean;
  baseUrl: string;
  project: ProjectDetail;
  session: TerminalSessionSummary;
  token: string;
}) {
  const { styles } = useTheme();
  const webViewRef = useRef<WebView>(null);
  const terminalUrl = useMemo(
    () => terminalWebSocketUrl(baseUrl, project.terminal.href, token, session.id),
    [baseUrl, project.terminal.href, session.id, token],
  );
  const terminalHtml = useMemo(
    () => terminalDocument(`${project.name} - ${session.title}`, terminalUrl),
    [project.name, session.title, terminalUrl],
  );

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
      style={[styles.terminalFrame, active && styles.terminalFrameActive]}
    >
      <WebView
        ref={webViewRef}
        domStorageEnabled
        javaScriptEnabled
        keyboardDisplayRequiresUserAction={false}
        mixedContentMode="always"
        originWhitelist={['*']}
        setSupportMultipleWindows={false}
        source={{ html: terminalHtml, baseUrl: normalizeBaseUrl(baseUrl) }}
        startInLoadingState
        style={styles.webView}
      />
    </View>
  );
}
