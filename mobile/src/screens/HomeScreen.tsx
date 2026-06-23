import {
  ChevronRight,
  FolderOpen,
  Server,
  Terminal as TerminalIcon,
} from 'lucide-react-native';
import { useCallback, useMemo } from 'react';
import { PanResponder, Pressable, ScrollView, Text, View } from 'react-native';

import {
  EmptyState,
  IconButton,
  InlineNotice,
  LoadingBlock,
  ScreenHeader,
} from '../components/ui';
import { useRefreshControl, useTheme } from '../theme';
import type { ProjectSummary, RootTerminalLink, SessionRecord } from '../types';
import { prependDeviceHostname } from '../utils/headers';

export function HomeScreen({
  baseUrl,
  deviceHostname,
  error,
  loading,
  onManageServers,
  onOpenProject,
  onOpenRootTerminal,
  onRefresh,
  onSwitchServer,
  projects,
  rootTerminal,
  serverSessions,
}: {
  baseUrl: string;
  deviceHostname?: string;
  error: string | null;
  loading: boolean;
  onManageServers: () => void;
  onOpenProject: (name: string) => void;
  onOpenRootTerminal: () => void;
  onRefresh: () => void | Promise<void>;
  onSwitchServer: (baseUrl: string) => void | Promise<void>;
  projects: ProjectSummary[];
  rootTerminal: RootTerminalLink;
  serverSessions: SessionRecord[];
}) {
  const { colors, styles } = useTheme();
  const refreshControl = useRefreshControl(loading, onRefresh);

  const switchAdjacentServer = useCallback(
    (direction: number) => {
      const currentIndex = serverSessions.findIndex(
        (serverSession) => serverSession.baseUrl === baseUrl,
      );
      const nextSession = serverSessions[currentIndex + direction];
      if (nextSession) {
        void onSwitchServer(nextSession.baseUrl);
      }
    },
    [baseUrl, onSwitchServer, serverSessions],
  );

  const serverSwipeResponder = useMemo(
    () =>
      PanResponder.create({
        onMoveShouldSetPanResponder: (_event, gesture) => {
          if (serverSessions.length < 2) {
            return false;
          }

          const absDx = Math.abs(gesture.dx);
          const absDy = Math.abs(gesture.dy);
          return absDx > 22 && absDx > absDy * 1.45;
        },
        onPanResponderRelease: (_event, gesture) => {
          const absDx = Math.abs(gesture.dx);
          const absDy = Math.abs(gesture.dy);
          const deliberateSwipe =
            (absDx > 76 || Math.abs(gesture.vx) > 0.5) &&
            absDx > absDy * 1.25;

          if (!deliberateSwipe) {
            return;
          }

          switchAdjacentServer(gesture.dx < 0 ? 1 : -1);
        },
        onPanResponderTerminationRequest: () => true,
      }),
    [serverSessions.length, switchAdjacentServer],
  );

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={prependDeviceHostname(baseUrl, deviceHostname)}
        right={
          <>
            <IconButton
              accessibilityLabel="Manage servers"
              icon={<Server color={colors.text} size={20} />}
              onPress={onManageServers}
            />
          </>
        }
        title="Projects"
      />
      <ScrollView
        {...serverSwipeResponder.panHandlers}
        contentContainerStyle={styles.screenContent}
        refreshControl={refreshControl}
      >
        {serverSessions.length > 1 && (
          <ScrollView
            horizontal
            contentContainerStyle={styles.serverSwitcher}
            showsHorizontalScrollIndicator={false}
          >
            {serverSessions.map((serverSession) => {
              const active = serverSession.baseUrl === baseUrl;
              const label = serverLabel(serverSession);
              return (
                <Pressable
                  accessibilityLabel={`Switch to ${label}`}
                  disabled={active}
                  key={serverSession.baseUrl}
                  onPress={() => {
                    void onSwitchServer(serverSession.baseUrl);
                  }}
                  style={({ pressed }) => [
                    styles.serverPill,
                    active && styles.serverPillActive,
                    pressed && !active && styles.pressed,
                  ]}
                >
                  <Server
                    color={active ? colors.onAccent : colors.text}
                    size={16}
                  />
                  <Text
                    numberOfLines={1}
                    style={[
                      styles.serverPillText,
                      active && styles.serverPillTextActive,
                    ]}
                  >
                    {label}
                  </Text>
                </Pressable>
              );
            })}
          </ScrollView>
        )}
        <View style={styles.list}>
          <Pressable
            onPress={onOpenRootTerminal}
            style={({ pressed }) => [
              styles.projectCard,
              pressed && styles.pressed,
            ]}
          >
            <View style={styles.cardIcon}>
              <TerminalIcon color={colors.accent} size={21} />
            </View>
            <View style={styles.cardBody}>
              <Text style={styles.cardTitle}>{rootTerminal.label}</Text>
              <Text style={styles.cardMeta}>{rootTerminal.description}</Text>
            </View>
            <ChevronRight color={colors.muted} size={20} />
          </Pressable>
        </View>
        {error && <InlineNotice tone="error" text={error} />}
        {loading && projects.length === 0 ? (
          <LoadingBlock label="Loading projects" />
        ) : projects.length === 0 ? (
          <EmptyState title="No enabled projects" />
        ) : (
          <View style={styles.list}>
            {projects.map((project) => (
              <Pressable
                key={project.name}
                onPress={() => onOpenProject(project.name)}
                style={({ pressed }) => [
                  styles.projectCard,
                  pressed && styles.pressed,
                ]}
              >
                <View style={styles.cardIcon}>
                  <FolderOpen color={colors.accent} size={21} />
                </View>
                <View style={styles.cardBody}>
                  <Text style={styles.cardTitle}>{project.name}</Text>
                  <Text style={styles.cardMeta}>{project.summary}</Text>
                </View>
                <ChevronRight color={colors.muted} size={20} />
              </Pressable>
            ))}
          </View>
        )}
      </ScrollView>
    </View>
  );
}

function serverLabel(session: SessionRecord): string {
  const hostname = session.deviceHostname?.trim();
  if (hostname) {
    return hostname;
  }

  try {
    return new URL(session.baseUrl).host;
  } catch {
    return session.baseUrl;
  }
}
