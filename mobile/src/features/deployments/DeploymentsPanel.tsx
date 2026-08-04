import {
  Archive,
  ArchiveRestore,
  ExternalLink,
  Share2,
} from 'lucide-react-native';
import { Alert, Pressable, ScrollView, Text, View } from 'react-native';
import { useState } from 'react';

import type { LatitudePublicApi } from '../../api';
import {
  AppButton,
  EmptyState,
  IconButton,
  InlineNotice,
} from '../../components/ui';
import { useRefreshControl, useTheme } from '../../theme';
import type { DeploymentSummary } from '../../types';
import { errorMessage } from '../../utils/errors';
import { deploymentIcon } from './deploymentIcon';
import { ShareManagerModal } from './ShareManagerModal';

export function DeploymentsPanel({
  api,
  archivedDeployments,
  baseUrl,
  deployments,
  onOpenViewer,
  onRefresh,
  refreshing,
  projectName,
}: {
  api: LatitudePublicApi;
  archivedDeployments?: DeploymentSummary[];
  baseUrl: string;
  deployments: DeploymentSummary[];
  onOpenViewer: (deployment: DeploymentSummary) => void;
  onRefresh: () => void | Promise<void>;
  refreshing: boolean;
  projectName: string;
}) {
  const { colors, styles } = useTheme();
  const [shareDeployment, setShareDeployment] =
    useState<DeploymentSummary | null>(null);
  const [showArchived, setShowArchived] = useState(false);
  const [pendingDeployment, setPendingDeployment] = useState<string | null>(
    null,
  );
  const [notice, setNotice] = useState<{
    text: string;
    tone: 'error' | 'success';
  } | null>(null);
  const archived = archivedDeployments ?? [];
  const refreshControl = useRefreshControl(refreshing, onRefresh);

  const setArchived = async (
    deployment: DeploymentSummary,
    archived: boolean,
  ) => {
    setPendingDeployment(deployment.name);
    setNotice(null);
    try {
      await api.setDeploymentArchived(projectName, deployment.name, archived);
      if (archived) setShowArchived(true);
      await onRefresh();
      setNotice({
        text: `${deployment.name} ${archived ? 'archived' : 'restored'}.`,
        tone: 'success',
      });
    } catch (archiveError) {
      setNotice({ text: errorMessage(archiveError), tone: 'error' });
    } finally {
      setPendingDeployment(null);
    }
  };

  const confirmArchive = (deployment: DeploymentSummary) => {
    Alert.alert(
      `Archive ${deployment.name}?`,
      'It will stop serving until restored. Its content and settings will be kept.',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Archive',
          style: 'destructive',
          onPress: () => void setArchived(deployment, true),
        },
      ],
    );
  };

  return (
    <ScrollView
      contentContainerStyle={styles.screenContent}
      nestedScrollEnabled
      refreshControl={refreshControl}
    >
      {notice && <InlineNotice text={notice.text} tone={notice.tone} />}
      {deployments.length === 0 ? (
        <EmptyState title="No enabled deployments" />
      ) : (
        <View style={styles.list}>
          {deployments.map((deployment) => (
            <View key={deployment.name} style={styles.deploymentCard}>
              <Pressable
                accessibilityRole="button"
                onPress={() => onOpenViewer(deployment)}
                style={({ pressed }) => [
                  styles.deploymentOpen,
                  pressed && styles.pressed,
                ]}
              >
                <View style={styles.cardIcon}>
                  {deploymentIcon(deployment, colors)}
                </View>
                <View style={styles.cardBody}>
                  <Text style={styles.cardTitle}>{deployment.name}</Text>
                  <Text style={styles.cardMeta}>
                    {deployment.title
                      ? `${deployment.label}: ${deployment.title}`
                      : deployment.label}
                  </Text>
                </View>
                <ExternalLink color={colors.muted} size={20} />
              </Pressable>
              <IconButton
                accessibilityLabel={`Manage shares for ${deployment.name}`}
                icon={<Share2 color={colors.accent} size={19} />}
                onPress={() => setShareDeployment(deployment)}
                style={styles.deploymentShareButton}
              />
              <IconButton
                accessibilityLabel={`Archive ${deployment.name}`}
                disabled={pendingDeployment === deployment.name}
                icon={<Archive color={colors.muted} size={18} />}
                onPress={() => confirmArchive(deployment)}
                style={styles.deploymentShareButton}
              />
            </View>
          ))}
        </View>
      )}
      {archived.length > 0 && (
        <AppButton
          compact
          icon={
            showArchived ? (
              <ArchiveRestore color={colors.text} size={17} />
            ) : (
              <Archive color={colors.text} size={17} />
            )
          }
          label={
            showArchived
              ? 'Hide archived'
              : `View archived (${archived.length})`
          }
          onPress={() => setShowArchived((current) => !current)}
          variant="secondary"
        />
      )}
      {showArchived && archived.length > 0 && (
        <View style={styles.list}>
          <View style={styles.sectionHeading}>
            <Text style={styles.sectionTitle}>Archived deployments</Text>
            <Text style={styles.sectionCount}>{archived.length}</Text>
          </View>
          {archived.map((deployment) => (
            <View key={deployment.name} style={styles.deploymentCard}>
              <View style={styles.deploymentOpen}>
                <View style={styles.cardIcon}>
                  <Archive color={colors.muted} size={20} />
                </View>
                <View style={styles.cardBody}>
                  <Text style={styles.cardTitle}>{deployment.name}</Text>
                  <Text style={styles.cardMeta}>{deployment.label}</Text>
                </View>
              </View>
              <IconButton
                accessibilityLabel={`Restore ${deployment.name}`}
                disabled={pendingDeployment === deployment.name}
                icon={<ArchiveRestore color={colors.accent} size={18} />}
                onPress={() => void setArchived(deployment, false)}
                style={styles.deploymentShareButton}
              />
            </View>
          ))}
        </View>
      )}
      <ShareManagerModal
        api={api}
        baseUrl={baseUrl}
        deployment={shareDeployment}
        onClose={() => setShareDeployment(null)}
        projectName={projectName}
      />
    </ScrollView>
  );
}
