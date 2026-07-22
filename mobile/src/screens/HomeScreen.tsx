import {
  Archive,
  ArchiveRestore,
  ChevronRight,
  FolderOpen,
  GitBranch,
  Monitor,
  Server,
  Terminal as TerminalIcon,
} from 'lucide-react-native';
import { useIsFocused } from '@react-navigation/native';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { Alert, AppState, PanResponder, Pressable, ScrollView, Text, View } from 'react-native';

import {
  EmptyState,
  AppButton,
  IconButton,
  InlineNotice,
  LoadingBlock,
  ScreenHeader,
} from '../components/ui';
import { useRefreshControl, useTheme } from '../theme';
import type {
  ProjectSummary,
  RootDesktopLink,
  RootTerminalLink,
  SessionRecord,
} from '../types';
import { prependDeviceHostname } from '../utils/headers';

export function HomeScreen({
  baseUrl,
  deviceHostname,
  error,
  loading,
  onManageServers,
  onOpenRootDesktop,
  onOpenProject,
  onOpenRootTerminal,
  onSetWorktreeArchived,
  onRefresh,
  onSwitchServer,
  projects,
  rootDesktop,
  rootTerminal,
  serverSessions,
}: {
  baseUrl: string;
  deviceHostname?: string;
  error: string | null;
  loading: boolean;
  onManageServers: () => void;
  onOpenRootDesktop: () => void;
  onOpenProject: (name: string) => void;
  onOpenRootTerminal: () => void;
  onSetWorktreeArchived: (name: string, archived: boolean) => void | Promise<void>;
  onRefresh: (fetchRemote?: boolean, quiet?: boolean) => void | Promise<void>;
  onSwitchServer: (baseUrl: string) => void | Promise<void>;
  projects: ProjectSummary[];
  rootDesktop: RootDesktopLink | null;
  rootTerminal: RootTerminalLink;
  serverSessions: SessionRecord[];
}) {
  const { colors, styles } = useTheme();
  const isFocused = useIsFocused();
  const refreshControl = useRefreshControl(loading, onRefresh);
  const [showArchived, setShowArchived] = useState(false);
  const activeProjects = projects.filter(
    (project) => !project.worktree?.archived,
  );
  const archivedProjects = projects.filter(
    (project) => project.worktree?.archived,
  );
  const activeProjectGroups = useMemo(
    () => groupProjects(activeProjects, projects),
    [activeProjects, projects],
  );

  useEffect(() => {
    if (!isFocused) return;
    let appActive = AppState.currentState === 'active';
    const refresh = (fetchRemote = false) => {
      if (appActive) void onRefresh(fetchRemote, true);
    };
    refresh(true);
    const refreshInterval = setInterval(() => refresh(false), 2_000);
    const fetchInterval = setInterval(() => refresh(true), 30_000);
    const subscription = AppState.addEventListener('change', (state) => {
      const wasActive = appActive;
      appActive = state === 'active';
      if (appActive && !wasActive) refresh(true);
    });
    return () => {
      clearInterval(refreshInterval);
      clearInterval(fetchInterval);
      subscription.remove();
    };
  }, [isFocused, onRefresh]);

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
          {rootDesktop && (
            <Pressable
              onPress={onOpenRootDesktop}
              style={({ pressed }) => [
                styles.projectCard,
                pressed && styles.pressed,
              ]}
            >
              <View style={styles.cardIcon}>
                <Monitor color={colors.accent} size={21} />
              </View>
              <View style={styles.cardBody}>
                <Text style={styles.cardTitle}>{rootDesktop.label}</Text>
                <Text style={styles.cardMeta}>{rootDesktop.description}</Text>
              </View>
              <ChevronRight color={colors.muted} size={20} />
            </Pressable>
          )}
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
        ) : activeProjects.length === 0 && archivedProjects.length === 0 ? (
          <EmptyState title="No enabled projects" />
        ) : (
          <View style={styles.list}>
            {activeProjectGroups.map((group) =>
              group.grouped ? (
                <View key={group.key} style={styles.worktreeGroup}>
                  <View style={styles.worktreeGroupHeader}>
                    <GitBranch color={colors.accent} size={18} />
                    <Text style={styles.worktreeGroupTitle}>{group.label}</Text>
                    <Text style={styles.worktreeGroupCount}>
                      {group.projects.length} worktrees
                    </Text>
                  </View>
                  <View style={styles.worktreeGroupList}>
                    {group.projects.map((project) => (
                      <ProjectCard
                        key={project.name}
                        project={project}
                        onArchive={onSetWorktreeArchived}
                        onOpen={onOpenProject}
                      />
                    ))}
                  </View>
                </View>
              ) : (
                <ProjectCard
                  key={group.key}
                  project={group.projects[0]}
                  onArchive={onSetWorktreeArchived}
                  onOpen={onOpenProject}
                />
              ),
            )}
            {archivedProjects.length > 0 && (
              <AppButton
                compact
                icon={<Archive color={colors.text} size={16} />}
                label={`${showArchived ? 'Hide' : 'Show'} archived (${archivedProjects.length})`}
                onPress={() => setShowArchived((current) => !current)}
                variant="secondary"
              />
            )}
            {showArchived &&
              archivedProjects.map((project) => (
                <View key={project.name} style={styles.projectCard}>
                  <View style={styles.cardIcon}>
                    <Archive color={colors.muted} size={21} />
                  </View>
                  <View style={styles.cardBody}>
                    <Text style={styles.cardTitle}>
                      {project.worktree?.discovered
                        ? (project.worktree.branch ?? project.name)
                        : project.name}
                    </Text>
                    <Text numberOfLines={1} style={styles.cardMeta}>
                      {project.worktree?.path ?? project.summary}
                    </Text>
                  </View>
                  <IconButton
                    accessibilityLabel={`Restore ${project.worktree?.branch ?? project.name}`}
                    icon={<ArchiveRestore color={colors.accent} size={18} />}
                    onPress={() => void onSetWorktreeArchived(project.name, false)}
                  />
                </View>
              ))}
          </View>
        )}
      </ScrollView>
    </View>
  );
}

