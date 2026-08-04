import { ArrowLeft } from 'lucide-react-native';
import { useIsFocused } from '@react-navigation/native';
import { useState } from 'react';
import { ScrollView, View } from 'react-native';

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
import { useProject } from '../features/projects/useProject';
import { useRefreshControl, useTheme } from '../theme';
import type { DeploymentSummary, SessionRecord } from '../types';
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
  const [tab, setTab] = useState<ProjectTab>(initialTab);
  const {
    error,
    loading: projectLoading,
    project,
    refresh: loadProject,
  } = useProject({
    active: isFocused,
    api,
    fetchRemote: tab !== 'code',
    projectName,
  });

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
