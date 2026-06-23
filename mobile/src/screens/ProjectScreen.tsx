import {
  ArrowLeft,
  GitBranch,
  Globe2,
  Terminal as TerminalIcon,
} from 'lucide-react-native';
import { useCallback, useMemo, useState } from 'react';
import { PanResponder, View } from 'react-native';

import type { LatitudePublicApi } from '../api';
import { IconButton, ScreenHeader, SegmentButton } from '../components/ui';
import { PROJECT_TABS } from '../constants';
import { DeploymentsPanel } from '../features/deployments/DeploymentsPanel';
import { DiffPanel } from '../features/git/DiffPanel';
import { TerminalPanel } from '../features/terminal/TerminalPanel';
import type { ProjectTab } from '../navigationTypes';
import { useTheme } from '../theme';
import type { DeploymentSummary, ProjectDetail, SessionRecord } from '../types';

export function ProjectScreen({
  api,
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
          if (codeInteractionActive) {
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
    [codeInteractionActive, selectAdjacentTab],
  );

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={project.summary}
        left={
          <IconButton
            accessibilityLabel="Back"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={onBack}
          />
        }
        title={project.name}
      />
      <View style={styles.segmented}>
        <SegmentButton
          active={tab === 'deployments'}
          icon={<Globe2 color={tab === 'deployments' ? colors.onAccent : colors.text} size={17} />}
          label="Deployments"
          onPress={() => selectTab('deployments')}
        />
        <SegmentButton
          active={tab === 'code'}
          icon={<GitBranch color={tab === 'code' ? colors.onAccent : colors.text} size={17} />}
          label="Code"
          onPress={() => selectTab('code')}
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
            deployments={project.deployments}
            onOpenViewer={onOpenViewer}
            onRefresh={onRefresh}
            refreshing={projectLoading}
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