function ProjectCard({
  onArchive,
  onOpen,
  project,
}: {
  onArchive: (name: string, archived: boolean) => void | Promise<void>;
  onOpen: (name: string) => void;
  project: ProjectSummary;
}) {
  const { colors, styles } = useTheme();
  const label = project.worktree?.discovered
    ? (project.worktree.branch ?? project.name)
    : project.name;

  return (
    <View style={styles.projectCard}>
      <Pressable
        onPress={() => onOpen(project.name)}
        style={({ pressed }) => [
          styles.projectOpen,
          pressed && styles.pressed,
        ]}
      >
        <View style={styles.cardIcon}>
          <FolderOpen color={colors.accent} size={21} />
        </View>
        <View style={styles.cardBody}>
          <Text style={styles.cardTitle}>{label}</Text>
          <Text numberOfLines={1} style={styles.cardMeta}>
            {project.worktree?.discovered
              ? project.worktree.path
              : project.summary}
          </Text>
        </View>
        {(project.git_dirty || project.git_ahead > 0 || project.git_behind > 0) && (
          <View
            accessibilityLabel="Git working tree and remote status"
            accessible
            style={styles.gitDirtyBadge}
          >
            {project.git_dirty && (
              <>
                {project.git_additions > 0 && (
                  <Text style={styles.gitAdditionsText}>+{project.git_additions}</Text>
                )}
                {project.git_deletions > 0 && (
                  <Text style={styles.gitDeletionsText}>-{project.git_deletions}</Text>
                )}
                {project.git_additions === 0 && project.git_deletions === 0 && (
                  <Text style={styles.gitAdditionsText}>changed</Text>
                )}
              </>
            )}
            {project.git_behind > 0 && (
              <Text style={styles.gitBehindText}>↓{project.git_behind}</Text>
            )}
            {project.git_ahead > 0 && (
              <Text style={styles.gitAheadText}>↑{project.git_ahead}</Text>
            )}
          </View>
        )}
        <ChevronRight color={colors.muted} size={20} />
      </Pressable>
      {project.worktree?.discovered && (
        <IconButton
          accessibilityLabel={`Archive ${label}`}
          icon={<Archive color={colors.muted} size={18} />}
          onPress={() => {
            Alert.alert(
              'Archive worktree?',
              `${label} will be hidden from the project list. Its files and Git branch will not be changed.`,
              [
                { text: 'Cancel', style: 'cancel' },
                {
                  text: 'Archive',
                  onPress: () => void onArchive(project.name, true),
                },
              ],
            );
          }}
        />
      )}
    </View>
  );
}

type ProjectGroup = {
  key: string;
  label: string;
  grouped: boolean;
  projects: ProjectSummary[];
};

function groupProjects(
  visibleProjects: ProjectSummary[],
  allProjects: ProjectSummary[],
): ProjectGroup[] {
  const repositoryLabels = new Map<string, string>();
  for (const project of allProjects) {
    const repository = project.worktree?.repository;
    if (!repository) continue;
    if (!project.worktree?.discovered) {
      repositoryLabels.set(repository, project.name);
    }
  }

  const groups = new Map<string, ProjectGroup>();
  for (const project of visibleProjects) {
    const repository = project.worktree?.repository;
    const key = repository ?? `project:${project.name}`;
    let group = groups.get(key);
    if (!group) {
      group = {
        key,
        label: repositoryLabels.get(key) ?? project.name,
        grouped: false,
        projects: [],
      };
      groups.set(key, group);
    }
    group.projects.push(project);
  }
  return [...groups.values()].map((group) => ({
    ...group,
    grouped: group.projects.length > 1,
  }));
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
