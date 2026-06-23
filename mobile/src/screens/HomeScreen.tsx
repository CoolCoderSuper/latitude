import { ChevronRight, FolderOpen, LogOut } from 'lucide-react-native';
import { Pressable, ScrollView, Text, View } from 'react-native';

import {
  EmptyState,
  IconButton,
  InlineNotice,
  LoadingBlock,
  ScreenHeader,
} from '../components/ui';
import { useRefreshControl, useTheme } from '../theme';
import type { ProjectSummary } from '../types';

export function HomeScreen({
  baseUrl,
  error,
  loading,
  onOpenProject,
  onRefresh,
  onSignOut,
  projects,
}: {
  baseUrl: string;
  error: string | null;
  loading: boolean;
  onOpenProject: (name: string) => void;
  onRefresh: () => void | Promise<void>;
  onSignOut: () => void;
  projects: ProjectSummary[];
}) {
  const { colors, styles } = useTheme();
  const refreshControl = useRefreshControl(loading, onRefresh);

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={baseUrl}
        right={
          <IconButton
            accessibilityLabel="Sign out"
            icon={<LogOut color={colors.text} size={20} />}
            onPress={onSignOut}
          />
        }
        title="Projects"
      />
      <ScrollView
        contentContainerStyle={styles.screenContent}
        refreshControl={refreshControl}
      >
        {error && <InlineNotice tone="error" text={error} />}
        {loading && projects.length === 0 ? (
          <LoadingBlock label="Loading projects" />
        ) : projects.length === 0 ? (
          <EmptyState title="No enabled projects" />
        ) : (
          <View style={styles.list}>
            {projects.map((project) => (
              <Pressable
                key={project.name}
                onPress={() => onOpenProject(project.name)}
                style={({ pressed }) => [
                  styles.projectCard,
                  pressed && styles.pressed,
                ]}
              >
                <View style={styles.cardIcon}>
                  <FolderOpen color={colors.accent} size={21} />
                </View>
                <View style={styles.cardBody}>
                  <Text style={styles.cardTitle}>{project.name}</Text>
                  <Text style={styles.cardMeta}>{project.summary}</Text>
                </View>
                <ChevronRight color={colors.muted} size={20} />
              </Pressable>
            ))}
          </View>
        )}
      </ScrollView>
    </View>
  );
}
