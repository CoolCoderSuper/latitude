import { ArrowLeft } from 'lucide-react-native';
import { useCallback, useEffect, useState } from 'react';
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
import { useRefreshControl, useTheme } from '../theme';
import type { DeploymentSummary, ProjectDetail, SessionRecord } from '../types';
import { errorMessage } from '../utils/errors';

export function ProjectRoute({
  api,
  initialTab,
  onBack,
  onOpenViewer,
  projectName,
  session,
}: {
  api: LatitudePublicApi;
  initialTab: ProjectTab;
  onBack: () => void;
  onOpenViewer: (deployment: DeploymentSummary) => void;
  projectName: string;
  session: SessionRecord;
}) {
  const { colors, styles } = useTheme();
  const [project, setProject] = useState<ProjectDetail | null>(null);
  const [projectLoading, setProjectLoading] = useState(true);
  const [tab, setTab] = useState<ProjectTab>(initialTab);
  const [error, setError] = useState<string | null>(null);

  const loadProject = useCallback(async () => {
    setProjectLoading(true);
    setError(null);
    try {
      setProject(await api.project(projectName));
    } catch (projectError) {
      setError(errorMessage(projectError));
    } finally {
      setProjectLoading(false);
    }
  }, [api, projectName]);

  useEffect(() => {
    void loadProject();
  }, [loadProject]);

  const refreshControl = useRefreshControl(projectLoading, loadProject);

  if (!project) {
    return (
      <View style={styles.flex}>
        <ScreenHeader
          eyebrow={projectLoading ? 'Loading project' : 'Project unavailable'}
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
      onBack={onBack}
      onOpenViewer={onOpenViewer}
      onRefresh={loadProject}
      onSelectTab={setTab}
    />
  );
}
