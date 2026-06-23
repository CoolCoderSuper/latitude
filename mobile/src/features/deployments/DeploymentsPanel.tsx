import { ExternalLink } from 'lucide-react-native';
import { Pressable, ScrollView, Text, View } from 'react-native';

import { EmptyState } from '../../components/ui';
import { useRefreshControl, useTheme } from '../../theme';
import type { DeploymentSummary } from '../../types';
import { deploymentIcon } from './deploymentIcon';

export function DeploymentsPanel({
  deployments,
  onOpenViewer,
  onRefresh,
  refreshing,
}: {
  deployments: DeploymentSummary[];
  onOpenViewer: (deployment: DeploymentSummary) => void;
  onRefresh: () => void | Promise<void>;
  refreshing: boolean;
}) {
  const { colors, styles } = useTheme();
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
            <Pressable
              key={deployment.name}
              onPress={() => onOpenViewer(deployment)}
              style={({ pressed }) => [
                styles.deploymentCard,
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
          ))}
        </View>
      )}
    </ScrollView>
  );
}
