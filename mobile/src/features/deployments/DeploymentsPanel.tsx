import { ExternalLink, Share2 } from 'lucide-react-native';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { useState } from 'react';

import type { LatitudePublicApi } from '../../api';
import { EmptyState, IconButton } from '../../components/ui';
import { useRefreshControl, useTheme } from '../../theme';
import type { DeploymentSummary } from '../../types';
import { deploymentIcon } from './deploymentIcon';
import { ShareManagerModal } from './ShareManagerModal';

export function DeploymentsPanel({
  api,
  baseUrl,
  deployments,
  onOpenViewer,
  onRefresh,
  refreshing,
  projectName,
}: {
  api: LatitudePublicApi;
  baseUrl: string;
  deployments: DeploymentSummary[];
  onOpenViewer: (deployment: DeploymentSummary) => void;
  onRefresh: () => void | Promise<void>;
  refreshing: boolean;
  projectName: string;
}) {
  const { colors, styles } = useTheme();
  const [shareDeployment, setShareDeployment] = useState<DeploymentSummary | null>(null);
  const refreshControl = useRefreshControl(refreshing, onRefresh);

  return (
    <ScrollView
      contentContainerStyle={styles.screenContent}
      nestedScrollEnabled
      refreshControl={refreshControl}
    >
      {deployments.length === 0 ? (
        <EmptyState title="No enabled deployments" />
      ) : (
        <View style={styles.list}>
          {deployments.map((deployment) => (
            <View key={deployment.name} style={styles.deploymentCard}>
              <Pressable
                accessibilityRole="button"
                onPress={() => onOpenViewer(deployment)}
                style={({ pressed }) => [styles.deploymentOpen, pressed && styles.pressed]}
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
