import {
  ArrowLeft,
  CheckCircle2,
  ChevronRight,
  GripVertical,
  Plus,
  Server,
  Trash2,
} from 'lucide-react-native';
import { useEffect, useRef, useState } from 'react';
import {
  Alert,
  Animated,
  PanResponder,
  ScrollView,
  Text,
  View,
} from 'react-native';

import {
  AppButton,
  EmptyState,
  IconButton,
  ScreenHeader,
} from '../components/ui';
import { useTheme } from '../theme';
import type { SessionRecord } from '../types';
import { appendDeviceHostname } from '../utils/headers';

export function ServersScreen({
  activeBaseUrl,
  deviceHostname,
  onAddServer,
  onBack,
  onReorderServers,
  onRemoveServer,
  onSwitchServer,
  sessions,
}: {
  activeBaseUrl: string;
  deviceHostname?: string;
  onAddServer: () => void;
  onBack: () => void;
  onReorderServers: (sessions: SessionRecord[]) => void | Promise<void>;
  onRemoveServer: (baseUrl: string) => void | Promise<void>;
  onSwitchServer: (baseUrl: string) => void | Promise<void>;
  sessions: SessionRecord[];
}) {
  const { colors, styles } = useTheme();
  const [orderedSessions, setOrderedSessions] = useState(sessions);
  const [draggingBaseUrl, setDraggingBaseUrl] = useState<string | null>(null);
  const [dropIndex, setDropIndex] = useState<number | null>(null);
  const dragY = useRef(new Animated.Value(0)).current;
  const dragSourceIndex = useRef(-1);
  const dropIndexRef = useRef<number | null>(null);
  const orderedSessionsRef = useRef(orderedSessions);
  const rowLayouts = useRef(new Map<string, { y: number; height: number }>());

  useEffect(() => {
    orderedSessionsRef.current = orderedSessions;
  }, [orderedSessions]);

  useEffect(() => {
    if (!draggingBaseUrl) {
      setOrderedSessions(sessions);
    }
  }, [draggingBaseUrl, sessions]);

  const confirmRemove = (baseUrl: string) => {
    const active = baseUrl === activeBaseUrl;
    const message = active
      ? sessions.length > 1
        ? 'Latitude will switch to another saved server.'
        : 'No saved servers will remain.'
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

  const targetIndexForDrag = (session: SessionRecord, dy: number): number => {
    const list = orderedSessionsRef.current;
    const sourceIndex = list.findIndex((item) => item.baseUrl === session.baseUrl);
    const fallbackIndex = clampIndex(sourceIndex + Math.round(dy / 88), list);
    const draggedLayout = rowLayouts.current.get(session.baseUrl);

    if (!draggedLayout) {
      return fallbackIndex;
    }

    const centerY = draggedLayout.y + draggedLayout.height / 2 + dy;
    for (let index = 0; index < list.length; index += 1) {
      const layout = rowLayouts.current.get(list[index].baseUrl);
      if (layout && centerY < layout.y + layout.height / 2) {
        return index;
      }
    }

    return list.length - 1;
  };

  const resetDrag = () => {
    dragY.setValue(0);
    dragSourceIndex.current = -1;
    dropIndexRef.current = null;
    setDraggingBaseUrl(null);
    setDropIndex(null);
  };

  const dropDraggedSession = () => {
    const sourceIndex = dragSourceIndex.current;
    const targetIndex = dropIndexRef.current ?? sourceIndex;
    const list = orderedSessionsRef.current;

    if (sourceIndex >= 0 && targetIndex >= 0 && sourceIndex !== targetIndex) {
      const reorderedSessions = reorderSessions(list, sourceIndex, targetIndex);
      orderedSessionsRef.current = reorderedSessions;
      setOrderedSessions(reorderedSessions);
      void onReorderServers(reorderedSessions);
    }

    resetDrag();
  };

  const dragHandlersFor = (session: SessionRecord) =>
    PanResponder.create({
      onStartShouldSetPanResponder: () => true,
      onMoveShouldSetPanResponder: () => true,
      onPanResponderGrant: () => {
        const index = orderedSessionsRef.current.findIndex(
          (item) => item.baseUrl === session.baseUrl,
        );
        dragSourceIndex.current = index;
        dropIndexRef.current = index;
        setDropIndex(index);
        setDraggingBaseUrl(session.baseUrl);
        dragY.setValue(0);
      },
      onPanResponderMove: (_event, gesture) => {
        dragY.setValue(gesture.dy);
        const targetIndex = targetIndexForDrag(session, gesture.dy);
        dropIndexRef.current = targetIndex;
        setDropIndex(targetIndex);
      },
      onPanResponderRelease: dropDraggedSession,
      onPanResponderTerminate: resetDrag,
      onPanResponderTerminationRequest: () => false,
    }).panHandlers;

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={appendDeviceHostname(`${sessions.length} saved`, deviceHostname)}
        left={
          <IconButton
            accessibilityLabel="Back"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={onBack}
          />
        }
        title="Servers"
      />
      <ScrollView
        contentContainerStyle={styles.screenContent}
        scrollEnabled={!draggingBaseUrl}
      >
        <AppButton
          icon={<Plus color={colors.onAccent} size={18} />}
          label="Add server"
          onPress={onAddServer}
        />

        {orderedSessions.length === 0 ? (
          <EmptyState title="No saved servers" />
        ) : (
          <View style={styles.serverManagerList}>
            {orderedSessions.map((session, index) => {
              const active = session.baseUrl === activeBaseUrl;
              const label = serverLabel(session);
              const dragging = draggingBaseUrl === session.baseUrl;
              const dropTarget =
                draggingBaseUrl !== null && dropIndex === index && !dragging;
              return (
                <Animated.View
                  key={session.baseUrl}
                  onLayout={(event) => {
                    rowLayouts.current.set(session.baseUrl, event.nativeEvent.layout);
                  }}
                  style={[
                    styles.serverManagerRow,
                    active && styles.serverManagerRowActive,
                    dropTarget && styles.serverManagerRowDropTarget,
                    dragging && styles.serverManagerRowDragging,
                    dragging && { transform: [{ translateY: dragY }] },
                  ]}
                >
                  <View
                    accessibilityLabel={`Drag ${label}`}
                    accessibilityRole="adjustable"
                    style={[
                      styles.iconButton,
                      dragging && styles.buttonDisabled,
                    ]}
                    {...dragHandlersFor(session)}
                  >
                    <GripVertical
                      color={dragging ? colors.accent : colors.muted}
                      size={18}
                    />
                  </View>
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
                      {label}
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
                </Animated.View>
              );
            })}
          </View>
        )}
      </ScrollView>
    </View>
  );
}

function serverLabel(session: SessionRecord): string {
  const hostname = session.deviceHostname?.trim();
  if (hostname) {
    return hostname;
  }

  try {
    return new URL(session.baseUrl).host;
  } catch {
    return session.baseUrl;
  }
}

function clampIndex(index: number, sessions: SessionRecord[]): number {
  if (sessions.length === 0) {
    return -1;
  }

  return Math.max(0, Math.min(sessions.length - 1, index));
}

function reorderSessions(
  sessions: SessionRecord[],
  sourceIndex: number,
  targetIndex: number,
): SessionRecord[] {
  const reorderedSessions = [...sessions];
  const [session] = reorderedSessions.splice(sourceIndex, 1);
  reorderedSessions.splice(targetIndex, 0, session);
  return reorderedSessions;
}
