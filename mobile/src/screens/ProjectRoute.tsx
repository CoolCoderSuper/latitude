import { ArrowLeft } from 'lucide-react-native';
import { useIsFocused } from '@react-navigation/native';
import { useCallback, useEffect, useRef, useState } from 'react';
import { AppState, ScrollView, View } from 'react-native';

import type { LatitudePublicApi } from '../api';
import {
  EmptyState,
  IconButton,
  InlineNotice,
  LoadingBlock,
  ScreenHeader,
} from '../components/ui';
import type { ProjectTab } from '../navigationTypes';
import { ProjectScreen } from './ProjectScreen';
import { useRefreshControl, useTheme } from '../theme';
import type { DeploymentSummary, ProjectDetail, SessionRecord } from '../types';
import { errorMessage } from '../utils/errors';
import { appendDeviceHostname } from '../utils/headers';

export function ProjectRoute({
  api,
  deviceHostname,
  initialTab,
  onBack,
  onOpenViewer,
  onOpenGitHistory,
  projectName,
  session,
}: {
  api: LatitudePublicApi;
  deviceHostname?: string;
  initialTab: ProjectTab;
  onBack: () => void;
  onOpenViewer: (deployment: DeploymentSummary) => void;
  onOpenGitHistory: () => void;
  projectName: string;
  session: SessionRecord;
}) {
  const { colors, styles } = useTheme();
  const isFocused = useIsFocused();
  const [project, setProject] = useState<ProjectDetail | null>(null);
  const [projectLoading, setProjectLoading] = useState(true);
  const [tab, setTab] = useState<ProjectTab>(initialTab);
  const [error, setError] = useState<string | null>(null);
  const requestPendingRef = useRef<string | null>(null);

  const loadProject = useCallback(async (fetchRemote = false, quiet = false) => {
    if (requestPendingRef.current === projectName) return;
    const requestKey = projectName;
    requestPendingRef.current = requestKey;
    if (!quiet) {
      setProjectLoading(true);
      setError(null);
    }
    try {
      setProject(await api.project(projectName, fetchRemote));
    } catch (projectError) {
      if (!quiet) setError(errorMessage(projectError));
    } finally {
      if (requestPendingRef.current === requestKey) {
        requestPendingRef.current = null;
      }
      if (!quiet) setProjectLoading(false);
    }
  }, [api, projectName]);

  useEffect(() => {
    void loadProject(true);
  }, [loadProject]);

  useEffect(() => {
    if (!isFocused) return;
    let appActive = AppState.currentState === 'active';
    const refresh = (fetchRemote = false) => {
      if (appActive) void loadProject(fetchRemote && tab !== 'code', true);
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
  }, [isFocused, loadProject, tab]);

  const refreshControl = useRefreshControl(projectLoading, loadProject);

  if (!project) {
    return (
      <View style={styles.flex}>
        <ScreenHeader
          eyebrow={appendDeviceHostname(
            projectLoading ? 'Loading project' : 'Project unavailable',
            deviceHostname,
          )}
          left={
            <IconButton
              accessibilityLabel="Back"
              icon={<ArrowLeft color={colors.text} size={22} />}
              onPress={onBack}
            />
          }
          title={projectName}
        />
        <ScrollView
          contentContainerStyle={styles.screenContent}
          refreshControl={refreshControl}
        >
          {projectLoading ? (
            <LoadingBlock label="Loading project" />
          ) : error ? (
            <InlineNotice text={error} tone="error" />
          ) : (
            <EmptyState title="Project unavailable" />
          )}
        </ScrollView>
      </View>
    );
  }

  return (
    <ProjectScreen
      api={api}
      project={project}
      projectLoading={projectLoading}
      session={session}
      tab={tab}
      deviceHostname={deviceHostname ?? project.device_hostname}
      onBack={onBack}
      onOpenViewer={onOpenViewer}
      onOpenGitHistory={onOpenGitHistory}
      onRefresh={loadProject}
      onSelectTab={setTab}
    />
  );
}
