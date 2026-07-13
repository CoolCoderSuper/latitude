import {
  ArrowLeft,
  FolderCode,
  GitBranch,
  Globe2,
  Terminal as TerminalIcon,
} from 'lucide-react-native';
import { useCallback, useMemo, useRef, useState } from 'react';
import { PanResponder, View } from 'react-native';

import type { LatitudePublicApi } from '../api';
import { IconButton, ScreenHeader, SegmentButton } from '../components/ui';
import { PROJECT_TABS } from '../constants';
import { DeploymentsPanel } from '../features/deployments/DeploymentsPanel';
import { DiffPanel } from '../features/git/DiffPanel';
import { FilesPanel, type FilesPanelHandle } from '../features/files/FilesPanel';
import { TerminalPanel } from '../features/terminal/TerminalPanel';
import type { ProjectTab } from '../navigationTypes';
import { useTheme } from '../theme';
import type { DeploymentSummary, ProjectDetail, SessionRecord } from '../types';
import { appendDeviceHostname } from '../utils/headers';

export function ProjectScreen({
  api,
  deviceHostname,
  onBack,
  onOpenViewer,
  onRefresh,
  onSelectTab,
  project,
  projectLoading,
  session,
  tab,
}: {
  api: LatitudePublicApi;
  deviceHostname?: string;
  onBack: () => void;
  onOpenViewer: (deployment: DeploymentSummary) => void;
  onRefresh: () => void | Promise<void>;
  onSelectTab: (tab: ProjectTab) => void;
  project: ProjectDetail;
  projectLoading: boolean;
  session: SessionRecord;
  tab: ProjectTab;
}) {
  const { colors, styles } = useTheme();
  const [codeInteractionActive, setCodeInteractionActive] = useState(false);
  const [filesCanGoBack, setFilesCanGoBack] = useState(false);
  const filesPanelRef = useRef<FilesPanelHandle>(null);

  const selectTab = useCallback(
    (nextTab: ProjectTab) => {
      onSelectTab(nextTab);
    },
    [onSelectTab],
  );

  const selectAdjacentTab = useCallback(
    (direction: number) => {
      const currentIndex = PROJECT_TABS.indexOf(tab);
      const nextTab = PROJECT_TABS[currentIndex + direction];
      if (nextTab) {
        selectTab(nextTab);
      }
    },
    [selectTab, tab],
  );

  const handleCodeInteractionChange = useCallback((active: boolean) => {
    setCodeInteractionActive(active);
  }, []);

  const tabSwipeResponder = useMemo(
    () =>
      PanResponder.create({
        onMoveShouldSetPanResponder: (_event, gesture) => {
          if (codeInteractionActive || tab === 'files') {
            return false;
          }

          const absDx = Math.abs(gesture.dx);
          const absDy = Math.abs(gesture.dy);
          return absDx > 18 && absDx > absDy * 1.35;
        },
        onPanResponderRelease: (_event, gesture) => {
          const absDx = Math.abs(gesture.dx);
          const absDy = Math.abs(gesture.dy);
          const deliberateSwipe =
            (absDx > 72 || Math.abs(gesture.vx) > 0.45) &&
            absDx > absDy * 1.2;

          if (!deliberateSwipe) {
            return;
          }

          selectAdjacentTab(gesture.dx < 0 ? 1 : -1);
        },
        onPanResponderTerminationRequest: () => true,
      }),
    [codeInteractionActive, selectAdjacentTab, tab],
  );

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={appendDeviceHostname(project.summary, deviceHostname)}
        left={
          <IconButton
            accessibilityLabel="Back"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={() => {
              if (tab === 'files' && filesCanGoBack) {
                filesPanelRef.current?.goBack();
              } else {
                onBack();
              }
            }}
          />
        }
        title={project.name}
      />
      <View style={styles.segmented}>
        <SegmentButton
          active={tab === 'deployments'}
          icon={<Globe2 color={tab === 'deployments' ? colors.onAccent : colors.text} size={17} />}
          label="Apps"
          onPress={() => selectTab('deployments')}
        />
        <SegmentButton
          active={tab === 'code'}
          icon={<GitBranch color={tab === 'code' ? colors.onAccent : colors.text} size={17} />}
          label="Code"
          onPress={() => selectTab('code')}
        />
        <SegmentButton
          active={tab === 'files'}
          icon={<FolderCode color={tab === 'files' ? colors.onAccent : colors.text} size={17} />}
          label="Files"
          onPress={() => selectTab('files')}
        />
        <SegmentButton
          active={tab === 'terminal'}
          icon={<TerminalIcon color={tab === 'terminal' ? colors.onAccent : colors.text} size={17} />}
          label="Terminal"
          onPress={() => selectTab('terminal')}
        />
      </View>
      <View style={styles.tabPager} {...tabSwipeResponder.panHandlers}>
        <View
          pointerEvents={tab === 'deployments' ? 'auto' : 'none'}
          style={[
            styles.tabPage,
            tab === 'deployments' ? styles.tabPageActive : styles.tabPageHidden,
          ]}
        >
          <DeploymentsPanel
            api={api}
            baseUrl={session.baseUrl}
            deployments={project.deployments}
            onOpenViewer={onOpenViewer}
            onRefresh={onRefresh}
            refreshing={projectLoading}
            projectName={project.name}
          />
        </View>
        <View
          pointerEvents={tab === 'code' ? 'auto' : 'none'}
          style={[
            styles.tabPage,
            tab === 'code' ? styles.tabPageActive : styles.tabPageHidden,
          ]}
        >
          <DiffPanel
            api={api}
            onCodeInteractionChange={handleCodeInteractionChange}
            projectName={project.name}
          />
        </View>
        <View
          pointerEvents={tab === 'files' ? 'auto' : 'none'}
          style={[
            styles.tabPage,
            tab === 'files' ? styles.tabPageActive : styles.tabPageHidden,
          ]}
        >
          <FilesPanel
            ref={filesPanelRef}
            active={tab === 'files'}
            api={api}
            projectName={project.name}
            session={session}
            onFolderNavigationChange={setFilesCanGoBack}
          />
        </View>
        <View
          pointerEvents={tab === 'terminal' ? 'auto' : 'none'}
          style={[
            styles.tabPage,
            tab === 'terminal' ? styles.tabPageActive : styles.tabPageHidden,
          ]}
        >
          <TerminalPanel api={api} project={project} session={session} />
        </View>
      </View>
    </View>
  );
}
