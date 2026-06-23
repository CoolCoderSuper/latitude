import {
  ArrowLeft,
  CheckCircle2,
  ChevronRight,
  Plus,
  Server,
  Trash2,
} from 'lucide-react-native';
import { Alert, ScrollView, Text, View } from 'react-native';

import {
  AppButton,
  EmptyState,
  IconButton,
  ScreenHeader,
} from '../components/ui';
import { useTheme } from '../theme';
import type { SessionRecord } from '../types';

export function ServersScreen({
  activeBaseUrl,
  onAddServer,
  onBack,
  onRemoveServer,
  onSwitchServer,
  sessions,
}: {
  activeBaseUrl: string;
  onAddServer: () => void;
  onBack: () => void;
  onRemoveServer: (baseUrl: string) => void | Promise<void>;
  onSwitchServer: (baseUrl: string) => void | Promise<void>;
  sessions: SessionRecord[];
}) {
  const { colors, styles } = useTheme();

  const confirmRemove = (baseUrl: string) => {
    const active = baseUrl === activeBaseUrl;
    const message = active
      ? sessions.length > 1
        ? 'Latitude will switch to another saved server.'
        : 'Removing it will sign you out.'
      : 'This saved server will be removed.';

    Alert.alert('Remove server?', message, [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Remove',
        style: 'destructive',
        onPress: () => {
          void onRemoveServer(baseUrl);
        },
      },
    ]);
  };

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={`${sessions.length} saved`}
        left={
          <IconButton
            accessibilityLabel="Back"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={onBack}
          />
        }
        title="Servers"
      />
      <ScrollView contentContainerStyle={styles.screenContent}>
        <AppButton
          icon={<Plus color={colors.onAccent} size={18} />}
          label="Add server"
          onPress={onAddServer}
        />

        {sessions.length === 0 ? (
          <EmptyState title="No saved servers" />
        ) : (
          <View style={styles.serverManagerList}>
            {sessions.map((session) => {
              const active = session.baseUrl === activeBaseUrl;
              return (
                <View
                  key={session.baseUrl}
                  style={[
                    styles.serverManagerRow,
                    active && styles.serverManagerRowActive,
                  ]}
                >
                  <View
                    style={[
                      styles.serverManagerIcon,
                      active && styles.serverManagerIconActive,
                    ]}
                  >
                    <Server
                      color={active ? colors.onAccent : colors.accent}
                      size={19}
                    />
                  </View>
                  <View style={styles.serverManagerBody}>
                    <Text numberOfLines={1} style={styles.serverManagerTitle}>
                      {serverLabel(session.baseUrl)}
                    </Text>
                    <Text numberOfLines={1} style={styles.serverManagerMeta}>
                      {session.baseUrl}
                    </Text>
                  </View>
                  <View style={styles.serverManagerActions}>
                    {active ? (
                      <View style={styles.serverActiveBadge}>
                        <CheckCircle2 color={colors.onAccent} size={15} />
                        <Text style={styles.serverActiveText}>Active</Text>
                      </View>
                    ) : (
                      <AppButton
                        compact
                        icon={<ChevronRight color={colors.text} size={16} />}
                        label="Switch"
                        onPress={() => {
                          void onSwitchServer(session.baseUrl);
                        }}
                        variant="secondary"
                      />
                    )}
                    <IconButton
                      accessibilityLabel={`Remove ${session.baseUrl}`}
                      icon={<Trash2 color={colors.danger} size={18} />}
                      onPress={() => confirmRemove(session.baseUrl)}
                    />
                  </View>
                </View>
              );
            })}
          </View>
        )}
      </ScrollView>
    </View>
  );
}

function serverLabel(baseUrl: string): string {
  try {
    return new URL(baseUrl).host;
  } catch {
    return baseUrl;
  }
}
